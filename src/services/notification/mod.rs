//! Notification service implementation.
//!
//! This module provides functionality to send notifications through various channels:
//! - Slack messages via webhooks
//! - HTTP webhooks (planned)
//! - Script execution (planned)
//!
//! Supports variable substitution in message templates.

use async_trait::async_trait;
use script::ScriptNotifier;
use std::collections::HashMap;

mod discord;
mod email;
mod error;
mod script;
mod slack;
mod telegram;
mod webhook;

use crate::models::{MonitorMatch, ScriptLanguage, Trigger, TriggerType};

pub use discord::DiscordNotifier;
pub use email::{EmailContent, EmailNotifier, SmtpConfig};
pub use error::NotificationError;
pub use slack::SlackNotifier;
pub use telegram::TelegramNotifier;
pub use webhook::WebhookNotifier;

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
	/// * `Result<(), NotificationError>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), NotificationError>;

	/// Executes the script and returns the result
	///
	/// # Arguments
	/// * `monitor_match` - The monitor match to send
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	async fn script_notify(
		&self,
		_monitor_match: &MonitorMatch,
		_trigger_scripts: &HashMap<String, (ScriptLanguage, String)>,
	) -> Result<(), NotificationError> {
		// Default implementation that does nothing
		Ok(())
	}
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
	/// * `trigger` - Trigger containing the notification type and parameters
	/// * `variables` - Variables to substitute in message templates
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	pub async fn execute(
		&self,
		trigger: &Trigger,
		variables: HashMap<String, String>,
		monitor_match: &MonitorMatch,
		trigger_scripts: &HashMap<String, (ScriptLanguage, String)>,
	) -> Result<(), NotificationError> {
		match &trigger.trigger_type {
			TriggerType::Slack => {
				let notifier = SlackNotifier::from_config(&trigger.config);
				if let Some(notifier) = notifier {
					notifier
						.notify(&notifier.format_message(&variables))
						.await
						.map_err(|e| NotificationError::config_error(e.to_string()))?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid slack configuration",
					));
				}
			}
			TriggerType::Email => {
				let notifier = EmailNotifier::from_config(&trigger.config);
				if let Some(notifier) = notifier {
					notifier
						.notify(&notifier.format_message(&variables))
						.await
						.map_err(|e| NotificationError::config_error(e.to_string()))?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid email configuration",
					));
				}
			}
			TriggerType::Webhook => {
				let notifier = WebhookNotifier::from_config(&trigger.config);
				if let Some(notifier) = notifier {
					notifier
						.notify(&notifier.format_message(&variables))
						.await
						.map_err(|e| NotificationError::config_error(e.to_string()))?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid webhook configuration",
					));
				}
			}
			TriggerType::Discord => {
				let notifier = DiscordNotifier::from_config(&trigger.config);

				if let Some(notifier) = notifier {
					notifier
						.notify(&notifier.format_message(&variables))
						.await
						.map_err(|e| NotificationError::config_error(e.to_string()))?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid discord configuration",
					));
				}
			}
			TriggerType::Telegram => {
				let notifier = TelegramNotifier::from_config(&trigger.config);
				if let Some(notifier) = notifier {
					notifier
						.notify(&notifier.format_message(&variables))
						.await
						.map_err(|e| NotificationError::config_error(e.to_string()))?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid telegram configuration",
					));
				}
			}
			TriggerType::Script => {
				let notifier = ScriptNotifier::from_config(&trigger.config);
				if let Some(notifier) = notifier {
					notifier
						.script_notify(monitor_match, trigger_scripts)
						.await
						.map_err(|e| NotificationError::config_error(e.to_string()))?;
				}
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
