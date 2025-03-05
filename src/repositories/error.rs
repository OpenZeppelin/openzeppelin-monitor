//! Error types for repository operations.
//!
//! This module defines the error types that can occur during repository operations,
//! including validation errors, loading errors, and internal errors. It provides
//! a consistent error handling interface across all repository implementations.

use crate::utils::ErrorContext;
use std::collections::HashMap;
use thiserror::Error as ThisError;

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
		Self::ValidationError(ErrorContext::new(msg.into(), source, metadata))
	}

	// Load error
	pub fn load_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::LoadError(ErrorContext::new(msg.into(), source, metadata))
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
	fn test_validation_error_formatting() {
		let error = RepositoryError::validation_error("test error", None, None);
		assert_eq!(error.to_string(), "Validation error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = RepositoryError::validation_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Validation error: test error");
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
		assert_eq!(error.to_string(), "Load error: test error");
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
		assert_eq!(error.to_string(), "Internal error: test error");
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
		use std::error::Error;
		let middle_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = RepositoryError::internal_error(
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
