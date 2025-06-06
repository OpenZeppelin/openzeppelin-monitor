//! Notification service implementation.
//!
//! This module provides functionality to send notifications through various channels
//! Supports variable substitution in message templates.

use async_trait::async_trait;

use std::collections::HashMap;

mod discord;
mod email;
mod error;
mod script;
mod slack;
mod telegram;
mod webhook;

use crate::{
	models::{MonitorMatch, ScriptLanguage, Trigger, TriggerType, TriggerTypeConfig},
	utils::normalize_string,
};

pub use discord::DiscordNotifier;
pub use email::{EmailContent, EmailNotifier, SmtpConfig};
pub use error::NotificationError;
pub use script::ScriptNotifier;
pub use slack::SlackNotifier;
pub use telegram::TelegramNotifier;
pub use webhook::{WebhookConfig, WebhookNotifier};

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

	/// Sends a notification with custom payload fields
	///
	/// # Arguments
	/// * `message` - The formatted message to send
	/// * `payload_fields` - Additional fields to include in the payload
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	async fn notify_with_payload(
		&self,
		message: &str,
		_payload_fields: HashMap<String, serde_json::Value>,
	) -> Result<(), NotificationError> {
		// Default implementation just calls notify
		self.notify(message).await
	}
}

/// Interface for executing scripts
///
/// This Interface is used to execute scripts for notifications.
/// It is implemented by the ScriptNotifier struct.
#[async_trait]
pub trait ScriptExecutor {
	/// Executes a script to send a custom notifications
	///
	/// # Arguments
	/// * `monitor_match` - The monitor match to send
	/// * `script_content` - The script content to execute
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	async fn script_notify(
		&self,
		monitor_match: &MonitorMatch,
		script_content: &(ScriptLanguage, String),
	) -> Result<(), NotificationError>;
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
	/// * `monitor_match` - Monitor match to send (needed for custom script trigger)
	/// * `trigger_scripts` - Contains the script content to execute (needed for custom script
	///   trigger)
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	pub async fn execute(
		&self,
		trigger: &Trigger,
		variables: &HashMap<String, String>,
		monitor_match: &MonitorMatch,
		trigger_scripts: &HashMap<String, (ScriptLanguage, String)>,
	) -> Result<(), NotificationError> {
		match &trigger.trigger_type {
			TriggerType::Slack => {
				let notifier = SlackNotifier::from_config(&trigger.config)?;
				let message = notifier.format_message(variables);
				notifier.notify(&message).await?;
			}
			TriggerType::Email => {
				let notifier = EmailNotifier::from_config(&trigger.config)?;
				let message = notifier.format_message(variables);
				notifier.notify(&message).await?;
			}
			TriggerType::Webhook => {
				let notifier = WebhookNotifier::from_config(&trigger.config)?;
				let message = notifier.format_message(variables);
				notifier.notify(&message).await?;
			}
			TriggerType::Discord => {
				let notifier = DiscordNotifier::from_config(&trigger.config)?;
				let message = notifier.format_message(variables);
				notifier.notify(&message).await?;
			}
			TriggerType::Telegram => {
				let notifier = TelegramNotifier::from_config(&trigger.config)?;
				let message = notifier.format_message(variables);
				notifier.notify(&message).await?;
			}
			TriggerType::Script => {
				let notifier = ScriptNotifier::from_config(&trigger.config)?;
				let monitor_name = match monitor_match {
					MonitorMatch::EVM(evm_match) => &evm_match.monitor.name,
					MonitorMatch::Stellar(stellar_match) => &stellar_match.monitor.name,
				};
				let script_path = match &trigger.config {
					TriggerTypeConfig::Script { script_path, .. } => script_path,
					_ => {
						return Err(NotificationError::config_error(
							"Invalid script configuration".to_string(),
							None,
							None,
						))
					}
				};
				let script = trigger_scripts
					.get(&format!(
						"{}|{}",
						normalize_string(monitor_name),
						script_path
					))
					.ok_or_else(|| {
						NotificationError::config_error(
							"Script content not found".to_string(),
							None,
							None,
						)
					});
				let script_content = match &script {
					Ok(content) => content,
					Err(e) => {
						return Err(NotificationError::config_error(e.to_string(), None, None))
					}
				};

				notifier
					.script_notify(monitor_match, script_content)
					.await?;
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		models::{
			AddressWithSpec, EVMMonitorMatch, EVMTransactionReceipt, EventCondition,
			FunctionCondition, MatchConditions, Monitor, MonitorMatch, ScriptLanguage,
			TransactionCondition, TriggerType,
		},
		utils::tests::{
			builders::{evm::monitor::MonitorBuilder, trigger::TriggerBuilder},
			evm::transaction::TransactionBuilder,
		},
	};
	use std::collections::HashMap;

	fn create_test_monitor(
		event_conditions: Vec<EventCondition>,
		function_conditions: Vec<FunctionCondition>,
		transaction_conditions: Vec<TransactionCondition>,
		addresses: Vec<AddressWithSpec>,
	) -> Monitor {
		let mut builder = MonitorBuilder::new()
			.name("test")
			.networks(vec!["evm_mainnet".to_string()]);

		// Add all conditions
		for event in event_conditions {
			builder = builder.event(&event.signature, event.expression);
		}
		for function in function_conditions {
			builder = builder.function(&function.signature, function.expression);
		}
		for transaction in transaction_conditions {
			builder = builder.transaction(transaction.status, transaction.expression);
		}

		// Add addresses
		for addr in addresses {
			builder = builder.address(&addr.address);
		}

		builder.build()
	}

	fn create_mock_monitor_match() -> MonitorMatch {
		MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor(vec![], vec![], vec![], vec![]),
			transaction: TransactionBuilder::new().build(),
			receipt: Some(EVMTransactionReceipt::default()),
			logs: Some(vec![]),
			network_slug: "evm_mainnet".to_string(),
			matched_on: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![],
			},
			matched_on_args: None,
		}))
	}

	#[tokio::test]
	async fn test_slack_notification_invalid_config() {
		let service = NotificationService::new();

		let trigger = TriggerBuilder::new()
			.name("test_slack")
			.script("invalid", ScriptLanguage::Python)
			.trigger_type(TriggerType::Slack) // Intentionally wrong config type
			.build();

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				&variables,
				&create_mock_monitor_match(),
				&HashMap::new(),
			)
			.await;
		assert!(result.is_err());
		match result {
			Err(NotificationError::ConfigError(ctx)) => {
				assert!(ctx.message.contains("Invalid slack configuration"));
			}
			_ => panic!("Expected ConfigError"),
		}
	}

	#[tokio::test]
	async fn test_email_notification_invalid_config() {
		let service = NotificationService::new();

		let trigger = TriggerBuilder::new()
			.name("test_email")
			.script("invalid", ScriptLanguage::Python)
			.trigger_type(TriggerType::Email) // Intentionally wrong config type
			.build();

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				&variables,
				&create_mock_monitor_match(),
				&HashMap::new(),
			)
			.await;
		assert!(result.is_err());
		match result {
			Err(NotificationError::ConfigError(ctx)) => {
				assert!(ctx.message.contains("Invalid email configuration"));
			}
			_ => panic!("Expected ConfigError"),
		}
	}

	#[tokio::test]
	async fn test_webhook_notification_invalid_config() {
		let service = NotificationService::new();

		let trigger = TriggerBuilder::new()
			.name("test_webhook")
			.script("invalid", ScriptLanguage::Python)
			.trigger_type(TriggerType::Webhook) // Intentionally wrong config type
			.build();

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				&variables,
				&create_mock_monitor_match(),
				&HashMap::new(),
			)
			.await;
		assert!(result.is_err());
		match result {
			Err(NotificationError::ConfigError(ctx)) => {
				assert!(ctx.message.contains("Invalid webhook configuration"));
			}
			_ => panic!("Expected ConfigError"),
		}
	}

	#[tokio::test]
	async fn test_discord_notification_invalid_config() {
		let service = NotificationService::new();

		let trigger = TriggerBuilder::new()
			.name("test_discord")
			.script("invalid", ScriptLanguage::Python)
			.trigger_type(TriggerType::Discord) // Intentionally wrong config type
			.build();

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				&variables,
				&create_mock_monitor_match(),
				&HashMap::new(),
			)
			.await;
		assert!(result.is_err());
		match result {
			Err(NotificationError::ConfigError(ctx)) => {
				assert!(ctx.message.contains("Invalid discord configuration"));
			}
			_ => panic!("Expected ConfigError"),
		}
	}

	#[tokio::test]
	async fn test_telegram_notification_invalid_config() {
		let service = NotificationService::new();

		let trigger = TriggerBuilder::new()
			.name("test_telegram")
			.script("invalid", ScriptLanguage::Python)
			.trigger_type(TriggerType::Telegram) // Intentionally wrong config type
			.build();

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				&variables,
				&create_mock_monitor_match(),
				&HashMap::new(),
			)
			.await;
		assert!(result.is_err());
		match result {
			Err(NotificationError::ConfigError(ctx)) => {
				assert!(ctx.message.contains("Invalid telegram configuration"));
			}
			_ => panic!("Expected ConfigError"),
		}
	}

	#[tokio::test]
	async fn test_script_notification_invalid_config() {
		let service = NotificationService::new();

		let trigger = TriggerBuilder::new()
			.name("test_script")
			.telegram("invalid", "invalid", false)
			.trigger_type(TriggerType::Script) // Intentionally wrong config type
			.build();

		let variables = HashMap::new();

		let result = service
			.execute(
				&trigger,
				&variables,
				&create_mock_monitor_match(),
				&HashMap::new(),
			)
			.await;

		assert!(result.is_err());
		match result {
			Err(NotificationError::ConfigError(ctx)) => {
				assert!(ctx.message.contains("Invalid script configuration"));
			}
			_ => panic!("Expected ConfigError"),
		}
	}
}
