use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to determine config directory")]
    NoConfigDir,
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Failed to serialize config: {0}")]
    SerializeError(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub flake_path: String,
    pub check_interval_minutes: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            flake_path: "/etc/nixos".to_string(),
            check_interval_minutes: 60,
        }
    }
}

impl Config {
    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        let config_dir = dirs::config_dir().ok_or(ConfigError::NoConfigDir)?;
        Ok(config_dir.join("nixos-update-checker").join("config.toml"))
    }

    /// Load config from the default XDG config path
    pub fn load() -> Result<Self, ConfigError> {
        let path = Self::config_path()?;

        if !path.exists() {
            // Return default config if file doesn't exist
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to the default XDG config path
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::config_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Check if the config is valid (has required fields set)
    pub fn is_valid(&self) -> bool {
        !self.flake_path.is_empty() && self.check_interval_minutes > 0
    }

    /// Get the flake path as a PathBuf
    pub fn flake_path(&self) -> PathBuf {
        PathBuf::from(&self.flake_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.check_interval_minutes, 60);
        assert_eq!(config.flake_path, "/etc/nixos");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            flake_path: "/home/user/nixos".to_string(),
            check_interval_minutes: 30,
        };

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.flake_path, config.flake_path);
        assert_eq!(parsed.check_interval_minutes, config.check_interval_minutes);
    }
}
