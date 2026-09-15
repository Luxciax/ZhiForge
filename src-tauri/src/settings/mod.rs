mod migration;
mod repository;
pub mod schema;

use std::{path::PathBuf, sync::RwLock};
use tauri::{AppHandle, Emitter, State};

use crate::domain::{app_rule::AppRule, trigger::TriggerProfile};
use migration::{migrate_legacy_settings, LegacyAiSettings};
use repository::{FileSettingsRepository, SettingsRepository};
use schema::AppConfig;

pub struct SettingsState {
    repository: FileSettingsRepository,
    config: RwLock<AppConfig>,
}

impl SettingsState {
    pub fn load_default() -> Result<Self, String> {
        let repository = FileSettingsRepository::new(default_config_path()?);
        let config = repository.load()?;
        Ok(Self {
            repository,
            config: RwLock::new(config),
        })
    }

    fn replace(&self, config: AppConfig) -> Result<AppConfig, String> {
        config.validate()?;
        self.repository.save(&config)?;
        *self.config.write().map_err(|_| "settings lock poisoned".to_string())? = config.clone();
        Ok(config)
    }

    pub fn trigger_profile(&self) -> Result<TriggerProfile, String> {
        self.config
            .read()
            .map_err(|_| "settings lock poisoned".to_string())
            .map(|config| config.trigger.clone())
    }

    pub fn app_rules(&self) -> Result<Vec<AppRule>, String> {
        self.config
            .read()
            .map_err(|_| "settings lock poisoned".to_string())
            .map(|config| config.app_rules.clone())
    }

    pub fn snapshot(&self) -> Result<AppConfig, String> {
        self.config
            .read()
            .map_err(|_| "settings lock poisoned".to_string())
            .map(|config| config.clone())
    }
}

#[tauri::command]
pub fn settings_get(state: State<'_, SettingsState>) -> Result<AppConfig, String> {
    state
        .config
        .read()
        .map_err(|_| "settings lock poisoned".to_string())
        .map(|config| config.clone())
}

#[tauri::command]
pub fn settings_replace(
    app: AppHandle,
    state: State<'_, SettingsState>,
    config: AppConfig,
) -> Result<AppConfig, String> {
    let config = state.replace(config)?;
    let _ = app.emit("settings://changed", config.clone());
    Ok(config)
}

#[tauri::command]
pub fn settings_migrate_legacy(
    app: AppHandle,
    state: State<'_, SettingsState>,
    legacy: LegacyAiSettings,
) -> Result<AppConfig, String> {
    let config = state.replace(migrate_legacy_settings(legacy)?)?;
    let _ = app.emit("settings://changed", config.clone());
    Ok(config)
}

fn default_config_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "APPDATA is unavailable".to_string())?;
    Ok(root.join("ZhiForge").join("config.json"))
}
