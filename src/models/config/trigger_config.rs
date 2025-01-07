//! Trigger configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Trigger configurations,
//! allowing triggers to be loaded from JSON files.

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::models::{ConfigLoader, Trigger, TriggerTypeConfig};

use super::error::ConfigError;

/// File structure for trigger configuration files
#[derive(Debug, Deserialize)]
pub struct TriggerConfigFile {
    /// Map of trigger names to their configurations
    #[serde(flatten)]
    pub triggers: HashMap<String, Trigger>,
}

impl ConfigLoader for Trigger {
    /// Load all trigger configurations from a directory
    ///
    /// Reads and parses all JSON files in the specified directory (or default
    /// config directory) as trigger configurations.
    fn load_all<T>(path: Option<&Path>) -> Result<T, ConfigError>
    where
        T: FromIterator<(String, Trigger)>,
    {
        let config_dir = path.unwrap_or(Path::new("config/triggers"));
        let entries = fs::read_dir(config_dir)?;

        let mut trigger_pairs = Vec::new();
        for entry in entries {
            let entry = entry?;
            if Self::is_json_file(&entry.path()) {
                let content = fs::read_to_string(&entry.path())?;
                let file_triggers: TriggerConfigFile = serde_json::from_str(&content)
                    .map_err(|e| ConfigError::parse_error(e.to_string()))?;
                trigger_pairs.extend(file_triggers.triggers.into_iter());
            }
        }
        Ok(T::from_iter(trigger_pairs))
    }

    /// Load a trigger configuration from a specific file
    ///
    /// Reads and parses a single JSON file as a trigger configuration.
    fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        let file = std::fs::File::open(path)?;
        let config: Trigger = serde_json::from_reader(file)?;

        // Validate the config after loading
        if let Err(validation_error) = config.validate() {
            return Err(ConfigError::validation_error(validation_error));
        }

        Ok(config)
    }

    /// Validate the trigger configuration
    ///
    /// Ensures that:
    /// - The trigger has a valid name
    /// - The trigger type is supported
    /// - Required configuration fields for the trigger type are present
    /// - URLs are valid for webhook and Slack triggers
    /// - Script paths exist for script triggers
    fn validate(&self) -> Result<(), String> {
        match &self.config {
            TriggerTypeConfig::Slack {
                webhook_url,
                title,
                body,
            } => {
                // Validate webhook URL
                if !webhook_url.starts_with("https://hooks.slack.com/") {
                    return Err("Invalid Slack webhook URL format".to_string());
                }
                // Validate channel format
                if title.trim().is_empty() {
                    return Err("Title cannot be empty".to_string());
                }
                // Validate template is not empty
                if body.trim().is_empty() {
                    return Err("Body cannot be empty".to_string());
                }
                // Name validation moved outside since it's part of TriggerConfig
                if self.name.trim().is_empty() {
                    return Err("Name cannot be empty".to_string());
                }
            }
            TriggerTypeConfig::Webhook { url, method, .. } => {
                // Validate URL format
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err("Invalid webhook URL format".to_string());
                }
                // Validate HTTP method
                match method.to_uppercase().as_str() {
                    "GET" | "POST" | "PUT" | "DELETE" => {}
                    _ => return Err("Invalid HTTP method".to_string()),
                }
                if self.name.trim().is_empty() {
                    return Err("Name cannot be empty".to_string());
                }
            }
            TriggerTypeConfig::Script { path, .. } => {
                // Validate script path exists
                if !Path::new(path).exists() {
                    return Err(format!("Script path does not exist: {}", path));
                }
                if self.name.trim().is_empty() {
                    return Err("Name cannot be empty".to_string());
                }
            }
        }
        Ok(())
    }
}
