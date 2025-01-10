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

impl EmailNotifier {
    /// Creates a new email notifier instance
    ///
    /// # Arguments
    /// * `host` - SMTP server host
    /// * `port` - SMTP server port
    /// * `username` - SMTP server username
    /// * `password` - SMTP server password
    /// * `subject` - Email subject
    /// * `body_template` - Message template with variables
    /// * `sender` - Email sender
    /// * `receipients` - Email receipients
    pub fn new(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
        subject: &str,
        body_template: &str,
        sender: &EmailAddress,
        receipients: &Vec<EmailAddress>,
    ) -> Self {
        let client = SmtpTransport::relay(host)
            .unwrap()
            .port(port)
            .credentials(Credentials::new(username.to_owned(), password.to_owned()))
            .build();

        Self {
            subject: subject.to_owned(),
            body_template: body_template.to_owned(),
            sender: sender.clone(),
            receipients: receipients.clone(),
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
                let client = SmtpTransport::relay(host)
                    .unwrap()
                    .port(port.unwrap_or(465))
                    .credentials(Credentials::new(username.clone(), password.clone()))
                    .build();

                Some(Self {
                    subject: subject.clone(),
                    body_template: body.clone(),
                    sender: sender.clone(),
                    receipients: receipients.clone(),
                    client,
                })
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
