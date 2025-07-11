use crate::{
	models::{core::ScriptLanguage, SecretValue},
	utils::RetryConfig,
};
use email_address::EmailAddress;
use serde::{Deserialize, Serialize};

/// Configuration for actions to take when monitored conditions are met.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Trigger {
	/// Unique name identifying this trigger
	pub name: String,

	/// Type of trigger (Email, Slack, Webhook, Telegram, Discord, Script)
	pub trigger_type: TriggerType,

	/// Configuration specific to the trigger type
	pub config: TriggerTypeConfig,
}

/// Supported trigger action types
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[serde(deny_unknown_fields)]
pub enum TriggerType {
	/// Send notification to Slack
	Slack,
	/// Send notification to email
	Email,
	/// Make HTTP request to webhook
	Webhook,
	/// Send notification to Telegram
	Telegram,
	/// Send notification to Discord
	Discord,
	/// Execute local script
	Script,
}

/// Notification message fields
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NotificationMessage {
	/// Notification title or subject
	pub title: String,
	/// Message template
	pub body: String,
}

/// Type-specific configuration for triggers
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum TriggerTypeConfig {
	/// Slack notification configuration
	Slack {
		/// Slack webhook URL
		slack_url: SecretValue,
		/// Notification message
		message: NotificationMessage,
		/// Retry policy for HTTP requests
		#[serde(default)]
		retry_policy: RetryConfig,
	},
	/// Email notification configuration
	Email {
		/// SMTP host
		host: String,
		/// SMTP port (default 465)
		port: Option<u16>,
		/// SMTP username
		username: SecretValue,
		/// SMTP password
		password: SecretValue,
		/// Notification message
		message: NotificationMessage,
		/// Email sender
		sender: EmailAddress,
		/// Email recipients
		recipients: Vec<EmailAddress>,
		/// Retry policy for SMTP requests
		#[serde(default)]
		retry_policy: RetryConfig,
	},
	/// Webhook configuration
	Webhook {
		/// Webhook endpoint URL
		url: SecretValue,
		/// HTTP method to use
		method: Option<String>,
		/// Secret
		secret: Option<SecretValue>,
		/// Optional HTTP headers
		headers: Option<std::collections::HashMap<String, String>>,
		/// Notification message
		message: NotificationMessage,
		/// Retry policy for HTTP requests
		#[serde(default)]
		retry_policy: RetryConfig,
	},
	/// Telegram notification configuration
	Telegram {
		/// Telegram bot token
		token: SecretValue,
		/// Telegram chat ID
		chat_id: String,
		/// Disable web preview
		disable_web_preview: Option<bool>,
		/// Notification message
		message: NotificationMessage,
		/// Retry policy for HTTP requests
		#[serde(default)]
		retry_policy: RetryConfig,
	},
	/// Discord notification configuration
	Discord {
		/// Discord webhook URL
		discord_url: SecretValue,
		/// Notification message
		message: NotificationMessage,
		/// Retry policy for HTTP requests
		#[serde(default)]
		retry_policy: RetryConfig,
	},
	/// Script execution configuration
	Script {
		/// Language of the script
		language: ScriptLanguage,
		/// Path to script file
		script_path: String,
		/// Command line arguments
		#[serde(default)]
		arguments: Option<Vec<String>>,
		/// Timeout in milliseconds
		timeout_ms: u32,
	},
}

impl TriggerTypeConfig {
	/// Get the retry policy for the trigger type, if applicable.
	pub fn get_retry_policy(&self) -> Option<RetryConfig> {
		match self {
			Self::Slack { retry_policy, .. } => Some(retry_policy.clone()),
			Self::Discord { retry_policy, .. } => Some(retry_policy.clone()),
			Self::Webhook { retry_policy, .. } => Some(retry_policy.clone()),
			Self::Telegram { retry_policy, .. } => Some(retry_policy.clone()),
			_ => None,
		}
	}
}
