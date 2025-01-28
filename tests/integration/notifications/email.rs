use async_trait::async_trait;
use mockall::mock;

use email_address::EmailAddress;
use lettre::Message;
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

// SmtpTransport mock to match the actual trait implementation
mock! {
	pub SmtpTransport {
		fn send(&self, email: &Message) -> Result<(), lettre::transport::smtp::Error>;
	}
}

#[tokio::test]
async fn test_email_notification_success() {
	let smtp_config = SmtpConfig {
		host: "smtp.test.com".to_string(),
		port: 587,
		username: "test".to_string(),
		password: "test".to_string(),
	};

	let email_content = EmailContent {
		subject: "Test".to_string(),
		body_template: "Test message".to_string(),
		sender: EmailAddress::new_unchecked("sender@test.com"),
		recipients: vec![EmailAddress::new_unchecked("recipient@test.com")],
	};

	// let mut mock_transport = SmtpTransport::unencrypted_localhost();
	// mock_transport.expect_send().times(1).returning(|_| Ok(()));

	// mock_transport
	// 	.expect_test_connection()
	// 	.times(1)
	// 	.returning(|| Ok(()));

	// Create EmailNotifier with the mock transport
	let notifier = EmailNotifier::new(smtp_config, email_content).unwrap();

	let result = notifier.notify("Test message").await;
	assert!(result.is_ok());
}
