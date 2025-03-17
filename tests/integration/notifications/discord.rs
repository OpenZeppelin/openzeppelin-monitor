use openzeppelin_monitor::{
	models::{
		BlockChainType, EVMMonitorMatch, MatchConditions, Monitor, MonitorMatch,
		NotificationMessage, TransactionType, Trigger, TriggerType, TriggerTypeConfig,
	},
	services::notification::{DiscordNotifier, NotificationService, Notifier},
};
use serde_json::json;
use std::collections::HashMap;

use crate::integration::mocks::{create_test_evm_transaction_receipt, create_test_transaction};

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
	let transaction = match create_test_transaction(BlockChainType::EVM) {
		TransactionType::EVM(transaction) => transaction,
		_ => panic!("Failed to create test transaction"),
	};

	MonitorMatch::EVM(Box::new(EVMMonitorMatch {
		monitor,
		transaction,
		receipt: create_test_evm_transaction_receipt(),
		matched_on: MatchConditions::default(),
		matched_on_args: None,
	}))
}

#[tokio::test]
async fn test_discord_notification_success() {
	// Setup async mock server
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Json(json!({
			"content": "*Test Alert*\n\nTest message with value 42",
			"username": null,
			"avatar_url": null,
			"embeds": null,
		})))
		.with_status(200)
		.create_async()
		.await;

	let notifier = DiscordNotifier::new(
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
async fn test_discord_notification_failure() {
	// Setup async mock server to simulate failure
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("POST", "/")
		.with_status(500)
		.with_body("Internal Server Error")
		.create_async()
		.await;

	let notifier = DiscordNotifier::new(
		server.url(),
		"Test Alert".to_string(),
		"Test message".to_string(),
	)
	.unwrap();

	let result = notifier.notify("Test message").await;

	assert!(result.is_err());
	mock.assert();
}

#[tokio::test]
async fn test_notification_service_discord_execution() {
	let notification_service = NotificationService::new();
	let mut server = mockito::Server::new_async().await;

	// Setup mock Discord webhook server
	let mock = server
		.mock("POST", "/")
		.with_status(200)
		.create_async()
		.await;

	// Create a Discord trigger
	let trigger = Trigger {
		name: "test_trigger".to_string(),
		trigger_type: TriggerType::Discord,
		config: TriggerTypeConfig::Discord {
			discord_url: server.url(),
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
async fn test_notification_service_discord_execution_failure() {
	let notification_service = NotificationService::new();
	let mut server = mockito::Server::new_async().await;

	// Setup mock Discord webhook server to simulate failure
	let mock = server
		.mock("POST", "/")
		.with_status(500)
		.with_body("Internal Server Error")
		.create_async()
		.await;

	let trigger = Trigger {
		name: "test_trigger".to_string(),
		trigger_type: TriggerType::Discord,
		config: TriggerTypeConfig::Discord {
			discord_url: server.url(),
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
