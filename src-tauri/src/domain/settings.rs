use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UiPreferences {
    pub target_language: String,
    pub alternate_language: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_provider_id: Option<String>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            target_language: "简体中文".into(),
            alternate_language: "English".into(),
            active_provider_id: None,
        }
    }
}
