//! Notification error types and handling.
//!
//! Provides error types for notification-related operations,
//! including network issues and configuration problems.

use crate::utils::{EnhancedContext, ErrorContext};
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

impl NotificationError {
	const TARGET: &str = "notification::error";

	/// Creates a new network error with logging
	pub fn network_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Network Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new network error with source
	pub fn network_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Network Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new configuration error with logging
	pub fn config_error(msg: impl Into<String>, metadata: Option<HashMap<String, String>>) -> Self {
		Self::ConfigError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Configuration Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new configuration error with source
	pub fn config_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConfigError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Configuration Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new internal error with logging
	pub fn internal_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Internal Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new internal error with source
	pub fn internal_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Internal Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
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
		let error = NotificationError::network_error("test error", None);
		assert!(error.to_string().contains("Network Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = NotificationError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			None,
		);
		assert!(error.to_string().contains("Network Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = NotificationError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Network Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_config_error_formatting() {
		let error = NotificationError::config_error("test error", None);
		assert!(error
			.to_string()
			.contains("Configuration Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = NotificationError::config_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			None,
		);
		assert!(error
			.to_string()
			.contains("Configuration Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = NotificationError::config_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error
			.to_string()
			.contains("Configuration Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = NotificationError::internal_error("test error", None);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = NotificationError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			None,
		);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = NotificationError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Network Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}
}
