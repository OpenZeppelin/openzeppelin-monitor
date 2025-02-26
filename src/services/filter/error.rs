//! Error types for filter operations.
//!
//! Defines the error cases that can occur during block filtering
//! and provides helper methods for error creation and formatting.

use std::collections::HashMap;

use crate::utils::{EnhancedContext, ErrorContext};

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

impl FilterError {
	const TARGET: &str = "filter::error";

	/// Creates a new block type mismatch error
	pub fn block_type_mismatch(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::BlockTypeMismatch(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Block Type Mismatch Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new block type mismatch error with source
	pub fn block_type_mismatch_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::BlockTypeMismatch(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Block Type Mismatch Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new network error
	pub fn network_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Network Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new network error with source
	pub fn network_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Network Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new internal error
	pub fn internal_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Internal Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new internal error with source
	pub fn internal_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Internal Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
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
		let error = FilterError::block_type_mismatch("test error", None);
		assert!(error
			.to_string()
			.contains("Block Type Mismatch Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = FilterError::block_type_mismatch_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error
			.to_string()
			.contains("Block Type Mismatch Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = FilterError::block_type_mismatch_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error
			.to_string()
			.contains("Block Type Mismatch Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_network_error_formatting() {
		let error = FilterError::network_error("test error", None);
		assert!(error.to_string().contains("Network Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = FilterError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Network Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));

		let error = FilterError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Network Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = FilterError::internal_error("test error", None);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = FilterError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));

		let error = FilterError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}
}
