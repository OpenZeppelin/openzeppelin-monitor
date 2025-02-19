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
pub struct EmailNotifier<T: Transport + Send + Sync> {
	/// Email subject
	subject: String,
	/// Message template with variable placeholders
	body_template: String,
	/// SMTP client for email delivery
	client: T,
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

impl<T: Transport + Send + Sync> EmailNotifier<T>
where
	T::Error: std::fmt::Display,
{
	/// Creates a new email notifier instance with a custom transport
	///
	/// # Arguments
	/// * `email_content` - Email content configuration
	/// * `transport` - SMTP transport
	///
	/// # Returns
	/// * `Self` - Email notifier instance
	pub fn with_transport(email_content: EmailContent, transport: T) -> Self {
		Self {
			subject: email_content.subject,
			body_template: email_content.body_template,
			sender: email_content.sender,
			recipients: email_content.recipients,
			client: transport,
		}
	}
}

impl EmailNotifier<SmtpTransport> {
	/// Creates a new email notifier instance
	///
	/// # Arguments
	/// * `smtp_config` - SMTP server configuration
	/// * `email_content` - Email content configuration
	///
	/// # Returns
	/// * `Result<Self, NotificationError>` - Email notifier instance or error
	pub fn new(
		smtp_config: SmtpConfig,
		email_content: EmailContent,
	) -> Result<Self, NotificationError> {
		let client = SmtpTransport::relay(&smtp_config.host)
			.unwrap()
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
				message,
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
					subject: message.title.clone(),
					body_template: message.body.clone(),
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
impl<T: Transport + Send + Sync> Notifier for EmailNotifier<T>
where
	T::Error: std::fmt::Display,
{
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

#[cfg(test)]
mod tests {
	use crate::models::NotificationMessage;

	use super::*;

	fn create_test_notifier() -> EmailNotifier<SmtpTransport> {
		let smtp_config = SmtpConfig {
			host: "dummy.smtp.com".to_string(),
			port: 465,
			username: "test".to_string(),
			password: "test".to_string(),
		};

		let email_content = EmailContent {
			subject: "Test Subject".to_string(),
			body_template: "Hello ${name}, your balance is ${balance}".to_string(),
			sender: "sender@test.com".parse().unwrap(),
			recipients: vec!["recipient@test.com".parse().unwrap()],
		};

		EmailNotifier::new(smtp_config, email_content).unwrap()
	}

	fn create_test_email_config(port: Option<u16>) -> TriggerTypeConfig {
		TriggerTypeConfig::Email {
			host: "smtp.test.com".to_string(),
			port,
			username: "testuser".to_string(),
			password: "testpass".to_string(),
			message: NotificationMessage {
				title: "Test Subject".to_string(),
				body: "Hello ${name}".to_string(),
			},
			sender: "sender@test.com".parse().unwrap(),
			recipients: vec!["recipient@test.com".parse().unwrap()],
		}
	}

	////////////////////////////////////////////////////////////
	// format_message tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_format_message_basic_substitution() {
		let notifier = create_test_notifier();
		let mut variables = HashMap::new();
		variables.insert("name".to_string(), "Alice".to_string());
		variables.insert("balance".to_string(), "100".to_string());

		let result = notifier.format_message(&variables);
		assert_eq!(result, "Hello Alice, your balance is 100");
	}

	#[test]
	fn test_format_message_missing_variable() {
		let notifier = create_test_notifier();
		let mut variables = HashMap::new();
		variables.insert("name".to_string(), "Bob".to_string());

		let result = notifier.format_message(&variables);
		assert_eq!(result, "Hello Bob, your balance is ${balance}");
	}

	#[test]
	fn test_format_message_empty_variables() {
		let notifier = create_test_notifier();
		let variables = HashMap::new();

		let result = notifier.format_message(&variables);
		assert_eq!(result, "Hello ${name}, your balance is ${balance}");
	}

	#[test]
	fn test_format_message_with_empty_values() {
		let notifier = create_test_notifier();
		let mut variables = HashMap::new();
		variables.insert("name".to_string(), "".to_string());
		variables.insert("balance".to_string(), "".to_string());

		let result = notifier.format_message(&variables);
		assert_eq!(result, "Hello , your balance is ");
	}

	////////////////////////////////////////////////////////////
	// from_config tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_from_config_valid_email_config() {
		let config = create_test_email_config(Some(587));

		let notifier = EmailNotifier::from_config(&config);
		assert!(notifier.is_some());

		let notifier = notifier.unwrap();
		assert_eq!(notifier.subject, "Test Subject");
		assert_eq!(notifier.body_template, "Hello ${name}");
		assert_eq!(notifier.sender.to_string(), "sender@test.com");
		assert_eq!(notifier.recipients.len(), 1);
		assert_eq!(notifier.recipients[0].to_string(), "recipient@test.com");
	}

	#[test]
	fn test_from_config_default_port() {
		let config = create_test_email_config(None);

		let notifier = EmailNotifier::from_config(&config);
		assert!(notifier.is_some());
	}

	////////////////////////////////////////////////////////////
	// notify tests
	////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_notify_failure() {
		let notifier = create_test_notifier();
		let result = notifier.notify("Test message").await;
		// Expected to fail since we're using a dummy SMTP server
		assert!(result.is_err());
	}
}
