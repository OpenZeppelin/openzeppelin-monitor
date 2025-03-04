//! Notification error types and handling.
//!
//! Provides error types for notification-related operations,
//! including network issues and configuration problems.

use crate::utils::{new_error, new_error_with_source, ErrorContext, ErrorContextProvider};
use std::collections::HashMap;
/// Represents possible errors during notification operations
#[derive(Debug)]
pub enum NotificationError {
	/// Network-related errors (e.g., webhook failures)
	NetworkError(ErrorContext<String>),
	/// Configuration-related errors
	ConfigError(ErrorContext<String>),
	/// Internal errors (e.g., failed to build email)
	InternalError(ErrorContext<String>),
}

impl ErrorContextProvider for NotificationError {
	fn target() -> &'static str {
		"notification"
	}
	fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
		match self {
			Self::NetworkError(ctx) => Some(ctx),
			Self::ConfigError(ctx) => Some(ctx),
			Self::InternalError(ctx) => Some(ctx),
		}
	}
}

impl NotificationError {
	/// Creates a new network error with logging
	pub fn network_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::NetworkError,
			"NotificationNetworkError",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new network error with source
	pub fn network_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::NetworkError,
			"NotificationNetworkError",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new configuration error with logging
	pub fn config_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::ConfigError,
			"NotificationConfigError",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new configuration error with source
	pub fn config_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::ConfigError,
			"NotificationConfigError",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new internal error with logging
	pub fn internal_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::InternalError,
			"NotificationInternalError",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new internal error with source
	pub fn internal_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::InternalError,
			"NotificationInternalError",
			msg,
			source,
			metadata,
			target,
		)
	}
}

impl std::error::Error for NotificationError {}

// Standard error trait implementations
impl std::fmt::Display for NotificationError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::NetworkError(ctx) => ctx.fmt(f),
			Self::ConfigError(ctx) => ctx.fmt(f),
			Self::InternalError(ctx) => ctx.fmt(f),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_network_error_formatting() {
		let error = NotificationError::network_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = NotificationError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");
		let error = NotificationError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_config_error_formatting() {
		let error = NotificationError::config_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = NotificationError::config_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");
		let error = NotificationError::config_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = NotificationError::internal_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = NotificationError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = NotificationError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}
}
