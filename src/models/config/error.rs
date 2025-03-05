//! Configuration error types.
//!
//! This module defines the error types that can occur during configuration
//! loading and validation.

use crate::utils::ErrorContext;
use std::collections::HashMap;
use thiserror::Error as ThisError;

/// Represents errors that can occur during configuration operations
#[derive(ThisError, Debug)]
pub enum ConfigError {
	/// Errors related to validation failures
	#[error("Validation error: {0}")]
	ValidationError(ErrorContext),

	/// Errors related to parsing failures
	#[error("Parse error: {0}")]
	ParseError(ErrorContext),

	/// Errors related to file system errors
	#[error("File error: {0}")]
	FileError(ErrorContext),

	/// Other errors that don't fit into the categories above
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

impl ConfigError {
	// Validation error
	pub fn validation_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ValidationError(ErrorContext::new(msg.into(), source, metadata))
	}

	// Parse error
	pub fn parse_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ParseError(ErrorContext::new(msg.into(), source, metadata))
	}

	// File error
	pub fn file_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::FileError(ErrorContext::new(msg.into(), source, metadata))
	}
}

impl From<std::io::Error> for ConfigError {
	fn from(err: std::io::Error) -> Self {
		Self::file_error(err.to_string(), None, None)
	}
}

impl From<serde_json::Error> for ConfigError {
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
		let error = ConfigError::validation_error("test error", None, None);
		assert_eq!(error.to_string(), "Validation error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = ConfigError::validation_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Validation error: test error");
	}

	#[test]
	fn test_execution_error_formatting() {
		let error = ConfigError::parse_error("test error", None, None);
		assert_eq!(error.to_string(), "Parse error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = ConfigError::parse_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Parse error: test error");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = ConfigError::file_error("test error", None, None);
		assert_eq!(error.to_string(), "File error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = ConfigError::file_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "File error: test error");
	}

	#[test]
	fn test_from_anyhow_error() {
		let anyhow_error = anyhow::anyhow!("test anyhow error");
		let config_error: ConfigError = anyhow_error.into();
		assert!(matches!(config_error, ConfigError::Other(_)));
		assert_eq!(config_error.to_string(), "test anyhow error");
	}

	#[test]
	fn test_error_source_chain() {
		use std::error::Error;
		let middle_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = ConfigError::file_error(
			"Failed to initialize",
			Some(Box::new(middle_error) as Box<dyn std::error::Error + Send + Sync>),
			None,
		);

		// Test the source chain
		let source = outer_error.source();
		assert!(source.is_some());
		assert_eq!(source.unwrap().to_string(), "while reading config");
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
