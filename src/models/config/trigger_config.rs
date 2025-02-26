//! Trigger configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Trigger configurations,
//! allowing triggers to be loaded from JSON files.

use email_address::EmailAddress;
use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path};

use crate::models::{
	config::error::ConfigError, ConfigLoader, Trigger, TriggerType, TriggerTypeConfig,
};

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
	fn load_all<T>(path: Option<&Path>) -> Result<T, Box<ConfigError>>
	where
		T: FromIterator<(String, Self)>,
	{
		let config_dir = path.unwrap_or(Path::new("config/triggers"));
		let entries = fs::read_dir(config_dir).map_err(|e| {
			ConfigError::file_error_with_source("Failed to read triggers directory", e, None)
		})?;

		let mut trigger_pairs = Vec::new();
		for entry in entries {
			let entry = entry.map_err(|e| {
				ConfigError::file_error_with_source("Failed to read triggers directory", e, None)
			})?;
			if Self::is_json_file(&entry.path()) {
				let content = fs::read_to_string(entry.path()).map_err(|e| {
					ConfigError::file_error_with_source("Failed to read trigger file", e, None)
				})?;
				let file_triggers: TriggerConfigFile = serde_json::from_str(&content)
					.map_err(|e| ConfigError::parse_error(e.to_string(), None))?;

				// Validate each trigger before adding it
				for (name, trigger) in file_triggers.triggers {
					if let Err(validation_error) = trigger.validate() {
						return Err(Box::new(ConfigError::validation_error(
							format!(
								"Validation failed for trigger '{}': {}",
								name, validation_error
							),
							None,
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
	fn load_from_path(path: &Path) -> Result<Self, Box<ConfigError>> {
		let file = std::fs::File::open(path)
			.map_err(|e| ConfigError::file_error_with_source("Failed to open file", e, None))?;
		let config: Trigger = serde_json::from_reader(file)
			.map_err(|e| ConfigError::parse_error_with_source("Failed to parse file", e, None))?;

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
	fn validate(&self) -> Result<(), Box<ConfigError>> {
		// Validate trigger name
		if self.name.is_empty() {
			return Err(Box::new(ConfigError::validation_error(
				"Trigger cannot be empty",
				None,
			)));
		}

		match &self.trigger_type {
			TriggerType::Slack => {
				if let TriggerTypeConfig::Slack { slack_url, message } = &self.config {
					// Validate webhook URL
					if !slack_url.starts_with("https://hooks.slack.com/") {
						return Err(Box::new(ConfigError::validation_error(
							"Invalid Slack webhook URL format",
							None,
						)));
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Title cannot be empty",
							None,
						)));
					}
					// Validate template is not empty
					if message.body.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Body cannot be empty",
							None,
						)));
					}
				}
			}
			TriggerType::Email => {
				if let TriggerTypeConfig::Email {
					host,
					port: _,
					username,
					password,
					message,
					sender,
					recipients,
				} = &self.config
				{
					// Validate host
					if host.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Host cannot be empty",
							None,
						)));
					}
					// Validate host format
					if !host.contains('.')
						|| !host
							.chars()
							.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
					{
						return Err(Box::new(ConfigError::validation_error(
							"Invalid SMTP host format",
							None,
						)));
					}

					// Basic username validation
					if username.is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"SMTP username cannot be empty",
							None,
						)));
					}
					if username.chars().any(|c| c.is_control()) {
						return Err(Box::new(ConfigError::validation_error(
							"SMTP username contains invalid control characters",
							None,
						)));
					}
					// Validate password
					if password.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Password cannot be empty",
							None,
						)));
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Title cannot be empty",
							None,
						)));
					}
					if message.body.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Body cannot be empty",
							None,
						)));
					}
					// Validate subject according to RFC 5322
					// Max length of 998 characters, no control chars except whitespace
					if message.title.len() > 998 {
						return Err(Box::new(ConfigError::validation_error(
							"Subject exceeds maximum length of 998 characters",
							None,
						)));
					}
					if message
						.title
						.chars()
						.any(|c| c.is_control() && !c.is_whitespace())
					{
						return Err(Box::new(ConfigError::validation_error(
							"Subject contains invalid control characters",
							None,
						)));
					}
					// Add minimum length check after trim
					if message.title.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Subject must contain at least 1 character",
							None,
						)));
					}

					// Validate email body according to RFC 5322
					// Check for control characters (except CR, LF, and whitespace)
					if message
						.body
						.chars()
						.any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t' | ' '))
					{
						return Err(Box::new(ConfigError::validation_error(
							"Body contains invalid control characters",
							None,
						)));
					}

					// Validate sender
					if !EmailAddress::is_valid(sender.as_str()) {
						return Err(Box::new(ConfigError::validation_error(
							format!("Invalid sender email address: {}", sender),
							None,
						)));
					}

					// Validate recipients
					if recipients.is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Recipients cannot be empty",
							None,
						)));
					}
					for recipient in recipients {
						if !EmailAddress::is_valid(recipient.as_str()) {
							return Err(Box::new(ConfigError::validation_error(
								format!("Invalid recipient email address: {}", recipient),
								None,
							)));
						}
					}
				}
			}
			TriggerType::Webhook => {
				if let TriggerTypeConfig::Webhook {
					url,
					method,
					message,
					..
				} = &self.config
				{
					// Validate URL format
					if !url.starts_with("http://") && !url.starts_with("https://") {
						return Err(Box::new(ConfigError::validation_error(
							"Invalid webhook URL format",
							None,
						)));
					}
					// Validate HTTP method
					if let Some(method) = method {
						match method.to_uppercase().as_str() {
							"GET" | "POST" | "PUT" | "DELETE" => {}
							_ => {
								return Err(Box::new(ConfigError::validation_error(
									"Invalid HTTP method",
									None,
								)))
							}
						}
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Title cannot be empty",
							None,
						)));
					}
					if message.body.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Body cannot be empty",
							None,
						)));
					}
				}
			}
			TriggerType::Telegram => {
				if let TriggerTypeConfig::Telegram {
					token,
					chat_id,
					message,
					..
				} = &self.config
				{
					// Validate token
					// /^[0-9]{8,10}:[a-zA-Z0-9_-]{35}$/ regex
					if token.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Token cannot be empty",
							None,
						)));
					}
					if !regex::Regex::new(r"^[0-9]{8,10}:[a-zA-Z0-9_-]{35}$")
						.unwrap()
						.is_match(token)
					{
						return Err(Box::new(ConfigError::validation_error(
							"Invalid token format",
							None,
						)));
					}

					// Validate chat ID
					if chat_id.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Chat ID cannot be empty",
							None,
						)));
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Title cannot be empty",
							None,
						)));
					}
					if message.body.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Body cannot be empty",
							None,
						)));
					}
				}
			}
			TriggerType::Discord => {
				if let TriggerTypeConfig::Discord {
					discord_url,
					message,
					..
				} = &self.config
				{
					// Validate webhook URL
					if !discord_url.starts_with("https://discord.com/api/webhooks/") {
						return Err(Box::new(ConfigError::validation_error(
							"Invalid Discord webhook URL format",
							None,
						)));
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Title cannot be empty",
							None,
						)));
					}
					if message.body.trim().is_empty() {
						return Err(Box::new(ConfigError::validation_error(
							"Body cannot be empty",
							None,
						)));
					}
				}
			}
			TriggerType::Script => {
				if let TriggerTypeConfig::Script { path, .. } = &self.config {
					// Validate script path exists
					if !Path::new(path).exists() {
						return Err(Box::new(ConfigError::validation_error(
							format!("Script path does not exist: {}", path),
							None,
						)));
					}
				}
			}
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{
		core::{Trigger, TriggerType},
		NotificationMessage,
	};

	#[test]
	fn test_slack_trigger_validation() {
		let valid_trigger = Trigger {
			name: "test_slack".to_string(),
			trigger_type: TriggerType::Slack,
			config: TriggerTypeConfig::Slack {
				slack_url: "https://hooks.slack.com/services/xxx".to_string(),
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};
		assert!(valid_trigger.validate().is_ok());

		// Test invalid webhook URL
		let invalid_webhook = Trigger {
			name: "test_slack".to_string(),
			trigger_type: TriggerType::Slack,
			config: TriggerTypeConfig::Slack {
				slack_url: "https://invalid-url.com".to_string(),
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};
		assert!(invalid_webhook.validate().is_err());

		// Test empty title
		let empty_title = Trigger {
			name: "test_slack".to_string(),
			trigger_type: TriggerType::Slack,
			config: TriggerTypeConfig::Slack {
				slack_url: "https://hooks.slack.com/services/xxx".to_string(),
				message: NotificationMessage {
					title: "".to_string(),
					body: "Test message".to_string(),
				},
			},
		};
		assert!(empty_title.validate().is_err());

		// Test empty body
		let empty_body = Trigger {
			name: "test_slack".to_string(),
			trigger_type: TriggerType::Slack,
			config: TriggerTypeConfig::Slack {
				slack_url: "https://hooks.slack.com/services/xxx".to_string(),
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "".to_string(),
				},
			},
		};
		assert!(empty_body.validate().is_err());
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
				message: NotificationMessage {
					title: "Test Subject".to_string(),
					body: "Test Body".to_string(),
				},
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
				message: NotificationMessage {
					title: "Test Subject".to_string(),
					body: "Test Body".to_string(),
				},
				sender: EmailAddress::new_unchecked("sender@example.com"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(invalid_host.validate().is_err());

		// Test empty host
		let empty_host = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "".to_string(),
				port: Some(587),
				username: "user".to_string(),
				password: "pass".to_string(),
				message: NotificationMessage {
					title: "Test Subject".to_string(),
					body: "Test Body".to_string(),
				},
				sender: EmailAddress::new_unchecked("sender@example.com"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(empty_host.validate().is_err());

		// Test invalid email address
		let invalid_email = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "smtp.example.com".to_string(),
				port: Some(587),
				username: "user".to_string(),
				password: "pass".to_string(),
				message: NotificationMessage {
					title: "Test Subject".to_string(),
					body: "Test Body".to_string(),
				},
				sender: EmailAddress::new_unchecked("invalid-email"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(invalid_email.validate().is_err());

		let invalid_trigger = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "smtp.example.com".to_string(),
				port: Some(587),
				username: "user".to_string(),
				password: "".to_string(), // Invalid password
				message: NotificationMessage {
					title: "Test Subject".to_string(),
					body: "Test Body".to_string(),
				},
				sender: EmailAddress::new_unchecked("sender@example.com"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(invalid_trigger.validate().is_err());

		let invalid_trigger = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "smtp.example.com".to_string(),
				port: Some(587),
				username: "user".to_string(),
				password: "pass".to_string(),
				message: NotificationMessage {
					title: "A".repeat(999).to_string(), // Exceeds max length
					body: "Test Body".to_string(),
				},
				sender: EmailAddress::new_unchecked("sender@example.com"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(invalid_trigger.validate().is_err());

		// Test empty title
		let empty_title = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "smtp.example.com".to_string(),
				port: Some(587),
				username: "user".to_string(),
				password: "pass".to_string(),
				message: NotificationMessage {
					title: "".to_string(),
					body: "Test Body".to_string(),
				},
				sender: EmailAddress::new_unchecked("sender@example.com"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(empty_title.validate().is_err());

		// Test empty body
		let empty_body = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "smtp.example.com".to_string(),
				port: Some(587),
				username: "user".to_string(),
				password: "pass".to_string(),
				message: NotificationMessage {
					title: "Test Subject".to_string(),
					body: "".to_string(),
				},
				sender: EmailAddress::new_unchecked("sender@example.com"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};
		assert!(empty_body.validate().is_err());
	}

	#[test]
	fn test_webhook_trigger_validation() {
		let valid_trigger = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "https://api.example.com/webhook".to_string(),
				secret: None,
				method: Some("POST".to_string()),
				headers: None,
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};
		assert!(valid_trigger.validate().is_ok());

		// Test invalid URL
		let invalid_url = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "invalid-url".to_string(),
				method: Some("POST".to_string()),
				headers: None,
				secret: None,
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};
		assert!(invalid_url.validate().is_err());

		// Test invalid method
		let invalid_method = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "https://api.example.com/webhook".to_string(),
				method: Some("INVALID".to_string()),
				headers: None,
				secret: None,
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};
		assert!(invalid_method.validate().is_err());

		// Test invalid message
		let invalid_title_message = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "https://api.example.com/webhook".to_string(),
				method: Some("POST".to_string()),
				headers: None,
				secret: None,
				message: NotificationMessage {
					title: "".to_string(),
					body: "Test message".to_string(),
				},
			},
		};
		assert!(invalid_title_message.validate().is_err());

		let invalid_body_message = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "https://api.example.com/webhook".to_string(),
				method: Some("POST".to_string()),
				headers: None,
				secret: None,
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "".to_string(),
				},
			},
		};
		assert!(invalid_body_message.validate().is_err());
	}

	#[test]
	fn test_discord_trigger_validation() {
		let valid_trigger = Trigger {
			name: "test_discord".to_string(),
			trigger_type: TriggerType::Discord,
			config: TriggerTypeConfig::Discord {
				discord_url: "https://discord.com/api/webhooks/xxx".to_string(),
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};
		assert!(valid_trigger.validate().is_ok());

		// Test invalid webhook URL
		let invalid_webhook = Trigger {
			name: "test_discord".to_string(),
			trigger_type: TriggerType::Discord,
			config: TriggerTypeConfig::Discord {
				discord_url: "https://invalid-url.com".to_string(),
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};
		assert!(invalid_webhook.validate().is_err());

		// Test invalid message
		let invalid_title_message = Trigger {
			name: "test_discord".to_string(),
			trigger_type: TriggerType::Discord,
			config: TriggerTypeConfig::Discord {
				discord_url: "https://discord.com/api/webhooks/123".to_string(),
				message: NotificationMessage {
					title: "".to_string(),
					body: "test".to_string(),
				},
			},
		};
		assert!(invalid_title_message.validate().is_err());

		let invalid_body_message = Trigger {
			name: "test_discord".to_string(),
			trigger_type: TriggerType::Discord,
			config: TriggerTypeConfig::Discord {
				discord_url: "https://discord.com/api/webhooks/123".to_string(),
				message: NotificationMessage {
					title: "test".to_string(),
					body: "".to_string(),
				},
			},
		};
		assert!(invalid_body_message.validate().is_err());
	}

	#[test]
	fn test_telegram_trigger_validation() {
		let valid_trigger = Trigger {
			name: "test_telegram".to_string(),
			trigger_type: TriggerType::Telegram,
			config: TriggerTypeConfig::Telegram {
				token: "1234567890:ABCdefGHIjklMNOpqrSTUvwxYZ123456789".to_string(), // noboost
				chat_id: "1730223038".to_string(),
				disable_web_preview: Some(true),
				message: NotificationMessage {
					title: "Test Subject".to_string(),
					body: "Test Body".to_string(),
				},
			},
		};
		assert!(valid_trigger.validate().is_ok());

		// Test invalid token
		let invalid_token = Trigger {
			name: "test_telegram".to_string(),
			trigger_type: TriggerType::Telegram,
			config: TriggerTypeConfig::Telegram {
				token: "invalid-token".to_string(),
				chat_id: "1730223038".to_string(),
				disable_web_preview: Some(true),
				message: NotificationMessage {
					title: "Test Subject".to_string(),
					body: "Test Body".to_string(),
				},
			},
		};
		assert!(invalid_token.validate().is_err());

		// Test invalid chat ID
		let invalid_chat_id = Trigger {
			name: "test_telegram".to_string(),
			trigger_type: TriggerType::Telegram,
			config: TriggerTypeConfig::Telegram {
				token: "1234567890:ABCdefGHIjklMNOpqrSTUvwxYZ123456789".to_string(), // noboost
				chat_id: "".to_string(),
				disable_web_preview: Some(true),
				message: NotificationMessage {
					title: "Test Subject".to_string(),
					body: "Test Body".to_string(),
				},
			},
		};
		assert!(invalid_chat_id.validate().is_err());

		// Test invalid message
		let invalid_title_message = Trigger {
			name: "test_telegram".to_string(),
			trigger_type: TriggerType::Telegram,
			config: TriggerTypeConfig::Telegram {
				token: "1234567890:ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(), // noboost
				chat_id: "1730223038".to_string(),
				disable_web_preview: Some(true),
				message: NotificationMessage {
					title: "".to_string(),
					body: "test".to_string(),
				},
			},
		};
		assert!(invalid_title_message.validate().is_err());

		let invalid_body_message = Trigger {
			name: "test_telegram".to_string(),
			trigger_type: TriggerType::Telegram,
			config: TriggerTypeConfig::Telegram {
				token: "1234567890:ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_string(), // noboost
				chat_id: "1730223038".to_string(),
				disable_web_preview: Some(true),
				message: NotificationMessage {
					title: "test".to_string(),
					body: "".to_string(),
				},
			},
		};
		assert!(invalid_body_message.validate().is_err());
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
