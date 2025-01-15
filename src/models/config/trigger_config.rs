//! Trigger configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Trigger configurations,
//! allowing triggers to be loaded from JSON files.

use email_address::EmailAddress;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::models::config::error::ConfigError;
use crate::models::{ConfigLoader, Trigger, TriggerTypeConfig};

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
                let content = fs::read_to_string(entry.path())?;
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
            return Err(ConfigError::validation_error(validation_error.to_string()));
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
    fn validate(&self) -> Result<(), ConfigError> {
        match &self.config {
            TriggerTypeConfig::Slack {
                webhook_url,
                title,
                body,
            } => {
                // Validate webhook URL
                if !webhook_url.starts_with("https://hooks.slack.com/") {
                    return Err(ConfigError::validation_error(
                        "Invalid Slack webhook URL format",
                    ));
                }
                // Validate channel format
                if title.trim().is_empty() {
                    return Err(ConfigError::validation_error("Title cannot be empty"));
                }
                // Validate template is not empty
                if body.trim().is_empty() {
                    return Err(ConfigError::validation_error("Body cannot be empty"));
                }
                // Name validation moved outside since it's part of TriggerConfig
                if self.name.trim().is_empty() {
                    return Err(ConfigError::validation_error("Name cannot be empty"));
                }
            }
            TriggerTypeConfig::Email {
                host,
                port,
                username,
                password,
                subject,
                body,
                sender,
                receipients,
            } => {
                // Validate host
                if host.trim().is_empty() {
                    return Err(ConfigError::validation_error("Host cannot be empty"));
                }
                // Validate port
                if port.is_none() {
                    return Err(ConfigError::validation_error("Port cannot be empty"));
                }
                // Basic username validation
                if username.is_empty() {
                    return Err(ConfigError::validation_error(
                        "SMTP username cannot be empty",
                    ));
                }
                if username.chars().any(|c| c.is_control()) {
                    return Err(ConfigError::validation_error(
                        "SMTP username contains invalid control characters",
                    ));
                }
                // Validate password
                if password.trim().is_empty() {
                    return Err(ConfigError::validation_error("Password cannot be empty"));
                }
                // Validate subject
                if subject.trim().is_empty() {
                    return Err(ConfigError::validation_error("Subject cannot be empty"));
                }
                // Validate subject according to RFC 5322
                // Max length of 998 characters, no control chars except whitespace
                if subject.len() > 998 {
                    return Err(ConfigError::validation_error(
                        "Subject exceeds maximum length of 998 characters",
                    ));
                }
                if subject
                    .chars()
                    .any(|c| c.is_control() && !c.is_whitespace())
                {
                    return Err(ConfigError::validation_error(
                        "Subject contains invalid control characters",
                    ));
                }

                // Validate email body according to RFC 5322
                // Check for control characters (except CR, LF, and whitespace)
                if body
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t' | ' '))
                {
                    return Err(ConfigError::validation_error(
                        "Body contains invalid control characters",
                    ));
                }

                // Validate sender
                if !EmailAddress::is_valid(sender.as_str()) {
                    return Err(ConfigError::validation_error(format!(
                        "Invalid sender email address: {}",
                        sender
                    )));
                }

                // Validate recipients
                if receipients.is_empty() {
                    return Err(ConfigError::validation_error("Recipients cannot be empty"));
                }
                for recipient in receipients {
                    if !EmailAddress::is_valid(recipient.as_str()) {
                        return Err(ConfigError::validation_error(format!(
                            "Invalid recipient email address: {}",
                            recipient
                        )));
                    }
                }
            }
            TriggerTypeConfig::Webhook { url, method, .. } => {
                // Validate URL format
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    return Err(ConfigError::validation_error("Invalid webhook URL format"));
                }
                // Validate HTTP method
                match method.to_uppercase().as_str() {
                    "GET" | "POST" | "PUT" | "DELETE" => {}
                    _ => return Err(ConfigError::validation_error("Invalid HTTP method")),
                }
                if self.name.trim().is_empty() {
                    return Err(ConfigError::validation_error("Name cannot be empty"));
                }
            }
            TriggerTypeConfig::Script { path, .. } => {
                // Validate script path exists
                if !Path::new(path).exists() {
                    return Err(ConfigError::validation_error(format!(
                        "Script path does not exist: {}",
                        path
                    )));
                }
                if self.name.trim().is_empty() {
                    return Err(ConfigError::validation_error("Name cannot be empty"));
                }
            }
        }
        Ok(())
    }
}
