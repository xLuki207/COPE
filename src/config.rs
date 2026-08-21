use crate::routes::{Destination, RouteConfig, RoutesConfig};
use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub routes: RoutesConfig,
    pub start_with_windows: bool,
    pub show_notifications: bool,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "cope", "COPE")
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
        let config_dir = proj_dirs.config_dir();
        fs::create_dir_all(config_dir)?;
        Ok(config_dir.join("config.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: Config = serde_json::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn get_route(&self, destination: Destination) -> Option<&RouteConfig> {
        self.routes.get(destination)
    }

    #[allow(dead_code)]
    pub fn get_route_mut(&mut self, destination: Destination) -> Option<&mut RouteConfig> {
        self.routes.get_mut(destination)
    }

    pub fn enabled_routes(&self) -> Vec<(&Destination, &RouteConfig)> {
        self.routes.enabled_routes()
    }

    /// Ensure all 7 default routes exist in the config.
    /// Migrates older configs that are missing the FOMO route or other defaults.
    pub fn ensure_default_routes(&mut self) -> bool {
        let mut changed = false;
        for dest in Destination::all() {
            if self.routes.get(*dest).is_none() {
                self.routes
                    .routes
                    .insert(*dest, RouteConfig::default_for(*dest));
                changed = true;
            }
        }
        changed
    }
}

pub fn config_dir() -> Result<PathBuf> {
    let proj_dirs = ProjectDirs::from("com", "cope", "COPE")
        .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?;
    Ok(proj_dirs.config_dir().to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.start_with_windows);
        assert!(!config.show_notifications);
        assert_eq!(config.routes.routes.len(), 7);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config.start_with_windows, deserialized.start_with_windows);
        assert_eq!(config.show_notifications, deserialized.show_notifications);
    }
}
