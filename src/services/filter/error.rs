//! Error types for filter operations.
//!
//! Defines the error cases that can occur during block filtering
//! and provides helper methods for error creation and formatting.

use std::collections::HashMap;

use crate::utils::{EnhancedContext, ErrorContext, ErrorContextProvider};

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
	fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
		match self {
			Self::BlockTypeMismatch(ctx) => Some(ctx),
			Self::NetworkError(ctx) => Some(ctx),
			Self::InternalError(ctx) => Some(ctx),
		}
	}
}

impl FilterError {
	const TARGET: &str = "filter";

	fn format_target(target: Option<&str>) -> String {
		if let Some(target) = target {
			format!("{}::{}", Self::TARGET, target)
		} else {
			Self::TARGET.to_string()
		}
	}
	/// Creates a new block type mismatch error
	pub fn block_type_mismatch(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::BlockTypeMismatch(
			ErrorContext::new(
				"Block Type Mismatch Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new block type mismatch error with source
	pub fn block_type_mismatch_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::BlockTypeMismatch(
			ErrorContext::new(
				"Block Type Mismatch Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new network error
	pub fn network_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::NetworkError(
			ErrorContext::new(
				"Network Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new network error with source
	pub fn network_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::NetworkError(
			ErrorContext::new(
				"Network Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new internal error
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

	/// Creates a new internal error with source
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
		let error = FilterError::network_error("test error", None, None);
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
