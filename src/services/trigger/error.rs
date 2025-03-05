//! Trigger error types and handling.
//!
//! Provides error types for trigger-related operations,
//! including execution failures and configuration issues.

use crate::utils::ErrorContext;
use std::collections::HashMap;
use thiserror::Error as ThisError;

/// Represents errors that can occur during trigger operations
#[derive(ThisError, Debug)]
pub enum TriggerError {
	/// Errors related to not found errors
	#[error("Not found error: {0}")]
	NotFound(ErrorContext),

	/// Errors related to execution failures
	#[error("Execution error: {0}")]
	ExecutionError(ErrorContext),

	/// Errors related to configuration errors
	#[error("Configuration error: {0}")]
	ConfigurationError(ErrorContext),

	/// Other errors that don't fit into the categories above
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

impl TriggerError {
	// Not found error
	pub fn not_found(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NotFound(ErrorContext::new(msg.into(), source, metadata))
	}

	// Execution error
	pub fn execution_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ExecutionError(ErrorContext::new(msg.into(), source, metadata))
	}

	// Configuration error
	pub fn configuration_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConfigurationError(ErrorContext::new(msg.into(), source, metadata))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	#[test]
	fn test_not_found_error_formatting() {
		let error = TriggerError::not_found("test error", None, None);
		assert_eq!(error.to_string(), "Not found error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = TriggerError::not_found(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Not found error: test error");
	}

	#[test]
	fn test_execution_error_formatting() {
		let error = TriggerError::execution_error("test error", None, None);
		assert_eq!(error.to_string(), "Execution error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = TriggerError::execution_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Execution error: test error");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = TriggerError::configuration_error("test error", None, None);
		assert_eq!(error.to_string(), "Configuration error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = TriggerError::configuration_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Configuration error: test error");
	}

	#[test]
	fn test_from_anyhow_error() {
		let anyhow_error = anyhow::anyhow!("test anyhow error");
		let trigger_error: TriggerError = anyhow_error.into();
		assert!(matches!(trigger_error, TriggerError::Other(_)));
		assert_eq!(trigger_error.to_string(), "test anyhow error");
	}

	#[test]
	fn test_error_source_chain() {
		use std::error::Error;
		let middle_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = TriggerError::configuration_error(
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
