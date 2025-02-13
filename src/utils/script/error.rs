//! Script error types and handling.
//!
//! Provides error types for script-related operations,
//! including execution failures and configuration issues.

use log::error;
use std::{error::Error, fmt};

/// Represents possible errors during script operations
#[derive(Debug)]
pub enum ScriptError {
	/// When a requested script cannot be found
	NotFound(String),
	/// When script execution fails
	ExecutionError(String),
	/// When script configuration is invalid
	ParseError(String),
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
}

impl fmt::Display for ScriptError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.format_message())
	}
}

impl Error for ScriptError {}
