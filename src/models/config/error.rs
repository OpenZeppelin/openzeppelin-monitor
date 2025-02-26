//! Configuration error types.
//!
//! This module defines the error types that can occur during configuration
//! loading and validation.

use std::collections::HashMap;

use crate::utils::{EnhancedContext, ErrorContext};

/// Errors that can occur during configuration operations
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum ConfigError {
	/// Configuration validation failed
	ValidationError(ErrorContext<String>),

	/// Failed to parse configuration file
	ParseError(ErrorContext<String>),

	/// File system error during configuration loading
	FileError(ErrorContext<String>),
}

impl ConfigError {
	const TARGET: &str = "config::error";

	/// Create a new validation error with logging
	pub fn validation_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ValidationError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Validation Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Create a new validation error with source
	pub fn validation_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ValidationError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Validation Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Create a new parse error with logging
	pub fn parse_error(msg: impl Into<String>, metadata: Option<HashMap<String, String>>) -> Self {
		Self::ParseError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Parse Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Create a new parse error with source
	pub fn parse_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ParseError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Parse Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Create a new file error with logging
	pub fn file_error(msg: impl Into<String>, metadata: Option<HashMap<String, String>>) -> Self {
		Self::FileError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("File Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Create a new file error with source
	pub fn file_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::FileError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("File Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}
}

impl std::error::Error for ConfigError {}

// Standard error trait implementations
impl std::fmt::Display for ConfigError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::ValidationError(ctx) => ctx.fmt(f),
			Self::ParseError(ctx) => ctx.fmt(f),
			Self::FileError(ctx) => ctx.fmt(f),
		}
	}
}

impl From<std::io::Error> for ConfigError {
	fn from(err: std::io::Error) -> Self {
		Self::file_error_with_source(err.to_string(), err, None)
	}
}

impl From<serde_json::Error> for ConfigError {
	fn from(err: serde_json::Error) -> Self {
		Self::parse_error_with_source(err.to_string(), err, None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_validation_error_formatting() {
		let error = ConfigError::validation_error("test error", None);
		assert!(error.to_string().contains("Validation Error: test error"));
		assert!(error.to_string().contains("[timestamp="));
		let error = ConfigError::validation_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Validation Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = ConfigError::validation_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Validation Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_parse_error_formatting() {
		let error = ConfigError::parse_error("test error", None);
		assert!(error.to_string().contains("Parse Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = ConfigError::parse_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Parse Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = ConfigError::parse_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Parse Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_file_error_formatting() {
		let error = ConfigError::file_error("test error", None);
		assert!(error.to_string().contains("File Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = ConfigError::file_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("File Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = ConfigError::file_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("File Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_io_error_conversion() {
		let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
		let config_error: ConfigError = io_error.into();
		assert!(matches!(config_error, ConfigError::FileError(_)));
	}

	#[test]
	fn test_serde_error_conversion() {
		let json = "invalid json";
		let serde_error = serde_json::from_str::<serde_json::Value>(json).unwrap_err();
		let config_error: ConfigError = serde_error.into();
		assert!(matches!(config_error, ConfigError::ParseError(_)));
	}
}
