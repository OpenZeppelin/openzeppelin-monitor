use async_trait::async_trait;
use mockall::mock;

use email_address::EmailAddress;
use lettre::{address::Envelope, Message, Transport};
use mockall::predicate::*;
use std::collections::HashMap;

use openzeppelin_monitor::services::notification::{
	EmailContent, EmailNotifier, NotificationError, Notifier, SmtpConfig,
};

mock! {
	pub EmailNotifier {
		pub fn new(smtp_config: SmtpConfig, email_content: EmailContent) -> Result<Self, NotificationError>;
		pub fn format_message(&self, variables: &HashMap<String, String>) -> String;
	}

	#[async_trait]
	impl Notifier for EmailNotifier {
		async fn notify(&self, message: &str) -> Result<(), NotificationError>;
	}
}

mock! {
	pub SmtpTransport {}

	impl Transport for SmtpTransport {
		type Ok = String;
		type Error = String;

		fn send_raw(&self, envelope: &Envelope, email: &[u8]) -> Result<String, String> {
			Ok("250 OK".to_string())
		}

		fn send(&self, message: &Message) -> Result<String, String> {
			Ok("250 OK".to_string())
		}
	}
}

#[tokio::test]
async fn test_email_notification_success() {
	let email_content = EmailContent {
		subject: "Test".to_string(),
		body_template: "Test message".to_string(),
		sender: EmailAddress::new_unchecked("sender@test.com"),
		recipients: vec![EmailAddress::new_unchecked("recipient@test.com")],
	};

	let mut mock_transport = MockSmtpTransport::new();

	mock_transport
		.expect_send()
		.times(1)
		.returning(|_| Ok("250 OK".to_string()));

	let notifier = EmailNotifier::with_transport(email_content, mock_transport);

	let result = notifier.notify("Test message").await;
	assert!(result.is_ok());
}

#[tokio::test]
async fn test_email_notification_failure() {
	let email_content = EmailContent {
		subject: "Test".to_string(),
		body_template: "Test message".to_string(),
		sender: EmailAddress::new_unchecked("sender@test.com"),
		recipients: vec![EmailAddress::new_unchecked("recipient@test.com")],
	};

	let mut mock_transport = MockSmtpTransport::new();

	mock_transport
		.expect_send()
		.times(1)
		.returning(|_| Err("500 Internal Server Error".to_string()));

	let notifier = EmailNotifier::with_transport(email_content, mock_transport);

	let result = notifier.notify("Test message").await;
	assert!(result.is_err());
}
