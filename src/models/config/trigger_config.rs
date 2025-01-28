//! Trigger configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Trigger configurations,
//! allowing triggers to be loaded from JSON files.

use email_address::EmailAddress;
use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path};

use crate::models::{config::error::ConfigError, ConfigLoader, Trigger, TriggerTypeConfig};

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
		T: FromIterator<(String, Self)>,
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

				// Validate each trigger before adding it
				for (name, trigger) in file_triggers.triggers {
					if let Err(validation_error) = trigger.validate() {
						return Err(ConfigError::validation_error(format!(
							"Validation failed for trigger '{}': {}",
							name, validation_error
						)));
					}
					trigger_pairs.push((name, trigger));
				}
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
		config.validate()?;

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
		// Validate trigger name
		if self.name.is_empty() {
			return Err(ConfigError::validation_error("Trigger cannot be empty"));
		}

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
			}
			TriggerTypeConfig::Email {
				host,
				port: _,
				username,
				password,
				subject,
				body,
				sender,
				recipients,
			} => {
				// Validate host
				if host.trim().is_empty() {
					return Err(ConfigError::validation_error("Host cannot be empty"));
				}
				// Validate host format
				if !host.contains('.')
					|| !host
						.chars()
						.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
				{
					return Err(ConfigError::validation_error("Invalid SMTP host format"));
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
				// Add minimum length check after trim
				if subject.trim().is_empty() {
					return Err(ConfigError::validation_error(
						"Subject must contain at least 1 character",
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
				if recipients.is_empty() {
					return Err(ConfigError::validation_error("Recipients cannot be empty"));
				}
				for recipient in recipients {
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
			}
			TriggerTypeConfig::Script { path, .. } => {
				// Validate script path exists
				if !Path::new(path).exists() {
					return Err(ConfigError::validation_error(format!(
						"Script path does not exist: {}",
						path
					)));
				}
			}
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::core::{Trigger, TriggerType};

	#[test]
	fn test_slack_trigger_validation() {
		let valid_trigger = Trigger {
			name: "test_slack".to_string(),
			trigger_type: TriggerType::Slack,
			config: TriggerTypeConfig::Slack {
				webhook_url: "https://hooks.slack.com/services/xxx".to_string(),
				title: "Alert".to_string(),
				body: "Test message".to_string(),
			},
		};
		assert!(valid_trigger.validate().is_ok());

		// Test invalid webhook URL
		let invalid_webhook = Trigger {
			name: "test_slack".to_string(),
			trigger_type: TriggerType::Slack,
			config: TriggerTypeConfig::Slack {
				webhook_url: "https://invalid-url.com".to_string(),
				title: "Alert".to_string(),
				body: "Test message".to_string(),
			},
		};
		assert!(invalid_webhook.validate().is_err());

		// Test empty title
		let empty_title = Trigger {
			name: "test_slack".to_string(),
			trigger_type: TriggerType::Slack,
			config: TriggerTypeConfig::Slack {
				webhook_url: "https://hooks.slack.com/services/xxx".to_string(),
				title: "".to_string(),
				body: "Test message".to_string(),
			},
		};
		assert!(empty_title.validate().is_err());
	}

	#[test]
	fn test_email_trigger_validation() {
		let valid_trigger = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "smtp.example.com".to_string(),
				port: Some(587),
				username: "user".to_string(),
				password: "pass".to_string(),
				subject: "Test Subject".to_string(),
				body: "Test Body".to_string(),
				sender: EmailAddress::new_unchecked("sender@example.com"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(valid_trigger.validate().is_ok());

		// Test invalid host
		let invalid_host = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "invalid@host".to_string(),
				port: Some(587),
				username: "user".to_string(),
				password: "pass".to_string(),
				subject: "Test Subject".to_string(),
				body: "Test Body".to_string(),
				sender: EmailAddress::new_unchecked("sender@example.com"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(invalid_host.validate().is_err());

		// Test invalid email address
		let invalid_email = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "smtp.example.com".to_string(),
				port: Some(587),
				username: "user".to_string(),
				password: "pass".to_string(),
				subject: "Test Subject".to_string(),
				body: "Test Body".to_string(),
				sender: EmailAddress::new_unchecked("invalid-email"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(invalid_email.validate().is_err());
	}

	#[test]
	fn test_webhook_trigger_validation() {
		let valid_trigger = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "https://api.example.com/webhook".to_string(),
				method: "POST".to_string(),
				headers: None,
			},
		};
		assert!(valid_trigger.validate().is_ok());

		// Test invalid URL
		let invalid_url = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "invalid-url".to_string(),
				method: "POST".to_string(),
				headers: None,
			},
		};
		assert!(invalid_url.validate().is_err());

		// Test invalid method
		let invalid_method = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "https://api.example.com/webhook".to_string(),
				method: "INVALID".to_string(),
				headers: None,
			},
		};
		assert!(invalid_method.validate().is_err());
	}

	#[test]
	fn test_script_trigger_validation() {
		let temp_dir = std::env::temp_dir();
		let script_path = temp_dir.join("test_script.sh");
		std::fs::write(&script_path, "#!/bin/bash\necho 'test'").unwrap();

		let valid_trigger = Trigger {
			name: "test_script".to_string(),
			trigger_type: TriggerType::Script,
			config: TriggerTypeConfig::Script {
				path: script_path.to_str().unwrap().to_string(),
				args: vec!["arg1".to_string(), "arg2".to_string()],
			},
		};
		assert!(valid_trigger.validate().is_ok());

		// Test non-existent script
		let invalid_path = Trigger {
			name: "test_script".to_string(),
			trigger_type: TriggerType::Script,
			config: TriggerTypeConfig::Script {
				path: "/non/existent/path".to_string(),
				args: vec!["arg1".to_string(), "arg2".to_string()],
			},
		};
		assert!(invalid_path.validate().is_err());

		std::fs::remove_file(script_path).unwrap();
	}
}
