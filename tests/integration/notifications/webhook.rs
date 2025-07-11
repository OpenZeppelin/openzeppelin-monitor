use openzeppelin_monitor::{
	models::{EVMMonitorMatch, MatchConditions, Monitor, MonitorMatch, TriggerType},
	services::notification::{
		NotificationError, NotificationService, Notifier, WebhookConfig, WebhookNotifier,
	},
	utils::{
		tests::{
			evm::{monitor::MonitorBuilder, transaction::TransactionBuilder},
			get_http_client_from_notification_pool,
			trigger::TriggerBuilder,
		},
		RetryConfig,
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

	let config = WebhookConfig {
		url: server.url(),
		url_params: None,
		title: "Test Alert".to_string(),
		body_template: "Test message with value ${value}".to_string(),
		method: Some("GET".to_string()),
		secret: None,
		headers: None,
		payload_fields: None,
	};
	let http_client = get_http_client_from_notification_pool().await;
	let notifier = WebhookNotifier::new(config, http_client).unwrap();

	// Prepare and send test message
	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());
	let message = notifier.format_message(&variables);

	let result = notifier.notify(&message).await;

	assert!(result.is_ok());
	mock.assert();
}

#[tokio::test]
async fn test_webhook_notification_failure_retryable_error() {
	// Setup async mock server to simulate failure
	let mut server = mockito::Server::new_async().await;
	let default_retries_count = RetryConfig::default().max_retries as usize;
	let mock = server
		.mock("GET", "/")
		.with_status(500)
		.with_body("Internal Server Error")
		.expect(1 + default_retries_count)
		.create_async()
		.await;

	let config = WebhookConfig {
		url: server.url(),
		url_params: None,
		title: "Test Alert".to_string(),
		body_template: "Test message".to_string(),
		method: Some("GET".to_string()),
		secret: None,
		headers: None,
		payload_fields: None,
	};
	let http_client = get_http_client_from_notification_pool().await;
	let notifier = WebhookNotifier::new(config, http_client).unwrap();

	let result = notifier.notify("Test message").await;

	assert!(result.is_err());
	mock.assert();
}

#[tokio::test]
async fn test_webhook_notification_failure_non_retryable_error() {
	// Setup async mock server to simulate failure
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("GET", "/")
		.with_status(400)
		.with_body("Bad Request")
		.expect(1) // 1 initial call, no retries for non-retryable
		.create_async()
		.await;

	let config = WebhookConfig {
		url: server.url(),
		url_params: None,
		title: "Test Alert".to_string(),
		body_template: "Test message".to_string(),
		method: Some("GET".to_string()),
		secret: None,
		headers: None,
		payload_fields: None,
	};
	let http_client = get_http_client_from_notification_pool().await;
	let notifier = WebhookNotifier::new(config, http_client).unwrap();

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
	let trigger = TriggerBuilder::new()
		.name("test_trigger")
		.webhook(&server.url())
		.webhook_method("GET")
		.message("Test Alert", "Test message ${value}")
		.build();

	let mut variables = HashMap::new();
	variables.insert("value".to_string(), "42".to_string());
	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));

	let result = notification_service
		.execute(&trigger, &variables, &monitor_match, &HashMap::new())
		.await;

	assert!(result.is_ok());
	mock.assert();
}

#[tokio::test]
async fn test_notification_service_webhook_execution_failure() {
	let notification_service = NotificationService::new();
	let mut server = mockito::Server::new_async().await;
	let default_retries_count = RetryConfig::default().max_retries as usize;

	// Setup mock webhook server with less strict matching
	let mock = server
		.mock("GET", "/")
		.with_status(500)
		.with_header("content-type", "application/json")
		.expect(1 + default_retries_count)
		.create_async()
		.await;

	let trigger = TriggerBuilder::new()
		.name("test_trigger")
		.webhook(&server.url())
		.webhook_method("GET")
		.message("Test Alert", "Test message")
		.build();

	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));

	let result = notification_service
		.execute(&trigger, &HashMap::new(), &monitor_match, &HashMap::new())
		.await;

	assert!(result.is_err());

	let error = result.unwrap_err();
	assert!(matches!(error, NotificationError::NotifyFailed(_)));

	mock.assert();
}

#[tokio::test]
async fn test_notification_service_webhook_execution_invalid_config() {
	let notification_service = NotificationService::new();

	let trigger = TriggerBuilder::new()
		.name("test_trigger")
		.slack("")
		.message("Test Alert", "Test message")
		.trigger_type(TriggerType::Webhook)
		.build();

	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));

	let result = notification_service
		.execute(&trigger, &HashMap::new(), &monitor_match, &HashMap::new())
		.await;

	// Verify we get the specific "Invalid webhook configuration" error
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Invalid webhook configuration"));
}

#[tokio::test]
async fn test_notify_with_payload_merges_default_fields() {
	let mut server = mockito::Server::new_async().await;
	let expected_payload = json!({
		"default_field": "default_value",
		"custom_field": "custom_value"
	});

	let mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Json(expected_payload))
		.with_status(200)
		.create_async()
		.await;

	let config = WebhookConfig {
		url: server.url(),
		url_params: None,
		title: "Test".to_string(),
		body_template: "Test message".to_string(),
		method: Some("POST".to_string()),
		secret: None,
		headers: None,
		payload_fields: Some(HashMap::from([(
			"default_field".to_string(),
			serde_json::json!("default_value"),
		)])),
	};
	let http_client = get_http_client_from_notification_pool().await;
	let notifier = WebhookNotifier::new(config, http_client).unwrap();

	let mut payload = HashMap::new();
	payload.insert(
		"custom_field".to_string(),
		serde_json::json!("custom_value"),
	);

	let result = notifier.notify_with_payload("Test message", payload).await;
	assert!(result.is_ok());
	mock.assert();
}
