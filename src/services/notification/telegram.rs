//! Telegram notification implementation.
//!
//! Provides functionality to send formatted messages to Telegram channels
//! via incoming webhooks, supporting message templates with variable substitution.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::{
	models::TriggerTypeConfig,
	services::notification::{NotificationError, Notifier, WebhookConfig, WebhookNotifier},
};

/// Implementation of Telegram notifications via webhooks
pub struct TelegramNotifier {
	inner: WebhookNotifier,
	/// Disable web preview
	disable_web_preview: bool,
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
	) -> Result<Self, Box<NotificationError>> {
		let url = format!(
			"{}/bot{}/sendMessage",
			base_url.unwrap_or("https://api.telegram.org".to_string()),
			token
		);

		// Set up initial URL parameters
		let mut url_params = HashMap::new();
		url_params.insert("chat_id".to_string(), chat_id);
		url_params.insert("parse_mode".to_string(), "markdown".to_string());

		Ok(Self {
			inner: WebhookNotifier::new(WebhookConfig {
				url,
				url_params: Some(url_params),
				title,
				body_template,
				method: Some("GET".to_string()),
				secret: None,
				headers: None,
				payload_fields: None,
			})?,
			disable_web_preview: disable_web_preview.unwrap_or(false),
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
		let message = self.inner.format_message(variables);
		format!("*{}* \n\n{}", self.inner.title, message)
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
			} => {
				let mut url_params = HashMap::new();
				url_params.insert("chat_id".to_string(), chat_id.clone());
				url_params.insert("parse_mode".to_string(), "markdown".to_string());

				WebhookNotifier::new(WebhookConfig {
					url: format!("https://api.telegram.org/bot{}/sendMessage", token),
					url_params: Some(url_params),
					title: message.title.clone(),
					body_template: message.body.clone(),
					method: Some("GET".to_string()),
					secret: None,
					headers: Some(HashMap::from([(
						"Content-Type".to_string(),
						"application/json".to_string(),
					)])),
					payload_fields: None,
				})
				.ok()
				.map(|inner| Self {
					inner,
					disable_web_preview: disable_web_preview.unwrap_or(false),
				})
			}
			_ => None,
		}
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
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), anyhow::Error> {
		// Add message and disable_web_preview to URL parameters
		let mut url_params = self.inner.url_params.clone().unwrap_or_default();
		url_params.insert("text".to_string(), message.to_string());
		url_params.insert(
			"disable_web_page_preview".to_string(),
			self.disable_web_preview.to_string(),
		);

		// Create a new WebhookNotifier with updated URL parameters
		let notifier = WebhookNotifier::new(WebhookConfig {
			url: self.inner.url.clone(),
			url_params: Some(url_params),
			title: self.inner.title.clone(),
			body_template: self.inner.body_template.clone(),
			method: Some("GET".to_string()),
			secret: None,
			headers: self.inner.headers.clone(),
			payload_fields: None,
		})?;

		notifier.notify_with_payload(message, HashMap::new()).await
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
	// from_config tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_from_config_with_telegram_config() {
		let config = create_test_telegram_config();

		let notifier = TelegramNotifier::from_config(&config);
		assert!(notifier.is_some());

		let notifier = notifier.unwrap();
		assert_eq!(
			notifier.inner.url,
			"https://api.telegram.org/bottest-token/sendMessage"
		);
		assert!(notifier.disable_web_preview);
		assert_eq!(notifier.inner.body_template, "Test message ${value}");
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

	#[tokio::test]
	async fn test_notify_with_payload_failure() {
		let notifier = create_test_notifier("Test message");
		let result = notifier
			.notify_with_payload("Test message", HashMap::new())
			.await;
		assert!(result.is_err());
	}
}
