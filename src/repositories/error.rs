//! Error types for repository operations.
//!
//! This module defines the error types that can occur during repository operations,
//! including validation errors, loading errors, and internal errors. It provides
//! a consistent error handling interface across all repository implementations.

use std::collections::HashMap;

use crate::utils::{EnhancedContext, ErrorContext, ErrorContextProvider};

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

impl ErrorContextProvider for RepositoryError {
	fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
		match self {
			Self::ValidationError(ctx) => Some(ctx),
			Self::LoadError(ctx) => Some(ctx),
			Self::InternalError(ctx) => Some(ctx),
		}
	}
}

impl RepositoryError {
	const TARGET: &str = "repository";

	fn format_target(target: Option<&str>) -> String {
		if let Some(target) = target {
			format!("{}::{}", Self::TARGET, target)
		} else {
			Self::TARGET.to_string()
		}
	}

	/// Create a new validation error with logging
	pub fn validation_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::ValidationError(
			ErrorContext::new(
				"Validation Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Create a new validation error with source
	pub fn validation_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::ValidationError(
			ErrorContext::new(
				"Validation Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Create a new load error with logging
	pub fn load_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::LoadError(
			ErrorContext::new(
				"Load Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Create a new load error with source
	pub fn load_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::LoadError(
			ErrorContext::new(
				"Load Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Create a new internal error with logging
	pub fn internal_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				"Internal Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Create a new internal error with source
	pub fn internal_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				"Internal Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
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
		let error = RepositoryError::validation_error("test error", None, None);
		assert!(error.to_string().contains("Validation Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = RepositoryError::validation_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert!(error.to_string().contains("Validation Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = RepositoryError::validation_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert!(error.to_string().contains("Validation Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_load_error_formatting() {
		let error = RepositoryError::load_error("test error", None, None);
		assert!(error.to_string().contains("Load Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = RepositoryError::load_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert!(error.to_string().contains("Load Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = RepositoryError::load_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert!(error.to_string().contains("Load Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = RepositoryError::internal_error("test error", None, None);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = RepositoryError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = RepositoryError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}
}
