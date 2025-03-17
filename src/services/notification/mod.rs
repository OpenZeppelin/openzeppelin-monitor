//! Notification service implementation.
//!
//! This module provides functionality to send notifications through various channels
//! Supports variable substitution in message templates.

use anyhow::Context;
use async_trait::async_trait;

use std::collections::HashMap;

mod discord;
mod email;
mod error;
mod script;
mod slack;
mod telegram;
mod webhook;

use crate::models::{MonitorMatch, ScriptLanguage, Trigger, TriggerType, TriggerTypeConfig};

pub use discord::DiscordNotifier;
pub use email::{EmailContent, EmailNotifier, SmtpConfig};
pub use error::NotificationError;
pub use script::ScriptNotifier;
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
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), anyhow::Error>;
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
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn script_notify(
		&self,
		monitor_match: &MonitorMatch,
		script_content: &(ScriptLanguage, String),
	) -> Result<(), anyhow::Error>;
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
						.with_context(|| {
							format!("Failed to execute notification {}", trigger.name)
						})?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid slack configuration",
						None,
						None,
					));
				}
			}
			TriggerType::Email => {
				let notifier = EmailNotifier::from_config(&trigger.config);
				if let Some(notifier) = notifier {
					notifier
						.notify(&notifier.format_message(&variables))
						.await
						.with_context(|| {
							format!("Failed to execute notification {}", trigger.name)
						})?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid email configuration",
						None,
						None,
					));
				}
			}
			TriggerType::Webhook => {
				let notifier = WebhookNotifier::from_config(&trigger.config);
				if let Some(notifier) = notifier {
					notifier
						.notify(&notifier.format_message(&variables))
						.await
						.with_context(|| {
							format!("Failed to execute notification {}", trigger.name)
						})?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid webhook configuration",
						None,
						None,
					));
				}
			}
			TriggerType::Discord => {
				let notifier = DiscordNotifier::from_config(&trigger.config);

				if let Some(notifier) = notifier {
					notifier
						.notify(&notifier.format_message(&variables))
						.await
						.with_context(|| {
							format!("Failed to execute notification {}", trigger.name)
						})?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid discord configuration",
						None,
						None,
					));
				}
			}
			TriggerType::Telegram => {
				let notifier = TelegramNotifier::from_config(&trigger.config);
				if let Some(notifier) = notifier {
					notifier
						.notify(&notifier.format_message(&variables))
						.await
						.with_context(|| {
							format!("Failed to execute notification {}", trigger.name)
						})?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid telegram configuration",
						None,
						None,
					));
				}
			}
			TriggerType::Script => {
				let notifier = ScriptNotifier::from_config(&trigger.config);
				if let Some(notifier) = notifier {
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
						.get(&format!("{}|{}", monitor_name, script_path))
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
						.await
						.with_context(|| {
							format!("Failed to execute notification {}", trigger.name)
						})?;
				} else {
					return Err(NotificationError::config_error(
						"Invalid script configuration".to_string(),
						None,
						None,
					));
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{
		AddressWithABI, EVMMonitorMatch, EVMTransaction, EVMTransactionReceipt, EventCondition,
		FunctionCondition, MatchConditions, Monitor, MonitorMatch, NotificationMessage,
		ScriptLanguage, TransactionCondition, Trigger, TriggerType, TriggerTypeConfig,
	};
	use std::collections::HashMap;

	fn create_test_monitor(
		event_conditions: Vec<EventCondition>,
		function_conditions: Vec<FunctionCondition>,
		transaction_conditions: Vec<TransactionCondition>,
		addresses: Vec<AddressWithABI>,
	) -> Monitor {
		Monitor {
			match_conditions: MatchConditions {
				events: event_conditions,
				functions: function_conditions,
				transactions: transaction_conditions,
			},
			addresses,
			name: "test".to_string(),
			networks: vec!["evm_mainnet".to_string()],
			..Default::default()
		}
	}

	fn create_test_evm_transaction() -> EVMTransaction {
		let tx = alloy::consensus::TxLegacy {
			chain_id: None,
			nonce: 0,
			gas_price: 0,
			gas_limit: 0,
			to: alloy::primitives::TxKind::Call(alloy::primitives::Address::ZERO),
			value: alloy::primitives::U256::ZERO,
			input: alloy::primitives::Bytes::default(),
		};

		let signature = alloy::signers::Signature::from_scalars_and_parity(
			alloy::primitives::B256::ZERO,
			alloy::primitives::B256::ZERO,
			false,
		);

		let hash = alloy::primitives::B256::ZERO;

		EVMTransaction::from(alloy::rpc::types::Transaction {
			inner: alloy::consensus::transaction::Recovered::new_unchecked(
				alloy::consensus::transaction::TxEnvelope::Legacy(
					alloy::consensus::Signed::new_unchecked(tx, signature, hash),
				),
				alloy::primitives::Address::ZERO,
			),
			block_hash: None,
			block_number: None,
			transaction_index: None,
			effective_gas_price: None,
		})
	}

	fn create_mock_monitor_match() -> MonitorMatch {
		MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor(vec![], vec![], vec![], vec![]),
			transaction: create_test_evm_transaction(),
			receipt: EVMTransactionReceipt::default(),
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

		let trigger = Trigger {
			name: "test_slack".to_string(),
			trigger_type: TriggerType::Slack,
			config: TriggerTypeConfig::Script {
				// Intentionally wrong config type
				script_path: "invalid".to_string(),
				language: ScriptLanguage::Python,
				arguments: None,
				timeout_ms: 1000,
			},
		};

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				variables,
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

		let trigger = Trigger {
			name: "test_email".to_string(),
			trigger_type: TriggerType::Email,
			config: TriggerTypeConfig::Script {
				// Intentionally wrong config type
				script_path: "invalid".to_string(),
				language: ScriptLanguage::Python,
				arguments: None,
				timeout_ms: 1000,
			},
		};

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				variables,
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

		// Create a trigger with invalid Webhook config
		let trigger = Trigger {
			name: "test_webhook".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Script {
				// Intentionally wrong config type
				script_path: "invalid".to_string(),
				language: ScriptLanguage::Python,
				arguments: None,
				timeout_ms: 1000,
			},
		};

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				variables,
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

		let trigger = Trigger {
			name: "test_discord".to_string(),
			trigger_type: TriggerType::Discord,
			config: TriggerTypeConfig::Script {
				// Intentionally wrong config type
				script_path: "invalid".to_string(),
				language: ScriptLanguage::Python,
				arguments: None,
				timeout_ms: 1000,
			},
		};

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				variables,
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

		let trigger = Trigger {
			name: "test_telegram".to_string(),
			trigger_type: TriggerType::Telegram,
			config: TriggerTypeConfig::Script {
				// Intentionally wrong config type
				script_path: "invalid".to_string(),
				language: ScriptLanguage::Python,
				arguments: None,
				timeout_ms: 1000,
			},
		};

		let variables = HashMap::new();
		let result = service
			.execute(
				&trigger,
				variables,
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

		let trigger = Trigger {
			name: "test_script".to_string(),
			trigger_type: TriggerType::Script,
			// Intentionally wrong config type
			config: TriggerTypeConfig::Telegram {
				token: "invalid".to_string(),
				chat_id: "invalid".to_string(),
				disable_web_preview: None,
				message: NotificationMessage {
					title: "invalid".to_string(),
					body: "invalid".to_string(),
				},
			},
		};

		let variables = HashMap::new();

		let result = service
			.execute(
				&trigger,
				variables,
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
