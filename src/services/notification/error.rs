//! Notification error types and handling.
//!
//! Provides error types for notification-related operations,
//! including network issues and configuration problems.

use log::error;
use std::{error::Error, fmt};

/// Represents possible errors during notification operations
#[derive(Debug)]
pub enum NotificationError {
	/// Network-related errors (e.g., webhook failures)
	NetworkError(String),
	/// Configuration-related errors
	ConfigError(String),
	/// Internal errors (e.g., failed to build email)
	InternalError(String),
	/// Script execution errors
	ExecutionError(String),
}

use reqwest;

impl NotificationError {
	/// Formats the error message based on the error type
	fn format_message(&self) -> String {
		match self {
			Self::NetworkError(msg) => format!("Network error: {}", msg),
			Self::ConfigError(msg) => format!("Config error: {}", msg),
			Self::InternalError(msg) => format!("Internal error: {}", msg),
			Self::ExecutionError(msg) => format!("Execution error: {}", msg),
		}
	}

	/// Creates a new network error with logging
	pub fn network_error(msg: impl Into<String>) -> Self {
		let error = Self::NetworkError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new configuration error with logging
	pub fn config_error(msg: impl Into<String>) -> Self {
		let error = Self::ConfigError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new internal error with logging
	pub fn internal_error(msg: impl Into<String>) -> Self {
		let error = Self::InternalError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new script execution error with logging
	pub fn execution_error(msg: impl Into<String>) -> Self {
		let error = Self::ExecutionError(msg.into());
		error!("{}", error.format_message());
		error
	}
}
impl From<reqwest::Error> for NotificationError {
	fn from(error: reqwest::Error) -> Self {
		Self::network_error(error.to_string())
	}
}

impl fmt::Display for NotificationError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.format_message())
	}
}

impl Error for NotificationError {}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_network_error_creation() {
		let error = NotificationError::network_error("Failed to connect");
		assert!(matches!(error, NotificationError::NetworkError(_)));
		assert_eq!(error.to_string(), "Network error: Failed to connect");
	}

	#[test]
	fn test_config_error_creation() {
		let error = NotificationError::config_error("Invalid configuration");
		assert!(matches!(error, NotificationError::ConfigError(_)));
		assert_eq!(error.to_string(), "Config error: Invalid configuration");
	}

	#[test]
	fn test_internal_error_creation() {
		let error = NotificationError::internal_error("Processing failed");
		assert!(matches!(error, NotificationError::InternalError(_)));
		assert_eq!(error.to_string(), "Internal error: Processing failed");
	}

	#[test]
	fn test_execution_error_creation() {
		let error = NotificationError::execution_error("Script failed");
		assert!(matches!(error, NotificationError::ExecutionError(_)));
		assert_eq!(error.to_string(), "Execution error: Script failed");
	}

	#[tokio::test]
	async fn test_reqwest_error_conversion() {
		let reqwest_error = reqwest::Client::new()
			.get("invalid-url")
			.send()
			.await
			.unwrap_err();
		let notification_error: NotificationError = reqwest_error.into();
		assert!(matches!(
			notification_error,
			NotificationError::NetworkError(_)
		));
	}

	#[test]
	fn test_error_display() {
		let errors = [
			NotificationError::NetworkError("network".into()),
			NotificationError::ConfigError("config".into()),
			NotificationError::InternalError("internal".into()),
			NotificationError::ExecutionError("execution".into()),
		];

		let expected = [
			"Network error: network",
			"Config error: config",
			"Internal error: internal",
			"Execution error: execution",
		];

		for (error, expected_msg) in errors.iter().zip(expected.iter()) {
			assert_eq!(error.to_string(), *expected_msg);
		}
	}
}
