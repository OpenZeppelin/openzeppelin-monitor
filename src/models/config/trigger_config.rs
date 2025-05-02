//! Trigger configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Trigger configurations,
//! allowing triggers to be loaded from JSON files.

use email_address::EmailAddress;
use serde::Deserialize;
use std::{collections::HashMap, fs, path::Path};

use crate::{
	models::{config::error::ConfigError, ConfigLoader, Trigger, TriggerType, TriggerTypeConfig},
	services::trigger::validate_script_config,
};

const TELEGRAM_MAX_BODY_LENGTH: usize = 4096;
const DISCORD_MAX_BODY_LENGTH: usize = 2000;

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

		if !config_dir.exists() {
			return Err(ConfigError::file_error(
				"triggers directory not found",
				None,
				Some(HashMap::from([(
					"path".to_string(),
					config_dir.display().to_string(),
				)])),
			));
		}

		let entries = fs::read_dir(config_dir).map_err(|e| {
			ConfigError::file_error(
				format!("failed to read triggers directory: {}", e),
				Some(Box::new(e)),
				Some(HashMap::from([(
					"path".to_string(),
					config_dir.display().to_string(),
				)])),
			)
		})?;

		let mut trigger_pairs = Vec::new();
		for entry in entries {
			let entry = entry.map_err(|e| {
				ConfigError::file_error(
					format!("failed to read directory entry: {}", e),
					Some(Box::new(e)),
					Some(HashMap::from([(
						"path".to_string(),
						config_dir.display().to_string(),
					)])),
				)
			})?;
			if Self::is_json_file(&entry.path()) {
				let file_path = entry.path();
				let content = fs::read_to_string(&file_path).map_err(|e| {
					ConfigError::file_error(
						format!("failed to read trigger config file: {}", e),
						Some(Box::new(e)),
						Some(HashMap::from([(
							"path".to_string(),
							file_path.display().to_string(),
						)])),
					)
				})?;
				let file_triggers: TriggerConfigFile =
					serde_json::from_str(&content).map_err(|e| {
						ConfigError::parse_error(
							format!("failed to parse trigger config: {}", e),
							Some(Box::new(e)),
							Some(HashMap::from([(
								"path".to_string(),
								file_path.display().to_string(),
							)])),
						)
					})?;

				// Validate each trigger before adding it
				for (name, trigger) in file_triggers.triggers {
					if let Err(validation_error) = trigger.validate() {
						return Err(ConfigError::validation_error(
							format!(
								"Validation failed for trigger '{}': {}",
								name, validation_error
							),
							Some(Box::new(validation_error)),
							Some(HashMap::from([
								("path".to_string(), file_path.display().to_string()),
								("trigger_name".to_string(), name.clone()),
							])),
						));
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
		let file = std::fs::File::open(path)
			.map_err(|e| ConfigError::file_error(e.to_string(), None, None))?;
		let config: Trigger = serde_json::from_reader(file)
			.map_err(|e| ConfigError::parse_error(e.to_string(), None, None))?;

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
			return Err(ConfigError::validation_error(
				"Trigger cannot be empty",
				None,
				None,
			));
		}

		match &self.trigger_type {
			TriggerType::Slack => {
				if let TriggerTypeConfig::Slack { slack_url, message } = &self.config {
					// Validate webhook URL
					if !slack_url.starts_with("https://hooks.slack.com/") {
						return Err(ConfigError::validation_error(
							"Invalid Slack webhook URL format",
							None,
							None,
						));
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Title cannot be empty",
							None,
							None,
						));
					}
					// Validate template is not empty
					if message.body.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Body cannot be empty",
							None,
							None,
						));
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
						return Err(ConfigError::validation_error(
							"Host cannot be empty",
							None,
							None,
						));
					}
					// Validate host format
					if !host.contains('.')
						|| !host
							.chars()
							.all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
					{
						return Err(ConfigError::validation_error(
							"Invalid SMTP host format",
							None,
							None,
						));
					}

					// Basic username validation
					if username.is_empty() {
						return Err(ConfigError::validation_error(
							"SMTP username cannot be empty",
							None,
							None,
						));
					}
					if username.chars().any(|c| c.is_control()) {
						return Err(ConfigError::validation_error(
							"SMTP username contains invalid control characters",
							None,
							None,
						));
					}
					// Validate password
					if password.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Password cannot be empty",
							None,
							None,
						));
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Title cannot be empty",
							None,
							None,
						));
					}
					if message.body.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Body cannot be empty",
							None,
							None,
						));
					}
					// Validate subject according to RFC 5322
					// Max length of 998 characters, no control chars except whitespace
					if message.title.len() > 998 {
						return Err(ConfigError::validation_error(
							"Subject exceeds maximum length of 998 characters",
							None,
							None,
						));
					}
					if message
						.title
						.chars()
						.any(|c| c.is_control() && !c.is_whitespace())
					{
						return Err(ConfigError::validation_error(
							"Subject contains invalid control characters",
							None,
							None,
						));
					}
					// Add minimum length check after trim
					if message.title.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Subject must contain at least 1 character",
							None,
							None,
						));
					}

					// Validate email body according to RFC 5322
					// Check for control characters (except CR, LF, and whitespace)
					if message
						.body
						.chars()
						.any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t' | ' '))
					{
						return Err(ConfigError::validation_error(
							"Body contains invalid control characters",
							None,
							None,
						));
					}

					// Validate sender
					if !EmailAddress::is_valid(sender.as_str()) {
						return Err(ConfigError::validation_error(
							format!("Invalid sender email address: {}", sender),
							None,
							None,
						));
					}

					// Validate recipients
					if recipients.is_empty() {
						return Err(ConfigError::validation_error(
							"Recipients cannot be empty",
							None,
							None,
						));
					}
					for recipient in recipients {
						if !EmailAddress::is_valid(recipient.as_str()) {
							return Err(ConfigError::validation_error(
								format!("Invalid recipient email address: {}", recipient),
								None,
								None,
							));
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
						return Err(ConfigError::validation_error(
							"Invalid webhook URL format",
							None,
							None,
						));
					}
					// Validate HTTP method
					if let Some(method) = method {
						match method.to_uppercase().as_str() {
							"GET" | "POST" | "PUT" | "DELETE" => {}
							_ => {
								return Err(ConfigError::validation_error(
									"Invalid HTTP method",
									None,
									None,
								));
							}
						}
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Title cannot be empty",
							None,
							None,
						));
					}
					if message.body.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Body cannot be empty",
							None,
							None,
						));
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
						return Err(ConfigError::validation_error(
							"Token cannot be empty",
							None,
							None,
						));
					}

					// Safely compile and use the regex
					match regex::Regex::new(r"^[0-9]{8,10}:[a-zA-Z0-9_-]{35}$") {
						Ok(re) => {
							if !re.is_match(token) {
								return Err(ConfigError::validation_error(
									"Invalid token format",
									None,
									None,
								));
							}
						}
						Err(e) => {
							return Err(ConfigError::validation_error(
								format!("Failed to validate token format: {}", e),
								None,
								None,
							));
						}
					}

					// Validate chat ID
					if chat_id.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Chat ID cannot be empty",
							None,
							None,
						));
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Title cannot be empty",
							None,
							None,
						));
					}
					if message.body.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Body cannot be empty",
							None,
							None,
						));
					}
					// Validate template max length
					if message.body.len() > TELEGRAM_MAX_BODY_LENGTH {
						return Err(ConfigError::validation_error(
							format!(
								"Message body should not exceed {} characters",
								TELEGRAM_MAX_BODY_LENGTH
							),
							None,
							None,
						));
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
						return Err(ConfigError::validation_error(
							"Invalid Discord webhook URL format",
							None,
							None,
						));
					}
					// Validate message
					if message.title.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Title cannot be empty",
							None,
							None,
						));
					}
					if message.body.trim().is_empty() {
						return Err(ConfigError::validation_error(
							"Body cannot be empty",
							None,
							None,
						));
					}
					// Validate template max length
					if message.body.len() > DISCORD_MAX_BODY_LENGTH {
						return Err(ConfigError::validation_error(
							format!(
								"Message body should not exceed {} characters",
								DISCORD_MAX_BODY_LENGTH
							),
							None,
							None,
						));
					}
				}
			}
			TriggerType::Script => {
				if let TriggerTypeConfig::Script {
					script_path,
					language,
					timeout_ms,
					..
				} = &self.config
				{
					validate_script_config(script_path, language, timeout_ms)?;
				}
			}
		}

		// Log a warning if the trigger uses an insecure protocol
		self.validate_protocol();

		Ok(())
	}

	/// Validate the safety of the protocols used in the trigger
	///
	/// Returns if safe, or logs a warning message if unsafe.
	fn validate_protocol(&self) {
		match &self.config {
			TriggerTypeConfig::Slack { slack_url, .. } => {
				if !slack_url.starts_with("https://") {
					tracing::warn!("Slack URL uses an insecure protocol: {}", slack_url);
				}
			}
			TriggerTypeConfig::Discord { discord_url, .. } => {
				if !discord_url.starts_with("https://") {
					tracing::warn!("Discord URL uses an insecure protocol: {}", discord_url);
				}
			}
			TriggerTypeConfig::Telegram { .. } => {}
			TriggerTypeConfig::Script { script_path, .. } => {
				// Check script file permissions on Unix systems
				#[cfg(unix)]
				{
					use std::os::unix::fs::PermissionsExt;
					if let Ok(metadata) = std::fs::metadata(script_path) {
						let permissions = metadata.permissions();
						let mode = permissions.mode();
						if mode & 0o022 != 0 {
							tracing::warn!(
								"Script file has overly permissive write permissions: {}.The recommended permissions are `644` (`rw-r--r--`)",
								script_path
							);
						}
					}
				}
			}
			TriggerTypeConfig::Email { port, .. } => {
				let secure_ports = [993, 587, 465];
				if let Some(port) = port {
					if !secure_ports.contains(port) {
						tracing::warn!("Email port is not using a secure protocol: {}", port);
					}
				}
			}
			TriggerTypeConfig::Webhook { url, headers, .. } => {
				if !url.starts_with("https://") {
					tracing::warn!("Webhook URL uses an insecure protocol: {}", url);
				}
				// Check for security headers
				match headers {
					Some(headers) => {
						if !headers.contains_key("X-API-Key")
							&& !headers.contains_key("Authorization")
						{
							tracing::warn!("Webhook lacks authentication headers");
						}
					}
					None => {
						tracing::warn!("Webhook lacks authentication headers");
					}
				}
			}
		};
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{core::Trigger, NotificationMessage, ScriptLanguage};
	use crate::utils::tests::builders::trigger::TriggerBuilder;
	use std::{fs::File, io::Write, os::unix::fs::PermissionsExt};
	use tempfile::TempDir;
	use tracing_test::traced_test;

	#[test]
	fn test_slack_trigger_validation() {
		// Valid trigger
		let valid_trigger = TriggerBuilder::new()
			.name("test_slack")
			.slack("https://hooks.slack.com/services/xxx")
			.message("Alert", "Test message")
			.build();
		assert!(valid_trigger.validate().is_ok());

		// Invalid webhook URL
		let invalid_webhook = TriggerBuilder::new()
			.name("test_slack")
			.slack("https://invalid-url.com")
			.build();
		assert!(invalid_webhook.validate().is_err());

		// Empty title
		let empty_title = TriggerBuilder::new()
			.name("test_slack")
			.slack("https://hooks.slack.com/services/xxx")
			.message("", "Test message")
			.build();
		assert!(empty_title.validate().is_err());

		// Empty body
		let empty_body = TriggerBuilder::new()
			.name("test_slack")
			.slack("https://hooks.slack.com/services/xxx")
			.message("Alert", "")
			.build();
		assert!(empty_body.validate().is_err());
	}

	#[test]
	fn test_email_trigger_validation() {
		// Valid trigger
		let valid_trigger = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"user",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.build();
		assert!(valid_trigger.validate().is_ok());

		// Test invalid host
		let invalid_host = TriggerBuilder::new()
			.name("test_email")
			.email(
				"invalid@host",
				"user",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.build();
		assert!(invalid_host.validate().is_err());

		// Test empty host
		let empty_host = TriggerBuilder::new()
			.name("test_email")
			.email(
				"",
				"user",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.build();
		assert!(empty_host.validate().is_err());

		// Test invalid email address
		let invalid_email = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"user",
				"pass",
				"invalid-email",
				vec!["recipient@example.com"],
			)
			.build();
		assert!(invalid_email.validate().is_err());

		// Test empty password
		let invalid_password = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"user",
				"", // Invalid password
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.build();
		assert!(invalid_password.validate().is_err());

		// Test subject too long
		let invalid_subject = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"user",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.message(&"A".repeat(999), "Test Body")  // Exceeds max length
			.build();
		assert!(invalid_subject.validate().is_err());

		// Test empty username
		let empty_username = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.build();
		assert!(empty_username.validate().is_err());

		// Test invalid control characters in username
		let invalid_control_chars = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"\0",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.build();
		assert!(invalid_control_chars.validate().is_err());

		// Test invalid recipient
		let invalid_recipient = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"user",
				"pass",
				"sender@example.com",
				vec!["invalid-email"],
			)
			.build();
		assert!(invalid_recipient.validate().is_err());

		// Test empty body
		let empty_body = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"user",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.message("Test Subject", "")
			.build();
		assert!(empty_body.validate().is_err());

		// Test control characters in subject
		let control_chars_subject = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"user",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.message("Test \0 Subject", "Test Body")
			.build();
		assert!(control_chars_subject.validate().is_err());

		// Test control characters in body
		let control_chars_body = TriggerBuilder::new()
			.name("test_email")
			.email(
				"smtp.example.com",
				"user",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.message("Test Subject", "Test \0 Body")
			.build();
		assert!(control_chars_body.validate().is_err());
	}

	#[test]
	fn test_webhook_trigger_validation() {
		// Valid trigger
		let valid_trigger = TriggerBuilder::new()
			.name("test_webhook")
			.webhook("https://api.example.com/webhook")
			.message("Alert", "Test message")
			.build();
		assert!(valid_trigger.validate().is_ok());

		// Invalid URL
		let invalid_url = TriggerBuilder::new()
			.name("test_webhook")
			.webhook("invalid-url")
			.build();
		assert!(invalid_url.validate().is_err());

		// Empty title
		let invalid_title = TriggerBuilder::new()
			.name("test_webhook")
			.webhook("https://api.example.com/webhook")
			.message("", "Test message")
			.build();
		assert!(invalid_title.validate().is_err());

		// Empty body
		let invalid_body = TriggerBuilder::new()
			.name("test_webhook")
			.webhook("https://api.example.com/webhook")
			.message("Alert", "")
			.build();
		assert!(invalid_body.validate().is_err());
	}

	#[test]
	fn test_discord_trigger_validation() {
		// Valid trigger
		let valid_trigger = TriggerBuilder::new()
			.name("test_discord")
			.discord("https://discord.com/api/webhooks/xxx")
			.message("Alert", "Test message")
			.build();
		assert!(valid_trigger.validate().is_ok());

		// Invalid webhook URL
		let invalid_webhook = TriggerBuilder::new()
			.name("test_discord")
			.discord("https://invalid-url.com")
			.build();
		assert!(invalid_webhook.validate().is_err());

		// Empty title
		let invalid_title = TriggerBuilder::new()
			.name("test_discord")
			.discord("https://discord.com/api/webhooks/123")
			.message("", "Test message")
			.build();
		assert!(invalid_title.validate().is_err());

		// Empty body
		let invalid_body = TriggerBuilder::new()
			.name("test_discord")
			.discord("https://discord.com/api/webhooks/123")
			.message("Alert", "")
			.build();
		assert!(invalid_body.validate().is_err());
	}

	#[test]
	fn test_telegram_trigger_validation() {
		let valid_trigger = TriggerBuilder::new()
			.name("test_telegram")
			.telegram(
				"1234567890:ABCdefGHIjklMNOpqrSTUvwxYZ123456789", // noboost
				"1730223038",
				true,
			)
			.build();
		assert!(valid_trigger.validate().is_ok());

		// Test invalid token
		let invalid_token = TriggerBuilder::new()
			.name("test_telegram")
			.telegram("invalid-token", "1730223038", true)
			.build();
		assert!(invalid_token.validate().is_err());

		// Test invalid chat ID
		let invalid_chat_id = TriggerBuilder::new()
			.name("test_telegram")
			.telegram(
				"1234567890:ABCdefGHIjklMNOpqrSTUvwxYZ123456789", // noboost
				"",
				true,
			)
			.build();
		assert!(invalid_chat_id.validate().is_err());

		// Test invalid message
		let invalid_title_message = TriggerBuilder::new()
			.name("test_telegram")
			.telegram(
				"1234567890:ABCdefGHIjklMNOpqrSTUvwxYZ123456789", // noboost
				"1730223038",
				true,
			)
			.message("", "Test Message")
			.build();
		assert!(invalid_title_message.validate().is_err());

		let invalid_body_message = TriggerBuilder::new()
			.name("test_telegram")
			.telegram(
				"1234567890:ABCdefGHIjklMNOpqrSTUvwxYZ123456789", // noboost
				"1730223038",
				true,
			)
			.message("Test Subject", "")
			.build();
		assert!(invalid_body_message.validate().is_err());
	}

	#[test]
	fn test_script_trigger_validation() {
		let temp_dir = std::env::temp_dir();
		let script_path = temp_dir.join("test_script.sh");
		std::fs::write(&script_path, "#!/bin/bash\necho 'test'").unwrap();

		// Valid trigger
		let valid_trigger = TriggerBuilder::new()
			.name("test_script")
			.script(script_path.to_str().unwrap(), ScriptLanguage::Bash)
			.build();
		assert!(valid_trigger.validate().is_ok());

		// Non-existent script
		let invalid_path = TriggerBuilder::new()
			.name("test_script")
			.script("/non/existent/path", ScriptLanguage::Python)
			.build();
		assert!(invalid_path.validate().is_err());

		std::fs::remove_file(script_path).unwrap();
	}

	#[test]
	fn test_invalid_load_from_path() {
		let path = Path::new("config/triggers/invalid.json");
		assert!(matches!(
			Trigger::load_from_path(path),
			Err(ConfigError::FileError(_))
		));
	}

	#[test]
	fn test_invalid_config_from_load_from_path() {
		use std::io::Write;
		use tempfile::NamedTempFile;

		let mut temp_file = NamedTempFile::new().unwrap();
		write!(temp_file, "{{\"invalid\": \"json").unwrap();

		let path = temp_file.path();

		assert!(matches!(
			Trigger::load_from_path(path),
			Err(ConfigError::ParseError(_))
		));
	}

	#[test]
	fn test_load_all_directory_not_found() {
		let non_existent_path = Path::new("non_existent_directory");

		let result: Result<HashMap<String, Trigger>, ConfigError> =
			Trigger::load_all(Some(non_existent_path));
		assert!(matches!(result, Err(ConfigError::FileError(_))));

		if let Err(ConfigError::FileError(err)) = result {
			assert!(err.message.contains("triggers directory not found"));
		}
	}

	#[test]
	#[cfg(unix)] // This test is Unix-specific due to permission handling
	fn test_load_all_unreadable_file() {
		// Create a temporary directory for our test
		let temp_dir = TempDir::new().unwrap();
		let config_dir = temp_dir.path().join("triggers");
		std::fs::create_dir(&config_dir).unwrap();

		// Create a JSON file with valid content but unreadable permissions
		let file_path = config_dir.join("unreadable.json");
		{
			let mut file = File::create(&file_path).unwrap();
			writeln!(file, r#"{{ "test_trigger": {{ "name": "test", "trigger_type": "Slack", "config": {{ "slack_url": "https://hooks.slack.com/services/xxx", "message": {{ "title": "Alert", "body": "Test message" }} }} }} }}"#).unwrap();
		}

		// Change permissions to make the file unreadable
		let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
		perms.set_mode(0o000); // No permissions
		std::fs::set_permissions(&file_path, perms).unwrap();

		// Try to load triggers from the directory
		let result: Result<HashMap<String, Trigger>, ConfigError> =
			Trigger::load_all(Some(&config_dir));

		// Verify we get the expected error
		assert!(matches!(result, Err(ConfigError::FileError(_))));
		if let Err(ConfigError::FileError(err)) = result {
			assert!(err.message.contains("failed to read trigger config file"));
		}

		// Clean up by making the file deletable
		let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
		perms.set_mode(0o644);
		std::fs::set_permissions(&file_path, perms).unwrap();
	}

	#[test]
	#[traced_test]
	fn test_validate_protocol_slack() {
		let insecure_trigger = Trigger {
			name: "test_slack".to_string(),
			trigger_type: TriggerType::Slack,
			config: TriggerTypeConfig::Slack {
				slack_url: "http://hooks.slack.com/services/xxx".to_string(),
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};

		insecure_trigger.validate_protocol();
		assert!(logs_contain("Slack URL uses an insecure protocol"));
	}

	#[test]
	#[traced_test]
	fn test_validate_protocol_discord() {
		let insecure_trigger = Trigger {
			name: "test_discord".to_string(),
			trigger_type: TriggerType::Discord,
			config: TriggerTypeConfig::Discord {
				discord_url: "http://discord.com/api/webhooks/xxx".to_string(),
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};

		insecure_trigger.validate_protocol();
		assert!(logs_contain("Discord URL uses an insecure protocol"));
	}

	#[test]
	#[traced_test]
	fn test_validate_protocol_webhook() {
		let insecure_trigger = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "http://api.example.com/webhook".to_string(),
				method: Some("POST".to_string()),
				headers: None,
				secret: None,
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};

		insecure_trigger.validate_protocol();
		assert!(logs_contain("Webhook URL uses an insecure protocol"));
		assert!(logs_contain("Webhook lacks authentication headers"));
	}

	#[test]
	#[traced_test]
	fn test_validate_protocol_email() {
		let insecure_trigger = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Email {
				host: "smtp.example.com".to_string(),
				port: Some(25), // Insecure port
				username: "user".to_string(),
				password: "pass".to_string(),
				message: NotificationMessage {
					title: "Test".to_string(),
					body: "Test message".to_string(),
				},
				sender: EmailAddress::new_unchecked("sender@example.com"),
				recipients: vec![EmailAddress::new_unchecked("recipient@example.com")],
			},
		};

		insecure_trigger.validate_protocol();
		assert!(logs_contain("Email port is not using a secure protocol"));
	}

	#[cfg(unix)]
	#[test]
	#[traced_test]
	fn test_validate_protocol_script() {
		use std::fs::File;
		use std::os::unix::fs::PermissionsExt;
		use tempfile::TempDir;

		let temp_dir = TempDir::new().unwrap();
		let script_path = temp_dir.path().join("test_script.sh");
		File::create(&script_path).unwrap();

		// Set overly permissive permissions (777)
		let metadata = std::fs::metadata(&script_path).unwrap();
		let mut permissions = metadata.permissions();
		permissions.set_mode(0o777);
		std::fs::set_permissions(&script_path, permissions).unwrap();

		let trigger = Trigger {
			name: "test_script".to_string(),
			trigger_type: TriggerType::Script,
			config: TriggerTypeConfig::Script {
				script_path: script_path.to_str().unwrap().to_string(),
				language: ScriptLanguage::Bash,
				timeout_ms: 1000,
				arguments: None,
			},
		};

		trigger.validate_protocol();
		assert!(logs_contain(
			"Script file has overly permissive write permissions"
		));
	}

	#[test]
	#[traced_test]
	fn test_validate_protocol_webhook_with_headers() {
		let mut headers = HashMap::new();
		headers.insert("Content-Type".to_string(), "application/json".to_string());

		let insecure_trigger = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: "http://api.example.com/webhook".to_string(),
				method: Some("POST".to_string()),
				headers: Some(headers),
				secret: None,
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
			},
		};

		insecure_trigger.validate_protocol();
		assert!(logs_contain("Webhook URL uses an insecure protocol"));
		assert!(logs_contain("Webhook lacks authentication headers"));
	}

	#[test]
	fn test_telegram_max_message_length() {
		let max_body_length = Trigger {
			name: "test_telegram".to_string(),
			trigger_type: TriggerType::Telegram,
			config: TriggerTypeConfig::Telegram {
				token: "1234567890:ABCdefGHIjklMNOpqrSTUvwxYZ123456789".to_string(),
				chat_id: "1730223038".to_string(),
				disable_web_preview: Some(true),
				message: NotificationMessage {
					title: "Test".to_string(),
					body: "x".repeat(TELEGRAM_MAX_BODY_LENGTH + 1), // Exceeds max length
				},
			},
		};
		assert!(max_body_length.validate().is_err());
	}

	#[test]
	fn test_discord_max_message_length() {
		let max_body_length = Trigger {
			name: "test_discord".to_string(),
			trigger_type: TriggerType::Discord,
			config: TriggerTypeConfig::Discord {
				discord_url: "https://discord.com/api/webhooks/xxx".to_string(),
				message: NotificationMessage {
					title: "Test".to_string(),
					body: "z".repeat(DISCORD_MAX_BODY_LENGTH + 1), // Exceeds max length
				},
			},
		};
		assert!(max_body_length.validate().is_err());
	}
}
