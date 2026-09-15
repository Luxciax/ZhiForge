use serde::Deserialize;

use crate::domain::{
    action::BUILTIN_ACTION_IDS,
    provider::ProviderProfile,
    route::{ActionRoute, ModelRef},
};

use super::schema::AppConfig;

pub const MIGRATED_PROVIDER_ID: &str = "migrated-default-provider";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyAiSettings {
    pub adapter: String,
    pub api_base: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_target_language")]
    pub target_language: String,
    #[serde(default = "default_alternate_language")]
    pub alternate_language: String,
}

pub fn migrate_legacy_settings(legacy: LegacyAiSettings) -> Result<AppConfig, String> {
    let adapter_id = match legacy.adapter.as_str() {
        "openai-compatible" => "openai-chat",
        "anthropic" => "anthropic",
        "gemini" => "gemini",
        other => return Err(format!("unsupported legacy provider adapter '{other}'")),
    };

    let mut config = AppConfig::default();
    config.ui.target_language = legacy.target_language;
    config.ui.alternate_language = legacy.alternate_language;
    config.ui.active_provider_id = Some(MIGRATED_PROVIDER_ID.into());
    config.providers.push(ProviderProfile {
        id: MIGRATED_PROVIDER_ID.into(),
        name: "迁移的默认渠道".into(),
        adapter_id: adapter_id.into(),
        api_base: legacy.api_base,
        headers: Default::default(),
        credential_id: Some(MIGRATED_PROVIDER_ID.into()),
        default_model: (!legacy.model.trim().is_empty()).then_some(legacy.model.clone()),
        enabled: true,
        defaults: Default::default(),
        extra: Default::default(),
    });

    if !legacy.model.trim().is_empty() {
        for action_id in BUILTIN_ACTION_IDS {
            let route_id = format!("builtin.{action_id}.route");
            config.routes.push(ActionRoute {
                id: route_id.clone(),
                action_id: action_id.into(),
                primary: ModelRef {
                    provider_id: MIGRATED_PROVIDER_ID.into(),
                    model: legacy.model.clone(),
                },
                fallbacks: Vec::new(),
                options: Default::default(),
            });
            if let Some(action) = config.actions.iter_mut().find(|action| action.id == action_id) {
                action.route_id = Some(route_id);
            }
        }
    }

    config.validate()?;
    Ok(config)
}

fn default_target_language() -> String {
    "简体中文".into()
}

fn default_alternate_language() -> String {
    "English".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_migrates_to_one_provider_and_five_routes() {
        let config = migrate_legacy_settings(LegacyAiSettings {
            adapter: "openai-compatible".into(),
            api_base: "https://example.com/v1".into(),
            model: "demo-model".into(),
            target_language: "简体中文".into(),
            alternate_language: "English".into(),
        })
        .unwrap();

        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.providers[0].adapter_id, "openai-chat");
        assert_eq!(config.routes.len(), 5);
        assert!(config.actions.iter().all(|action| action.route_id.is_some()));
    }
}
