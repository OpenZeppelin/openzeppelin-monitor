//! Security error types and error handling utilities.
//!
//! This module defines error types for handling security-related operations.
//! The `SecurityError` type provides a structured way to handle and propagate security-related errors
//! with rich context and tracing capabilities.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error as ThisError;
use uuid::Uuid;

/// Result type alias for security operations
pub type SecurityResult<T> = Result<T, Box<SecurityError>>;

/// Represents errors that can occur during security operations.
///
/// This error type is used throughout the security module to provide consistent error handling
/// with rich context and tracing capabilities. Each variant includes an `ErrorContext` that
/// contains detailed information about the error, including a trace ID for distributed tracing.

#[derive(ThisError, Debug)]
pub enum SecurityError {
	/// Errors related to validation failures.
	#[error("Validation error: {0}")]
	ValidationError(ErrorContext),

	/// Errors related to parsing failures.
	#[error("Parse error: {0}")]
	ParseError(ErrorContext),

	/// Errors related to network failures.
	#[error("Network error: {0}")]
	NetworkError(ErrorContext),

	/// Other errors that don't fit into the categories above.
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

impl SecurityError {
	// Validation error
	pub fn validation_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ValidationError(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Parse error
	pub fn parse_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ParseError(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Network error
	pub fn network_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(ErrorContext::new_with_log(msg, source, metadata))
	}
}

impl TraceableError for SecurityError {
	fn trace_id(&self) -> String {
		match self {
			Self::ValidationError(ctx) => ctx.trace_id.clone(),
			Self::ParseError(ctx) => ctx.trace_id.clone(),
			Self::NetworkError(ctx) => ctx.trace_id.clone(),
			Self::Other(_) => Uuid::new_v4().to_string(),
		}
	}
}

impl From<std::io::Error> for SecurityError {
	fn from(err: std::io::Error) -> Self {
		Self::parse_error(err.to_string(), None, None)
	}
}

impl From<serde_json::Error> for SecurityError {
	fn from(err: serde_json::Error) -> Self {
		Self::parse_error(err.to_string(), None, None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	#[test]
	fn test_validation_error_formatting() {
		let error = SecurityError::validation_error("test error", None, None);
		assert_eq!(error.to_string(), "Validation error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = SecurityError::validation_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Validation error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_parse_error_formatting() {
		let error = SecurityError::parse_error("test error", None, None);
		assert_eq!(error.to_string(), "Parse error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = SecurityError::parse_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Parse error: test error [key1=value1]");
	}

	#[test]
	fn test_network_error_formatting() {
		let error = SecurityError::network_error("test error", None, None);
		assert_eq!(error.to_string(), "Network error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = SecurityError::network_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Network error: test error [key1=value1]");
	}

	#[test]
	fn test_from_anyhow_error() {
		let anyhow_error = anyhow::anyhow!("test anyhow error");
		let security_error: SecurityError = anyhow_error.into();
		assert!(matches!(security_error, SecurityError::Other(_)));
		assert_eq!(security_error.to_string(), "test anyhow error");
	}

	#[test]
	fn test_error_source_chain() {
		let io_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error =
			SecurityError::parse_error("Failed to initialize", Some(Box::new(io_error)), None);

		// Just test the string representation instead of the source chain
		assert!(outer_error.to_string().contains("Failed to initialize"));

		// For SecurityError::ParseError, we know the implementation details
		if let SecurityError::ParseError(ctx) = &outer_error {
			// Check that the context has the right message
			assert_eq!(ctx.message, "Failed to initialize");

			// Check that the context has the source error
			assert!(ctx.source.is_some());

			if let Some(src) = &ctx.source {
				assert_eq!(src.to_string(), "while reading config");
			}
		} else {
			panic!("Expected ParseError variant");
		}
	}

	#[test]
	fn test_io_error_conversion() {
		let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
		let security_error: SecurityError = io_error.into();
		assert!(matches!(security_error, SecurityError::ParseError(_)));
	}

	#[test]
	fn test_serde_error_conversion() {
		let json = "invalid json";
		let serde_error = serde_json::from_str::<serde_json::Value>(json).unwrap_err();
		let security_error: SecurityError = serde_error.into();
		assert!(matches!(security_error, SecurityError::ParseError(_)));
	}

	#[test]
	fn test_trace_id_propagation() {
		// Create an error context with a known trace ID
		let error_context = ErrorContext::new("Inner error", None, None);
		let original_trace_id = error_context.trace_id.clone();

		// Wrap it in a SecurityError
		let security_error = SecurityError::ParseError(error_context);

		// Verify the trace ID is preserved
		assert_eq!(security_error.trace_id(), original_trace_id);

		// Test trace ID propagation through error chain
		let source_error = IoError::new(ErrorKind::Other, "Source error");
		let error_context = ErrorContext::new("Middle error", Some(Box::new(source_error)), None);
		let original_trace_id = error_context.trace_id.clone();

		let security_error = SecurityError::ParseError(error_context);
		assert_eq!(security_error.trace_id(), original_trace_id);

		// Test Other variant
		let anyhow_error = anyhow::anyhow!("Test anyhow error");
		let security_error: SecurityError = anyhow_error.into();

		// Other variant should generate a new UUID
		assert!(!security_error.trace_id().is_empty());
	}
}
