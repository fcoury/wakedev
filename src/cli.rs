use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "ding", version, about = "Multi-provider notification CLI")]
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
    /// Manage configured sources
    Sources {
        #[command(subcommand)]
        command: SourcesCmd,
    },
    /// Install integrations for Claude Code or Codex
    Install(InstallArgs),
    /// Hook entrypoint for Claude Code or Codex notify
    Hook(HookArgs),
    /// Focus the originating terminal/tmux context
    Focus(FocusArgs),
    /// Listen for remote notifications
    Listen(ListenArgs),
    /// Remote provider utilities
    Remote {
        #[command(subcommand)]
        command: RemoteCmd,
    },
    /// Telegram provider utilities
    Telegram {
        #[command(subcommand)]
        command: TelegramCmd,
    },
    /// Internal macOS click-wait helper
    #[command(hide = true)]
    WaitMacos(WaitMacosArgs),
}

#[derive(Debug, Args)]
pub struct SendArgs {
    /// Notification title (optional)
    #[arg(long)]
    pub title: Option<String>,

    /// Notification message/body
    #[arg(value_name = "MESSAGE")]
    pub message: String,

    /// Icon path (provider-specific)
    #[arg(long)]
    pub icon: Option<PathBuf>,

    /// Disable icon usage
    #[arg(long)]
    pub no_icon: bool,

    /// Optional URL to associate with the notification
    #[arg(long)]
    pub link: Option<String>,

    /// Sound name to play (macOS)
    #[arg(long)]
    pub sound: Option<String>,

    /// Disable notification sound
    #[arg(long)]
    pub silent: bool,

    /// Telegram bot token (telegram provider only)
    #[arg(long)]
    pub telegram_token: Option<String>,

    /// Telegram chat ID (telegram provider only)
    #[arg(long)]
    pub telegram_chat_id: Option<String>,

    /// Telegram parse mode (MarkdownV2 or HTML)
    #[arg(long)]
    pub telegram_parse_mode: Option<String>,

    /// Disable Telegram notifications (telegram provider only)
    #[arg(long)]
    pub telegram_silent: bool,

    /// Notification urgency
    #[arg(long, value_enum)]
    pub urgency: Option<UrgencyArg>,

    /// Optional tag/category (provider-specific)
    #[arg(long)]
    pub tag: Option<String>,

    /// Source identifier to resolve icon/logo (e.g. claude, codex)
    #[arg(long)]
    pub source: Option<String>,

    /// Command to execute on click
    #[arg(long)]
    pub on_click: Option<String>,

    /// Wait for user click (blocking)
    #[arg(long)]
    pub wait_for_click: bool,

    /// Detach and wait for click in background (implies --wait-for-click)
    #[arg(long)]
    pub background: bool,

    /// Output a JSON report to stdout
    #[arg(long)]
    pub json: bool,

    /// Provider override (e.g. macos)
    #[arg(long)]
    pub provider: Option<String>,

    /// Remote listener host (remote provider only)
    #[arg(long)]
    pub remote_host: Option<String>,

    /// Remote listener port (remote provider only)
    #[arg(long)]
    pub remote_port: Option<u16>,

    /// Remote listener auth token (remote provider only)
    #[arg(long)]
    pub remote_token: Option<String>,

    /// Remote listener timeout in milliseconds (remote provider only)
    #[arg(long)]
    pub remote_timeout_ms: Option<u64>,

    /// Remote listener retry count (remote provider only)
    #[arg(long)]
    pub remote_retries: Option<u32>,

    /// Disable fallback to local provider if remote delivery fails
    #[arg(long)]
    pub no_fallback: bool,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Create a default config file
    Init(ConfigInitArgs),
    /// Set a config key (supports dotted paths)
    Set(ConfigSetArgs),
    /// Show the resolved config path
    Path,
    /// Show current config contents
    List,
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

#[derive(Debug, Args)]
pub struct ConfigSetArgs {
    /// Config key to set (e.g. remote.host)
    pub key: String,

    /// Value to set
    pub value: String,
}

#[derive(Debug, Subcommand)]
pub enum ProvidersCmd {
    /// List providers available on this platform
    List,
}

#[derive(Debug, Subcommand)]
pub enum SourcesCmd {
    /// List configured sources
    List,
}

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Target tool (claude or codex)
    #[arg(value_enum)]
    pub target: InstallTarget,

    /// Apply changes (default is dry-run)
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum InstallTarget {
    Claude,
    Codex,
}

#[derive(Debug, Args)]
pub struct HookArgs {
    /// Target tool (claude or codex)
    #[arg(value_enum)]
    pub target: InstallTarget,

    /// JSON payload (if not provided, read from stdin)
    pub json: Option<String>,
}

#[derive(Debug, Args)]
pub struct FocusArgs {
    /// tmux session name
    #[arg(long)]
    pub tmux_session: Option<String>,

    /// tmux window id (e.g. @1)
    #[arg(long)]
    pub tmux_window: Option<String>,

    /// tmux pane id (e.g. %3)
    #[arg(long)]
    pub tmux_pane: Option<String>,

    /// Terminal app name (ghostty, iterm, terminal)
    #[arg(long)]
    pub terminal: Option<String>,

    /// Skip terminal activation
    #[arg(long)]
    pub no_activate: bool,
}

#[derive(Debug, Args)]
pub struct WaitMacosArgs {
    /// Path to payload JSON
    #[arg(long)]
    pub payload: PathBuf,
}

#[derive(Debug, Args)]
pub struct ListenArgs {
    /// Bind address (default 0.0.0.0)
    #[arg(long)]
    pub bind: Option<String>,

    /// Port to listen on (default 4280)
    #[arg(long)]
    pub port: Option<u16>,

    /// Auth token for incoming requests
    #[arg(long)]
    pub token: Option<String>,

    /// Require auth token even if none is configured
    #[arg(long)]
    pub require_token: bool,

    /// Prefix notification titles with hostname
    #[arg(long)]
    pub prefix_hostname: bool,

    /// Allowed remote hosts (repeatable)
    #[arg(long)]
    pub allow_host: Vec<String>,

    /// Command to execute on click (defaults to \"ding focus\")
    #[arg(long)]
    pub on_click: Option<String>,

    /// Disable click handling entirely
    #[arg(long)]
    pub no_click: bool,
}

#[derive(Debug, Subcommand)]
pub enum RemoteCmd {
    /// Ping the configured remote listener
    Ping(RemotePingArgs),
    /// Toggle remote forwarding for all notifications
    Forward(RemoteForwardArgs),
}

#[derive(Debug, Subcommand)]
pub enum TelegramCmd {
    /// Fetch recent chat IDs for the bot
    ChatId(TelegramChatIdArgs),
}

#[derive(Debug, Args)]
pub struct TelegramChatIdArgs {
    /// Telegram bot token
    #[arg(long)]
    pub token: Option<String>,

    /// Apply and set telegram.chat_id in config
    #[arg(long)]
    pub apply: bool,
}

#[derive(Debug, Args)]
pub struct RemotePingArgs {
    /// Remote listener host
    #[arg(long)]
    pub remote_host: Option<String>,

    /// Remote listener port
    #[arg(long)]
    pub remote_port: Option<u16>,

    /// Remote listener auth token
    #[arg(long)]
    pub remote_token: Option<String>,
}

#[derive(Debug, Args)]
pub struct RemoteForwardArgs {
    /// Desired forward state
    #[arg(value_enum)]
    pub state: ForwardState,

    /// Remote listener host (sets remote.host when enabling)
    #[arg(long)]
    pub host: Option<String>,

    /// Remote listener port (sets remote.port when enabling)
    #[arg(long)]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ForwardState {
    On,
    Off,
    Toggle,
    Status,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum UrgencyArg {
    Low,
    Normal,
    High,
}
