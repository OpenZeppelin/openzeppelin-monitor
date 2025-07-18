//! Webhook notification implementation.
//!
//! Provides functionality to send formatted messages to webhooks
//! via incoming webhooks, supporting message templates with variable substitution.

use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{
	header::{HeaderMap, HeaderName, HeaderValue},
	Method,
};
use reqwest_middleware::ClientWithMiddleware;
use sha2::Sha256;
use std::{collections::HashMap, sync::Arc};

use crate::{models::TriggerTypeConfig, services::notification::NotificationError};

/// HMAC SHA256 type alias
type HmacSha256 = Hmac<Sha256>;

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
	/// Configured HTTP client for webhook requests with retry capabilities
	pub client: Arc<ClientWithMiddleware>,
	/// HTTP method to use for the webhook request
	pub method: Option<String>,
	/// Secret to use for the webhook request
	pub secret: Option<String>,
	/// Headers to use for the webhook request
	pub headers: Option<HashMap<String, String>>,
	/// Payload fields to use for the webhook request
	pub payload_fields: Option<HashMap<String, serde_json::Value>>,
}

impl WebhookNotifier {
	/// Creates a new Webhook notifier instance
	///
	/// # Arguments
	/// * `config` - Webhook configuration
	/// * `http_client` - HTTP client with middleware for retries
	///
	/// # Returns
	/// * `Result<Self, NotificationError>` - Notifier instance if config is valid
	pub fn new(
		config: WebhookConfig,
		http_client: Arc<ClientWithMiddleware>,
	) -> Result<Self, NotificationError> {
		let mut headers = config.headers.unwrap_or_default();
		if !headers.contains_key("Content-Type") {
			headers.insert("Content-Type".to_string(), "application/json".to_string());
		}
		Ok(Self {
			url: config.url,
			url_params: config.url_params,
			title: config.title,
			client: http_client,
			method: Some(config.method.unwrap_or("POST".to_string())),
			secret: config.secret,
			headers: Some(headers),
			payload_fields: config.payload_fields,
		})
	}

	/// Creates a Webhook notifier from a trigger configuration
	///
	/// # Arguments
	/// * `config` - Trigger configuration containing Webhook parameters
	/// * `http_client` - HTTP client with middleware for retries
	///
	/// # Returns
	/// * `Result<Self>` - Notifier instance if config is Webhook type
	pub fn from_config(
		config: &TriggerTypeConfig,
		http_client: Arc<ClientWithMiddleware>,
	) -> Result<Self, NotificationError> {
		if let TriggerTypeConfig::Webhook {
			url,
			message,
			method,
			secret,
			headers,
			..
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

			WebhookNotifier::new(webhook_config, http_client)
		} else {
			let msg = format!("Invalid webhook configuration: {:?}", config);
			Err(NotificationError::config_error(msg, None, None))
		}
	}

	pub fn sign_payload(
		&self,
		secret: &str,
		payload: &serde_json::Value,
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
		let serialized_payload = serde_json::to_string(payload).map_err(|e| {
			NotificationError::internal_error(
				format!("Failed to serialize payload: {}", e),
				Some(e.into()),
				None,
			)
		})?;
		let message = format!("{}{}", serialized_payload, timestamp);
		mac.update(message.as_bytes());

		// Get the HMAC result
		let signature = hex::encode(mac.finalize().into_bytes());

		Ok((signature, timestamp.to_string()))
	}

	/// Sends a JSON payload to Webhook
	///
	/// # Arguments
	/// * `payload` - The JSON payload to send
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	pub async fn notify_json(&self, payload: &serde_json::Value) -> Result<(), NotificationError> {
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
			let (signature, timestamp) = self.sign_payload(secret, payload).map_err(|e| {
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

		// Send request with custom payload
		let response = self
			.client
			.request(method, url.as_str())
			.headers(headers)
			.json(payload)
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
	use crate::{
		models::{NotificationMessage, SecretString, SecretValue},
		services::notification::{GenericWebhookPayloadBuilder, WebhookPayloadBuilder},
		utils::{tests::create_test_http_client, RetryConfig},
	};

	use super::*;
	use mockito::{Matcher, Mock};
	use serde_json::json;

	fn create_test_notifier(
		url: &str,
		secret: Option<&str>,
		headers: Option<HashMap<String, String>>,
	) -> WebhookNotifier {
		let http_client = create_test_http_client();
		let config = WebhookConfig {
			url: url.to_string(),
			url_params: None,
			title: "Alert".to_string(),
			body_template: "Test message".to_string(),
			method: Some("POST".to_string()),
			secret: secret.map(|s| s.to_string()),
			headers,
			payload_fields: None,
		};
		WebhookNotifier::new(config, http_client).unwrap()
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
			retry_policy: RetryConfig::default(),
		}
	}

	fn create_test_payload() -> serde_json::Value {
		GenericWebhookPayloadBuilder.build_payload(
			"Test Alert",
			"Test message with value ${value}",
			&HashMap::from([("value".to_string(), "42".to_string())]),
		)
	}

	////////////////////////////////////////////////////////////
	// sign_request tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_sign_request() {
		let notifier =
			create_test_notifier("https://webhook.example.com", Some("test-secret"), None);
		let payload = json!({
			"title": "Test Title",
			"body": "Test message"
		});
		let secret = "test-secret";

		let result = notifier.sign_payload(secret, &payload).unwrap();
		let (signature, timestamp) = result;

		assert!(!signature.is_empty());
		assert!(!timestamp.is_empty());
	}

	#[test]
	fn test_sign_request_fails_empty_secret() {
		let notifier = create_test_notifier("https://webhook.example.com", None, None);
		let payload = json!({
			"title": "Test Title",
			"body": "Test message"
		});
		let empty_secret = "";

		let result = notifier.sign_payload(empty_secret, &payload);
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
		let http_client = create_test_http_client();
		let notifier = WebhookNotifier::from_config(&config, http_client);
		assert!(notifier.is_ok());

		let notifier = notifier.unwrap();
		assert_eq!(notifier.url, "https://webhook.example.com");
		assert_eq!(notifier.title, "Test Alert");
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
			retry_policy: RetryConfig::default(),
		};

		let http_client = create_test_http_client();
		let notifier = WebhookNotifier::from_config(&config, http_client);
		assert!(notifier.is_err());

		let error = notifier.unwrap_err();
		assert!(matches!(error, NotificationError::ConfigError { .. }));
	}

	////////////////////////////////////////////////////////////
	// notify tests
	////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_notify_failure() {
		let notifier = create_test_notifier("https://webhook.example.com", None, None);
		let payload = create_test_payload();
		let result = notifier.notify_json(&payload).await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_notify_includes_signature_and_timestamp() {
		let mut server = mockito::Server::new_async().await;
		let mock: Mock = server
			.mock("POST", "/")
			.match_header("X-Signature", Matcher::Regex("^[0-9a-f]{64}$".to_string()))
			.match_header("X-Timestamp", Matcher::Regex("^[0-9]+$".to_string()))
			.match_header("Content-Type", "application/json")
			.with_status(200)
			.create_async()
			.await;

		let notifier = create_test_notifier(
			server.url().as_str(),
			Some("top-secret"),
			Some(HashMap::from([(
				"Content-Type".to_string(),
				"application/json".to_string(),
			)])),
		);

		let payload = create_test_payload();
		let result = notifier.notify_json(&payload).await;

		assert!(result.is_ok());

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

		let notifier = create_test_notifier(server.url().as_str(), None, Some(invalid_headers));
		let payload = create_test_payload();
		let result = notifier.notify_json(&payload).await;
		let err = result.unwrap_err();
		assert!(err.to_string().contains("Invalid header name"));
	}

	#[tokio::test]
	async fn test_notify_with_invalid_header_value() {
		let server = mockito::Server::new_async().await;
		let invalid_headers =
			HashMap::from([("X-Custom-Header".to_string(), "Invalid\nValue".to_string())]);

		let notifier = create_test_notifier(server.url().as_str(), None, Some(invalid_headers));

		let payload = create_test_payload();
		let result = notifier.notify_json(&payload).await;
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

		let notifier = create_test_notifier(server.url().as_str(), None, Some(valid_headers));

		let payload = create_test_payload();
		let result = notifier.notify_json(&payload).await;
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

		let notifier = create_test_notifier(server.url().as_str(), Some("test-secret"), None);

		let payload = create_test_payload();
		let result = notifier.notify_json(&payload).await;
		assert!(result.is_ok());
		mock.assert();
	}

	#[test]
	fn test_sign_request_validation() {
		let notifier =
			create_test_notifier("https://webhook.example.com", Some("test-secret"), None);

		let payload = create_test_payload();

		let result = notifier.sign_payload("test-secret", &payload).unwrap();
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
}
