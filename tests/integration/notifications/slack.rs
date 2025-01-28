use openzeppelin_monitor::services::notification::{Notifier, SlackNotifier};
use serde_json::json;
use std::collections::HashMap;

#[tokio::test]
async fn test_slack_notification_success() {
	// Setup async mock server
	let mut server = mockito::Server::new_async().await;
	let mock = server
		.mock("POST", "/")
		.match_body(mockito::Matcher::Json(json!({
			"text": "*Test Alert*\n\nTest message with value 42"
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
