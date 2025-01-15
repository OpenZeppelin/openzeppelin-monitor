//! Notification service implementation.
//!
//! This module provides functionality to send notifications through various channels:
//! - Slack messages via webhooks
//! - HTTP webhooks (planned)
//! - Script execution (planned)
//!
//! Supports variable substitution in message templates.

use std::collections::HashMap;

use async_trait::async_trait;

mod email;
mod error;
mod slack;

pub use email::{EmailContent, EmailNotifier, SmtpConfig};
pub use error::NotificationError;
pub use slack::SlackNotifier;

use crate::models::TriggerTypeConfig;

/// Interface for notification implementations
///
/// All notification types must implement this trait to provide
/// consistent notification behavior.
#[async_trait]
pub trait Notifier {
	/// Sends a notification with the given message
	///
	/// # Arguments
	/// * `message` - The formatted message to send
	///
	/// # Returns
	/// * `Result<(), Box<dyn std::error::Error>>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), Box<dyn std::error::Error>>;
}

/// Service for managing notifications across different channels
pub struct NotificationService;

impl NotificationService {
	/// Creates a new notification service instance
	pub fn new() -> Self {
		NotificationService
	}

	/// Executes a notification based on the trigger configuration
	///
	/// # Arguments
	/// * `config` - Configuration specifying the notification type and parameters
	/// * `variables` - Variables to substitute in message templates
	///
	/// # Returns
	/// * `Result<(), Box<dyn std::error::Error>>` - Success or error
	pub async fn execute(
		&self,
		config: &TriggerTypeConfig,
		variables: HashMap<String, String>,
	) -> Result<(), Box<dyn std::error::Error>> {
		match config {
			TriggerTypeConfig::Slack {
				webhook_url,
				title,
				body,
			} => {
				let notifier = SlackNotifier::new(webhook_url.clone(), title.clone(), body.clone());
				notifier
					.notify(&notifier.format_message(&variables))
					.await?;
			}
			TriggerTypeConfig::Email {
				host,
				port,
				username,
				password,
				subject,
				body,
				sender,
				recipients,
			} => {
				let smtp_config = SmtpConfig {
					host: host.clone(),
					port: port.unwrap_or(465),
					username: username.clone(),
					password: password.clone(),
				};
				let email_content = EmailContent {
					subject: subject.clone(),
					body_template: body.clone(),
					sender: sender.clone(),
					recipients: recipients.clone(),
				};
				let notifier = EmailNotifier::new(smtp_config, email_content);
				notifier
					.notify(&notifier.format_message(&variables))
					.await?;
			}
			TriggerTypeConfig::Webhook { .. } => {
				// TODO: Implement webhook notifier
				todo!("Implement webhook notification")
			}
			TriggerTypeConfig::Script { .. } => {
				// TODO: Implement script notifier
				todo!("Implement script execution")
			}
		}
		Ok(())
	}
}

impl Default for NotificationService {
	fn default() -> Self {
		Self::new()
	}
}
