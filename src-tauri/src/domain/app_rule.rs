use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AppRuleBehavior {
    #[default]
    Inherit,
    Enable,
    Disable,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardFallbackPolicy {
    #[default]
    Inherit,
    Allow,
    Deny,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppRule {
    pub id: String,
    pub process_pattern: String,
    #[serde(default)]
    pub behavior: AppRuleBehavior,
    #[serde(default)]
    pub clipboard_fallback: ClipboardFallbackPolicy,
}
