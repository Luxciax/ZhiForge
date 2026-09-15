use std::{fs, io::Write, path::PathBuf};

use super::schema::AppConfig;

pub trait SettingsRepository: Send + Sync {
    fn load(&self) -> Result<AppConfig, String>;
    fn save(&self, config: &AppConfig) -> Result<(), String>;
}

#[derive(Clone, Debug)]
pub struct FileSettingsRepository {
    path: PathBuf,
}

impl FileSettingsRepository {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl SettingsRepository for FileSettingsRepository {
    fn load(&self) -> Result<AppConfig, String> {
        if !self.path.exists() {
            return Ok(AppConfig::default());
        }
        let raw = fs::read_to_string(&self.path)
            .map_err(|error| format!("failed to read settings: {error}"))?;
        let config: AppConfig = serde_json::from_str(&raw)
            .map_err(|error| format!("failed to parse settings: {error}"))?;
        config.validate()?;
        Ok(config)
    }

    fn save(&self, config: &AppConfig) -> Result<(), String> {
        config.validate()?;
        let parent = self.path.parent().ok_or_else(|| "settings path has no parent".to_string())?;
        fs::create_dir_all(parent).map_err(|error| format!("failed to create settings directory: {error}"))?;

        let encoded = serde_json::to_vec_pretty(config)
            .map_err(|error| format!("failed to serialize settings: {error}"))?;
        let temp = self.path.with_extension("json.tmp");
        let swap = self.path.with_extension("json.swap");
        let backup = self.path.with_extension("json.bak");

        {
            let mut file = fs::File::create(&temp)
                .map_err(|error| format!("failed to create temporary settings: {error}"))?;
            file.write_all(&encoded)
                .map_err(|error| format!("failed to write temporary settings: {error}"))?;
            file.sync_all()
                .map_err(|error| format!("failed to flush temporary settings: {error}"))?;
        }

        if self.path.exists() {
            fs::copy(&self.path, &backup)
                .map_err(|error| format!("failed to backup settings: {error}"))?;
            if swap.exists() {
                let _ = fs::remove_file(&swap);
            }
            fs::rename(&self.path, &swap)
                .map_err(|error| format!("failed to stage old settings: {error}"))?;
        }

        if let Err(error) = fs::rename(&temp, &self.path) {
            if swap.exists() {
                let _ = fs::rename(&swap, &self.path);
            }
            return Err(format!("failed to replace settings: {error}"));
        }
        if swap.exists() {
            let _ = fs::remove_file(swap);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn repository_round_trips_valid_config() {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("zhiforge-settings-{unique}"));
        let path = root.join("config.json");
        let repository = FileSettingsRepository::new(path.clone());
        let config = AppConfig::default();

        repository.save(&config).unwrap();
        assert_eq!(repository.load().unwrap(), config);
        let _ = fs::remove_dir_all(root);
    }
}
