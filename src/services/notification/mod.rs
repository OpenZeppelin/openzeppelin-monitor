//! Notification service implementation.
//!
//! This module provides functionality to send notifications through various channels
//! Supports variable substitution in message templates.

use async_trait::async_trait;

use std::{collections::HashMap, sync::Arc};

mod email;
mod error;
pub mod payload_builder;
mod pool;
mod script;
mod webhook;

use crate::{
	models::{
		MonitorMatch, NotificationMessage, ScriptLanguage, Trigger, TriggerType, TriggerTypeConfig,
	},
	utils::{normalize_string, RetryConfig},
};

pub use email::{EmailContent, EmailNotifier, SmtpConfig};
pub use error::NotificationError;
pub use payload_builder::{
	DiscordPayloadBuilder, GenericWebhookPayloadBuilder, SlackPayloadBuilder,
	TelegramPayloadBuilder, WebhookPayloadBuilder,
};
pub use pool::NotificationClientPool;
pub use script::ScriptNotifier;
pub use webhook::{WebhookConfig, WebhookNotifier};

/// A container for all components needed to configure and send a webhook notification.
struct WebhookComponents {
	config: WebhookConfig,
	retry_policy: RetryConfig,
	builder: Box<dyn WebhookPayloadBuilder>,
}

/// A type alias to simplify the complex tuple returned by the internal `match` statement.
type WebhookParts = (
	String,                          // url
	NotificationMessage,             // message
	Option<String>,                  // method
	Option<String>,                  // secret
	Option<HashMap<String, String>>, // headers
	Box<dyn WebhookPayloadBuilder>,  // payload builder
);

/// A trait for trigger configurations that can be sent via webhook.
/// This abstracts away the specific details of each webhook provider.
trait AsWebhookComponents {
	/// Consolidates the logic for creating webhook components from a trigger config.
	/// It returns the generic `WebhookConfig`, RetryConfig and the specific `WebhookPayloadBuilder`
	/// needed for the given trigger type.
	fn as_webhook_components(&self) -> Result<WebhookComponents, NotificationError>;
}

impl AsWebhookComponents for TriggerTypeConfig {
	fn as_webhook_components(&self) -> Result<WebhookComponents, NotificationError> {
		let (url, message, method, secret, headers, builder): WebhookParts = match self {
			TriggerTypeConfig::Webhook {
				url,
				message,
				method,
				secret,
				headers,
				..
			} => (
				url.as_ref().to_string(),
				message.clone(),
				method.clone(),
				secret.as_ref().map(|s| s.as_ref().to_string()),
				headers.clone(),
				Box::new(GenericWebhookPayloadBuilder),
			),
			TriggerTypeConfig::Discord {
				discord_url,
				message,
				..
			} => (
				discord_url.as_ref().to_string(),
				message.clone(),
				Some("POST".to_string()),
				None,
				None,
				Box::new(DiscordPayloadBuilder),
			),
			TriggerTypeConfig::Telegram {
				token,
				message,
				chat_id,
				disable_web_preview,
				..
			} => (
				format!("https://api.telegram.org/bot{}/sendMessage", token),
				message.clone(),
				Some("POST".to_string()),
				None,
				None,
				Box::new(TelegramPayloadBuilder {
					chat_id: chat_id.clone(),
					disable_web_preview: disable_web_preview.unwrap_or(false),
				}),
			),
			TriggerTypeConfig::Slack {
				slack_url, message, ..
			} => (
				slack_url.as_ref().to_string(),
				message.clone(),
				Some("POST".to_string()),
				None,
				None,
				Box::new(SlackPayloadBuilder),
			),
			_ => {
				return Err(NotificationError::config_error(
					format!("Trigger type is not webhook-compatible: {:?}", self),
					None,
					None,
				))
			}
		};

		// Construct the final WebhookConfig from the extracted parts.
		let config = WebhookConfig {
			url,
			title: message.title,
			body_template: message.body,
			method,
			secret,
			headers,
			url_params: None,
			payload_fields: None,
		};

		// Use the retry policy from the trigger config
		let retry_policy = self.get_retry_policy().ok_or_else(|| {
			NotificationError::config_error(
				"Webhook trigger config is unexpectedly missing a retry policy.",
				None,
				None,
			)
		})?;

		Ok(WebhookComponents {
			config,
			retry_policy,
			builder,
		})
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
pub struct NotificationService {
	/// Client pool for managing notification clients (HTTP, SMTP)
	client_pool: Arc<NotificationClientPool>,
}

impl NotificationService {
	/// Creates a new notification service instance
	pub fn new() -> Self {
		NotificationService {
			client_pool: Arc::new(NotificationClientPool::new()),
		}
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
			// Match Webhook-based triggers
			TriggerType::Slack
			| TriggerType::Discord
			| TriggerType::Webhook
			| TriggerType::Telegram => {
				// Use the Webhookable trait to get config, retry policy and payload builder
				let components = trigger.config.as_webhook_components()?;

				// Get or create the HTTP client from the pool based on the retry policy
				let http_client = self
					.client_pool
					.get_or_create_http_client(&components.retry_policy)
					.await
					.map_err(|e| {
						NotificationError::execution_error(
							"Failed to get or create HTTP client from pool".to_string(),
							Some(e.into()),
							None,
						)
					})?;

				// Build the payload
				let payload = components.builder.build_payload(
					&components.config.title,
					&components.config.body_template,
					variables,
				);

				// Create the notifier
				let notifier = WebhookNotifier::new(components.config, http_client)?;

				notifier.notify_json(&payload).await?;
			}
			TriggerType::Email => {
				// Extract SMTP configuration from the trigger
				let smtp_config = match &trigger.config {
					TriggerTypeConfig::Email {
						host,
						port,
						username,
						password,
						..
					} => SmtpConfig {
						host: host.clone(),
						port: port.unwrap_or(465),
						username: username.as_ref().to_string(),
						password: password.as_ref().to_string(),
					},
					_ => {
						return Err(NotificationError::config_error(
							"Invalid email configuration".to_string(),
							None,
							None,
						));
					}
				};

				// Get or create the SMTP client from the pool
				let smtp_client = self
					.client_pool
					.get_or_create_smtp_client(&smtp_config)
					.await
					.map_err(|e| {
						NotificationError::execution_error(
							"Failed to get SMTP client from pool".to_string(),
							Some(e.into()),
							None,
						)
					})?;

				let notifier = EmailNotifier::from_config(&trigger.config, smtp_client)?;
				let message = EmailNotifier::format_message(notifier.body_template(), variables);
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
						));
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
						return Err(NotificationError::config_error(e.to_string(), None, None));
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
			FunctionCondition, MatchConditions, Monitor, MonitorMatch, NotificationMessage,
			ScriptLanguage, SecretString, SecretValue, TransactionCondition, TriggerType,
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
				assert!(ctx
					.message
					.contains("Trigger type is not webhook-compatible"));
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
				assert!(ctx
					.message
					.contains("Trigger type is not webhook-compatible"));
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
				assert!(ctx
					.message
					.contains("Trigger type is not webhook-compatible"));
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
				assert!(ctx
					.message
					.contains("Trigger type is not webhook-compatible"));
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

	#[test]
	fn as_webhook_components_trait_for_slack_config() {
		let title = "Slack Title";
		let message = "Slack Body";

		let slack_config = TriggerTypeConfig::Slack {
			slack_url: SecretValue::Plain(SecretString::new(
				"https://slack.example.com".to_string(),
			)),
			message: NotificationMessage {
				title: title.to_string(),
				body: message.to_string(),
			},
			retry_policy: RetryConfig::default(),
		};

		let components = slack_config.as_webhook_components().unwrap();

		// Assert WebhookConfig is correct
		assert_eq!(components.config.url, "https://slack.example.com");
		assert_eq!(components.config.title, title);
		assert_eq!(components.config.body_template, message);
		assert_eq!(components.config.method, Some("POST".to_string()));
		assert!(components.config.secret.is_none());

		// Assert the builder creates the correct payload
		let payload = components
			.builder
			.build_payload(title, message, &HashMap::new());
		assert!(
			payload.get("blocks").is_some(),
			"Expected a Slack payload with 'blocks'"
		);
		assert!(
			payload.get("content").is_none(),
			"Did not expect a Discord payload"
		);
	}

	#[test]
	fn as_webhook_components_trait_for_discord_config() {
		let title = "Discord Title";
		let message = "Discord Body";
		let discord_config = TriggerTypeConfig::Discord {
			discord_url: SecretValue::Plain(SecretString::new(
				"https://discord.example.com".to_string(),
			)),
			message: NotificationMessage {
				title: title.to_string(),
				body: message.to_string(),
			},
			retry_policy: RetryConfig::default(),
		};

		let components = discord_config.as_webhook_components().unwrap();

		// Assert WebhookConfig is correct
		assert_eq!(components.config.url, "https://discord.example.com");
		assert_eq!(components.config.title, title);
		assert_eq!(components.config.body_template, message);
		assert_eq!(components.config.method, Some("POST".to_string()));

		// Assert the builder creates the correct payload
		let payload = components
			.builder
			.build_payload(title, message, &HashMap::new());
		assert!(
			payload.get("content").is_some(),
			"Expected a Discord payload with 'content'"
		);
		assert!(
			payload.get("blocks").is_none(),
			"Did not expect a Slack payload"
		);
	}

	#[test]
	fn as_webhook_components_trait_for_telegram_config() {
		let title = "Telegram Title";
		let message = "Telegram Body";
		let telegram_config = TriggerTypeConfig::Telegram {
			token: SecretValue::Plain(SecretString::new("test-token".to_string())),
			chat_id: "12345".to_string(),
			disable_web_preview: Some(true),
			message: NotificationMessage {
				title: title.to_string(),
				body: message.to_string(),
			},
			retry_policy: RetryConfig::default(),
		};

		let components = telegram_config.as_webhook_components().unwrap();

		// Assert WebhookConfig is correct
		assert_eq!(
			components.config.url,
			"https://api.telegram.org/bottest-token/sendMessage"
		);
		assert_eq!(components.config.title, title);
		assert_eq!(components.config.body_template, message);

		// Assert the builder creates the correct payload
		let payload = components
			.builder
			.build_payload(title, message, &HashMap::new());
		assert_eq!(payload.get("chat_id").unwrap(), "12345");
		assert_eq!(payload.get("disable_web_page_preview").unwrap(), &true);
		assert!(payload.get("text").is_some());
	}

	#[test]
	fn as_webhook_components_trait_for_generic_webhook_config() {
		let title = "Generic Title";
		let body_template = "Generic Body";
		let webhook_config = TriggerTypeConfig::Webhook {
			url: SecretValue::Plain(SecretString::new("https://generic.example.com".to_string())),
			message: NotificationMessage {
				title: title.to_string(),
				body: body_template.to_string(),
			},
			method: Some("PUT".to_string()),
			secret: Some(SecretValue::Plain(SecretString::new(
				"my-secret".to_string(),
			))),
			headers: Some([("X-Custom".to_string(), "Value".to_string())].into()),
			retry_policy: RetryConfig::default(),
		};

		let components = webhook_config.as_webhook_components().unwrap();

		// Assert WebhookConfig is correct
		assert_eq!(components.config.url, "https://generic.example.com");
		assert_eq!(components.config.method, Some("PUT".to_string()));
		assert_eq!(components.config.secret, Some("my-secret".to_string()));
		assert!(components.config.headers.is_some());
		assert_eq!(
			components.config.headers.unwrap().get("X-Custom").unwrap(),
			"Value"
		);

		// Assert the builder creates the correct payload
		let payload = components
			.builder
			.build_payload(title, body_template, &HashMap::new());
		assert!(payload.get("title").is_some());
		assert!(payload.get("body").is_some());
	}
}
