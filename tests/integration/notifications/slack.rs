use openzeppelin_monitor::{
	models::{EVMMonitorMatch, MatchConditions, Monitor, MonitorMatch, ScriptLanguage},
	services::notification::{NotificationService, Notifier, SlackNotifier},
	utils::tests::{
		evm::{monitor::MonitorBuilder, transaction::TransactionBuilder},
		trigger::TriggerBuilder,
	},
};
use serde_json::json;
use std::collections::HashMap;

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

fn create_test_trigger_scripts() -> HashMap<String, (ScriptLanguage, String)> {
	let mut scripts = HashMap::new();
	scripts.insert(
		"test_monitor|test_script.py".to_string(),
		(ScriptLanguage::Python, "print(True)".to_string()),
	);
	scripts
}

#[tokio::test]
async fn test_slack_notification_success() {
	// Setup async mock server
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Json(json!({
			"blocks": [
				{
					"type": "section",
					"text": {
						"type": "mrkdwn",
						"text": "*Test Alert*\n\nTest message with value 42"
					}
				}
			]
		})))
		.with_status(200)
		.create_async()
		.await;

	let notifier = SlackNotifier::new(
		server.url(),
		"Test Alert".to_string(),
		"Test message with value ${value}".to_string(),
	)
	.unwrap();

	// Prepare and send test message
	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());
	let message = notifier.format_message(&variables);

	let result = notifier.notify(&message).await;
	assert!(result.is_ok());
	mock.assert();
}

#[tokio::test]
async fn test_slack_notification_failure() {
	// Setup async mock server to simulate failure
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("POST", "/")
		.with_status(500)
		.with_body("Internal Server Error")
		.create_async()
		.await;

	let notifier = SlackNotifier::new(
		server.url(),
		"Test Alert".to_string(),
		"Test message".to_string(),
	)
	.unwrap();

	let result = notifier.notify("Test message").await;

	assert!(result.is_err());
	mock.assert();
}

/// Test the notification service with a slack trigger
#[tokio::test]
async fn test_notification_service_slack_execution_success() {
	let notification_service = NotificationService::new();
	// Setup async mock server to simulate failure
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Json(json!({
			"blocks": [
				{
					"type": "section",
					"text": {
						"type": "mrkdwn",
						"text": "*Test Alert*\n\nTest message with value 42"
					}
				}
			]
		})))
		.with_status(200)
		.create_async()
		.await;

	// Create a slack trigger
	let trigger = TriggerBuilder::new()
		.name("test_trigger")
		.slack(&server.url())
		.message("Test Alert", "Test message with value ${value}")
		.build();

	// Prepare and send test message
	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());

	// Create monitor match and trigger scripts (needed for function signature)
	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));
	let trigger_scripts = create_test_trigger_scripts();

	// Execute the notification
	let result = notification_service
		.execute(&trigger, &variables, &monitor_match, &trigger_scripts)
		.await;

	assert!(result.is_ok());
	mock.assert();
}

#[tokio::test]
async fn test_notification_service_slack_execution_failure() {
	let notification_service = NotificationService::new();
	// Setup async mock server to simulate failure
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("POST", "/")
		.with_status(500)
		.with_body("Internal Server Error")
		.create_async()
		.await;

	// Create a slack trigger
	let trigger = TriggerBuilder::new()
		.name("test_trigger")
		.slack(&server.url())
		.message("Test Alert", "Test message with value ${value}")
		.build();

	// Prepare and send test message
	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());

	// Create monitor match and trigger scripts (needed for function signature)
	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));
	let trigger_scripts = create_test_trigger_scripts();

	// Execute the notification
	let result = notification_service
		.execute(&trigger, &variables, &monitor_match, &trigger_scripts)
		.await;

	assert!(result.is_err());
	mock.assert();
}
