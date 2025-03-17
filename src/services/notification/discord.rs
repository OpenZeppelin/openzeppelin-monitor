//! Discord notification implementation.
//!
//! Provides functionality to send formatted messages to Discord channels
//! via incoming webhooks, supporting message templates with variable substitution.

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;

use crate::{
	models::TriggerTypeConfig,
	services::notification::{NotificationError, Notifier},
};

/// Implementation of Discord notifications via webhooks
pub struct DiscordNotifier {
	/// Discord webhook URL for message delivery
	url: String,
	/// Title to display in the message
	title: String,
	/// Message template with variable placeholders
	body_template: String,
	/// HTTP client for webhook requests
	client: Client,
}

/// Represents a field in a Discord embed message
#[derive(Serialize)]
struct DiscordField {
	/// The name of the field (max 256 characters)
	name: String,
	/// The value of the field (max 1024 characters)
	value: String,
	/// Indicates whether the field should be displayed inline with other fields (optional)
	inline: Option<bool>,
}

/// Represents an embed message in Discord
#[derive(Serialize)]
struct DiscordEmbed {
	/// The title of the embed (max 256 characters)
	title: String,
	/// The description of the embed (max 4096 characters)
	description: Option<String>,
	/// A URL that the title links to (optional)
	url: Option<String>,
	/// The color of the embed represented as a hexadecimal integer (optional)
	color: Option<u32>,
	/// A list of fields included in the embed (max 25 fields, optional)
	fields: Option<Vec<DiscordField>>,
	/// Indicates whether text-to-speech is enabled for the embed (optional)
	tts: Option<bool>,
	/// A thumbnail image for the embed (optional)
	thumbnail: Option<String>,
	/// An image for the embed (optional)
	image: Option<String>,
	/// Footer information for the embed (max 2048 characters, optional)
	footer: Option<String>,
	/// Author information for the embed (max 256 characters, optional)
	author: Option<String>,
	/// A timestamp for the embed (optional)
	timestamp: Option<String>,
}

/// Represents a formatted Discord message
#[derive(Serialize)]
struct DiscordMessage {
	/// The content of the message
	content: String,
	/// The username to display as the sender of the message (optional)
	username: Option<String>,
	/// The avatar URL to display for the sender (optional)
	avatar_url: Option<String>,
	/// A list of embeds included in the message (max 10 embeds, optional)
	embeds: Option<Vec<DiscordEmbed>>,
}

impl DiscordNotifier {
	/// Creates a new Discord notifier instance
	///
	/// # Arguments
	/// * `url` - Discord webhook URL
	/// * `title` - Message title
	/// * `body_template` - Message template with variables
	pub fn new(
		url: String,
		title: String,
		body_template: String,
	) -> Result<Self, Box<NotificationError>> {
		Ok(Self {
			url,
			title,
			body_template,
			client: Client::new(),
		})
	}

	/// Formats a message by substituting variables in the template
	///
	/// # Arguments
	/// * `variables` - Map of variable names to values
	///
	/// # Returns
	/// * `String` - Formatted message with variables replaced
	pub fn format_message(&self, variables: &HashMap<String, String>) -> String {
		let mut message = self.body_template.clone();
		for (key, value) in variables {
			message = message.replace(&format!("${{{}}}", key), value);
		}
		format!("*{}*\n\n{}", self.title, message)
	}

	/// Creates a Discord notifier from a trigger configuration
	///
	/// # Arguments
	/// * `config` - Trigger configuration containing Discord parameters
	///
	/// # Returns
	/// * `Option<Self>` - Notifier instance if config is Discord type
	pub fn from_config(config: &TriggerTypeConfig) -> Option<Self> {
		match config {
			TriggerTypeConfig::Discord {
				discord_url,
				message,
			} => Some(Self {
				url: discord_url.clone(),
				title: message.title.clone(),
				body_template: message.body.clone(),
				client: Client::new(),
			}),
			_ => None,
		}
	}
}

#[async_trait]
impl Notifier for DiscordNotifier {
	/// Sends a formatted message to Discord
	///
	/// # Arguments
	/// * `message` - The formatted message to send
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), anyhow::Error> {
		let payload = DiscordMessage {
			content: message.to_string(),
			username: None,
			avatar_url: None,
			embeds: None,
		};

		let response = match self
			.client
			.post(&self.url)
			.header("Content-Type", "application/json")
			.json(&payload)
			.send()
			.await
		{
			Ok(resp) => resp,
			Err(e) => {
				return Err(anyhow::anyhow!(
					"Failed to send Discord notification: {}",
					e
				));
			}
		};

		if !response.status().is_success() {
			return Err(anyhow::anyhow!(
				"Discord webhook returned error status: {}",
				response.status()
			));
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use crate::models::NotificationMessage;

	use super::*;

	fn create_test_notifier(body_template: &str) -> DiscordNotifier {
		DiscordNotifier::new(
			"https://non-existent-url-discord-webhook.com".to_string(),
			"Alert".to_string(),
			body_template.to_string(),
		)
		.unwrap()
	}

	fn create_test_discord_config() -> TriggerTypeConfig {
		TriggerTypeConfig::Discord {
			discord_url: "https://discord.example.com".to_string(),
			message: NotificationMessage {
				title: "Test Alert".to_string(),
				body: "Test message ${value}".to_string(),
			},
		}
	}

	////////////////////////////////////////////////////////////
	// format_message tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_format_message() {
		let notifier = create_test_notifier("Value is ${value} and status is ${status}");

		let mut variables = HashMap::new();
		variables.insert("value".to_string(), "100".to_string());
		variables.insert("status".to_string(), "critical".to_string());

		let result = notifier.format_message(&variables);
		assert_eq!(result, "*Alert*\n\nValue is 100 and status is critical");
	}

	#[test]
	fn test_format_message_with_missing_variables() {
		let notifier = create_test_notifier("Value is ${value} and status is ${status}");

		let mut variables = HashMap::new();
		variables.insert("value".to_string(), "100".to_string());
		// status variable is not provided

		let result = notifier.format_message(&variables);
		assert_eq!(result, "*Alert*\n\nValue is 100 and status is ${status}");
	}

	#[test]
	fn test_format_message_with_empty_template() {
		let notifier = create_test_notifier("");

		let variables = HashMap::new();
		let result = notifier.format_message(&variables);
		assert_eq!(result, "*Alert*\n\n");
	}

	////////////////////////////////////////////////////////////
	// from_config tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_from_config_with_discord_config() {
		let config = create_test_discord_config();

		let notifier = DiscordNotifier::from_config(&config);
		assert!(notifier.is_some());

		let notifier = notifier.unwrap();
		assert_eq!(notifier.url, "https://discord.example.com");
		assert_eq!(notifier.title, "Test Alert");
		assert_eq!(notifier.body_template, "Test message ${value}");
	}

	////////////////////////////////////////////////////////////
	// notify tests
	////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_notify_failure() {
		let notifier = create_test_notifier("Test message");
		let result = notifier.notify("Test message").await;
		assert!(result.is_err());
	}
}
