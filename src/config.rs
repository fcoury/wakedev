use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub default_provider: Option<String>,
    pub macos: Option<MacosConfig>,
    pub remote: Option<RemoteConfig>,
    pub listener: Option<ListenerConfig>,
    pub sources: Option<BTreeMap<String, SourceConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MacosConfig {
    pub sound: Option<String>,
    pub app_bundle_id: Option<String>,
    pub icon: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceConfig {
    pub icon: Option<PathBuf>,
    pub app_bundle_id: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RemoteConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub token: Option<String>,
    pub timeout_ms: Option<u64>,
    pub retries: Option<u32>,
    pub fallback_to_local: Option<bool>,
    pub forward_enabled: Option<bool>,
    pub previous_provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListenerConfig {
    pub bind: Option<String>,
    pub port: Option<u16>,
    pub token: Option<String>,
    pub require_token: Option<bool>,
    pub prefix_hostname: Option<bool>,
    pub allow_hosts: Option<Vec<String>>,
    pub on_click: Option<String>,
}

impl Config {
    pub fn template() -> &'static str {
        r#"# wakedev config
# default_provider = "macos"

[macos]
# sound = "default" # use "none" to disable
# app_bundle_id = "com.apple.Terminal"
# icon = "/path/to/icon.png"

[remote]
# host = "127.0.0.1"
# port = 4280
# token = "..."
# timeout_ms = 2000
# retries = 2
# fallback_to_local = true
# forward_enabled = true
# previous_provider = "macos"

[listener]
# bind = "127.0.0.1"
# port = 4280
# token = "..."
# require_token = true
# prefix_hostname = true
# allow_hosts = ["127.0.0.1"]
# on_click = "wakedev focus"

[sources.claude]
# icon = "/path/to/claude.icns"
# app_bundle_id = "com.apple.Terminal"

[sources.codex]
# icon = "/path/to/openai.icns"
"#
    }
}
