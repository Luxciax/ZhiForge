use serde::{Deserialize, Serialize};

use super::provider::GenerationOptions;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRef {
    pub provider_id: String,
    pub model: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActionRoute {
    pub id: String,
    pub action_id: String,
    pub primary: ModelRef,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallbacks: Vec<ModelRef>,
    #[serde(default)]
    pub options: GenerationOptions,
}
