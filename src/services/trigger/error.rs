//! Trigger error types and handling.
//!
//! Provides error types for trigger-related operations,
//! including execution failures and configuration issues.

use crate::utils::{EnhancedContext, ErrorContext};
use std::collections::HashMap;

/// Represents possible errors during trigger operations
#[derive(Debug)]
pub enum TriggerError {
	/// When a requested trigger cannot be found
	NotFound(ErrorContext<String>),
	/// When trigger execution fails
	ExecutionError(ErrorContext<String>),
	/// When trigger configuration is invalid
	ConfigurationError(ErrorContext<String>),
}

impl TriggerError {
	const TARGET: &str = "trigger::error";

	/// Creates a new not found error with logging
	pub fn not_found(msg: impl Into<String>, metadata: Option<HashMap<String, String>>) -> Self {
		Self::NotFound(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Trigger Not Found Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new not found error with source
	pub fn not_found_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NotFound(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Trigger Not Found Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new execution error with logging
	pub fn execution_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ExecutionError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Execution Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new execution error with source
	pub fn execution_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ExecutionError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Execution Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new configuration error with logging
	pub fn configuration_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConfigurationError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Configuration Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new configuration error with source
	pub fn configuration_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConfigurationError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Configuration Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}
}

impl std::error::Error for TriggerError {}

// Standard error trait implementations
impl std::fmt::Display for TriggerError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::NotFound(ctx) => ctx.fmt(f),
			Self::ExecutionError(ctx) => ctx.fmt(f),
			Self::ConfigurationError(ctx) => ctx.fmt(f),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_not_found_error_formatting() {
		let error = TriggerError::not_found("test error", None);
		assert!(error
			.to_string()
			.contains("Trigger Not Found Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = TriggerError::not_found_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error
			.to_string()
			.contains("Trigger Not Found Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));
		let error = TriggerError::not_found_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error
			.to_string()
			.contains("Trigger Not Found Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_execution_error_formatting() {
		let error = TriggerError::execution_error("test error", None);
		assert!(error.to_string().contains("Execution Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = TriggerError::execution_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Execution Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));

		let error = TriggerError::execution_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Execution Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_configuration_error_formatting() {
		let error = TriggerError::configuration_error("test error", None);
		assert!(error
			.to_string()
			.contains("Configuration Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = TriggerError::configuration_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error
			.to_string()
			.contains("Configuration Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));

		let error = TriggerError::configuration_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error
			.to_string()
			.contains("Configuration Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}
}
