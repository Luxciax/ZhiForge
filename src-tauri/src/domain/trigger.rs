use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RequiredModifier {
    #[default]
    None,
    Ctrl,
    Alt,
    Shift,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TriggerProfile {
    pub selection_enabled: bool,
    pub drag_enabled: bool,
    pub double_click_enabled: bool,
    pub shift_click_enabled: bool,
    pub required_modifier: RequiredModifier,
    pub min_drag_distance: u32,
    pub max_drag_duration_ms: u32,
}

impl Default for TriggerProfile {
    fn default() -> Self {
        Self {
            selection_enabled: true,
            drag_enabled: true,
            double_click_enabled: true,
            shift_click_enabled: true,
            required_modifier: RequiredModifier::None,
            min_drag_distance: 8,
            max_drag_duration_ms: 8_000,
        }
    }
}
