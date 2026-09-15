use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::domain::{
    action::{builtin_actions, ActionDefinition},
    app_rule::AppRule,
    provider::ProviderProfile,
    route::ActionRoute,
    settings::UiPreferences,
    trigger::TriggerProfile,
};

pub const CURRENT_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub providers: Vec<ProviderProfile>,
    #[serde(default = "builtin_actions")]
    pub actions: Vec<ActionDefinition>,
    #[serde(default)]
    pub routes: Vec<ActionRoute>,
    #[serde(default)]
    pub trigger: TriggerProfile,
    #[serde(default)]
    pub app_rules: Vec<AppRule>,
    #[serde(default)]
    pub ui: UiPreferences,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            providers: Vec::new(),
            actions: builtin_actions(),
            routes: Vec::new(),
            trigger: TriggerProfile::default(),
            app_rules: Vec::new(),
            ui: UiPreferences::default(),
        }
    }
}

impl AppConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(format!("unsupported config schema version {}", self.schema_version));
        }

        let mut provider_ids = HashSet::new();
        for provider in &self.providers {
            provider.validate()?;
            if !provider_ids.insert(provider.id.as_str()) {
                return Err(format!("duplicate provider id '{}'", provider.id));
            }
        }

        if let Some(active_provider_id) = self.ui.active_provider_id.as_deref() {
            if !provider_ids.contains(active_provider_id) {
                return Err(format!("active provider '{}' does not exist", active_provider_id));
            }
        }

        if !(1..=64).contains(&self.trigger.min_drag_distance) {
            return Err("trigger min drag distance must be between 1 and 64 pixels".into());
        }
        if !(500..=30_000).contains(&self.trigger.max_drag_duration_ms) {
            return Err("trigger max drag duration must be between 500 and 30000 ms".into());
        }

        let mut app_rule_ids = HashSet::new();
        for rule in &self.app_rules {
            if rule.id.trim().is_empty() {
                return Err("app rule id is empty".into());
            }
            if rule.process_pattern.trim().is_empty() {
                return Err(format!("app rule '{}' has an empty process pattern", rule.id));
            }
            if !app_rule_ids.insert(rule.id.as_str()) {
                return Err(format!("duplicate app rule id '{}'", rule.id));
            }
        }

        let mut action_ids = HashSet::new();
        for action in &self.actions {
            if action.id.trim().is_empty() {
                return Err("action id is empty".into());
            }
            if action.id.len() > 128
                || !action
                    .id
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
            {
                return Err(format!("action '{}' has an invalid id", action.id));
            }
            if action.name.trim().is_empty() || action.name.chars().count() > 64 {
                return Err(format!("action '{}' has an invalid name", action.id));
            }
            if action.prompt_template_id.trim().is_empty() {
                return Err(format!("action '{}' has an empty prompt template id", action.id));
            }
            if action.prompt.as_deref().map(str::len).unwrap_or(0) > 65_536 {
                return Err(format!("action '{}' prompt is too large", action.id));
            }
            for (label, language) in [
                ("target", action.target_language.as_deref()),
                ("alternate", action.alternate_language.as_deref()),
            ] {
                if language.map(str::trim).map(str::len).unwrap_or(0) > 64 {
                    return Err(format!("action '{}' {label} language is too long", action.id));
                }
            }
            if !action.builtin
                && action
                    .prompt
                    .as_deref()
                    .map(str::trim)
                    .filter(|prompt| !prompt.is_empty())
                    .is_none()
            {
                return Err(format!("custom action '{}' requires a prompt", action.id));
            }
            if !action_ids.insert(action.id.as_str()) {
                return Err(format!("duplicate action id '{}'", action.id));
            }
        }

        let mut route_ids = HashSet::new();
        for route in &self.routes {
            if !route_ids.insert(route.id.as_str()) {
                return Err(format!("duplicate route id '{}'", route.id));
            }
            if !action_ids.contains(route.action_id.as_str()) {
                return Err(format!("route '{}' references unknown action '{}'", route.id, route.action_id));
            }
            validate_model_ref(&route.id, &route.primary.provider_id, &route.primary.model, &provider_ids)?;
            for fallback in &route.fallbacks {
                validate_model_ref(&route.id, &fallback.provider_id, &fallback.model, &provider_ids)?;
            }
            route
                .options
                .validate()
                .map_err(|error| format!("route '{}' has invalid options: {error}", route.id))?;
        }
        Ok(())
    }
}

fn validate_model_ref(
    route_id: &str,
    provider_id: &str,
    model: &str,
    provider_ids: &HashSet<&str>,
) -> Result<(), String> {
    if !provider_ids.contains(provider_id) {
        return Err(format!("route '{route_id}' references unknown provider '{provider_id}'"));
    }
    if model.trim().is_empty() {
        return Err(format!("route '{route_id}' contains an empty model"));
    }
    Ok(())
}
