//! Error types for filter operations.
//!
//! Defines the error cases that can occur during block filtering
//! and provides helper methods for error creation and formatting.

use std::collections::HashMap;

use crate::utils::{new_error, new_error_with_source, ErrorContext, ErrorContextProvider};

/// Represents errors that can occur during filter operations
#[derive(Debug)]
pub enum FilterError {
	/// Error when block type doesn't match expected chain
	BlockTypeMismatch(ErrorContext<String>),
	/// Error during network operations
	NetworkError(ErrorContext<String>),
	/// Internal processing errors
	InternalError(ErrorContext<String>),
}

impl ErrorContextProvider for FilterError {
	fn target() -> &'static str {
		"filter"
	}
	fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
		match self {
			Self::BlockTypeMismatch(ctx) => Some(ctx),
			Self::NetworkError(ctx) => Some(ctx),
			Self::InternalError(ctx) => Some(ctx),
		}
	}
}

impl FilterError {
	/// Creates a new block type mismatch error
	pub fn block_type_mismatch(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::BlockTypeMismatch,
			"Block Type Mismatch Error",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new block type mismatch error with source
	pub fn block_type_mismatch_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::BlockTypeMismatch,
			"Block Type Mismatch Error",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new network error with logging
	pub fn network_error<T: ErrorContextProvider + 'static>(
		msg: impl Into<String>,
		source: Option<T>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		if let Some(source) = source {
			new_error_with_source(
				Self::NetworkError,
				"Network Error",
				msg,
				source,
				metadata,
				target,
			)
		} else {
			new_error(Self::NetworkError, "Network Error", msg, metadata, target)
		}
	}

	/// Creates a new network error with source
	pub fn network_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::NetworkError,
			"Network Error",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new internal error
	pub fn internal_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(Self::InternalError, "Internal Error", msg, metadata, target)
	}

	/// Creates a new internal error with source
	pub fn internal_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::InternalError,
			"Internal Error",
			msg,
			source,
			metadata,
			target,
		)
	}
}

impl std::error::Error for FilterError {}

// Standard error trait implementations
impl std::fmt::Display for FilterError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::BlockTypeMismatch(ctx) => ctx.fmt(f),
			Self::NetworkError(ctx) => ctx.fmt(f),
			Self::InternalError(ctx) => ctx.fmt(f),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_block_type_mismatch_error_formatting() {
		let error = FilterError::block_type_mismatch("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = FilterError::block_type_mismatch_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = FilterError::block_type_mismatch_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_network_error_formatting() {
		let error = FilterError::network_error::<FilterError>("test error", None, None, None);
		assert_eq!(error.to_string(), "test error");

		let error = FilterError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = FilterError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = FilterError::internal_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = FilterError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = FilterError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}
}
