use async_trait::async_trait;
use mockall::mock;

use email_address::EmailAddress;
use lettre::{address::Envelope, Message, Transport};
use mockall::predicate::*;
use std::collections::HashMap;

use openzeppelin_monitor::{
	models::{
		EVMMonitorMatch, MatchConditions, Monitor, MonitorMatch, NotificationMessage, SecretString,
		SecretValue, TriggerType, TriggerTypeConfig,
	},
	services::notification::{
		EmailContent, EmailNotifier, NotificationError, NotificationService, Notifier, SmtpConfig,
	},
	utils::tests::{
		evm::{monitor::MonitorBuilder, transaction::TransactionBuilder},
		trigger::TriggerBuilder,
	},
};

use crate::integration::mocks::{create_test_evm_logs, create_test_evm_transaction_receipt};

fn create_test_monitor(name: &str) -> Monitor {
	MonitorBuilder::new()
		.name(name)
		.networks(vec!["ethereum_mainnet".to_string()])
		.paused(false)
		.triggers(vec!["test_trigger".to_string()])
		.build()
}

fn create_test_evm_match(monitor: Monitor) -> MonitorMatch {
	let transaction = TransactionBuilder::new().build();

	MonitorMatch::EVM(Box::new(EVMMonitorMatch {
		monitor,
		transaction,
		receipt: Some(create_test_evm_transaction_receipt()),
		logs: Some(create_test_evm_logs()),
		network_slug: "ethereum_mainnet".to_string(),
		matched_on: MatchConditions::default(),
		matched_on_args: None,
	}))
}

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

	let error = result.unwrap_err();
	assert!(matches!(error, NotificationError::NotifyFailed(_)));
}

#[tokio::test]
async fn test_notification_service_email_execution_failure() {
	let notification_service = NotificationService::new();

	let trigger_config = TriggerTypeConfig::Email {
		host: "dummy.smtp.host.invalid".to_string(), // Will cause SmtpTransport to fail connection
		port: Some(587),
		username: SecretValue::Plain(SecretString::new("user".to_string())),
		password: SecretValue::Plain(SecretString::new("pass".to_string())),
		message: NotificationMessage {
			title: "Email Test Alert".to_string(),
			body: "Test email message with value ${value}".to_string(),
		},
		sender: "sender@example.com".parse().unwrap(),
		recipients: vec!["recipient@example.com".parse().unwrap()],
	};

	let trigger = TriggerBuilder::new()
		.name("test_email_trigger_service")
		.config(trigger_config)
		.trigger_type(TriggerType::Email)
		.build();

	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "123".to_string());
	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor_email"));

	let result = notification_service
		.execute(&trigger, &variables, &monitor_match, &HashMap::new())
		.await;

	assert!(
		result.is_err(),
		"Expected email notification to fail due to dummy SMTP host"
	);

	match result.unwrap_err() {
		NotificationError::NotifyFailed(ctx) => {
			assert!(ctx.message.contains("Failed to send email"));
		}
		e => panic!("Expected NotifyFailed, got {:?}", e),
	}
}
