//! Slack notification implementation.
//!
//! Provides functionality to send formatted messages to Slack channels
//! via incoming webhooks, supporting message templates with variable substitution.

use async_trait::async_trait;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;

use crate::{
	models::TriggerTypeConfig,
	services::notification::{NotificationError, Notifier},
};

/// Implementation of Slack notifications via webhooks
pub struct SlackNotifier {
	/// Slack webhook URL for message delivery
	webhook_url: String,
	/// Title to display in the message
	title: String,
	/// Message template with variable placeholders
	body_template: String,
	/// HTTP client for webhook requests
	client: Client,
}

/// Represents a formatted Slack message
#[derive(Serialize)]
struct SlackMessage {
	/// The formatted text to send to Slack
	text: String,
}

impl SlackNotifier {
	/// Creates a new Slack notifier instance
	///
	/// # Arguments
	/// * `webhook_url` - Slack webhook URL
	/// * `title` - Message title
	/// * `body_template` - Message template with variables
	pub fn new(webhook_url: String, title: String, body_template: String) -> Self {
		Self {
			webhook_url,
			title,
			body_template,
			client: Client::new(),
		}
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

	/// Creates a Slack notifier from a trigger configuration
	///
	/// # Arguments
	/// * `config` - Trigger configuration containing Slack parameters
	///
	/// # Returns
	/// * `Option<Self>` - Notifier instance if config is Slack type
	pub fn from_config(config: &TriggerTypeConfig) -> Option<Self> {
		match config {
			TriggerTypeConfig::Slack {
				webhook_url,
				title,
				body,
			} => Some(Self {
				webhook_url: webhook_url.clone(),
				title: title.clone(),
				body_template: body.clone(),
				client: Client::new(),
			}),
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
	/// * `Result<(), Box<dyn std::error::Error>>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
		let payload = SlackMessage {
			text: message.to_string(),
		};

		self.client
			.post(&self.webhook_url)
			.json(&payload)
			.send()
			.await
			.map_err(|e| Box::new(NotificationError::network_error(e.to_string())))?;

		Ok(())
	}
}
