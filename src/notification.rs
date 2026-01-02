use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub icon: Option<PathBuf>,
    pub link: Option<String>,
    pub urgency: Option<Urgency>,
    pub tag: Option<String>,
    pub sender: Option<String>,
    pub dedupe_key: Option<String>,
    pub metadata: Option<BTreeMap<String, String>>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub label: String,
    pub url: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Urgency {
    Low,
    Normal,
    High,
}

impl Default for Urgency {
    fn default() -> Self {
        Self::Normal
    }
}
