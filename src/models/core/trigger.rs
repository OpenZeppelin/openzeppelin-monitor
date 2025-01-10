use email_address::EmailAddress;
use serde::{Deserialize, Serialize};

/// Configuration for actions to take when monitored conditions are met.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Trigger {
    /// Unique name identifying this trigger
    pub name: String,

    /// Type of trigger (Slack, Webhook, Script)
    pub trigger_type: TriggerType,

    /// Configuration specific to the trigger type
    pub config: TriggerTypeConfig,
}

/// Supported trigger action types
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TriggerType {
    /// Send notification to Slack
    Slack,
    /// Send notification to email
    Email,
    /// Make HTTP request to webhook
    Webhook,
    /// Execute local script
    Script,
}

/// Type-specific configuration for triggers
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TriggerTypeConfig {
    /// Slack notification configuration
    Slack {
        /// Slack webhook URL
        webhook_url: String,
        /// Notification title
        title: String,
        /// Message template
        body: String,
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
        /// Email subject
        subject: String,
        /// Email body
        body: String,
        /// Email sender
        sender: EmailAddress,
        /// Email receipients
        receipients: Vec<EmailAddress>,
    },
    /// Webhook configuration
    Webhook {
        /// Webhook endpoint URL
        url: String,
        /// HTTP method to use
        method: String,
        /// Optional HTTP headers
        headers: Option<std::collections::HashMap<String, String>>,
    },
    /// Script execution configuration
    Script {
        /// Path to script file
        path: String,
        /// Command line arguments
        args: Vec<String>,
    },
}
