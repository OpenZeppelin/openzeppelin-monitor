use openzeppelin_monitor::services::notification::{Notifier, WebhookNotifier};
use serde_json::json;
use std::collections::HashMap;

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
