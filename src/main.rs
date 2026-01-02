mod cli;
mod config;
mod context;
mod error;
mod notification;
mod payload;
mod provider;

use crate::cli::{
    Cli, Commands, ConfigCmd, FocusArgs, HookArgs, InstallArgs, ProvidersCmd, SendArgs,
    SourcesCmd, UrgencyArg,
};
use crate::config::{Config, MacosConfig, SourceConfig};
use crate::context::{detect_context, Context};
use crate::error::NotifallError;
use crate::notification::{Notification, Urgency};
use crate::payload::WaitPayload;
use crate::provider::{macos::MacosProvider, DeliveryOutcome, Provider, SendOptions};
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use std::str::FromStr;

fn main() -> Result<(), NotifallError> {
    let cli = Cli::parse();

    let config_path = cli.config.clone();

    match cli.command {
        Commands::Send(args) => handle_send(config_path.as_ref(), args),
        Commands::Config { command: ConfigCmd::Init(args) } => {
            handle_config_init(config_path.as_ref(), args)
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
    }
}

fn handle_send(config_path: Option<&PathBuf>, args: SendArgs) -> Result<(), NotifallError> {
    let config = load_config(config_path)?;
    let provider_name = resolve_provider(args.provider.as_deref(), config.as_ref())?;
    let source = args.source.as_ref().map(|s| s.to_lowercase());
    let source_config = resolve_source_config(config.as_ref(), source.as_deref());
    let context = detect_context();

    if args.background && args.on_click.is_none() {
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
    let notification = Notification {
        title,
        message: args.message,
        source: source.clone(),
        icon,
        link: args.link.clone(),
        sound: args.sound.clone(),
        urgency: args.urgency.map(map_urgency),
        tag: args.tag.clone(),
        sender: None,
        dedupe_key: None,
        metadata: None,
        actions: Vec::new(),
    };

    match provider_name.as_str() {
        "macos" => {
            let macos_config = resolve_macos_config(config.as_ref(), source_config, source.as_deref());

            if args.background {
                let payload = WaitPayload {
                    notification,
                    macos: macos_config,
                    on_click: args.on_click.clone(),
                    context,
                };
                let payload_path = spawn_background_wait(payload)?;
                if args.json {
                    print_send_output(
                        "macos",
                        None,
                        true,
                        Some(payload_path.to_string_lossy().to_string()),
                    )?;
                }
                return Ok(());
            }

            let wait_for_click = args.wait_for_click || args.on_click.is_some();
            let provider = MacosProvider::new(macos_config)?;
            let report = provider.send(&notification, SendOptions { wait_for_click })?;
            if wait_for_click {
                handle_click(
                    report.outcome.clone(),
                    args.on_click.as_deref(),
                    &notification,
                    context.as_ref(),
                )?;
            }
            if args.json {
                print_send_output("macos", report.outcome, false, None)?;
            }
        }
        other => return Err(NotifallError::ProviderUnsupported(other.to_string())),
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
        urgency: None,
        tag: None,
        source: Some("claude".to_string()),
        on_click: Some(on_click),
        wait_for_click: false,
        background: true,
        json: false,
        provider: None,
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
        urgency: None,
        tag: None,
        source: Some("codex".to_string()),
        on_click: Some(on_click),
        wait_for_click: false,
        background: true,
        json: false,
        provider: None,
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
