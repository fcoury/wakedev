use crate::notification::Notification;

pub mod macos;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct DeliveryReport {
    pub provider: &'static str,
    pub id: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider not available on this platform")]
    Unsupported,
    #[error("provider error: {0}")]
    Message(String),
}

pub trait Provider {
    fn name(&self) -> &'static str;
    fn send(&self, notification: &Notification) -> Result<DeliveryReport, ProviderError>;
}
