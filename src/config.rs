use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub default_provider: Option<String>,
    pub macos: Option<MacosConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MacosConfig {
    pub sound: Option<String>,
    pub app_bundle_id: Option<String>,
    pub icon: Option<PathBuf>,
}

impl Config {
    pub fn template() -> &'static str {
        r#"# notifall config
# default_provider = "macos"

[macos]
# sound = "default"
# app_bundle_id = "com.apple.Terminal"
# icon = "/path/to/icon.png"
"#
    }
}
