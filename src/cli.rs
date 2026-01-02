use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "notifall", version, about = "Multi-provider notification CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Path to config file (TOML)
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Send a notification
    Send(SendArgs),
    /// Manage config
    Config {
        #[command(subcommand)]
        command: ConfigCmd,
    },
    /// List available providers
    Providers {
        #[command(subcommand)]
        command: ProvidersCmd,
    },
}

#[derive(Debug, Args)]
pub struct SendArgs {
    /// Notification title
    #[arg(long)]
    pub title: String,

    /// Notification message/body
    #[arg(long)]
    pub message: String,

    /// Icon path (provider-specific)
    #[arg(long)]
    pub icon: Option<PathBuf>,

    /// Optional URL to associate with the notification
    #[arg(long)]
    pub link: Option<String>,

    /// Notification urgency
    #[arg(long, value_enum)]
    pub urgency: Option<UrgencyArg>,

    /// Optional tag/category (provider-specific)
    #[arg(long)]
    pub tag: Option<String>,

    /// Provider override (e.g. macos)
    #[arg(long)]
    pub provider: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Create a default config file
    Init(ConfigInitArgs),
}

#[derive(Debug, Args)]
pub struct ConfigInitArgs {
    /// Override path for config file
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// Overwrite if the config file already exists
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Subcommand)]
pub enum ProvidersCmd {
    /// List providers available on this platform
    List,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum UrgencyArg {
    Low,
    Normal,
    High,
}
