//! Error types for repository operations.
//!
//! This module defines the error types that can occur during repository operations,
//! including validation errors, loading errors, and internal errors. It provides
//! a consistent error handling interface across all repository implementations.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error as ThisError;
use uuid::Uuid;

/// Represents errors that can occur during repository operations
#[derive(ThisError, Debug)]
pub enum RepositoryError {
	/// Errors related to validation errors
	#[error("Validation error: {0}")]
	ValidationError(ErrorContext),

	/// Errors related to load errors
	#[error("Load error: {0}")]
	LoadError(ErrorContext),

	/// Errors related to internal errors
	#[error("Internal error: {0}")]
	InternalError(ErrorContext),

	/// Other errors that don't fit into the categories above
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

impl RepositoryError {
	// Validation error
	pub fn validation_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ValidationError(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Load error
	pub fn load_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::LoadError(ErrorContext::new_with_log(msg, source, metadata))
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

impl TraceableError for RepositoryError {
	fn trace_id(&self) -> String {
		match self {
			Self::ValidationError(ctx) => ctx.trace_id.clone(),
			Self::LoadError(ctx) => ctx.trace_id.clone(),
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
	fn test_validation_error_formatting() {
		let error = RepositoryError::validation_error("test error", None, None);
		assert_eq!(error.to_string(), "Validation error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = RepositoryError::validation_error(
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
	fn test_load_error_formatting() {
		let error = RepositoryError::load_error("test error", None, None);
		assert_eq!(error.to_string(), "Load error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = RepositoryError::load_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Load error: test error [key1=value1]");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = RepositoryError::internal_error("test error", None, None);
		assert_eq!(error.to_string(), "Internal error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = RepositoryError::internal_error(
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
		let repository_error: RepositoryError = anyhow_error.into();
		assert!(matches!(repository_error, RepositoryError::Other(_)));
		assert_eq!(repository_error.to_string(), "test anyhow error");
	}

	#[test]
	fn test_error_source_chain() {
		let io_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error =
			RepositoryError::load_error("Failed to initialize", Some(Box::new(io_error)), None);

		// Just test the string representation instead of the source chain
		assert!(outer_error.to_string().contains("Failed to initialize"));

		// For RepositoryError::LoadError, we know the implementation details
		if let RepositoryError::LoadError(ctx) = &outer_error {
			// Check that the context has the right message
			assert_eq!(ctx.message, "Failed to initialize");

			// Check that the context has the source error
			assert!(ctx.source.is_some());

			if let Some(src) = &ctx.source {
				assert_eq!(src.to_string(), "while reading config");
			}
		} else {
			panic!("Expected LoadError variant");
		}
	}

	#[test]
	fn test_trace_id_propagation() {
		// Create an error context with a known trace ID
		let error_context = ErrorContext::new("Inner error", None, None);
		let original_trace_id = error_context.trace_id.clone();

		// Wrap it in a RepositoryError
		let repository_error = RepositoryError::LoadError(error_context);

		// Verify the trace ID is preserved
		assert_eq!(repository_error.trace_id(), original_trace_id);

		// Test trace ID propagation through error chain
		let source_error = IoError::new(ErrorKind::Other, "Source error");
		let error_context = ErrorContext::new("Middle error", Some(Box::new(source_error)), None);
		let original_trace_id = error_context.trace_id.clone();

		let repository_error = RepositoryError::LoadError(error_context);
		assert_eq!(repository_error.trace_id(), original_trace_id);

		// Test Other variant
		let anyhow_error = anyhow::anyhow!("Test anyhow error");
		let repository_error: RepositoryError = anyhow_error.into();

		// Other variant should generate a new UUID
		assert!(!repository_error.trace_id().is_empty());
	}
}
