use openzeppelin_monitor::{
	models::{EVMMonitorMatch, MatchConditions, Monitor, MonitorMatch, TriggerType},
	services::notification::{NotificationError, NotificationService, Notifier, TelegramNotifier},
	utils::{
		tests::{
			evm::{monitor::MonitorBuilder, transaction::TransactionBuilder},
			get_http_client_from_notification_pool,
			trigger::TriggerBuilder,
		},
		HttpRetryConfig,
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

#[tokio::test]
async fn test_telegram_notification_success() {
	// Setup async mock server
	let mut server = mockito::Server::new_async().await;

	let expected_payload = json!({
		"text": "*Test Alert* \n\nTest message with value 42",
		"chat_id": "test_chat_id",
		"parse_mode": "MarkdownV2",
		"disable_web_page_preview": false,
	});

	// Mock the Telegram API endpoint
	let mock = server
		.mock("POST", "/bottest_token/sendMessage")
		.match_header("content-type", "application/json")
		.match_body(mockito::Matcher::Json(expected_payload))
		.with_status(200)
		.with_body(r#"{"ok": true, "result": {}}"#)
		.create_async()
		.await;

	let notifier = TelegramNotifier::new(
		Some(server.url()),
		"test_token".to_string(),
		"test_chat_id".to_string(),
		None,
		"Test Alert".to_string(),
		"Test message with value ${value}".to_string(),
		get_http_client_from_notification_pool().await,
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
async fn test_telegram_notification_failure_retryable_error() {
	// Setup async mock server to simulate failure
	let mut server = mockito::Server::new_async().await;
	let default_retries_count = HttpRetryConfig::default().max_retries as usize;
	let mock = server
		.mock("POST", "/bottest_token/sendMessage")
		.with_status(500)
		.expect(1 + default_retries_count)
		.with_body("Internal Server Error")
		.create_async()
		.await;

	let notifier = TelegramNotifier::new(
		Some(server.url()),
		"test_token".to_string(),
		"test_chat_id".to_string(),
		None,
		"Test Alert".to_string(),
		"Test message with value ${value}".to_string(),
		get_http_client_from_notification_pool().await,
	)
	.unwrap();

	let result = notifier.notify("Test message").await;

	assert!(result.is_err());

	let error = result.unwrap_err();
	assert!(matches!(error, NotificationError::NotifyFailed(_)));

	mock.assert();
}

#[tokio::test]
async fn test_telegram_notification_failure_non_retryable_error() {
	// Setup async mock server to simulate failure
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("POST", "/bottest_token/sendMessage")
		.with_status(400)
		.expect(1) // 1 initial call, no retries for non-retryable status codes
		.with_body("Bad Request")
		.create_async()
		.await;

	let notifier = TelegramNotifier::new(
		Some(server.url()),
		"test_token".to_string(),
		"test_chat_id".to_string(),
		None,
		"Test Alert".to_string(),
		"Test message with value ${value}".to_string(),
		get_http_client_from_notification_pool().await,
	)
	.unwrap();

	let result = notifier.notify("Test message").await;

	assert!(result.is_err());

	let error = result.unwrap_err();
	assert!(matches!(error, NotificationError::NotifyFailed(_)));

	mock.assert();
}

#[tokio::test]
async fn test_notification_service_telegram_execution_failure() {
	let notification_service = NotificationService::new();

	let trigger = TriggerBuilder::new()
		.name("test_trigger")
		.telegram("random token", "random chat_id", true) // Should fail due to invalid token
		.trigger_type(TriggerType::Telegram)
		.message("Test Alert", "Test message")
		.build();

	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));

	let result = notification_service
		.execute(&trigger, &HashMap::new(), &monitor_match, &HashMap::new())
		.await;

	assert!(result.is_err());

	match result.unwrap_err() {
		NotificationError::NotifyFailed(_) => {}
		_ => panic!("Expected NotificationError::NotifyFailed variant"),
	}
}
