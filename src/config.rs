use crate::routes::{Destination, RouteConfig, RoutesConfig};
use anyhow::Result;
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
        let dir = config_dir()?;
        Ok(dir.join("config.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let metadata = fs::metadata(&path)?;
            if metadata.len() > 1024 * 1024 {
                anyhow::bail!(
                    "Config file exceeds 1MB limit — refusing to load possibly corrupted config"
                );
            }
            let content = fs::read_to_string(&path)?;
            match serde_json::from_str(&content) {
                Ok(config) => Ok(config),
                Err(error) => {
                    log::warn!("Unable to parse config at {}: {error}", path.display());
                    Ok(Config::default())
                }
            }
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

    /// Ensure all default routes exist in the config.
    /// Migrates older configs that are missing newer routes.
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
        // Migrate the old RugCheck default to the released shortcut.
        if let Some(route) = self.routes.get_mut(Destination::RugCheck) {
            if route.modifiers == 0x0001 && route.vk_code == 0x52 {
                *route = RouteConfig::default_for(Destination::RugCheck);
                changed = true;
            }
        }
        changed
    }
}

pub fn config_dir() -> Result<PathBuf> {
    if let Ok(test_dir) = std::env::var("COPE_TEST_DATA_DIR") {
        let dir = PathBuf::from(test_dir);
        fs::create_dir_all(&dir)?;
        return Ok(dir);
    }

    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("LOCALAPPDATA is not available"))?;
    let dir = local_app_data.join("COPE");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.start_with_windows);
        assert!(!config.show_notifications);
        assert_eq!(config.routes.routes.len(), 9);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(config.start_with_windows, deserialized.start_with_windows);
        assert_eq!(config.show_notifications, deserialized.show_notifications);
    }

    #[test]
    fn test_stale_config_missing_axiom_does_not_panic() {
        let stale_json = r#"{
  "routes": {
    "routes": {
      "fomo": { "destination": "fomo", "modifiers": 1, "vk_code": 70, "enabled": true },
      "dexscreener": { "destination": "dexscreener", "modifiers": 1, "vk_code": 68, "enabled": true },
      "solscan": { "destination": "solscan", "modifiers": 1, "vk_code": 83, "enabled": true },
      "pumpfun": { "destination": "pumpfun", "modifiers": 1, "vk_code": 80, "enabled": true },
      "xsearch": { "destination": "xsearch", "modifiers": 1, "vk_code": 88, "enabled": true },
      "gmgn": { "destination": "gmgn", "modifiers": 1, "vk_code": 71, "enabled": true }
    }
  },
  "start_with_windows": true,
  "show_notifications": false
}"#;
        let mut config: Config = serde_json::from_str(stale_json).unwrap();
        assert_eq!(
            config.routes.routes.len(),
            6,
            "stale config has only 6 routes"
        );

        let changed = config.ensure_default_routes();
        assert!(changed, "ensure_default_routes should add missing routes");
        assert_eq!(
            config.routes.routes.len(),
            9,
            "all 9 routes present after repair"
        );

        for dest in Destination::all() {
            let route = config.get_route(*dest);
            assert!(
                route.is_some(),
                "route for {:?} must exist after ensure_default_routes",
                dest
            );
            assert!(
                route.unwrap().enabled,
                "route for {:?} must be enabled",
                dest
            );
        }

        let axiom_url = Destination::Axiom.build_url(&crate::parser::SolanaAddress(
            "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU".to_string(),
        ));
        assert!(axiom_url.starts_with("https://axiom.trade/t/"));
        assert!(axiom_url.ends_with("?chain=sol"));
    }

    #[test]
    fn test_missing_route_combinations_migrate_to_all_nine() {
        let combinations = [
            vec![Destination::Axiom],
            vec![Destination::RugCheck],
            vec![Destination::BundleChecker],
            vec![Destination::Axiom, Destination::RugCheck],
            vec![Destination::Axiom, Destination::BundleChecker],
            vec![Destination::RugCheck, Destination::BundleChecker],
        ];

        for missing in combinations {
            let mut config = Config::default();
            for destination in missing {
                config.routes.routes.remove(&destination);
            }
            assert!(config.ensure_default_routes());
            assert_eq!(config.routes.routes.len(), 9);
            for destination in Destination::all() {
                assert!(config.get_route(*destination).is_some());
            }
        }
    }

    #[test]
    fn test_old_rugcheck_hotkey_migrates_to_alt_q() {
        let mut config = Config::default();
        config
            .routes
            .get_mut(Destination::RugCheck)
            .unwrap()
            .vk_code = 0x52;
        assert!(config.ensure_default_routes());
        assert_eq!(
            config.get_route(Destination::RugCheck).unwrap().vk_code,
            0x51
        );
    }
}
