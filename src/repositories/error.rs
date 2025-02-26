//! Error types for repository operations.
//!
//! This module defines the error types that can occur during repository operations,
//! including validation errors, loading errors, and internal errors. It provides
//! a consistent error handling interface across all repository implementations.

use crate::utils::{EnhancedContext, ErrorContext};

/// Errors that can occur during repository operations
#[derive(Debug)]
pub enum RepositoryError {
	/// Error that occurs when configuration validation fails
	ValidationError(ErrorContext<String>),

	/// Error that occurs when loading configurations from files
	LoadError(ErrorContext<String>),

	/// Error that occurs due to internal repository operations
	InternalError(ErrorContext<String>),
}

impl RepositoryError {
	const TARGET: &str = "repository::error";

	/// Create a new validation error with logging
	pub fn validation_error(msg: impl Into<String>) -> Self {
		Self::ValidationError(
			ErrorContext::new(msg.into(), EnhancedContext::new("Validation Error"))
				.with_target(Self::TARGET),
		)
	}

	/// Create a new validation error with source
	pub fn validation_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
	) -> Self {
		Self::ValidationError(
			ErrorContext::new(msg.into(), EnhancedContext::new("Validation Error"))
				.with_source(source)
				.with_target(Self::TARGET),
		)
	}

	/// Create a new load error with logging
	pub fn load_error(msg: impl Into<String>) -> Self {
		Self::LoadError(
			ErrorContext::new(msg.into(), EnhancedContext::new("Load Error"))
				.with_target(Self::TARGET),
		)
	}

	/// Create a new load error with source
	pub fn load_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
	) -> Self {
		Self::LoadError(
			ErrorContext::new(msg.into(), EnhancedContext::new("Load Error"))
				.with_source(source)
				.with_target(Self::TARGET),
		)
	}

	/// Create a new internal error with logging
	pub fn internal_error(msg: impl Into<String>) -> Self {
		Self::InternalError(
			ErrorContext::new(msg.into(), EnhancedContext::new("Internal Error"))
				.with_target(Self::TARGET),
		)
	}

	/// Create a new internal error with source
	pub fn internal_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(msg.into(), EnhancedContext::new("Internal Error"))
				.with_source(source)
				.with_target(Self::TARGET),
		)
	}
}

impl std::error::Error for RepositoryError {}

// Standard error trait implementations
impl std::fmt::Display for RepositoryError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::ValidationError(ctx) => ctx.fmt(f),
			Self::LoadError(ctx) => ctx.fmt(f),
			Self::InternalError(ctx) => ctx.fmt(f),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_validation_error_formatting() {
		let error = RepositoryError::validation_error("test error");
		assert_eq!(error.to_string(), "Validation Error: test error");

		let error = RepositoryError::validation_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
		);
		assert_eq!(
			error.to_string(),
			"Validation Error: test error (test source)"
		);
	}

	#[test]
	fn test_load_error_formatting() {
		let error = RepositoryError::load_error("test error");
		assert_eq!(error.to_string(), "Load Error: test error");

		let error = RepositoryError::load_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
		);
		assert_eq!(error.to_string(), "Load Error: test error (test source)");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = RepositoryError::internal_error("test error");
		assert_eq!(error.to_string(), "Internal Error: test error");

		let error = RepositoryError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
		);
		assert_eq!(
			error.to_string(),
			"Internal Error: test error (test source)"
		);
	}
}
