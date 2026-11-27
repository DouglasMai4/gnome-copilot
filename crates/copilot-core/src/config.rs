use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub active_model: Option<String>,

    pub threads: u32,

    pub confirm_sensitive: bool,

    pub confidence_floor: f32,

    pub disabled_actions: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            active_model: None,
            threads: 0,
            confirm_sensitive: true,
            confidence_floor: 0.5,
            disabled_actions: vec!["run_command".into()],
        }
    }
}

impl Config {
    pub fn load() -> Self {
        Self::load_from(&paths::config_file())
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&paths::config_file())
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn action_enabled(&self, name: &str) -> bool {
        !self.disabled_actions.iter().any(|n| n == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("copilot-cfg-{}", std::process::id()));
        let file = dir.join("config.toml");
        assert_eq!(Config::load_from(&file), Config::default());
        let mut c = Config::default();
        c.active_model = Some("needle3".into());
        c.save_to(&file).unwrap();
        assert_eq!(Config::load_from(&file), c);
        assert!(!c.action_enabled("run_command"));
        std::fs::remove_dir_all(dir).ok();
    }
}
