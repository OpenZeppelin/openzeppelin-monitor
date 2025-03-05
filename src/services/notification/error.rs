//! Notification error types and handling.
//!
//! Provides error types for notification-related operations,
//! including network issues and configuration problems.

use crate::utils::ErrorContext;
use std::collections::HashMap;
use thiserror::Error as ThisError;

/// Represents errors that can occur during notification operations
#[derive(ThisError, Debug)]
pub enum NotificationError {
	/// Errors related to network connectivity issues
	#[error("Network error: {0}")]
	NetworkError(ErrorContext),

	/// Errors related to malformed requests or invalid responses
	#[error("Config error: {0}")]
	ConfigError(ErrorContext),

	/// Errors related to internal processing errors
	#[error("Internal error: {0}")]
	InternalError(ErrorContext),

	/// Other errors that don't fit into the categories above
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

impl NotificationError {
	// Network error
	pub fn network_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(ErrorContext::new(msg.into(), source, metadata))
	}

	// Config error
	pub fn config_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConfigError(ErrorContext::new(msg.into(), source, metadata))
	}

	// Internal error
	pub fn internal_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(ErrorContext::new(msg.into(), source, metadata))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	#[test]
	fn test_network_error_formatting() {
		let error = NotificationError::network_error("test error", None, None);
		assert_eq!(error.to_string(), "Network error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = NotificationError::network_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Network error: test error");
	}

	#[test]
	fn test_config_error_formatting() {
		let error = NotificationError::config_error("test error", None, None);
		assert_eq!(error.to_string(), "Config error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = NotificationError::config_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Config error: test error");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = NotificationError::internal_error("test error", None, None);
		assert_eq!(error.to_string(), "Internal error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = NotificationError::internal_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Internal error: test error");
	}

	#[test]
	fn test_from_anyhow_error() {
		let anyhow_error = anyhow::anyhow!("test anyhow error");
		let notification_error: NotificationError = anyhow_error.into();
		assert!(matches!(notification_error, NotificationError::Other(_)));
		assert_eq!(notification_error.to_string(), "test anyhow error");
	}

	#[test]
	fn test_error_source_chain() {
		use std::error::Error;
		let middle_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = NotificationError::network_error(
			"Failed to initialize",
			Some(Box::new(middle_error) as Box<dyn std::error::Error + Send + Sync>),
			None,
		);

		// Test the source chain
		let source = outer_error.source();
		assert!(source.is_some());
		assert_eq!(source.unwrap().to_string(), "while reading config");
	}
}
