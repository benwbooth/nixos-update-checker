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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IntervalUnit {
    #[default]
    Hours,
    Days,
    Weeks,
}

impl IntervalUnit {
    pub fn to_minutes(&self, value: u32) -> u32 {
        match self {
            IntervalUnit::Hours => value * 60,
            IntervalUnit::Days => value * 60 * 24,
            IntervalUnit::Weeks => value * 60 * 24 * 7,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            IntervalUnit::Hours => "hours",
            IntervalUnit::Days => "days",
            IntervalUnit::Weeks => "weeks",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "days" => IntervalUnit::Days,
            "weeks" => IntervalUnit::Weeks,
            _ => IntervalUnit::Hours,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub flake_path: String,
    pub check_interval: u32,
    #[serde(default)]
    pub check_interval_unit: IntervalUnit,
    /// Unix timestamp of last check (seconds since epoch)
    #[serde(default)]
    pub last_check_timestamp: i64,
    /// Cached updates from last check (JSON array)
    #[serde(default)]
    pub cached_updates_json: String,
    /// Whether to commit and push after updating
    #[serde(default = "default_true")]
    pub commit_and_push: bool,
    /// Whether to run garbage collection after updating
    #[serde(default)]
    pub run_gc_after_update: bool,
    /// Cached download size from last check (e.g. "500.00 MiB")
    #[serde(default)]
    pub cached_download_size: String,
    /// Cached unpacked size from last check (e.g. "1500.00 MiB")
    #[serde(default)]
    pub cached_unpacked_size: String,
}

fn default_true() -> bool { true }

impl Default for Config {
    fn default() -> Self {
        Self {
            flake_path: "/etc/nixos".to_string(),
            check_interval: 1,
            check_interval_unit: IntervalUnit::Hours,
            last_check_timestamp: 0,
            cached_updates_json: String::new(),
            commit_and_push: true,
            run_gc_after_update: false,
            cached_download_size: String::new(),
            cached_unpacked_size: String::new(),
        }
    }
}

impl Config {
    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf, ConfigError> {
        let project_dirs = directories::ProjectDirs::from("", "", "nixos-update-checker")
            .ok_or(ConfigError::NoConfigDir)?;
        Ok(project_dirs.config_dir().join("config.toml"))
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

    /// Save config to the default XDG config path (atomic write via temp file + rename)
    pub fn save(&self) -> Result<(), ConfigError> {
        let path = Self::config_path()?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;

        // Write to temp file in the same directory, then rename for atomicity.
        // rename() on the same filesystem is atomic on POSIX.
        let rand: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(std::process::id() as u64);
        let tmp_path = path.with_extension(format!("toml.{rand}.tmp"));
        fs::write(&tmp_path, &content)?;
        fs::rename(&tmp_path, &path)?;
        Ok(())
    }

    /// Check if the config is valid (has required fields set)
    pub fn is_valid(&self) -> bool {
        !self.flake_path.is_empty() && self.check_interval > 0
    }

    /// Get the flake path as a PathBuf
    pub fn flake_path(&self) -> PathBuf {
        PathBuf::from(&self.flake_path)
    }

    /// Get the check interval in minutes
    pub fn interval_minutes(&self) -> u32 {
        self.check_interval_unit.to_minutes(self.check_interval)
    }

    /// Check if it's time for a new check
    pub fn is_check_due(&self) -> bool {
        if self.last_check_timestamp == 0 {
            // Never checked before
            return true;
        }

        let now = chrono::Utc::now().timestamp();
        let interval_seconds = (self.interval_minutes() as i64) * 60;
        let next_check = self.last_check_timestamp + interval_seconds;

        now >= next_check
    }

    /// Update the last check timestamp to now and save
    pub fn mark_checked(&mut self) -> Result<(), ConfigError> {
        self.last_check_timestamp = chrono::Utc::now().timestamp();
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.check_interval, 1);
        assert_eq!(config.check_interval_unit, IntervalUnit::Hours);
        assert_eq!(config.flake_path, "/etc/nixos");
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            flake_path: "/home/user/nixos".to_string(),
            check_interval: 6,
            commit_and_push: true,
            run_gc_after_update: false,
            check_interval_unit: IntervalUnit::Hours,
            last_check_timestamp: 0,
            cached_updates_json: String::new(),
            cached_download_size: String::new(),
            cached_unpacked_size: String::new(),
        };

        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed.flake_path, config.flake_path);
        assert_eq!(parsed.check_interval, config.check_interval);
        assert_eq!(parsed.check_interval_unit, config.check_interval_unit);
    }

    #[test]
    fn test_interval_minutes() {
        let mut config = Config::default();

        config.check_interval = 2;
        config.check_interval_unit = IntervalUnit::Hours;
        assert_eq!(config.interval_minutes(), 120);

        config.check_interval = 1;
        config.check_interval_unit = IntervalUnit::Days;
        assert_eq!(config.interval_minutes(), 1440);

        config.check_interval = 1;
        config.check_interval_unit = IntervalUnit::Weeks;
        assert_eq!(config.interval_minutes(), 10080);
    }
}
