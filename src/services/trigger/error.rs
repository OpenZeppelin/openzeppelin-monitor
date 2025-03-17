//! Trigger error types and handling.
//!
//! Provides error types for trigger-related operations,
//! including execution failures and configuration issues.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error as ThisError;
use uuid::Uuid;

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
		Self::NotFound(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Execution error
	pub fn execution_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ExecutionError(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Execution error without logging
	pub fn execution_error_without_log(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ExecutionError(ErrorContext::new(msg, source, metadata))
	}

	// Configuration error
	pub fn configuration_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConfigurationError(ErrorContext::new_with_log(msg, source, metadata))
	}
}

impl TraceableError for TriggerError {
	fn trace_id(&self) -> String {
		match self {
			Self::NotFound(ctx) => ctx.trace_id.clone(),
			Self::ExecutionError(ctx) => ctx.trace_id.clone(),
			Self::ConfigurationError(ctx) => ctx.trace_id.clone(),
			Self::Other(_) => Uuid::new_v4().to_string(),
		}
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
		assert_eq!(
			error.to_string(),
			"Not found error: test error [key1=value1]"
		);
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
		assert_eq!(
			error.to_string(),
			"Execution error: test error [key1=value1]"
		);
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
		assert_eq!(
			error.to_string(),
			"Configuration error: test error [key1=value1]"
		);
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
		let io_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = TriggerError::configuration_error(
			"Failed to initialize",
			Some(Box::new(io_error)),
			None,
		);

		// Just test the string representation instead of the source chain
		assert!(outer_error.to_string().contains("Failed to initialize"));

		// For TriggerError::ConfigurationError, we know the implementation details
		if let TriggerError::ConfigurationError(ctx) = &outer_error {
			// Check that the context has the right message
			assert_eq!(ctx.message, "Failed to initialize");

			// Check that the context has the source error
			assert!(ctx.source.is_some());

			if let Some(src) = &ctx.source {
				assert_eq!(src.to_string(), "while reading config");
			}
		} else {
			panic!("Expected ConfigurationError variant");
		}
	}

	#[test]
	fn test_trace_id_propagation() {
		// Create an error context with a known trace ID
		let error_context = ErrorContext::new("Inner error", None, None);
		let original_trace_id = error_context.trace_id.clone();

		// Wrap it in a TriggerError
		let trigger_error = TriggerError::ConfigurationError(error_context);

		// Verify the trace ID is preserved
		assert_eq!(trigger_error.trace_id(), original_trace_id);

		// Test trace ID propagation through error chain
		let source_error = IoError::new(ErrorKind::Other, "Source error");
		let error_context = ErrorContext::new("Middle error", Some(Box::new(source_error)), None);
		let original_trace_id = error_context.trace_id.clone();

		let trigger_error = TriggerError::ConfigurationError(error_context);
		assert_eq!(trigger_error.trace_id(), original_trace_id);

		// Test Other variant
		let anyhow_error = anyhow::anyhow!("Test anyhow error");
		let trigger_error: TriggerError = anyhow_error.into();

		// Other variant should generate a new UUID
		assert!(!trigger_error.trace_id().is_empty());
	}
}
