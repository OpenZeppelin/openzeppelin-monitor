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

/// Implementation of webhook notifications via webhooks
pub struct WebhookNotifier {
	/// Webhook URL for message delivery
	url: String,
	/// Title to display in the message
	title: String,
	/// Message template with variable placeholders
	body_template: String,
	/// HTTP client for webhook requests
	client: Client,
	/// HTTP method to use for the webhook request
	method: Option<String>,
	/// Secret to use for the webhook request
	secret: Option<String>,
	/// Headers to use for the webhook request
	headers: Option<HashMap<String, String>>,
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
	/// * `url` - Webhook URL
	/// * `title` - Message title
	/// * `body_template` - Message template with variables
	/// * `method` - HTTP method to use for the webhook request (optional, defaults to POST)
	/// * `secret` - Secret to use for the webhook request (optional)
	/// * `headers` - Headers to use for the webhook request (optional)
	pub fn new(
		url: String,
		title: String,
		body_template: String,
		method: Option<String>,
		secret: Option<String>,
		headers: Option<HashMap<String, String>>,
	) -> Result<Self, NotificationError> {
		Ok(Self {
			url,
			title,
			body_template,
			client: Client::new(),
			method: Some(method.unwrap_or("POST".to_string())),
			secret: secret.map(|s| s.to_string()),
			headers,
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
	/// * `Option<Self>` - Notifier instance if config is Webhook type
	pub fn from_config(config: &TriggerTypeConfig) -> Option<Self> {
		match config {
			TriggerTypeConfig::Webhook {
				url,
				message,
				method,
				secret,
				headers,
			} => Some(Self {
				url: url.clone(),
				title: message.title.clone(),
				body_template: message.body.clone(),
				client: Client::new(),
				method: method.clone(),
				secret: secret.clone(),
				headers: headers.clone(),
			}),
			_ => None,
		}
	}

	pub fn sign_request(
		&self,
		secret: &str,
		payload: &WebhookMessage,
	) -> Result<(String, String), NotificationError> {
		let timestamp = Utc::now().timestamp_millis();

		// Create HMAC instance
		let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
			.map_err(|_| NotificationError::config_error("Invalid secret"))?; // Handle error if secret is invalid

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
		let payload = WebhookMessage {
			title: self.title.clone(),
			body: message.to_string(),
		};

		let method = if let Some(ref m) = self.method {
			Method::from_bytes(m.as_bytes()).unwrap()
		} else {
			Method::POST
		};

		let mut headers = HeaderMap::new();

		if let Some(secret) = &self.secret {
			let (signature, timestamp) = self.sign_request(secret, &payload)?;
			headers.insert(
				HeaderName::from_bytes(b"X-Signature").unwrap(),
				HeaderValue::from_str(&signature).unwrap(),
			);
			headers.insert(
				HeaderName::from_bytes(b"X-Timestamp").unwrap(),
				HeaderValue::from_str(&timestamp).unwrap(),
			);
		}

		if let Some(headers_map) = &self.headers {
			for (key, value) in headers_map {
				headers.insert(
					HeaderName::from_bytes(key.as_bytes()).unwrap(),
					HeaderValue::from_str(value).unwrap(),
				);
			}
		}

		let response = self
			.client
			.request(method, self.url.as_str())
			.headers(headers)
			.json(&payload)
			.send()
			.await
			.map_err(|e| NotificationError::network_error(e.to_string()))?;

		if !response.status().is_success() {
			return Err(NotificationError::network_error(format!(
				"Webhook returned error status: {}",
				response.status()
			)));
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use crate::models::NotificationMessage;

	use super::*;
	use mockito::{Matcher, Mock};

	fn create_test_notifier(
		url: &str,
		body_template: &str,
		secret: Option<&str>,
		headers: Option<HashMap<String, String>>,
	) -> WebhookNotifier {
		WebhookNotifier::new(
			url.to_string(),
			"Alert".to_string(),
			body_template.to_string(),
			Some("POST".to_string()),
			secret.map(|s| s.to_string()),
			headers,
		)
		.unwrap()
	}

	fn create_test_webhook_config() -> TriggerTypeConfig {
		TriggerTypeConfig::Webhook {
			url: "https://webhook.example.com".to_string(),
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

	////////////////////////////////////////////////////////////
	// from_config tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_from_config_with_webhook_config() {
		let config = create_test_webhook_config();

		let notifier = WebhookNotifier::from_config(&config);
		assert!(notifier.is_some());

		let notifier = notifier.unwrap();
		assert_eq!(notifier.url, "https://webhook.example.com");
		assert_eq!(notifier.title, "Test Alert");
		assert_eq!(notifier.body_template, "Test message ${value}");
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
}
