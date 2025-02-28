//! Script error types and handling.
//!
//! Provides error types for script-related operations,
//! including execution failures and configuration issues.

use log::error;
use std::{error::Error, fmt};

/// Represents possible errors during script operations
#[derive(Debug, Clone)]
pub enum ScriptError {
	/// When a requested script cannot be found
	NotFound(String),
	/// When script execution fails
	ExecutionError(String),
	/// When script configuration is invalid
	ParseError(String),
	/// When a system error occurs
	SystemError(String),
}

impl ScriptError {
	/// Formats the error message based on the error type
	fn format_message(&self) -> String {
		match self {
			ScriptError::NotFound(msg) => format!("Script not found: {}", msg),
			ScriptError::ExecutionError(msg) => format!("Script execution error: {}", msg),
			ScriptError::ParseError(msg) => {
				format!("Script parse error: {}", msg)
			}
			ScriptError::SystemError(msg) => {
				format!("System error: {}", msg)
			}
		}
	}

	/// Creates a new not found error with logging
	pub fn not_found(msg: impl Into<String>) -> Self {
		let error = ScriptError::NotFound(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new execution error with logging
	pub fn execution_error(msg: impl Into<String>) -> Self {
		let error = ScriptError::ExecutionError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new configuration error with logging
	pub fn parse_error(msg: impl Into<String>) -> Self {
		let error = ScriptError::ParseError(msg.into());
		error!("{}", error.format_message());
		error
	}

	pub fn system_error(msg: impl Into<String>) -> Self {
		let error = ScriptError::SystemError(msg.into());
		error!("{}", error.format_message());
		error
	}
}

impl fmt::Display for ScriptError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.format_message())
	}
}

impl Error for ScriptError {}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_script_error_not_found() {
		let error = ScriptError::not_found("test script");
		match &error {
			ScriptError::NotFound(msg) => {
				assert_eq!(msg, "test script");
				assert_eq!(error.to_string(), "Script not found: test script");
			}
			_ => panic!("Expected NotFound error"),
		}
	}

	#[test]
	fn test_script_error_system_error() {
		let error = ScriptError::system_error("system failure");
		match &error {
			ScriptError::SystemError(msg) => {
				assert_eq!(msg, "system failure");
				assert_eq!(error.to_string(), "System error: system failure");
			}
			_ => panic!("Expected SystemError"),
		}
	}

	#[test]
	fn test_script_error_display() {
		let errors = vec![
			(
				ScriptError::not_found("script.py"),
				"Script not found: script.py",
			),
			(
				ScriptError::execution_error("failed"),
				"Script execution error: failed",
			),
			(
				ScriptError::parse_error("invalid"),
				"Script parse error: invalid",
			),
			(ScriptError::system_error("crash"), "System error: crash"),
		];

		for (error, expected) in errors {
			assert_eq!(error.to_string(), expected);
		}
	}
}
