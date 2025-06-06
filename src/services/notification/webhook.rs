//! Webhook notification implementation.
//!
//! Provides functionality to send formatted messages to webhooks
//! via incoming webhooks, supporting message templates with variable substitution.

use async_trait::async_trait;
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{
	header::{HeaderMap, HeaderName, HeaderValue},
	Client, Method,
};
use serde::Serialize;
use sha2::Sha256;
use std::collections::HashMap;

use crate::{
	models::TriggerTypeConfig,
	services::notification::{NotificationError, Notifier},
};

/// HMAC SHA256 type alias
type HmacSha256 = Hmac<Sha256>;

/// Represents a webhook payload with additional fields
#[derive(Serialize, Debug)]
pub struct WebhookPayload {
	#[serde(flatten)]
	fields: HashMap<String, serde_json::Value>,
}

/// Represents a webhook configuration
#[derive(Clone)]
pub struct WebhookConfig {
	pub url: String,
	pub url_params: Option<HashMap<String, String>>,
	pub title: String,
	pub body_template: String,
	pub method: Option<String>,
	pub secret: Option<String>,
	pub headers: Option<HashMap<String, String>>,
	pub payload_fields: Option<HashMap<String, serde_json::Value>>,
}

/// Implementation of webhook notifications via webhooks
#[derive(Debug)]
pub struct WebhookNotifier {
	/// Webhook URL for message delivery
	pub url: String,
	/// URL parameters to use for the webhook request
	pub url_params: Option<HashMap<String, String>>,
	/// Title to display in the message
	pub title: String,
	/// Message template with variable placeholders
	pub body_template: String,
	/// HTTP client for webhook requests
	pub client: Client,
	/// HTTP method to use for the webhook request
	pub method: Option<String>,
	/// Secret to use for the webhook request
	pub secret: Option<String>,
	/// Headers to use for the webhook request
	pub headers: Option<HashMap<String, String>>,
	/// Payload fields to use for the webhook request
	pub payload_fields: Option<HashMap<String, serde_json::Value>>,
}

/// Represents a formatted webhook message
#[derive(Serialize, Debug)]
pub struct WebhookMessage {
	/// The content of the message
	title: String,
	body: String,
}

impl WebhookNotifier {
	/// Creates a new Webhook notifier instance
	///
	/// # Arguments
	/// * `config` - Webhook configuration
	///
	/// # Returns
	/// * `Result<Self, NotificationError>` - Notifier instance if config is valid
	pub fn new(config: WebhookConfig) -> Result<Self, NotificationError> {
		let mut headers = config.headers.unwrap_or_default();
		if !headers.contains_key("Content-Type") {
			headers.insert("Content-Type".to_string(), "application/json".to_string());
		}
		Ok(Self {
			url: config.url,
			url_params: config.url_params,
			title: config.title,
			body_template: config.body_template,
			client: Client::new(),
			method: Some(config.method.unwrap_or("POST".to_string())),
			secret: config.secret,
			headers: Some(headers),
			payload_fields: config.payload_fields,
		})
	}

	/// Formats a message by substituting variables in the template
	///
	/// # Arguments
	/// * `variables` - Map of variable names to values
	///
	/// # Returns
	/// * `String` - Formatted message with variables replaced
	pub fn format_message(&self, variables: &HashMap<String, String>) -> String {
		let mut message = self.body_template.clone();
		for (key, value) in variables {
			message = message.replace(&format!("${{{}}}", key), value);
		}
		message
	}

	/// Creates a Webhook notifier from a trigger configuration
	///
	/// # Arguments
	/// * `config` - Trigger configuration containing Webhook parameters
	///
	/// # Returns
	/// * `Result<Self>` - Notifier instance if config is Webhook type
	pub fn from_config(config: &TriggerTypeConfig) -> Result<Self, NotificationError> {
		if let TriggerTypeConfig::Webhook {
			url,
			message,
			method,
			secret,
			headers,
		} = config
		{
			let webhook_config = WebhookConfig {
				url: url.as_ref().to_string(),
				url_params: None,
				title: message.title.clone(),
				body_template: message.body.clone(),
				method: method.clone(),
				secret: secret.as_ref().map(|s| s.as_ref().to_string()),
				headers: headers.clone(),
				payload_fields: None,
			};

			WebhookNotifier::new(webhook_config)
		} else {
			let msg = format!("Invalid webhook configuration: {:?}", config);
			Err(NotificationError::config_error(msg, None, None))
		}
	}

	pub fn sign_request(
		&self,
		secret: &str,
		payload: &WebhookMessage,
	) -> Result<(String, String), NotificationError> {
		// Explicitly reject empty secret, because `HmacSha256::new_from_slice` currently allows empty secrets
		if secret.is_empty() {
			return Err(NotificationError::notify_failed(
				"Invalid secret: cannot be empty.".to_string(),
				None,
				None,
			));
		}

		let timestamp = Utc::now().timestamp_millis();

		// Create HMAC instance
		let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|e| {
			NotificationError::config_error(format!("Invalid secret: {}", e), None, None)
		})?; // Handle error if secret is invalid

		// Create the message to sign
		let message = format!("{:?}{}", payload, timestamp);
		mac.update(message.as_bytes());

		// Get the HMAC result
		let signature = hex::encode(mac.finalize().into_bytes());

		Ok((signature, timestamp.to_string()))
	}
}

#[async_trait]
impl Notifier for WebhookNotifier {
	/// Sends a formatted message to Webhook
	///
	/// # Arguments
	/// * `message` - The formatted message to send
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), NotificationError> {
		// Default payload with title and body
		let mut payload_fields = HashMap::new();
		payload_fields.insert("title".to_string(), serde_json::json!(self.title));
		payload_fields.insert("body".to_string(), serde_json::json!(message));

		self.notify_with_payload(message, payload_fields).await
	}

	/// Sends a formatted message to Webhook with custom payload fields
	///
	/// # Arguments
	/// * `message` - The formatted message to send
	/// * `payload_fields` - Additional fields to include in the payload
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	async fn notify_with_payload(
		&self,
		message: &str,
		mut payload_fields: HashMap<String, serde_json::Value>,
	) -> Result<(), NotificationError> {
		let mut url = self.url.clone();
		// Add URL parameters if present
		if let Some(params) = &self.url_params {
			let params_str: Vec<String> = params
				.iter()
				.map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
				.collect();
			if !params_str.is_empty() {
				url = format!("{}?{}", url, params_str.join("&"));
			}
		}

		// Merge with default payload fields if they exist
		if let Some(default_fields) = &self.payload_fields {
			for (key, value) in default_fields {
				if !payload_fields.contains_key(key) {
					payload_fields.insert(key.clone(), value.clone());
				}
			}
		}

		let method = if let Some(ref m) = self.method {
			Method::from_bytes(m.as_bytes()).unwrap_or(Method::POST)
		} else {
			Method::POST
		};

		// Add default headers
		let mut headers = HeaderMap::new();
		headers.insert(
			HeaderName::from_static("content-type"),
			HeaderValue::from_static("application/json"),
		);

		if let Some(secret) = &self.secret {
			// Create a WebhookMessage for signing
			let payload_for_signing = WebhookMessage {
				title: self.title.clone(),
				body: message.to_string(),
			};

			let (signature, timestamp) =
				self.sign_request(secret, &payload_for_signing)
					.map_err(|e| {
						NotificationError::internal_error(e.to_string(), Some(e.into()), None)
					})?;

			// Add signature headers
			headers.insert(
				HeaderName::from_static("x-signature"),
				HeaderValue::from_str(&signature).map_err(|e| {
					NotificationError::notify_failed(
						"Invalid signature value".to_string(),
						Some(e.into()),
						None,
					)
				})?,
			);
			headers.insert(
				HeaderName::from_static("x-timestamp"),
				HeaderValue::from_str(&timestamp).map_err(|e| {
					NotificationError::notify_failed(
						"Invalid timestamp value".to_string(),
						Some(e.into()),
						None,
					)
				})?,
			);
		}

		// Add custom headers
		if let Some(headers_map) = &self.headers {
			for (key, value) in headers_map {
				let header_name = HeaderName::from_bytes(key.as_bytes()).map_err(|e| {
					NotificationError::notify_failed(
						format!("Invalid header name: {}", key),
						Some(e.into()),
						None,
					)
				})?;
				let header_value = HeaderValue::from_str(value).map_err(|e| {
					NotificationError::notify_failed(
						format!("Invalid header value for {}: {}", key, value),
						Some(e.into()),
						None,
					)
				})?;
				headers.insert(header_name, header_value);
			}
		}

		let payload = WebhookPayload {
			fields: payload_fields,
		};

		// Send request with custom payload
		let response = self
			.client
			.request(method, url.as_str())
			.headers(headers)
			.json(&payload)
			.send()
			.await
			.map_err(|e| {
				NotificationError::notify_failed(
					format!("Failed to send webhook request: {}", e),
					Some(e.into()),
					None,
				)
			})?;

		let status = response.status();

		if !status.is_success() {
			return Err(NotificationError::notify_failed(
				format!("Webhook request failed with status: {}", status),
				None,
				None,
			));
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use crate::models::{NotificationMessage, SecretString, SecretValue};

	use super::*;
	use mockito::{Matcher, Mock};
	use serde_json::json;

	fn create_test_notifier(
		url: &str,
		body_template: &str,
		secret: Option<&str>,
		headers: Option<HashMap<String, String>>,
	) -> WebhookNotifier {
		WebhookNotifier::new(WebhookConfig {
			url: url.to_string(),
			url_params: None,
			title: "Alert".to_string(),
			body_template: body_template.to_string(),
			method: Some("POST".to_string()),
			secret: secret.map(|s| s.to_string()),
			headers,
			payload_fields: None,
		})
		.unwrap()
	}

	fn create_test_webhook_config() -> TriggerTypeConfig {
		TriggerTypeConfig::Webhook {
			url: SecretValue::Plain(SecretString::new("https://webhook.example.com".to_string())),
			method: Some("POST".to_string()),
			secret: None,
			headers: None,
			message: NotificationMessage {
				title: "Test Alert".to_string(),
				body: "Test message ${value}".to_string(),
			},
		}
	}

	////////////////////////////////////////////////////////////
	// format_message tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_format_message() {
		let notifier = create_test_notifier(
			"https://webhook.example.com",
			"Value is ${value} and status is ${status}",
			None,
			None,
		);

		let mut variables = HashMap::new();
		variables.insert("value".to_string(), "100".to_string());
		variables.insert("status".to_string(), "critical".to_string());

		let result = notifier.format_message(&variables);
		assert_eq!(result, "Value is 100 and status is critical");
	}

	#[test]
	fn test_format_message_with_missing_variables() {
		let notifier = create_test_notifier(
			"https://webhook.example.com",
			"Value is ${value} and status is ${status}",
			None,
			None,
		);

		let mut variables = HashMap::new();
		variables.insert("value".to_string(), "100".to_string());
		// status variable is not provided

		let result = notifier.format_message(&variables);
		assert_eq!(result, "Value is 100 and status is ${status}");
	}

	#[test]
	fn test_format_message_with_empty_template() {
		let notifier = create_test_notifier("https://webhook.example.com", "", None, None);

		let variables = HashMap::new();
		let result = notifier.format_message(&variables);
		assert_eq!(result, "");
	}

	////////////////////////////////////////////////////////////
	// sign_request tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_sign_request() {
		let notifier = create_test_notifier(
			"https://webhook.example.com",
			"Test message",
			Some("test-secret"),
			None,
		);
		let payload = WebhookMessage {
			title: "Test Title".to_string(),
			body: "Test message".to_string(),
		};
		let secret = "test-secret";

		let result = notifier.sign_request(secret, &payload).unwrap();
		let (signature, timestamp) = result;

		assert!(!signature.is_empty());
		assert!(!timestamp.is_empty());
	}

	#[test]
	fn test_sign_request_fails_empty_secret() {
		let notifier =
			create_test_notifier("https://webhook.example.com", "Test message", None, None);
		let payload = WebhookMessage {
			title: "Test Title".to_string(),
			body: "Test message".to_string(),
		};
		let empty_secret = "";

		let result = notifier.sign_request(empty_secret, &payload);
		assert!(result.is_err());

		let error = result.unwrap_err();
		assert!(matches!(error, NotificationError::NotifyFailed(_)));
	}

	////////////////////////////////////////////////////////////
	// from_config tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_from_config_with_webhook_config() {
		let config = create_test_webhook_config();

		let notifier = WebhookNotifier::from_config(&config);
		assert!(notifier.is_ok());

		let notifier = notifier.unwrap();
		assert_eq!(notifier.url, "https://webhook.example.com");
		assert_eq!(notifier.title, "Test Alert");
		assert_eq!(notifier.body_template, "Test message ${value}");
	}

	#[test]
	fn test_from_config_invalid_type() {
		// Create a config that is not a Telegram type
		let config = TriggerTypeConfig::Slack {
			slack_url: SecretValue::Plain(SecretString::new(
				"https://slack.example.com".to_string(),
			)),
			message: NotificationMessage {
				title: "Test Alert".to_string(),
				body: "Test message ${value}".to_string(),
			},
		};

		let notifier = WebhookNotifier::from_config(&config);
		assert!(notifier.is_err());

		let error = notifier.unwrap_err();
		assert!(matches!(error, NotificationError::ConfigError { .. }));
	}

	////////////////////////////////////////////////////////////
	// notify tests
	////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_notify_failure() {
		let notifier =
			create_test_notifier("https://webhook.example.com", "Test message", None, None);
		let result = notifier.notify("Test message").await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_notify_includes_signature_and_timestamp() {
		let mut server = mockito::Server::new_async().await;
		let mock: Mock = server
			.mock("POST", "/")
			.match_header("X-Signature", Matcher::Regex("^[0-9a-f]{64}$".to_string()))
			.match_header("X-Timestamp", Matcher::Regex("^[0-9]+$".to_string()))
			.match_header("Content-Type", "text/plain")
			.with_status(200)
			.create_async()
			.await;

		let notifier = create_test_notifier(
			server.url().as_str(),
			"Test message",
			Some("top-secret"),
			Some(HashMap::from([(
				"Content-Type".to_string(),
				"text/plain".to_string(),
			)])),
		);

		let response = notifier.notify("Test message").await;

		assert!(response.is_ok());

		mock.assert();
	}

	////////////////////////////////////////////////////////////
	// notify header validation tests
	////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_notify_with_invalid_header_name() {
		let server = mockito::Server::new_async().await;
		let invalid_headers =
			HashMap::from([("Invalid Header!@#".to_string(), "value".to_string())]);

		let notifier = create_test_notifier(
			server.url().as_str(),
			"Test message",
			None,
			Some(invalid_headers),
		);

		let result = notifier.notify("Test message").await;
		let err = result.unwrap_err();
		assert!(err.to_string().contains("Invalid header name"));
	}

	#[tokio::test]
	async fn test_notify_with_invalid_header_value() {
		let server = mockito::Server::new_async().await;
		let invalid_headers =
			HashMap::from([("X-Custom-Header".to_string(), "Invalid\nValue".to_string())]);

		let notifier = create_test_notifier(
			server.url().as_str(),
			"Test message",
			None,
			Some(invalid_headers),
		);

		let result = notifier.notify("Test message").await;
		let err = result.unwrap_err();
		assert!(err.to_string().contains("Invalid header value"));
	}

	#[tokio::test]
	async fn test_notify_with_valid_headers() {
		let mut server = mockito::Server::new_async().await;
		let valid_headers = HashMap::from([
			("X-Custom-Header".to_string(), "valid-value".to_string()),
			("Accept".to_string(), "application/json".to_string()),
		]);

		let mock = server
			.mock("POST", "/")
			.match_header("X-Custom-Header", "valid-value")
			.match_header("Accept", "application/json")
			.with_status(200)
			.create_async()
			.await;

		let notifier = create_test_notifier(
			server.url().as_str(),
			"Test message",
			None,
			Some(valid_headers),
		);

		let result = notifier.notify("Test message").await;
		assert!(result.is_ok());
		mock.assert();
	}

	#[tokio::test]
	async fn test_notify_signature_header_cases() {
		let mut server = mockito::Server::new_async().await;

		let mock = server
			.mock("POST", "/")
			.match_header("X-Signature", Matcher::Any)
			.match_header("X-Timestamp", Matcher::Any)
			.with_status(200)
			.create_async()
			.await;

		let notifier = create_test_notifier(
			server.url().as_str(),
			"Test message",
			Some("test-secret"),
			None,
		);

		let result = notifier.notify("Test message").await;
		assert!(result.is_ok());
		mock.assert();
	}

	#[test]
	fn test_sign_request_validation() {
		let notifier = create_test_notifier(
			"https://webhook.example.com",
			"Test message",
			Some("test-secret"),
			None,
		);

		let payload = WebhookMessage {
			title: "Test Title".to_string(),
			body: "Test message".to_string(),
		};

		let result = notifier.sign_request("test-secret", &payload).unwrap();
		let (signature, timestamp) = result;

		// Validate signature format (should be a hex string)
		assert!(
			hex::decode(&signature).is_ok(),
			"Signature should be valid hex"
		);

		// Validate timestamp format (should be a valid i64)
		assert!(
			timestamp.parse::<i64>().is_ok(),
			"Timestamp should be valid i64"
		);
	}

	////////////////////////////////////////////////////////////
	// notify_with_payload tests
	////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_notify_with_payload_success() {
		let mut server = mockito::Server::new_async().await;
		let expected_payload = json!({
			"title": "Alert",
			"body": "Test message",
			"custom_field": "custom_value"
		});

		let mock = server
			.mock("POST", "/")
			.match_header("content-type", "application/json")
			.match_body(Matcher::Json(expected_payload))
			.with_header("content-type", "application/json")
			.with_body("{}")
			.with_status(200)
			.expect(1)  // Expect exactly one request
			.create_async()
			.await;

		let notifier = create_test_notifier(server.url().as_str(), "Test message", None, None);
		let mut payload = HashMap::new();
		// Insert fields in the same order as they appear in expected_payload
		payload.insert("title".to_string(), serde_json::json!("Alert"));
		payload.insert("body".to_string(), serde_json::json!("Test message"));
		payload.insert(
			"custom_field".to_string(),
			serde_json::json!("custom_value"),
		);

		let result = notifier.notify_with_payload("Test message", payload).await;
		assert!(result.is_ok());
		mock.assert();
	}

	#[tokio::test]
	async fn test_notify_with_payload_and_url_params() {
		let mut server = mockito::Server::new_async().await;
		let mock = server
			.mock("POST", "/")
			.match_query(mockito::Matcher::AllOf(vec![
				mockito::Matcher::UrlEncoded("param1".into(), "value1".into()),
				mockito::Matcher::UrlEncoded("param2".into(), "value2".into()),
			]))
			.with_status(200)
			.create_async()
			.await;

		let mut url_params = HashMap::new();
		url_params.insert("param1".to_string(), "value1".to_string());
		url_params.insert("param2".to_string(), "value2".to_string());

		let notifier = WebhookNotifier::new(WebhookConfig {
			url: server.url(),
			url_params: Some(url_params),
			title: "Alert".to_string(),
			body_template: "Test message".to_string(),
			method: None,
			secret: None,
			headers: None,
			payload_fields: None,
		})
		.unwrap();

		let result = notifier
			.notify_with_payload("Test message", HashMap::new())
			.await;
		assert!(result.is_ok());
		mock.assert();
	}

	#[tokio::test]
	async fn test_notify_with_payload_and_method_override() {
		let mut server = mockito::Server::new_async().await;
		let mock = server
			.mock("GET", "/")
			.with_status(200)
			.create_async()
			.await;

		let notifier = WebhookNotifier::new(WebhookConfig {
			url: server.url(),
			url_params: None,
			title: "Alert".to_string(),
			body_template: "Test message".to_string(),
			method: Some("GET".to_string()),
			secret: None,
			headers: None,
			payload_fields: None,
		})
		.unwrap();

		let result = notifier
			.notify_with_payload("Test message", HashMap::new())
			.await;
		assert!(result.is_ok());
		mock.assert();
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

		let mut default_fields = HashMap::new();
		default_fields.insert(
			"default_field".to_string(),
			serde_json::json!("default_value"),
		);

		let notifier = WebhookNotifier::new(WebhookConfig {
			url: server.url(),
			url_params: None,
			title: "Alert".to_string(),
			body_template: "Test message".to_string(),
			method: None,
			secret: None,
			headers: None,
			payload_fields: Some(default_fields),
		})
		.unwrap();

		let mut payload = HashMap::new();
		payload.insert(
			"custom_field".to_string(),
			serde_json::json!("custom_value"),
		);

		let result = notifier.notify_with_payload("Test message", payload).await;
		assert!(result.is_ok());
		mock.assert();
	}

	#[tokio::test]
	async fn test_notify_with_payload_custom_fields_override_defaults() {
		let mut server = mockito::Server::new_async().await;

		let expected_payload = json!({
			 "custom_field": "custom_value"
		});

		let mock = server
			.mock("POST", "/")
			.match_body(mockito::Matcher::Json(expected_payload))
			.with_status(200)
			.create_async()
			.await;

		let mut default_fields = HashMap::new();
		default_fields.insert(
			"custom_field".to_string(),
			serde_json::json!("default_value"),
		);

		let notifier = WebhookNotifier::new(WebhookConfig {
			url: server.url(),
			url_params: None,
			title: "Alert".to_string(),
			body_template: "Test message".to_string(),
			method: None,
			secret: None,
			headers: None,
			payload_fields: Some(default_fields),
		})
		.unwrap();

		let mut payload = HashMap::new();
		payload.insert(
			"custom_field".to_string(),
			serde_json::json!("custom_value"),
		);

		let result = notifier.notify_with_payload("Test message", payload).await;
		assert!(result.is_ok());
		mock.assert();
	}

	#[tokio::test]
	async fn test_notify_with_payload_invalid_url() {
		let notifier = create_test_notifier("invalid-url", "Test message", None, None);

		let result = notifier
			.notify_with_payload("Test message", HashMap::new())
			.await;
		assert!(result.is_err());

		let error = result.unwrap_err();
		assert!(matches!(error, NotificationError::NotifyFailed { .. }));
	}

	#[tokio::test]
	async fn test_notify_with_payload_failure() {
		let mut server = mockito::Server::new_async().await;
		let mock = server
			.mock("POST", "/")
			.with_status(500)
			.with_body("Internal Server Error")
			.create_async()
			.await;

		let notifier = create_test_notifier(server.url().as_str(), "Test message", None, None);

		let result = notifier
			.notify_with_payload("Test message", HashMap::new())
			.await;

		assert!(result.is_err());
		mock.assert();
	}
}
