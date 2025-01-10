//! Monitor configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Monitor configurations,
//! allowing monitors to be loaded from JSON files.

use std::fs;
use std::path::Path;

use crate::models::config::error::ConfigError;
use crate::models::{ConfigLoader, Monitor};

impl ConfigLoader for Monitor {
    /// Load all monitor configurations from a directory
    ///
    /// Reads and parses all JSON files in the specified directory (or default
    /// config directory) as monitor configurations.
    fn load_all<T>(path: Option<&Path>) -> Result<T, ConfigError>
    where
        T: FromIterator<(String, Self)>,
    {
        let monitor_dir = path.unwrap_or(Path::new("config/monitors"));
        let mut pairs = Vec::new();

        if !monitor_dir.exists() {
            return Err(ConfigError::file_error("monitors directory not found"));
        }

        for entry in fs::read_dir(monitor_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !Self::is_json_file(&path) {
                continue;
            }

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            if let Ok(monitor) = Self::load_from_path(&path) {
                pairs.push((name, monitor));
            }
        }

        Ok(T::from_iter(pairs))
    }

    /// Load a monitor configuration from a specific file
    ///
    /// Reads and parses a single JSON file as a monitor configuration.
    fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        let file = std::fs::File::open(path)?;
        let config: Monitor = serde_json::from_reader(file)?;

        // Validate the config after loading
        if let Err(validation_error) = config.validate() {
            return Err(ConfigError::validation_error(validation_error.to_string()));
        }

        Ok(config)
    }

    /// Validate the monitor configuration
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate function signatures
        for func in &self.match_conditions.functions {
            if !func.signature.contains('(') || !func.signature.contains(')') {
                return Err(ConfigError::validation_error(format!(
                    "Invalid function signature format: {}",
                    func.signature
                )));
            }
        }

        // Validate event signatures
        for event in &self.match_conditions.events {
            if !event.signature.contains('(') || !event.signature.contains(')') {
                return Err(ConfigError::validation_error(format!(
                    "Invalid event signature format: {}",
                    event.signature
                )));
            }
        }

        Ok(())
    }
}
