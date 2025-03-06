use openzeppelin_monitor::{
	models::{
		EVMMonitorMatch, EVMTransaction, MatchConditions, Monitor, MonitorMatch,
		NotificationMessage, Trigger, TriggerType, TriggerTypeConfig,
	},
	services::notification::{NotificationService, Notifier, WebhookNotifier},
};
use serde_json::json;
use std::collections::HashMap;
use web3::types::{H160, U256};

fn create_test_monitor(name: &str) -> Monitor {
	Monitor {
		name: name.to_string(),
		networks: vec!["ethereum_mainnet".to_string()],
		paused: false,
		triggers: vec!["test_trigger".to_string()],
		..Default::default()
	}
}

fn create_test_evm_match(monitor: Monitor) -> MonitorMatch {
	let transaction = EVMTransaction::from(web3::types::Transaction {
		from: Some(H160::default()),
		to: Some(H160::default()),
		value: U256::default(),
		..Default::default()
	});

	MonitorMatch::EVM(Box::new(EVMMonitorMatch {
		monitor,
		transaction,
		receipt: web3::types::TransactionReceipt::default(),
		matched_on: MatchConditions::default(),
		matched_on_args: None,
	}))
}

#[tokio::test]
async fn test_webhook_notification_success() {
	// Setup async mock server
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("GET", "/")
		.match_body(mockito::Matcher::Json(json!({
			"title": "Test Alert",
			"body": "Test message with value 42"
		})))
		.with_status(200)
		.create_async()
		.await;

	let notifier = WebhookNotifier::new(
		server.url(),
		"Test Alert".to_string(),
		"Test message with value ${value}".to_string(),
		Some("GET".to_string()),
		None,
		None,
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
async fn test_webhook_notification_failure() {
	// Setup async mock server to simulate failure
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("GET", "/")
		.with_status(500)
		.with_body("Internal Server Error")
		.create_async()
		.await;

	let notifier = WebhookNotifier::new(
		server.url(),
		"Test Alert".to_string(),
		"Test message".to_string(),
		Some("GET".to_string()),
		None,
		None,
	)
	.unwrap();

	let result = notifier.notify("Test message").await;

	assert!(result.is_err());
	mock.assert();
}

#[tokio::test]
async fn test_notification_service_webhook_execution() {
	let notification_service = NotificationService::new();
	let mut server = mockito::Server::new_async().await;

	// Setup mock webhook server with less strict matching
	let mock = server
		.mock("GET", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.create_async()
		.await;

	// Create a webhook trigger
	let trigger = Trigger {
		name: "test_trigger".to_string(),
		trigger_type: TriggerType::Webhook,
		config: TriggerTypeConfig::Webhook {
			url: server.url(),
			method: Some("GET".to_string()),
			headers: None,
			secret: None,
			message: NotificationMessage {
				title: "Test Alert".to_string(),
				body: "Test message ${value}".to_string(),
			},
		},
	};

	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());
	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));

	let result = notification_service
		.execute(&trigger, variables, &monitor_match, &HashMap::new())
		.await;

	assert!(result.is_ok());
	mock.assert();
}

#[tokio::test]
async fn test_notification_service_webhook_execution_failure() {
	let notification_service = NotificationService::new();
	let mut server = mockito::Server::new_async().await;

	// Setup mock webhook server with less strict matching
	let mock = server
		.mock("GET", "/")
		.with_status(500)
		.with_header("content-type", "application/json")
		.create_async()
		.await;

	let trigger = Trigger {
		name: "test_trigger".to_string(),
		trigger_type: TriggerType::Webhook,
		config: TriggerTypeConfig::Webhook {
			url: server.url(),
			method: Some("GET".to_string()),
			headers: None,
			secret: None,
			message: NotificationMessage {
				title: "Test Alert".to_string(),
				body: "Test message".to_string(),
			},
		},
	};

	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));

	let result = notification_service
		.execute(&trigger, HashMap::new(), &monitor_match, &HashMap::new())
		.await;

	assert!(result.is_err());
	mock.assert();
}
