use crate::provider::ProviderError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum NotifallError {
    #[error("config parse error: {0}")]
    ConfigParse(#[from] toml::de::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("provider error: {0}")]
    Provider(#[from] ProviderError),
    #[error("unsupported provider: {0}")]
    ProviderUnsupported(String),
    #[error("config file already exists: {0}")]
    ConfigExists(PathBuf),
    #[error("no provider available for this platform")]
    NoProviderAvailable,
}
