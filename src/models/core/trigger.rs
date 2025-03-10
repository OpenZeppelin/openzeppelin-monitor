use crate::models::core::ScriptLanguage;
use email_address::EmailAddress;
use serde::{Deserialize, Serialize};

/// Configuration for actions to take when monitored conditions are met.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
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
pub struct NotificationMessage {
	/// Notification title or subject
	pub title: String,
	/// Message template
	pub body: String,
}

/// Type-specific configuration for triggers
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum TriggerTypeConfig {
	/// Slack notification configuration
	Slack {
		/// Slack webhook URL
		slack_url: String,
		/// Notification message
		message: NotificationMessage,
	},
	/// Email notification configuration
	Email {
		/// SMTP host
		host: String,
		/// SMTP port (default 465)
		port: Option<u16>,
		/// SMTP username
		username: String,
		/// SMTP password
		password: String,
		/// Notification message
		message: NotificationMessage,
		/// Email sender
		sender: EmailAddress,
		/// Email recipients
		recipients: Vec<EmailAddress>,
	},
	/// Webhook configuration
	Webhook {
		/// Webhook endpoint URL
		url: String,
		/// HTTP method to use
		method: Option<String>,
		/// Secret
		secret: Option<String>,
		/// Optional HTTP headers
		headers: Option<std::collections::HashMap<String, String>>,
		/// Notification message
		message: NotificationMessage,
	},
	/// Telegram notification configuration
	Telegram {
		/// Telegram bot token
		token: String,
		/// Telegram chat ID
		chat_id: String,
		/// Disable web preview
		disable_web_preview: Option<bool>,
		/// Notification message
		message: NotificationMessage,
	},
	/// Discord notification configuration
	Discord {
		/// Discord webhook URL
		discord_url: String,
		/// Notification message
		message: NotificationMessage,
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
