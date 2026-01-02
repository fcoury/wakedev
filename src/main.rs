mod cli;
mod config;
mod error;
mod notification;
mod provider;

use crate::cli::{Cli, Commands, ConfigCmd, ProvidersCmd, SendArgs, UrgencyArg};
use crate::config::Config;
use crate::error::NotifallError;
use crate::notification::{Notification, Urgency};
use crate::provider::{macos::MacosProvider, Provider};
use clap::Parser;
use std::fs;
use std::path::PathBuf;

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
    }
}

fn handle_send(config_path: Option<&PathBuf>, args: SendArgs) -> Result<(), NotifallError> {
    let config = load_config(config_path)?;
    let provider_name = resolve_provider(args.provider.as_deref(), config.as_ref())?;

    let notification = Notification {
        title: args.title,
        message: args.message,
        icon: args.icon.clone(),
        link: args.link.clone(),
        urgency: args.urgency.map(map_urgency),
        tag: args.tag.clone(),
        sender: None,
        dedupe_key: None,
        metadata: None,
        actions: Vec::new(),
    };

    match provider_name.as_str() {
        "macos" => {
            let provider = MacosProvider::new(config.and_then(|c| c.macos))?;
            provider.send(&notification)?;
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

fn load_config(path: Option<&PathBuf>) -> Result<Option<Config>, NotifallError> {
    let path = path.cloned().unwrap_or_else(default_config_path);
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&contents)?;
    Ok(Some(config))
}

fn default_config_path() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("notifall/config.toml");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".config/notifall/config.toml");
    }
    PathBuf::from("notifall.toml")
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
