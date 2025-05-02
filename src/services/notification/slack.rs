//! Slack notification implementation.
//!
//! Provides functionality to send formatted messages to Slack channels
//! via incoming webhooks, supporting message templates with variable substitution.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::{
	models::TriggerTypeConfig,
	services::notification::{NotificationError, Notifier, WebhookConfig, WebhookNotifier},
};

/// Implementation of Slack notifications via webhooks
pub struct SlackNotifier {
	inner: WebhookNotifier,
}

impl SlackNotifier {
	/// Creates a new Slack notifier instance
	///
	/// # Arguments
	/// * `url` - Slack webhook URL
	/// * `title` - Message title
	/// * `body_template` - Message template with variables
	pub fn new(
		url: String,
		title: String,
		body_template: String,
	) -> Result<Self, Box<NotificationError>> {
		Ok(Self {
			inner: WebhookNotifier::new(WebhookConfig {
				url,
				url_params: None,
				title,
				body_template,
				method: Some("POST".to_string()),
				secret: None,
				headers: None,
				payload_fields: None,
			})?,
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
		format!("*{}*\n\n{}", self.inner.title, message)
	}

	/// Creates a Slack notifier from a trigger configuration
	///
	/// # Arguments
	/// * `config` - Trigger configuration containing Slack parameters
	///
	/// # Returns
	/// * `Option<Self>` - Notifier instance if config is Slack type
	pub fn from_config(config: &TriggerTypeConfig) -> Option<Self> {
		match config {
			TriggerTypeConfig::Slack { slack_url, message } => {
				let mut headers = HashMap::new();
				headers.insert("Content-Type".to_string(), "application/json".to_string());

				WebhookNotifier::new(WebhookConfig {
					url: slack_url.clone(),
					url_params: None,
					title: message.title.clone(),
					body_template: message.body.clone(),
					method: Some("POST".to_string()),
					secret: None,
					headers: Some(headers),
					payload_fields: None,
				})
				.ok()
				.map(|inner| Self { inner })
			}
			_ => None,
		}
	}
}

#[async_trait]
impl Notifier for SlackNotifier {
	/// Sends a formatted message to Slack
	///
	/// # Arguments
	/// * `message` - The formatted message to send
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), anyhow::Error> {
		let mut payload_fields = HashMap::new();
		let blocks = serde_json::json!([
			{
				"type": "section",
				"text": {
					"type": "mrkdwn",
					"text": message
				}
			}
		]);
		payload_fields.insert("blocks".to_string(), blocks);

		self.inner
			.notify_with_payload(message, payload_fields)
			.await
	}
}

#[cfg(test)]
mod tests {
	use crate::models::NotificationMessage;

	use super::*;

	fn create_test_notifier(body_template: &str) -> SlackNotifier {
		SlackNotifier::new(
			"https://non-existent-url-slack-webhook.com".to_string(),
			"Alert".to_string(),
			body_template.to_string(),
		)
		.unwrap()
	}

	fn create_test_slack_config() -> TriggerTypeConfig {
		TriggerTypeConfig::Slack {
			slack_url: "https://slack.example.com".to_string(),
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
	fn test_from_config_with_slack_config() {
		let config = create_test_slack_config();

		let notifier = SlackNotifier::from_config(&config);
		assert!(notifier.is_some());

		let notifier = notifier.unwrap();
		assert_eq!(notifier.inner.url, "https://slack.example.com");
		assert_eq!(notifier.inner.title, "Test Alert");
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
