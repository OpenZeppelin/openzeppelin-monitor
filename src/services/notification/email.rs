//! Email notification implementation.
//!
//! Provides functionality to send formatted messages to email addresses
//! via SMTP, supporting message templates with variable substitution.

use async_trait::async_trait;
use email_address::EmailAddress;
use lettre::{
    message::{
        header::{self, ContentType},
        Mailboxes,
    },
    transport::smtp::authentication::Credentials,
    Message, SmtpTransport, Transport,
};
use std::collections::HashMap;

use crate::models::TriggerTypeConfig;
use crate::services::notification::{NotificationError, Notifier};

/// Implementation of email notifications via SMTP
pub struct EmailNotifier {
    /// Email subject
    subject: String,
    /// Message template with variable placeholders
    body_template: String,
    /// SMTP client for email delivery
    client: SmtpTransport,
    /// Email sender
    sender: EmailAddress,
    /// Email receipients
    receipients: Vec<EmailAddress>,
}

/// Configuration for SMTP connection
#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
}

/// Configuration for email content
#[derive(Clone)]
pub struct EmailContent {
    pub subject: String,
    pub body_template: String,
    pub sender: EmailAddress,
    pub receipients: Vec<EmailAddress>,
}

impl EmailNotifier {
    /// Creates a new email notifier instance
    ///
    /// # Arguments
    /// * `smtp_config` - SMTP server configuration
    /// * `email_content` - Email content configuration
    pub fn new(smtp_config: SmtpConfig, email_content: EmailContent) -> Self {
        let client = SmtpTransport::relay(&smtp_config.host)
            .unwrap()
            .port(smtp_config.port)
            .credentials(Credentials::new(smtp_config.username, smtp_config.password))
            .build();

        Self {
            subject: email_content.subject,
            body_template: email_content.body_template,
            sender: email_content.sender,
            receipients: email_content.receipients,
            client,
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
        variables
            .iter()
            .fold(self.body_template.clone(), |message, (key, value)| {
                message.replace(&format!("${{{}}}", key), value)
            })
    }

    /// Creates an email notifier from a trigger configuration
    ///
    /// # Arguments
    /// * `config` - Trigger configuration containing email parameters
    ///
    /// # Returns
    /// * `Option<Self>` - Notifier instance if config is email type
    pub fn from_config(config: &TriggerTypeConfig) -> Option<Self> {
        match config {
            TriggerTypeConfig::Email {
                host,
                port,
                username,
                password,
                subject,
                body,
                sender,
                receipients,
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
                    receipients: receipients.clone(),
                };

                Some(Self::new(smtp_config, email_content))
            }
            _ => None,
        }
    }
}

#[async_trait]
impl Notifier for EmailNotifier {
    /// Sends a formatted message to email
    ///
    /// # Arguments
    /// * `message` - The formatted message to send
    ///
    /// # Returns
    /// * `Result<(), Box<dyn std::error::Error>>` - Success or error
    async fn notify(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let recipients_str = self
            .receipients
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");

        let mailboxes: Mailboxes = recipients_str.parse().unwrap();
        let receipients_header: header::To = mailboxes.into();

        let email = Message::builder()
            .mailbox(receipients_header)
            .from(self.sender.to_string().parse()?)
            .reply_to(self.sender.to_string().parse()?)
            .subject(&self.subject)
            .header(ContentType::TEXT_PLAIN)
            .body(message.to_owned())
            .unwrap();

        self.client
            .send(&email)
            .map_err(|e| NotificationError::network_error(e.to_string()))?;

        Ok(())
    }
}
