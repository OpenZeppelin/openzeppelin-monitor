//! Error types for filter operations.
//!
//! Defines the error cases that can occur during block filtering
//! and provides helper methods for error creation and formatting.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error as ThisError;
use uuid::Uuid;

/// Represents errors that can occur during filter operations
#[derive(ThisError, Debug)]
pub enum FilterError {
	/// Errors related to network connectivity issues
	#[error("Block type mismatch error: {0}")]
	BlockTypeMismatch(ErrorContext),

	/// Errors related to malformed requests or invalid responses
	#[error("Network error: {0}")]
	NetworkError(ErrorContext),

	/// Errors related to internal processing errors
	#[error("Internal error: {0}")]
	InternalError(ErrorContext),

	/// Other errors that don't fit into the categories above
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

impl FilterError {
	// Block type mismatch error
	pub fn block_type_mismatch(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::BlockTypeMismatch(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Network error
	pub fn network_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Internal error
	pub fn internal_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(ErrorContext::new_with_log(msg, source, metadata))
	}
}

impl TraceableError for FilterError {
	fn trace_id(&self) -> String {
		match self {
			Self::BlockTypeMismatch(ctx) => ctx.trace_id.clone(),
			Self::NetworkError(ctx) => ctx.trace_id.clone(),
			Self::InternalError(ctx) => ctx.trace_id.clone(),
			Self::Other(_) => Uuid::new_v4().to_string(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	#[test]
	fn test_block_type_mismatch_error_formatting() {
		let error = FilterError::block_type_mismatch("test error", None, None);
		assert_eq!(error.to_string(), "Block type mismatch error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = FilterError::block_type_mismatch(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Block type mismatch error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_network_error_formatting() {
		let error = FilterError::network_error("test error", None, None);
		assert_eq!(error.to_string(), "Network error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = FilterError::network_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Network error: test error [key1=value1]");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = FilterError::internal_error("test error", None, None);
		assert_eq!(error.to_string(), "Internal error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = FilterError::internal_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Internal error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_from_anyhow_error() {
		let anyhow_error = anyhow::anyhow!("test anyhow error");
		let filter_error: FilterError = anyhow_error.into();
		assert!(matches!(filter_error, FilterError::Other(_)));
		assert_eq!(filter_error.to_string(), "test anyhow error");
	}

	#[test]
	fn test_error_source_chain() {
		let io_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = FilterError::block_type_mismatch(
			"Failed to initialize",
			Some(Box::new(io_error)),
			None,
		);

		// Just test the string representation instead of the source chain
		assert!(outer_error.to_string().contains("Failed to initialize"));

		// For FilterError::BlockTypeMismatch, we know the implementation details
		if let FilterError::BlockTypeMismatch(ctx) = &outer_error {
			// Check that the context has the right message
			assert_eq!(ctx.message, "Failed to initialize");

			// Check that the context has the source error
			assert!(ctx.source.is_some());

			if let Some(src) = &ctx.source {
				assert_eq!(src.to_string(), "while reading config");
			}
		} else {
			panic!("Expected BlockTypeMismatch variant");
		}
	}

	#[test]
	fn test_trace_id_propagation() {
		// Create an error context with a known trace ID
		let error_context = ErrorContext::new("Inner error", None, None);
		let original_trace_id = error_context.trace_id.clone();

		// Wrap it in a FilterError
		let filter_error = FilterError::BlockTypeMismatch(error_context);

		// Verify the trace ID is preserved
		assert_eq!(filter_error.trace_id(), original_trace_id);

		// Test trace ID propagation through error chain
		let source_error = IoError::new(ErrorKind::Other, "Source error");
		let error_context = ErrorContext::new("Middle error", Some(Box::new(source_error)), None);
		let original_trace_id = error_context.trace_id.clone();

		let filter_error = FilterError::BlockTypeMismatch(error_context);
		assert_eq!(filter_error.trace_id(), original_trace_id);

		// Test Other variant
		let anyhow_error = anyhow::anyhow!("Test anyhow error");
		let filter_error: FilterError = anyhow_error.into();

		// Other variant should generate a new UUID
		assert!(!filter_error.trace_id().is_empty());
	}
}
