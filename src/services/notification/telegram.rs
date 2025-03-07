//! Telegram notification implementation.
//!
//! Provides functionality to send formatted messages to Telegram channels
//! via incoming webhooks, supporting message templates with variable substitution.

use async_trait::async_trait;
use reqwest::Client;
use std::collections::HashMap;

use crate::{
	models::TriggerTypeConfig,
	services::notification::{NotificationError, Notifier},
};

/// Implementation of Telegram notifications via webhooks
pub struct TelegramNotifier {
	/// Telegram bot token
	token: String,
	/// Telegram chat ID
	chat_id: String,
	/// Disable web preview
	disable_web_preview: bool,
	/// Title to display in the message
	title: String,
	/// Message template with variable placeholders
	body_template: String,
	/// HTTP client for webhook requests
	client: Client,
	/// Base URL for the Telegram API
	base_url: String,
}

impl TelegramNotifier {
	/// Creates a new Telegram notifier instance
	///
	/// # Arguments
	/// * `token` - Telegram bot token
	/// * `chat_id` - Telegram chat ID
	/// * `disable_web_preview` - Disable web preview
	/// * `title` - Title to display in the message
	/// * `body_template` - Message template with variables
	pub fn new(
		base_url: Option<String>,
		token: String,
		chat_id: String,
		disable_web_preview: Option<bool>,
		title: String,
		body_template: String,
	) -> Result<Self, NotificationError> {
		Ok(Self {
			base_url: base_url.unwrap_or("https://api.telegram.org".to_string()),
			token,
			chat_id,
			disable_web_preview: disable_web_preview.unwrap_or(false),
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
		// Markdown formatting for Telegram
		// Double asterisks for bold text
		// Double whitespaces for new line
		format!("*{}* \n\n{}", self.title, message)
	}

	/// Creates a Telegram notifier from a trigger configuration
	///
	/// # Arguments
	/// * `config` - Trigger configuration containing Telegram parameters
	///
	/// # Returns
	/// * `Option<Self>` - Notifier instance if config is Telegram type
	pub fn from_config(config: &TriggerTypeConfig) -> Option<Self> {
		match config {
			TriggerTypeConfig::Telegram {
				token,
				chat_id,
				message,
				disable_web_preview,
			} => Some(Self {
				base_url: "https://api.telegram.org".to_string(),
				token: token.clone(),
				chat_id: chat_id.clone(),
				disable_web_preview: disable_web_preview.unwrap_or(false),
				title: message.title.clone(),
				body_template: message.body.clone(),
				client: Client::new(),
			}),
			_ => None,
		}
	}

	pub fn construct_url(&self, message: &str) -> String {
		format!(
			"{}/bot{}/sendMessage?text={}&chat_id={}&parse_mode=markdown&\
			 disable_web_page_preview={}",
			self.base_url,
			self.token,
			urlencoding::encode(message),
			self.chat_id,
			self.disable_web_preview
		)
	}
}

#[async_trait]
impl Notifier for TelegramNotifier {
	/// Sends a formatted message to Telegram
	///
	/// # Arguments
	/// * `message` - The formatted message to send
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), NotificationError> {
		let url = self.construct_url(message);
		let response = self
			.client
			.get(&url)
			.send()
			.await
			.map_err(|e| NotificationError::network_error(e.to_string()))?;

		if !response.status().is_success() {
			return Err(NotificationError::network_error(format!(
				"Telegram webhook returned error status: {}",
				response.status()
			)));
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use crate::models::NotificationMessage;

	use super::*;

	fn create_test_notifier(body_template: &str) -> TelegramNotifier {
		TelegramNotifier::new(
			None,
			"test-token".to_string(),
			"test-chat-id".to_string(),
			Some(true),
			"Alert".to_string(),
			body_template.to_string(),
		)
		.unwrap()
	}

	fn create_test_telegram_config() -> TriggerTypeConfig {
		TriggerTypeConfig::Telegram {
			token: "test-token".to_string(),
			chat_id: "test-chat-id".to_string(),
			disable_web_preview: Some(true),
			message: NotificationMessage {
				title: "Alert".to_string(),
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
		assert_eq!(result, "*Alert* \n\nValue is 100 and status is critical");
	}

	#[test]
	fn test_format_message_with_missing_variables() {
		let notifier = create_test_notifier("Value is ${value} and status is ${status}");

		let mut variables = HashMap::new();
		variables.insert("value".to_string(), "100".to_string());
		// status variable is not provided

		let result = notifier.format_message(&variables);
		assert_eq!(result, "*Alert* \n\nValue is 100 and status is ${status}");
	}

	#[test]
	fn test_format_message_with_empty_template() {
		let notifier = create_test_notifier("");

		let variables = HashMap::new();
		let result = notifier.format_message(&variables);
		assert_eq!(result, "*Alert* \n\n");
	}

	////////////////////////////////////////////////////////////
	// construct_url tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_construct_url() {
		let notifier = create_test_notifier("Test message");
		let url = notifier.construct_url("Test message");
		assert_eq!(url, "https://api.telegram.org/bottest-token/sendMessage?text=Test%20message&chat_id=test-chat-id&parse_mode=markdown&disable_web_page_preview=true");
	}

	////////////////////////////////////////////////////////////
	// from_config tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_from_config_with_telegram_config() {
		let config = create_test_telegram_config();

		let notifier = TelegramNotifier::from_config(&config);
		assert!(notifier.is_some());

		let notifier = notifier.unwrap();
		assert_eq!(notifier.token, "test-token");
		assert_eq!(notifier.chat_id, "test-chat-id");
		assert!(notifier.disable_web_preview);
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
