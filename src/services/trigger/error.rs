//! Trigger error types and handling.
//!
//! Provides error types for trigger-related operations,
//! including execution failures and configuration issues.

use crate::utils::{new_error, new_error_with_source, ErrorContext, ErrorContextProvider};
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

impl ErrorContextProvider for TriggerError {
	fn target() -> &'static str {
		"trigger"
	}
	fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
		match self {
			Self::NotFound(ctx) => Some(ctx),
			Self::ExecutionError(ctx) => Some(ctx),
			Self::ConfigurationError(ctx) => Some(ctx),
		}
	}
}

impl TriggerError {
	/// Creates a new not found error with logging
	pub fn not_found(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::NotFound,
			"TriggerNotFoundError",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new not found error with source
	pub fn not_found_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::NotFound,
			"TriggerNotFoundError",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new execution error with logging
	pub fn execution_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::ExecutionError,
			"TriggerExecutionError",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new execution error with source
	pub fn execution_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::ExecutionError,
			"TriggerExecutionError",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new configuration error with logging
	pub fn configuration_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::ConfigurationError,
			"TriggerConfigurationError",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new configuration error with source
	pub fn configuration_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::ConfigurationError,
			"TriggerConfigurationError",
			msg,
			source,
			metadata,
			target,
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
		let error = TriggerError::not_found("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = TriggerError::not_found_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");
		let error = TriggerError::not_found_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_execution_error_formatting() {
		let error = TriggerError::execution_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = TriggerError::execution_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = TriggerError::execution_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_configuration_error_formatting() {
		let error = TriggerError::configuration_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = TriggerError::configuration_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = TriggerError::configuration_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}
}
