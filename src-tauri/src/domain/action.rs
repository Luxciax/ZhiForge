use serde::{Deserialize, Serialize};

pub const BUILTIN_ACTION_IDS: [&str; 5] = ["translate", "explain", "internalize", "summarize", "polish"];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OutputMode {
    Markdown,
    PlainText,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActionDefinition {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub enabled: bool,
    pub builtin: bool,
    #[serde(default = "default_true")]
    pub show_in_toolbar: bool,
    pub prompt_template_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alternate_language: Option<String>,
    pub output_mode: OutputMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_id: Option<String>,
    pub order: i32,
}

fn default_true() -> bool {
    true
}

pub fn builtin_actions() -> Vec<ActionDefinition> {
    [
        ("translate", "翻译", "Languages", OutputMode::PlainText),
        ("explain", "解释", "MessageCircleQuestion", OutputMode::Markdown),
        ("internalize", "内化", "BrainCircuit", OutputMode::Markdown),
        ("summarize", "总结", "ListCollapse", OutputMode::Markdown),
        ("polish", "润色", "WandSparkles", OutputMode::PlainText),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (id, name, icon, output_mode))| ActionDefinition {
        id: id.into(),
        name: name.into(),
        icon: icon.into(),
        enabled: true,
        builtin: true,
        show_in_toolbar: true,
        prompt_template_id: format!("builtin.{id}"),
        prompt: None,
        target_language: None,
        alternate_language: None,
        output_mode,
        route_id: None,
        order: index as i32,
    })
    .collect()
}
