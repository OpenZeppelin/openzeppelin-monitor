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

use crate::{
	models::TriggerTypeConfig,
	services::notification::{NotificationError, Notifier},
};

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
	/// Email recipients
	recipients: Vec<EmailAddress>,
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
	pub recipients: Vec<EmailAddress>,
}

impl EmailNotifier {
	/// Creates a new email notifier instance
	///
	/// # Arguments
	/// * `smtp_config` - SMTP server configuration
	/// * `email_content` - Email content configuration
	pub fn new(
		smtp_config: SmtpConfig,
		email_content: EmailContent,
	) -> Result<Self, NotificationError> {
		let relay = SmtpTransport::relay(&smtp_config.host).map_err(|e| {
			NotificationError::internal_error(format!("Failed to build client: {}", e))
		})?;

		let client = relay
			.port(smtp_config.port)
			.credentials(Credentials::new(smtp_config.username, smtp_config.password))
			.build();

		Ok(Self {
			subject: email_content.subject,
			body_template: email_content.body_template,
			sender: email_content.sender,
			recipients: email_content.recipients,
			client,
		})
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

				Self::new(smtp_config, email_content).ok()
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
	/// * `Result<(), NotificationError>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), NotificationError> {
		let recipients_str = self
			.recipients
			.iter()
			.map(ToString::to_string)
			.collect::<Vec<_>>()
			.join(", ");

		let mailboxes: Mailboxes = recipients_str.parse().map_err(|e| {
			NotificationError::internal_error(format!("Failed to parse email recipients: {}", e))
		})?;
		let recipients_header: header::To = mailboxes.into();

		let email = Message::builder()
			.mailbox(recipients_header)
			.from(self.sender.to_string().parse().map_err(|e| {
				NotificationError::internal_error(format!("Failed to parse email sender: {}", e))
			})?)
			.reply_to(self.sender.to_string().parse().map_err(|e| {
				NotificationError::internal_error(format!("Failed to parse email sender: {}", e))
			})?)
			.subject(&self.subject)
			.header(ContentType::TEXT_PLAIN)
			.body(message.to_owned())
			.map_err(|e| {
				NotificationError::internal_error(format!("Failed to build email: {}", e))
			})?;

		self.client
			.send(&email)
			.map_err(|e| NotificationError::network_error(e.to_string()))?;

		Ok(())
	}
}
