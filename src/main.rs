mod cli;
mod config;
mod context;
mod error;
mod notification;
mod payload;
mod provider;
mod remote;

use crate::cli::{
    Cli, Commands, ConfigCmd, ConfigSetArgs, FocusArgs, ForwardState, HookArgs, InstallArgs,
    ListenArgs, ProvidersCmd, RemoteCmd, RemoteForwardArgs, RemotePingArgs, SendArgs, SourcesCmd,
    UrgencyArg,
};
use crate::config::{Config, MacosConfig, SourceConfig};
use crate::context::{detect_context, Context};
use crate::error::NotifallError;
use crate::notification::{Notification, Urgency};
use crate::payload::WaitPayload;
use crate::provider::{macos::MacosProvider, DeliveryOutcome, Provider, ProviderError, SendOptions};
use crate::remote::{RemoteContext, RemoteEnvelope};
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use std::str::FromStr;
use std::time::Duration;

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), NotifallError> {
    let cli = Cli::parse();

    let config_path = cli.config.clone();

    match cli.command {
        Commands::Send(args) => handle_send(config_path.as_ref(), args),
        Commands::Config { command: ConfigCmd::Init(args) } => {
            handle_config_init(config_path.as_ref(), args)
        }
        Commands::Config { command: ConfigCmd::Set(args) } => {
            handle_config_set(config_path.as_ref(), args)
        }
        Commands::Config { command: ConfigCmd::Path } => {
            handle_config_path(config_path.as_ref())
        }
        Commands::Config { command: ConfigCmd::List } => {
            handle_config_list(config_path.as_ref())
        }
        Commands::Providers {
            command: ProvidersCmd::List,
        } => handle_providers_list(),
        Commands::Sources {
            command: SourcesCmd::List,
        } => handle_sources_list(config_path.as_ref()),
        Commands::Install(args) => handle_install(args),
        Commands::Hook(args) => handle_hook(args),
        Commands::Focus(args) => handle_focus(args),
        Commands::WaitMacos(args) => handle_wait_macos(args),
        Commands::Listen(args) => handle_listen(config_path.as_ref(), args),
        Commands::Remote { command } => handle_remote(command, config_path.as_ref()),
    }
}

fn handle_send(config_path: Option<&PathBuf>, args: SendArgs) -> Result<(), NotifallError> {
    let config = load_config(config_path)?;
    let provider_name = resolve_provider(args.provider.as_deref(), config.as_ref())?;
    let source = args.source.as_ref().map(|s| s.to_lowercase());
    let source_config = resolve_source_config(config.as_ref(), source.as_deref());
    let context = detect_context();

    if args.background && args.on_click.is_none() && provider_name == "macos" {
        return Err(NotifallError::BackgroundRequiresOnClick);
    }

    let title = resolve_title(args.title.clone(), source_config, source.as_deref());
    let mut icon = if args.no_icon {
        None
    } else {
        resolve_icon(args.icon.clone(), source_config, source.as_deref())
    };
    if icon.is_some() && !allow_image_icons() {
        icon = None;
    }
    let sound = if args.silent {
        Some("none".to_string())
    } else {
        args.sound.clone()
    };
    let notification = Notification {
        title,
        message: args.message.clone(),
        source: source.clone(),
        icon,
        link: args.link.clone(),
        sound,
        urgency: args.urgency.map(map_urgency),
        tag: args.tag.clone(),
        sender: None,
        dedupe_key: None,
        metadata: None,
        actions: Vec::new(),
    };
    let mut remote_notification = notification.clone();
    remote_notification.icon = None;

    match provider_name.as_str() {
        "macos" => {
            let macos_config = resolve_macos_config(config.as_ref(), source_config, source.as_deref());
            deliver_macos(
                notification,
                macos_config,
                args.on_click.clone(),
                args.background,
                args.wait_for_click,
                args.json,
                context,
            )?;
        }
        "remote" => {
            handle_remote_send(
                config.as_ref(),
                &args,
                notification,
                remote_notification,
                context,
                source_config,
                source.as_deref(),
            )?;
        }
        other => return Err(NotifallError::ProviderUnsupported(other.to_string())),
    }

    Ok(())
}

fn deliver_macos(
    notification: Notification,
    macos_config: Option<MacosConfig>,
    on_click: Option<String>,
    background: bool,
    wait_for_click: bool,
    json: bool,
    context: Option<Context>,
) -> Result<(), NotifallError> {
    if background {
        let payload = WaitPayload {
            notification,
            macos: macos_config,
            on_click,
            context,
        };
        let payload_path = spawn_background_wait(payload)?;
        if json {
            print_send_output(
                "macos",
                None,
                true,
                Some(payload_path.to_string_lossy().to_string()),
            )?;
        }
        return Ok(());
    }

    let wait_for_click = wait_for_click || on_click.is_some();
    let provider = MacosProvider::new(macos_config)?;
    let report = provider.send(&notification, SendOptions { wait_for_click })?;
    if wait_for_click {
        handle_click(
            report.outcome.clone(),
            on_click.as_deref(),
            &notification,
            context.as_ref(),
        )?;
    }
    if json {
        print_send_output("macos", report.outcome, false, None)?;
    }
    Ok(())
}

fn handle_config_init(
    config_path: Option<&PathBuf>,
    args: crate::cli::ConfigInitArgs,
) -> Result<(), NotifallError> {
    let path = args
        .path
        .or_else(|| config_path.cloned())
        .unwrap_or_else(default_config_path);

    if path.exists() && !args.force {
        return Err(NotifallError::ConfigExists(path));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(&path, Config::template())?;
    println!("wrote config: {}", path.display());
    Ok(())
}

fn handle_providers_list() -> Result<(), NotifallError> {
    println!("remote");
    if cfg!(target_os = "macos") {
        println!("macos");
    } else {
        println!("(no providers available on this platform yet)");
    }
    Ok(())
}

fn handle_install(args: InstallArgs) -> Result<(), NotifallError> {
    match args.target {
        crate::cli::InstallTarget::Claude => install_claude(args.apply),
        crate::cli::InstallTarget::Codex => install_codex(args.apply),
    }
}

fn handle_hook(args: HookArgs) -> Result<(), NotifallError> {
    let payload = read_hook_payload(args.json.as_deref())?;
    match args.target {
        crate::cli::InstallTarget::Claude => handle_claude_hook(payload),
        crate::cli::InstallTarget::Codex => handle_codex_hook(payload),
    }
}

fn handle_sources_list(config_path: Option<&PathBuf>) -> Result<(), NotifallError> {
    let config = load_config(config_path)?;
    let sources = match config.and_then(|c| c.sources) {
        Some(sources) if !sources.is_empty() => sources,
        _ => {
            println!("(no sources configured)");
            return Ok(());
        }
    };

    for (name, source) in sources {
        let icon = source
            .icon
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "-".to_string());
        let bundle = source
            .app_bundle_id
            .as_deref()
            .unwrap_or("-");
        println!("{name}\t{icon}\t{bundle}");
    }
    Ok(())
}

fn handle_focus(args: FocusArgs) -> Result<(), NotifallError> {
    let terminal = args
        .terminal
        .clone()
        .or_else(|| std::env::var("WAKEDEV_TERMINAL_APP").ok())
        .or_else(|| std::env::var("TERM_PROGRAM").ok());

    if !args.no_activate {
        activate_terminal(terminal.as_deref());
    }

    let tmux_session = args
        .tmux_session
        .or_else(|| std::env::var("WAKEDEV_TMUX_SESSION").ok());
    let tmux_window = args
        .tmux_window
        .or_else(|| std::env::var("WAKEDEV_TMUX_WINDOW").ok());
    let tmux_pane = args
        .tmux_pane
        .or_else(|| std::env::var("WAKEDEV_TMUX_PANE").ok());

    if tmux_session.is_none() && tmux_window.is_none() && tmux_pane.is_none() {
        return Ok(());
    }

    if let Some(session) = tmux_session.as_deref() {
        Command::new("tmux")
            .args(["switch-client", "-t", session])
            .status()?;
    }
    if let Some(window) = tmux_window.as_deref() {
        Command::new("tmux")
            .args(["select-window", "-t", window])
            .status()?;
    }
    if let Some(pane) = tmux_pane.as_deref() {
        Command::new("tmux")
            .args(["select-pane", "-t", pane])
            .status()?;
    }

    Ok(())
}

fn handle_listen(
    config_path: Option<&PathBuf>,
    args: ListenArgs,
) -> Result<(), NotifallError> {
    let config = load_config(config_path)?;
    let listener_cfg = config.as_ref().and_then(|c| c.listener.clone()).unwrap_or_default();

    let bind = args
        .bind
        .or(listener_cfg.bind)
        .unwrap_or_else(|| "0.0.0.0".to_string());
    let port = args.port.or(listener_cfg.port).unwrap_or(4280);
    let token = args.token.or(listener_cfg.token);
    let require_token = if args.require_token {
        true
    } else {
        listener_cfg.require_token.unwrap_or(token.is_some())
    };
    let prefix_hostname = if args.prefix_hostname {
        true
    } else {
        listener_cfg.prefix_hostname.unwrap_or(true)
    };
    let allow_hosts = if !args.allow_host.is_empty() {
        args.allow_host
    } else {
        listener_cfg.allow_hosts.unwrap_or_default()
    };
    let on_click = if args.no_click {
        None
    } else {
        args.on_click
            .or(listener_cfg.on_click)
            .or_else(default_focus_command)
    };

    let addr = format!("{}:{}", bind, port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    println!("wakedev listener on {addr}");

    for mut request in server.incoming_requests() {
        let path = request.url().split('?').next().unwrap_or("");
        if path == "/ping" {
            let response = json_response(200, r#"{"status":"ok"}"#);
            let _ = request.respond(response);
            continue;
        }

        if path != "/notify" {
            let response = json_response(404, r#"{"error":"not found"}"#);
            let _ = request.respond(response);
            continue;
        }

        if request.method() != &tiny_http::Method::Post {
            let response = json_response(405, r#"{"error":"method not allowed"}"#);
            let _ = request.respond(response);
            continue;
        }

        if !allow_hosts.is_empty() {
            if let Some(remote) = request.remote_addr() {
                let host = remote.ip().to_string();
                if !allow_hosts.iter().any(|allowed| allowed == &host) {
                    let response = json_response(403, r#"{"error":"forbidden"}"#);
                    let _ = request.respond(response);
                    continue;
                }
            }
        }

        if require_token {
            let incoming = extract_token(request.headers());
            if token.as_deref() != incoming.as_deref() {
                let response = json_response(401, r#"{"error":"unauthorized"}"#);
                let _ = request.respond(response);
                continue;
            }
        }

        let mut body = String::new();
        if request.as_reader().read_to_string(&mut body).is_err() {
            let response = json_response(400, r#"{"error":"invalid body"}"#);
            let _ = request.respond(response);
            continue;
        }

        let envelope: RemoteEnvelope = match serde_json::from_str(&body) {
            Ok(payload) => payload,
            Err(_) => {
                let response = json_response(400, r#"{"error":"invalid json"}"#);
                let _ = request.respond(response);
                continue;
            }
        };

        let mut notification = envelope.notification;
        notification.icon = None;
        if notification.title.trim().is_empty() {
            notification.title = "Notification".to_string();
        }

        if prefix_hostname {
            if let Some(host) = envelope
                .context
                .as_ref()
                .and_then(|ctx| ctx.origin_host.as_deref())
            {
                let suffix = format!(" [{host}]");
                if !notification.title.ends_with(&suffix) {
                    notification.title = format!("{}{}", notification.title, suffix);
                }
            }
        }

        let source_key = notification.source.as_deref();
        let source_config = resolve_source_config(config.as_ref(), source_key);
        let macos_config = resolve_macos_config(config.as_ref(), source_config, source_key);

        let local_context = detect_context();
        let _ = deliver_macos(
            notification,
            macos_config,
            on_click.clone(),
            on_click.is_some(),
            false,
            false,
            local_context,
        );

        let response = json_response(200, r#"{"status":"ok"}"#);
        let _ = request.respond(response);
    }

    Ok(())
}

fn handle_remote(command: RemoteCmd, config_path: Option<&PathBuf>) -> Result<(), NotifallError> {
    match command {
        RemoteCmd::Ping(args) => handle_remote_ping(args, config_path),
        RemoteCmd::Forward(args) => handle_remote_forward(args, config_path),
    }
}

fn handle_remote_ping(
    args: RemotePingArgs,
    config_path: Option<&PathBuf>,
) -> Result<(), NotifallError> {
    let config = load_config(config_path)?;
    let remote_cfg = config.and_then(|c| c.remote).unwrap_or_default();
    let target = resolve_remote_target(
        args.remote_host.as_deref(),
        args.remote_port,
        remote_cfg.host.as_deref(),
        remote_cfg.port,
        remote_cfg.url.as_deref(),
    )
    .ok_or_else(|| {
        NotifallError::Provider(ProviderError::Message(
            "remote host is not configured".to_string(),
        ))
    })?;
    let token = args.remote_token.or(remote_cfg.token);
    let ping_url = to_ping_url(&target.0);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(2000))
        .timeout_read(Duration::from_millis(2000))
        .build();
    let mut request = agent.get(&ping_url);
    if let Some(token) = token.as_deref() {
        request = request.set("Authorization", &format!("Bearer {token}"));
    }
    match request.call() {
        Ok(_) => {
            println!("ok");
            Ok(())
        }
        Err(err) => Err(NotifallError::Provider(
            ProviderError::Message(format!("remote ping failed: {err}")),
        )),
    }
}

fn handle_remote_forward(
    args: RemoteForwardArgs,
    config_path: Option<&PathBuf>,
) -> Result<(), NotifallError> {
    let path = config_path
        .cloned()
        .unwrap_or_else(default_config_path);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let mut doc = toml_edit::DocumentMut::from_str(&existing)?;

    let enabled = forward_enabled(&doc);
    let desired = match args.state {
        ForwardState::Status => {
            let state = if enabled { "on" } else { "off" };
            println!("{state}");
            return Ok(());
        }
        ForwardState::Toggle => !enabled,
        ForwardState::On => true,
        ForwardState::Off => false,
    };

    if desired {
        if let Some(host) = args.host.as_deref() {
            set_remote_field(&mut doc, "host", toml_edit::Value::from(host));
        }
        if let Some(port) = args.port {
            set_remote_field(&mut doc, "port", toml_edit::Value::from(port as i64));
        }

        if remote_host_from_doc(&doc).is_none() {
            let path_display = path.display();
            let message = format!(
                "Remote forwarding needs a remote host.\n\n\
Set it with:\n  wakedev remote forward on --host mba --port 4280\n\
or:\n  wakedev config set remote.host mba\n  wakedev config set remote.port 4280\n\n\
Config file: {path_display}\n\
If missing, run: wakedev config init"
            );
            return Err(NotifallError::RemoteForwardMissingHost(message));
        }

        if remote_port_from_doc(&doc).is_none() {
            set_remote_field(&mut doc, "port", toml_edit::Value::from(4280i64));
        }
        let current_default = doc
            .get("default_provider")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(current) = current_default.as_deref() {
            if current != "remote" {
                set_remote_field(&mut doc, "previous_provider", toml_edit::Value::from(current));
            }
        }
        doc["default_provider"] = toml_edit::value("remote");
        set_remote_field(&mut doc, "forward_enabled", toml_edit::Value::from(true));
    } else {
        set_remote_field(&mut doc, "forward_enabled", toml_edit::Value::from(false));
        let previous = doc
            .get("remote")
            .and_then(|v| v.get("previous_provider"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(prev) = previous.as_deref() {
            if prev != "remote" {
                doc["default_provider"] = toml_edit::value(prev);
            }
        } else if cfg!(target_os = "macos") {
            doc["default_provider"] = toml_edit::value("macos");
        } else {
            doc.remove("default_provider");
        }
    }

    let new_contents = doc.to_string();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, new_contents)?;
    println!(
        "remote forwarding {}",
        if desired { "enabled" } else { "disabled" }
    );
    Ok(())
}

fn handle_config_set(
    config_path: Option<&PathBuf>,
    args: ConfigSetArgs,
) -> Result<(), NotifallError> {
    let path = config_path
        .cloned()
        .unwrap_or_else(default_config_path);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let mut doc = toml_edit::DocumentMut::from_str(&existing)?;
    let value = parse_config_value(&args.value);
    set_toml_key(&mut doc, &args.key, value)?;
    let new_contents = doc.to_string();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, new_contents)?;
    println!("set {} in {}", args.key, path.display());
    Ok(())
}

fn handle_config_path(config_path: Option<&PathBuf>) -> Result<(), NotifallError> {
    let path = config_path
        .cloned()
        .unwrap_or_else(default_config_path);
    println!("{}", path.display());
    Ok(())
}

fn handle_config_list(config_path: Option<&PathBuf>) -> Result<(), NotifallError> {
    let path = config_path
        .cloned()
        .unwrap_or_else(default_config_path);
    if !path.exists() {
        println!("(no config file at {})", path.display());
        return Ok(());
    }
    let contents = fs::read_to_string(&path)?;
    print!("{contents}");
    Ok(())
}

fn remote_url_from_doc(doc: &toml_edit::DocumentMut) -> Option<String> {
    doc.get("remote")
        .and_then(|v| v.get("url"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

fn remote_host_from_doc(doc: &toml_edit::DocumentMut) -> Option<String> {
    doc.get("remote")
        .and_then(|v| v.get("host"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| remote_url_from_doc(doc).and_then(|url| parse_remote_url(&url).map(|t| t.0)))
}

fn remote_port_from_doc(doc: &toml_edit::DocumentMut) -> Option<u16> {
    doc.get("remote")
        .and_then(|v| v.get("port"))
        .and_then(|v| v.as_integer())
        .and_then(|v| u16::try_from(v).ok())
        .or_else(|| remote_url_from_doc(doc).and_then(|url| parse_remote_url(&url).map(|t| t.1)))
}

fn forward_enabled(doc: &toml_edit::DocumentMut) -> bool {
    if let Some(enabled) = doc
        .get("remote")
        .and_then(|v| v.get("forward_enabled"))
        .and_then(|v| v.as_bool())
    {
        return enabled;
    }
    matches!(
        doc.get("default_provider").and_then(|v| v.as_str()),
        Some("remote")
    )
}

fn set_remote_field(doc: &mut toml_edit::DocumentMut, key: &str, value: toml_edit::Value) {
    let table = doc.entry("remote").or_insert(toml_edit::table());
    if let Some(table) = table.as_table_mut() {
        table[key] = toml_edit::Item::Value(value);
    }
}

fn parse_config_value(raw: &str) -> toml_edit::Value {
    if raw.eq_ignore_ascii_case("true") {
        return toml_edit::Value::from(true);
    }
    if raw.eq_ignore_ascii_case("false") {
        return toml_edit::Value::from(false);
    }
    if let Ok(int_value) = raw.parse::<i64>() {
        return toml_edit::Value::from(int_value);
    }
    toml_edit::Value::from(raw)
}

fn set_toml_key(
    doc: &mut toml_edit::DocumentMut,
    key: &str,
    value: toml_edit::Value,
) -> Result<(), NotifallError> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return Ok(());
    }
    let mut table = doc.as_table_mut();
    for part in &parts[..parts.len().saturating_sub(1)] {
        if !table.contains_key(part) {
            table[part] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if !table[part].is_table() {
            table[part] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if let Some(next) = table[part].as_table_mut() {
            table = next;
        } else {
            return Err(NotifallError::Provider(ProviderError::Message(
                "invalid config path".to_string(),
            )));
        }
    }
    let last = parts[parts.len() - 1];
    table[last] = toml_edit::Item::Value(value);
    Ok(())
}

fn handle_remote_send(
    config: Option<&Config>,
    args: &SendArgs,
    notification: Notification,
    remote_notification: Notification,
    context: Option<Context>,
    source_config: Option<&SourceConfig>,
    source: Option<&str>,
) -> Result<(), NotifallError> {
    let remote_cfg = config.and_then(|c| c.remote.clone()).unwrap_or_default();
    let target = resolve_remote_target(
        args.remote_host.as_deref(),
        args.remote_port,
        remote_cfg.host.as_deref(),
        remote_cfg.port,
        remote_cfg.url.as_deref(),
    );
    let token = args.remote_token.clone().or(remote_cfg.token);
    let timeout_ms = args.remote_timeout_ms.or(remote_cfg.timeout_ms).unwrap_or(2000);
    let retries = args.remote_retries.or(remote_cfg.retries).unwrap_or(2);
    let fallback = !args.no_fallback && remote_cfg.fallback_to_local.unwrap_or(true);

    let envelope = RemoteEnvelope {
        notification: remote_notification,
        context: Some(RemoteContext::from_local(context.clone())),
    };

    let send_result = match target {
        Some((url, _host, _port)) => {
            send_remote_request(&url, token.as_deref(), timeout_ms, retries, &envelope)
        }
        None => Err(NotifallError::Provider(ProviderError::Message(
            "remote host is not configured".to_string(),
        ))),
    };
    if send_result.is_ok() {
        if args.json {
            print_send_output("remote", None, false, None)?;
        }
        return Ok(());
    }

    if fallback && cfg!(target_os = "macos") {
        let macos_config = resolve_macos_config(config, source_config, source);
        return deliver_macos(
            notification,
            macos_config,
            args.on_click.clone(),
            args.background,
            args.wait_for_click,
            args.json,
            context,
        );
    }

    send_result
}

fn send_remote_request(
    url: &str,
    token: Option<&str>,
    timeout_ms: u64,
    retries: u32,
    envelope: &RemoteEnvelope,
) -> Result<(), NotifallError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_millis(timeout_ms))
        .timeout_read(Duration::from_millis(timeout_ms))
        .build();
    let body = serde_json::to_value(envelope)?;
    let mut last_err = None;

    for _ in 0..=retries {
        let mut request = agent.post(url).set("Content-Type", "application/json");
        if let Some(token) = token {
            request = request.set("Authorization", &format!("Bearer {token}"));
        }
        match request.send_json(body.clone()) {
            Ok(response) => {
                if response.status() >= 200 && response.status() < 300 {
                    return Ok(());
                }
                last_err = Some(format!("remote error: status {}", response.status()));
            }
            Err(ureq::Error::Status(code, _)) => {
                last_err = Some(format!("remote error: status {}", code));
            }
            Err(err) => {
                last_err = Some(format!("remote error: {err}"));
            }
        }
    }

    Err(NotifallError::Provider(ProviderError::Message(
        last_err.unwrap_or_else(|| "remote error".to_string()),
    )))
}

fn to_ping_url(url: &str) -> String {
    if url.ends_with("/notify") {
        return url.trim_end_matches("/notify").to_string() + "/ping";
    }
    if url.ends_with('/') {
        return format!("{url}ping");
    }
    format!("{url}/ping")
}

fn resolve_remote_target(
    cli_host: Option<&str>,
    cli_port: Option<u16>,
    cfg_host: Option<&str>,
    cfg_port: Option<u16>,
    cfg_url: Option<&str>,
) -> Option<(String, String, u16)> {
    if let Some(host) = cli_host {
        let port = cli_port.or(cfg_port).unwrap_or(4280);
        let url = format!("http://{host}:{port}/notify");
        return Some((url, host.to_string(), port));
    }

    if let Some(host) = cfg_host {
        let port = cli_port.or(cfg_port).unwrap_or(4280);
        let url = format!("http://{host}:{port}/notify");
        return Some((url, host.to_string(), port));
    }

    if let Some(url) = cfg_url {
        if let Some((host, port)) = parse_remote_url(url) {
            let port = cli_port.unwrap_or(port);
            let url = format!("http://{host}:{port}/notify");
            return Some((url, host, port));
        }
    }

    None
}

fn parse_remote_url(url: &str) -> Option<(String, u16)> {
    let trimmed = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    let host_port = trimmed.split('/').next().unwrap_or(trimmed);
    if let Some((host, port)) = host_port.rsplit_once(':') {
        if let Ok(port) = port.parse::<u16>() {
            return Some((host.to_string(), port));
        }
    }
    None
}

fn default_focus_command() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("{} focus", exe.display()))
}

fn extract_token(headers: &[tiny_http::Header]) -> Option<String> {
    for header in headers {
        let name = header.field.as_str().to_string();
        if name.eq_ignore_ascii_case("authorization") {
            let value = header.value.as_str();
            if let Some(token) = value.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
        if name.eq_ignore_ascii_case("x-wakedev-token") {
            return Some(header.value.as_str().to_string());
        }
    }
    None
}

fn json_response(status: u16, body: &str) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let mut response = tiny_http::Response::from_string(body.to_string());
    let header = tiny_http::Header::from_bytes("Content-Type", "application/json").ok();
    if let Some(header) = header {
        response.add_header(header);
    }
    response.with_status_code(status)
}

fn handle_wait_macos(args: crate::cli::WaitMacosArgs) -> Result<(), NotifallError> {
    let contents = fs::read_to_string(&args.payload)?;
    let payload: WaitPayload = serde_json::from_str(&contents)?;
    let provider = MacosProvider::new(payload.macos)?;
    let report = provider.send(&payload.notification, SendOptions { wait_for_click: true })?;
    handle_click(
        report.outcome,
        payload.on_click.as_deref(),
        &payload.notification,
        payload.context.as_ref(),
    )?;
    let _ = fs::remove_file(&args.payload);
    Ok(())
}

fn load_config(path: Option<&PathBuf>) -> Result<Option<Config>, NotifallError> {
    let path = path.cloned().unwrap_or_else(default_config_path);
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&contents)?;
    Ok(Some(config))
}

fn resolve_source_config<'a>(
    config: Option<&'a Config>,
    source: Option<&str>,
) -> Option<&'a SourceConfig> {
    let source = source?;
    if let Some(cfg) = config
        .and_then(|c| c.sources.as_ref())
        .and_then(|sources| sources.get(source))
    {
        return Some(cfg);
    }
    None
}

fn resolve_title(
    cli_title: Option<String>,
    source_config: Option<&SourceConfig>,
    source: Option<&str>,
) -> String {
    if let Some(title) = cli_title {
        return title;
    }
    if let Some(display) = source_config.and_then(|cfg| cfg.display_name.as_ref()) {
        return display.clone();
    }
    if let Some(source) = source {
        return title_from_source(source);
    }
    "Wakedev".to_string()
}

fn title_from_source(source: &str) -> String {
    let mut chars = source.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Wakedev".to_string(),
    }
}

fn resolve_icon(
    cli_icon: Option<PathBuf>,
    source_config: Option<&SourceConfig>,
    source: Option<&str>,
) -> Option<PathBuf> {
    if cli_icon.is_some() {
        return cli_icon;
    }
    source_config.and_then(|cfg| cfg.icon.clone())
        .or_else(|| default_source_icon(source))
}

fn resolve_macos_config(
    config: Option<&Config>,
    source_config: Option<&SourceConfig>,
    source: Option<&str>,
) -> Option<MacosConfig> {
    let mut macos = config.and_then(|c| c.macos.clone());
    if let Some(source_cfg) = source_config {
        if source_cfg.app_bundle_id.is_some() {
            let entry = macos.get_or_insert_with(MacosConfig::default);
            entry.app_bundle_id = source_cfg.app_bundle_id.clone();
        }
    }
    if macos.as_ref().and_then(|m| m.app_bundle_id.as_ref()).is_none() {
        if let Some(bundle_id) = default_source_bundle_id(source) {
            let entry = macos.get_or_insert_with(MacosConfig::default);
            entry.app_bundle_id = Some(bundle_id);
        }
    }
    macos
}

fn default_source_icon(source: Option<&str>) -> Option<PathBuf> {
    let _ = source?;
    None
}

fn default_source_bundle_id(source: Option<&str>) -> Option<String> {
    let source = source?;
    if source == "claude" {
        return ensure_source_bundle(
            "claude",
            "Wakedev Claude",
            "com.wakedev.claude",
            include_bytes!(
                "../assets/brands/anthropic/claude/icons/claude-symbol-clay.icns"
            ),
        );
    }
    if source == "codex" {
        return ensure_source_bundle(
            "codex",
            "Wakedev Codex",
            "com.wakedev.codex",
            include_bytes!("../assets/brands/codex/icons/openai-blossom-light.icns"),
        );
    }
    None
}

fn ensure_source_bundle(
    source: &str,
    display_name: &str,
    bundle_id: &str,
    icon_bytes: &[u8],
) -> Option<String> {
    let base_dir = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|_| std::env::temp_dir());
    let app_dir = base_dir.join("wakedev/apps").join(format!("{}.app", source));
    let contents = app_dir.join("Contents");
    let macos = contents.join("MacOS");
    let resources = contents.join("Resources");

    if fs::create_dir_all(&macos).is_err() || fs::create_dir_all(&resources).is_err() {
        return None;
    }

    let icon_name = format!("{}.icns", source);
    let icon_path = resources.join(&icon_name);
    let icon_changed = match write_if_changed(&icon_path, icon_bytes) {
        Ok(changed) => changed,
        Err(_) => return None,
    };

    let plist_path = contents.join("Info.plist");
    if !plist_path.exists() || icon_changed {
        let icon_version = icon_bytes
            .iter()
            .fold(0u32, |acc, byte| acc.wrapping_add(*byte as u32));
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>{}</string>
  <key>CFBundleIdentifier</key>
  <string>{}</string>
  <key>CFBundleVersion</key>
  <string>{}</string>
  <key>CFBundleShortVersionString</key>
  <string>{}</string>
  <key>CFBundleExecutable</key>
  <string>wakedev-helper</string>
  <key>CFBundleIconFile</key>
  <string>{}</string>
  <key>LSUIElement</key>
  <true/>
</dict>
</plist>
"#,
            display_name, bundle_id, icon_version, icon_version, icon_name
        );
        if fs::write(&plist_path, plist).is_err() {
            return None;
        }
    }

    let exec_path = macos.join("wakedev-helper");
    if !exec_path.exists() {
        let script = b"#!/bin/sh\nexit 0\n";
        if fs::write(&exec_path, script).is_err() {
            return None;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mut perms) = fs::metadata(&exec_path).map(|m| m.permissions()) {
                perms.set_mode(0o755);
                let _ = fs::set_permissions(&exec_path, perms);
            }
        }
    }

    let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    let _ = Command::new(lsregister).arg("-f").arg(&app_dir).status();

    Some(bundle_id.to_string())
}

fn allow_image_icons() -> bool {
    std::env::var("WAKEDEV_ALLOW_IMAGE_ICONS").map(|v| v == "1").unwrap_or(false)
}

fn write_if_changed(path: &PathBuf, contents: &[u8]) -> Result<bool, std::io::Error> {
    if let Ok(existing) = fs::read(path) {
        if existing == contents {
            return Ok(false);
        }
    }
    fs::write(path, contents)?;
    Ok(true)
}

fn default_config_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("wakedev/config.toml");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/wakedev/config.toml");
    }
    PathBuf::from("wakedev.toml")
}

fn resolve_provider(
    cli_provider: Option<&str>,
    config: Option<&Config>,
) -> Result<String, NotifallError> {
    if let Some(provider) = cli_provider {
        return Ok(provider.to_lowercase());
    }
    if let Some(remote_enabled) = config
        .and_then(|c| c.remote.as_ref())
        .and_then(|r| r.forward_enabled)
    {
        if remote_enabled {
            return Ok("remote".to_string());
        }
    }
    if let Some(default_provider) = config.and_then(|c| c.default_provider.as_ref()) {
        return Ok(default_provider.to_lowercase());
    }
    if cfg!(target_os = "macos") {
        return Ok("macos".to_string());
    }
    Err(NotifallError::NoProviderAvailable)
}

fn map_urgency(arg: UrgencyArg) -> Urgency {
    match arg {
        UrgencyArg::Low => Urgency::Low,
        UrgencyArg::Normal => Urgency::Normal,
        UrgencyArg::High => Urgency::High,
    }
}

fn handle_click(
    outcome: Option<DeliveryOutcome>,
    on_click: Option<&str>,
    notification: &Notification,
    context: Option<&Context>,
) -> Result<(), NotifallError> {
    let cmd = match (outcome, on_click) {
        (Some(DeliveryOutcome::Clicked), Some(cmd)) => cmd,
        (Some(DeliveryOutcome::ActionButton(_)), Some(cmd)) => cmd,
        _ => return Ok(()),
    };

    let mut child = Command::new("sh");
    child.arg("-c").arg(cmd);
    if let Some(source) = notification.source.as_deref() {
        child.env("WAKEDEV_SOURCE", source);
    }
    child.env("WAKEDEV_TITLE", &notification.title);
    child.env("WAKEDEV_MESSAGE", &notification.message);
    if let Some(tag) = notification.tag.as_deref() {
        child.env("WAKEDEV_TAG", tag);
    }
    if let Some(context) = context {
        if let Some(tmux) = context.tmux.as_ref() {
            child.env("WAKEDEV_TMUX_SESSION", &tmux.session);
            child.env("WAKEDEV_TMUX_WINDOW", &tmux.window);
            child.env("WAKEDEV_TMUX_PANE", &tmux.pane);
            if let Some(client) = tmux.client.as_deref() {
                child.env("WAKEDEV_TMUX_CLIENT", client);
            }
        }
        if let Some(terminal) = context.terminal.as_ref().and_then(|t| t.app.as_deref()) {
            child.env("WAKEDEV_TERMINAL_APP", terminal);
        }
        if let Ok(json) = serde_json::to_string(context) {
            child.env("WAKEDEV_CONTEXT_JSON", json);
        }
    }

    child.spawn()?;
    Ok(())
}

fn spawn_background_wait(payload: WaitPayload) -> Result<PathBuf, NotifallError> {
    let payload_path = write_payload(payload)?;
    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("wait-macos").arg("--payload").arg(&payload_path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    cmd.spawn()?;
    Ok(payload_path)
}

fn write_payload(payload: WaitPayload) -> Result<PathBuf, NotifallError> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let file_name = format!("wakedev-payload-{}-{}.json", std::process::id(), ts);
    let path = std::env::temp_dir().join(file_name);
    let data = serde_json::to_vec(&payload)?;
    fs::write(&path, data)?;
    Ok(path)
}

fn activate_terminal(terminal: Option<&str>) {
    if !cfg!(target_os = "macos") {
        return;
    }

    let app = match terminal {
        Some(name) if name.eq_ignore_ascii_case("ghostty") => Some("Ghostty"),
        Some(name) if name.eq_ignore_ascii_case("iterm") => Some("iTerm"),
        Some(name) if name.eq_ignore_ascii_case("iterm.app") => Some("iTerm"),
        Some(name) if name.eq_ignore_ascii_case("terminal") => Some("Terminal"),
        Some(name) if name.eq_ignore_ascii_case("apple_terminal") => Some("Terminal"),
        Some(name) if name.eq_ignore_ascii_case("apple_terminal.app") => Some("Terminal"),
        Some(name) if name.eq_ignore_ascii_case("Apple_Terminal") => Some("Terminal"),
        Some(name) if name.eq_ignore_ascii_case("Apple_Terminal.app") => Some("Terminal"),
        _ => None,
    };

    if let Some(app) = app {
        let _ = Command::new("osascript")
            .args(["-e", &format!("tell application \"{}\" to activate", app)])
            .status();
    }
}

fn print_send_output(
    provider: &str,
    outcome: Option<DeliveryOutcome>,
    background: bool,
    payload: Option<String>,
) -> Result<(), NotifallError> {
    #[derive(serde::Serialize)]
    struct SendOutput<'a> {
        provider: &'a str,
        background: bool,
        outcome: serde_json::Value,
        payload: Option<String>,
    }

    let outcome_value = match outcome {
        None => serde_json::Value::Null,
        Some(DeliveryOutcome::Delivered) => serde_json::Value::String("delivered".to_string()),
        Some(DeliveryOutcome::Clicked) => serde_json::Value::String("clicked".to_string()),
        Some(DeliveryOutcome::ActionButton(label)) => serde_json::json!({
            "type": "action",
            "label": label,
        }),
        Some(DeliveryOutcome::Closed(label)) => serde_json::json!({
            "type": "closed",
            "label": label,
        }),
        Some(DeliveryOutcome::Replied(text)) => serde_json::json!({
            "type": "reply",
            "text": text,
        }),
    };

    let output = SendOutput {
        provider,
        background,
        outcome: outcome_value,
        payload,
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn install_claude(apply: bool) -> Result<(), NotifallError> {
    let home = home_dir()?;
    let claude_dir = home.join(".claude");
    let settings_path = claude_dir.join("settings.json");

    let settings = fs::read_to_string(&settings_path).unwrap_or_else(|_| "{}".to_string());
    let mut json: serde_json::Value = serde_json::from_str(&settings)?;

    let exe = std::env::current_exe()?;
    let command = format!("{} hook claude", exe.display());
    let hook_entry = serde_json::json!([{
        "matcher": "",
        "hooks": [{ "type": "command", "command": command }]
    }]);

    json["hooks"]["Notification"] = hook_entry.clone();
    json["hooks"]["Stop"] = hook_entry.clone();

    let new_contents = serde_json::to_string_pretty(&json)?;
    if !apply {
        print_diff(
            &settings_path,
            &settings,
            &new_contents,
            "wakedev install claude --apply",
        )?;
        return Ok(());
    }

    if settings_path.exists() {
        backup_file(&settings_path)?;
    }
    fs::write(&settings_path, new_contents)?;

    println!("Installed Claude hooks in {}", settings_path.display());
    Ok(())
}

fn install_codex(apply: bool) -> Result<(), NotifallError> {
    let home = home_dir()?;
    let codex_dir = home.join(".codex");
    let config_path = codex_dir.join("config.toml");

    let config = fs::read_to_string(&config_path).unwrap_or_default();
    let mut doc = toml_edit::DocumentMut::from_str(&config)?;
    let exe = std::env::current_exe()?;
    let mut notify = toml_edit::Array::default();
    notify.push(exe.display().to_string());
    notify.push("hook");
    notify.push("codex");
    doc["notify"] = toml_edit::value(notify);

    let new_contents = doc.to_string();
    if !apply {
        print_diff(
            &config_path,
            &config,
            &new_contents,
            "wakedev install codex --apply",
        )?;
        return Ok(());
    }

    if config_path.exists() {
        backup_file(&config_path)?;
    }
    fs::write(&config_path, new_contents)?;

    println!("Installed Codex notify in {}", config_path.display());
    Ok(())
}

fn home_dir() -> Result<PathBuf, NotifallError> {
    std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|_| NotifallError::MissingHome)
}

fn backup_file(path: &PathBuf) -> Result<(), NotifallError> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let backup = path.with_extension(format!("bak-{}", ts));
    fs::copy(path, &backup)?;
    Ok(())
}

fn print_diff(
    path: &PathBuf,
    old: &str,
    new: &str,
    apply_command: &str,
) -> Result<(), NotifallError> {
    let temp_dir = std::env::temp_dir();
    let old_path = temp_dir.join(format!(
        "wakedev-old-{}-{}",
        std::process::id(),
        path.file_name().and_then(|s| s.to_str()).unwrap_or("file")
    ));
    let new_path = temp_dir.join(format!(
        "wakedev-new-{}-{}",
        std::process::id(),
        path.file_name().and_then(|s| s.to_str()).unwrap_or("file")
    ));

    fs::write(&old_path, old)?;
    fs::write(&new_path, new)?;

    let output = diff_output(
        old_path.to_str().unwrap(),
        new_path.to_str().unwrap(),
    );

    let _ = fs::remove_file(&old_path);
    let _ = fs::remove_file(&new_path);

    let mut header = format!(
        "The following changes would be made to {}:\n\n",
        path.display()
    );
    let mut footer = format!(
        "\nTo apply these changes automatically re-run `{}` with the --apply command.\n",
        apply_command
    );
    if should_use_color() {
        let colored_path = format!("{}{}{}", "\x1b[33m", path.display(), "\x1b[0m");
        header = header.replace(&path.display().to_string(), &colored_path);
        header = colorize_inline_code(&header);
        footer = colorize_inline_code(&footer);
    }

    let body = match output {
        Ok(out) => {
            if !out.stdout.is_empty() {
                String::from_utf8_lossy(&out.stdout).to_string()
            } else {
                format!("No changes for {}.\n", path.display())
            }
        }
        Err(_) => {
            let mut fallback = String::new();
            fallback.push_str("Diff tool unavailable. Proposed new contents:\n\n");
            fallback.push_str(new);
            fallback.push('\n');
            fallback
        }
    };

    let output_text = format!("{header}{body}{footer}");
    if let Some(mut child) = spawn_pager() {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(output_text.as_bytes());
        }
        let _ = child.wait();
        return Ok(());
    }

    print!("{output_text}");
    Ok(())
}

fn spawn_pager() -> Option<std::process::Child> {
    if let Ok(pager) = std::env::var("PAGER") {
        if !pager.trim().is_empty() {
            return Command::new("sh")
                .arg("-c")
                .arg(&pager)
                .stdin(Stdio::piped())
                .spawn()
                .ok();
        }
    }

    for candidate in ["less", "more"] {
        if let Ok(mut cmd) = which_command(candidate) {
            if candidate == "less" {
                cmd.arg("-FRSX");
            }
            if let Ok(child) = cmd.stdin(Stdio::piped()).spawn() {
                return Some(child);
            }
        }
    }

    None
}

fn which_command(name: &str) -> Result<Command, NotifallError> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {} >/dev/null 2>&1", name))
        .status();
    match status {
        Ok(s) if s.success() => Ok(Command::new(name)),
        _ => Err(NotifallError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("pager {} not found", name),
        ))),
    }
}

fn diff_output(old_path: &str, new_path: &str) -> Result<std::process::Output, std::io::Error> {
    let use_color = should_use_color();
    if use_color {
        let output = Command::new("diff")
            .args(["-u", "--color=always", old_path, new_path])
            .output();
        if let Ok(out) = &output {
            if out.status.code() != Some(2) {
                return output;
            }
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.contains("illegal option") && !stderr.contains("unknown option") {
                return output;
            }
        }
    }
    Command::new("diff")
        .args(["-u", old_path, new_path])
        .output()
}

fn should_use_color() -> bool {
    if std::env::var("NO_COLOR").is_ok() {
        return false;
    }
    if std::env::var("FORCE_COLOR").is_ok() {
        return true;
    }
    if pager_available() {
        return true;
    }
    stdout_is_tty()
}

fn colorize_inline_code(text: &str) -> String {
    const COLOR: &str = "\x1b[36m";
    const RESET: &str = "\x1b[0m";
    let mut out = String::new();
    let mut segment = String::new();
    let mut in_code = false;

    for ch in text.chars() {
        if ch == '`' {
            if in_code {
                out.push('`');
                out.push_str(COLOR);
                out.push_str(&segment);
                out.push_str(RESET);
                out.push('`');
                segment.clear();
                in_code = false;
            } else {
                out.push_str(&segment);
                segment.clear();
                in_code = true;
            }
        } else {
            segment.push(ch);
        }
    }

    if in_code {
        out.push('`');
        out.push_str(&segment);
    } else {
        out.push_str(&segment);
    }
    out
}

fn pager_available() -> bool {
    if let Ok(pager) = std::env::var("PAGER") {
        if !pager.trim().is_empty() {
            return true;
        }
    }
    command_exists("less") || command_exists("more")
}

fn command_exists(name: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {} >/dev/null 2>&1", name))
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn stdout_is_tty() -> bool {
    #[cfg(unix)]
    unsafe {
        return libc::isatty(libc::STDOUT_FILENO) == 1;
    }
    #[cfg(not(unix))]
    {
        false
    }
}

fn read_hook_payload(json: Option<&str>) -> Result<serde_json::Value, NotifallError> {
    if let Some(raw) = json {
        return Ok(serde_json::from_str(raw)?);
    }
    let mut stdin = std::io::stdin();
    let mut buf = Vec::new();
    use std::io::Read;
    stdin.read_to_end(&mut buf)?;
    if buf.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    Ok(serde_json::from_slice(&buf)?)
}

fn handle_claude_hook(payload: serde_json::Value) -> Result<(), NotifallError> {
    let hook = payload
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown");
    let mut message = payload
        .get("message")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("prompt").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    let title = if hook == "Notification" {
        let ntype = payload
            .get("notification_type")
            .and_then(|v| v.as_str())
            .unwrap_or("notification");
        format!("Claude Code: {}", ntype)
    } else if hook == "Stop" || hook == "SubagentStop" {
        if message.is_empty() {
            message = "Task completed".to_string();
        }
        "Claude Code: finished".to_string()
    } else {
        format!("Claude Code: {}", hook)
    };

    if message.is_empty() {
        if let Some(tool) = payload.get("tool_name").and_then(|v| v.as_str()) {
            message = tool.to_string();
        } else {
            message = " ".to_string();
        }
    }

    let (title, message) = truncate_message(title, message);
    let on_click = format!("{} focus", std::env::current_exe()?.display());
    let args = SendArgs {
        title: Some(title),
        message,
        icon: None,
        no_icon: false,
        link: None,
        sound: None,
        silent: false,
        urgency: None,
        tag: None,
        source: Some("claude".to_string()),
        on_click: Some(on_click),
        wait_for_click: false,
        background: true,
        json: false,
        provider: None,
        remote_host: None,
        remote_port: None,
        remote_token: None,
        remote_timeout_ms: None,
        remote_retries: None,
        no_fallback: false,
    };

    handle_send(None, args)
}

fn handle_codex_hook(payload: serde_json::Value) -> Result<(), NotifallError> {
    let ntype = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if ntype != "agent-turn-complete" {
        return Ok(());
    }

    let assistant_message = payload
        .get("last-assistant-message")
        .and_then(|v| v.as_str());
    let title = if let Some(msg) = assistant_message {
        format!("Codex: {}", msg)
    } else {
        "Codex: Turn Complete".to_string()
    };

    let input_messages = payload
        .get("input_messages")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut message = input_messages
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    if message.is_empty() {
        message = " ".to_string();
    }

    let (title, message) = truncate_message(title, message);
    let on_click = format!("{} focus", std::env::current_exe()?.display());
    let args = SendArgs {
        title: Some(title),
        message,
        icon: None,
        no_icon: false,
        link: None,
        sound: None,
        silent: false,
        urgency: None,
        tag: None,
        source: Some("codex".to_string()),
        on_click: Some(on_click),
        wait_for_click: false,
        background: true,
        json: false,
        provider: None,
        remote_host: None,
        remote_port: None,
        remote_token: None,
        remote_timeout_ms: None,
        remote_retries: None,
        no_fallback: false,
    };

    handle_send(None, args)
}

fn truncate_message(title: String, message: String) -> (String, String) {
    let max_title = 120;
    let max_message = 300;
    (
        truncate_to(title, max_title),
        truncate_to(message, max_message),
    )
}

fn truncate_to(value: String, max_len: usize) -> String {
    if value.len() <= max_len {
        return value;
    }
    let suffix = "...";
    let take_len = max_len.saturating_sub(suffix.len());
    let mut truncated = value.chars().take(take_len).collect::<String>();
    truncated.push_str(suffix);
    truncated
}
