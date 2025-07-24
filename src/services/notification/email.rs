//! Email notification implementation.
//!
//! Provides functionality to send formatted messages to email addresses
//! via SMTP, supporting message templates with variable substitution.

use backon::{BackoffBuilder, ExponentialBuilder, Retryable};
use email_address::EmailAddress;
use lettre::{
	message::{
		header::{self, ContentType},
		Mailbox, Mailboxes,
	},
	transport::smtp::Error as SmtpError,
	AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use pulldown_cmark::{html, Options, Parser};
use std::{collections::HashMap, error::Error as StdError, sync::Arc};

use crate::{
	models::TriggerTypeConfig,
	services::notification::NotificationError,
	utils::{JitterSetting, RetryConfig},
};

/// Implementation of email notifications via SMTP
#[derive(Debug)]
pub struct EmailNotifier<T: AsyncTransport + Send + Sync> {
	/// Email subject
	subject: String,
	/// Message template with variable placeholders
	body_template: String,
	/// SMTP client for email delivery
	client: Arc<T>,
	/// Email sender
	sender: EmailAddress,
	/// Email recipients
	recipients: Vec<EmailAddress>,
	/// Retry policy for SMTP requests
	retry_policy: RetryConfig,
}

/// Configuration for SMTP connection
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
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

// This implementation is only for testing purposes
impl<T: AsyncTransport + Send + Sync> EmailNotifier<T>
where
	T::Ok: Send + Sync,
	T::Error: StdError + Send + Sync + 'static,
{
	/// Creates a new email notifier instance with a custom transport
	///
	/// # Arguments
	/// * `email_content` - Email content configuration
	/// * `transport` - SMTP transport
	/// * `retry_policy` - Retry policy for SMTP requests
	///
	/// # Returns
	/// * `Self` - Email notifier instance
	pub fn with_transport(
		email_content: EmailContent,
		transport: T,
		retry_policy: RetryConfig,
	) -> Self {
		Self {
			subject: email_content.subject,
			body_template: email_content.body_template,
			sender: email_content.sender,
			recipients: email_content.recipients,
			client: Arc::new(transport),
			retry_policy,
		}
	}

	/// Sends a formatted message to email
	///
	/// # Arguments
	/// * `message` - The formatted message to send
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	pub async fn notify(&self, message: &str) -> Result<(), NotificationError> {
		let recipients_str = self
			.recipients
			.iter()
			.map(ToString::to_string)
			.collect::<Vec<_>>()
			.join(", ");

		let mailboxes: Mailboxes = recipients_str.parse::<Mailboxes>().map_err(|e| {
			NotificationError::notify_failed(
				format!("Failed to parse recipients: {}", e),
				Some(e.into()),
				None,
			)
		})?;
		let recipients_header: header::To = mailboxes.into();

		let email = Message::builder()
			.mailbox(recipients_header)
			.from(self.sender.to_string().parse::<Mailbox>().map_err(|e| {
				NotificationError::notify_failed(
					format!("Failed to parse sender: {}", e),
					Some(e.into()),
					None,
				)
			})?)
			.reply_to(self.sender.to_string().parse::<Mailbox>().map_err(|e| {
				NotificationError::notify_failed(
					format!("Failed to parse reply-to: {}", e),
					Some(e.into()),
					None,
				)
			})?)
			.subject(&self.subject)
			.header(ContentType::TEXT_HTML)
			.body(message.to_owned())
			.map_err(|e| {
				NotificationError::notify_failed(
					format!("Failed to build email message: {}", e),
					Some(e.into()),
					None,
				)
			})?;

		let operation = || async {
			self.client.send(email.clone()).await.map_err(|e| {
				NotificationError::notify_failed(
					format!("Failed to send email: {}", e),
					Some(Box::new(e)),
					None,
				)
			})?;

			Ok(())
		};

		let backoff = ExponentialBuilder::default()
			.with_min_delay(self.retry_policy.initial_backoff)
			.with_max_delay(self.retry_policy.max_backoff);

		let backoff_with_jitter = match self.retry_policy.jitter {
			JitterSetting::Full => backoff.with_jitter(),
			JitterSetting::None => backoff,
		};

		// Retry if the error is SmtpError and not permanent
		let should_retry = |e: &NotificationError| -> bool {
			if let NotificationError::NotifyFailed(context) = e {
				if let Some(source) = context.source() {
					if let Some(smtp_error) = source.downcast_ref::<SmtpError>() {
						return !smtp_error.is_permanent();
					}
				}
			}
			true
		};

		operation
			.retry(
				backoff_with_jitter
					.build()
					.take(self.retry_policy.max_retries as usize),
			)
			.when(should_retry)
			.await
	}
}

impl EmailNotifier<AsyncSmtpTransport<Tokio1Executor>> {
	/// Creates a new email notifier instance
	///
	/// # Arguments
	/// * `smtp_client` - SMTP client
	/// * `email_content` - Email content configuration
	///
	/// # Returns
	/// * `Result<Self, NotificationError>` - Email notifier instance or error
	pub fn new(
		smtp_client: Arc<AsyncSmtpTransport<Tokio1Executor>>,
		email_content: EmailContent,
		retry_policy: RetryConfig,
	) -> Result<Self, NotificationError> {
		Ok(Self {
			subject: email_content.subject,
			body_template: email_content.body_template,
			sender: email_content.sender,
			recipients: email_content.recipients,
			client: smtp_client,
			retry_policy,
		})
	}

	/// Returns the body template of the email.
	pub fn body_template(&self) -> &str {
		&self.body_template
	}

	/// Formats a message by substituting variables in the template and converts it to HTML
	/// Method is static because property-based tests do not have tokio runtime available,
	/// which is required for AsyncSmtpTransport
	///
	/// # Arguments
	/// * `variables` - Map of variable names to values
	///
	/// # Returns
	/// * `String` - Formatted message with variables replaced and converted to HTML
	pub fn format_message(body_template: &str, variables: &HashMap<String, String>) -> String {
		let formatted_message = variables
			.iter()
			.fold(body_template.to_string(), |message, (key, value)| {
				message.replace(&format!("${{{}}}", key), value)
			});

		Self::markdown_to_html(&formatted_message)
	}

	/// Convert a Markdown string into HTML
	pub fn markdown_to_html(md: &str) -> String {
		// enable all the extensions you like; or just Parser::new(md) for pure CommonMark
		let opts = Options::all();
		let parser = Parser::new_ext(md, opts);

		let mut html_out = String::new();
		html::push_html(&mut html_out, parser);
		html_out
	}

	/// Creates an email notifier from a trigger configuration
	///
	/// # Arguments
	/// * `config` - Trigger configuration containing email parameters
	///
	/// # Returns
	/// * `Result<Self, NotificationError>` - Notifier instance if config is email type
	pub fn from_config(
		config: &TriggerTypeConfig,
		smtp_client: Arc<AsyncSmtpTransport<Tokio1Executor>>,
	) -> Result<Self, NotificationError> {
		if let TriggerTypeConfig::Email {
			message,
			sender,
			recipients,
			retry_policy,
			..
		} = config
		{
			let email_content = EmailContent {
				subject: message.title.clone(),
				body_template: message.body.clone(),
				sender: sender.clone(),
				recipients: recipients.clone(),
			};

			Self::new(smtp_client, email_content, retry_policy.clone())
		} else {
			Err(NotificationError::config_error(
				format!("Invalid email configuration: {:?}", config),
				None,
				None,
			))
		}
	}
}

#[cfg(test)]
mod tests {
	use lettre::transport::{smtp::authentication::Credentials, stub::AsyncStubTransport};

	use crate::{
		models::{NotificationMessage, SecretString, SecretValue},
		services::notification::pool::NotificationClientPool,
		utils::RetryConfig,
	};

	use super::*;

	fn create_test_email_content() -> EmailContent {
		EmailContent {
			subject: "Test Subject".to_string(),
			body_template: "Hello ${name}, your balance is ${balance}".to_string(),
			sender: "sender@test.com".parse().unwrap(),
			recipients: vec!["recipient@test.com".parse().unwrap()],
		}
	}

	fn create_test_notifier() -> EmailNotifier<AsyncSmtpTransport<Tokio1Executor>> {
		let smtp_config = SmtpConfig {
			host: "dummy.smtp.com".to_string(),
			port: 465,
			username: "test".to_string(),
			password: "test".to_string(),
		};

		let client = AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_config.host)
			.unwrap()
			.port(smtp_config.port)
			.credentials(Credentials::new(smtp_config.username, smtp_config.password))
			.build();

		let email_content = create_test_email_content();

		EmailNotifier::new(Arc::new(client), email_content, RetryConfig::default()).unwrap()
	}

	fn create_test_email_config(port: Option<u16>) -> TriggerTypeConfig {
		TriggerTypeConfig::Email {
			host: "smtp.test.com".to_string(),
			port,
			username: SecretValue::Plain(SecretString::new("testuser".to_string())),
			password: SecretValue::Plain(SecretString::new("testpass".to_string())),
			message: NotificationMessage {
				title: "Test Subject".to_string(),
				body: "Hello ${name}".to_string(),
			},
			sender: "sender@test.com".parse().unwrap(),
			recipients: vec!["recipient@test.com".parse().unwrap()],
			retry_policy: RetryConfig::default(),
		}
	}

	////////////////////////////////////////////////////////////
	// format_message tests
	////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_format_message_basic_substitution() {
		let notifier = create_test_notifier();
		let mut variables = HashMap::new();
		variables.insert("name".to_string(), "Alice".to_string());
		variables.insert("balance".to_string(), "100".to_string());

		let result = EmailNotifier::format_message(notifier.body_template(), &variables);
		let expected_result = "<p>Hello Alice, your balance is 100</p>\n";
		assert_eq!(result, expected_result);
	}

	#[tokio::test]
	async fn test_format_message_missing_variable() {
		let notifier = create_test_notifier();
		let mut variables = HashMap::new();
		variables.insert("name".to_string(), "Bob".to_string());

		let result = EmailNotifier::format_message(notifier.body_template(), &variables);
		let expected_result = "<p>Hello Bob, your balance is ${balance}</p>\n";
		assert_eq!(result, expected_result);
	}

	#[tokio::test]
	async fn test_format_message_empty_variables() {
		let notifier = create_test_notifier();
		let variables = HashMap::new();

		let result = EmailNotifier::format_message(notifier.body_template(), &variables);
		let expected_result = "<p>Hello ${name}, your balance is ${balance}</p>\n";
		assert_eq!(result, expected_result);
	}

	#[tokio::test]
	async fn test_format_message_with_empty_values() {
		let notifier = create_test_notifier();
		let mut variables = HashMap::new();
		variables.insert("name".to_string(), "".to_string());
		variables.insert("balance".to_string(), "".to_string());

		let result = EmailNotifier::format_message(notifier.body_template(), &variables);
		let expected_result = "<p>Hello , your balance is</p>\n";
		assert_eq!(result, expected_result);
	}

	////////////////////////////////////////////////////////////
	// from_config tests
	////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_from_config_valid_email_config() {
		let config = create_test_email_config(Some(587));
		let smtp_config = match &config {
			TriggerTypeConfig::Email {
				host,
				port,
				username,
				password,
				..
			} => SmtpConfig {
				host: host.clone(),
				port: port.unwrap_or(587),
				username: username.to_string(),
				password: password.to_string(),
			},
			_ => panic!("Expected Email config"),
		};
		let pool = NotificationClientPool::new();
		let smtp_client = pool.get_or_create_smtp_client(&smtp_config).await.unwrap();
		let notifier = EmailNotifier::from_config(&config, smtp_client);
		assert!(notifier.is_ok());

		let notifier = notifier.unwrap();
		assert_eq!(notifier.subject, "Test Subject");
		assert_eq!(notifier.body_template, "Hello ${name}");
		assert_eq!(notifier.sender.to_string(), "sender@test.com");
		assert_eq!(notifier.recipients.len(), 1);
		assert_eq!(notifier.recipients[0].to_string(), "recipient@test.com");
	}

	#[tokio::test]
	async fn test_from_config_invalid_type() {
		// Create a config that is not Email type
		let wrong_config = TriggerTypeConfig::Slack {
			slack_url: SecretValue::Plain(SecretString::new(
				"https://slack.com/api/chat.postMessage".to_string(),
			)),
			message: NotificationMessage {
				title: "Test Slack".to_string(),
				body: "Hello ${name}".to_string(),
			},
			retry_policy: RetryConfig::default(),
		};

		// Correct config to create SmtpTransport
		let smtp_config = SmtpConfig {
			host: "dummy.smtp.com".to_string(),
			port: 465,
			username: "test".to_string(),
			password: "test".to_string(),
		};

		let smtp_client = Arc::new(
			AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_config.host)
				.unwrap()
				.port(smtp_config.port)
				.credentials(Credentials::new(smtp_config.username, smtp_config.password))
				.build(),
		);

		let result = EmailNotifier::from_config(&wrong_config, smtp_client);
		assert!(result.is_err());
		assert!(matches!(
			result.unwrap_err(),
			NotificationError::ConfigError(_)
		));
	}

	#[tokio::test]
	async fn test_from_config_default_port() {
		let config = create_test_email_config(None);
		let smtp_config = match &config {
			TriggerTypeConfig::Email {
				host,
				port,
				username,
				password,
				..
			} => SmtpConfig {
				host: host.clone(),
				port: port.unwrap_or(587),
				username: username.to_string(),
				password: password.to_string(),
			},
			_ => panic!("Expected Email config"),
		};
		let pool = NotificationClientPool::new();
		let smtp_client = pool.get_or_create_smtp_client(&smtp_config).await.unwrap();
		let notifier = EmailNotifier::from_config(&config, smtp_client);
		assert!(notifier.is_ok());
	}

	////////////////////////////////////////////////////////////
	// notify tests
	////////////////////////////////////////////////////////////
	#[tokio::test]
	async fn test_notify_succeeds_on_first_try() {
		let transport = AsyncStubTransport::new_ok();
		let notifier = EmailNotifier::with_transport(
			create_test_email_content(),
			transport.clone(),
			RetryConfig::default(),
		);

		notifier.notify("test message").await.unwrap();
		assert_eq!(transport.messages().await.len(), 1);
	}

	#[tokio::test]
	async fn test_notify_fails_after_all_retries() {
		let transport = AsyncStubTransport::new_error();
		let retry_policy = RetryConfig::default();
		let default_max_retries = retry_policy.max_retries as usize;
		let notifier = EmailNotifier::with_transport(
			create_test_email_content(),
			transport.clone(),
			retry_policy,
		);

		let result = notifier.notify("test message").await;
		assert!(result.is_err());
		assert_eq!(
			transport.messages().await.len(),
			1 + default_max_retries,
			"Should be called 1 time + default max retries"
		);
	}
}
