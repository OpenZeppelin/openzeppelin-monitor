//! Trigger error types and handling.
//!
//! Provides error types for trigger-related operations,
//! including execution failures and configuration issues.

use log::error;
use std::{error::Error, fmt};

/// Represents possible errors during trigger operations
#[derive(Debug)]
pub enum TriggerError {
	/// When a requested trigger cannot be found
	NotFound(String),
	/// When trigger execution fails
	ExecutionError(String),
	/// When trigger configuration is invalid
	ConfigurationError(String),
}

impl TriggerError {
	/// Formats the error message based on the error type
	fn format_message(&self) -> String {
		match self {
			TriggerError::NotFound(msg) => format!("Trigger not found: {}", msg),
			TriggerError::ExecutionError(msg) => format!("Trigger execution error: {}", msg),
			TriggerError::ConfigurationError(msg) => {
				format!("Trigger configuration error: {}", msg)
			}
		}
	}

	/// Creates a new not found error with logging
	pub fn not_found(msg: impl Into<String>) -> Self {
		let error = TriggerError::NotFound(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new execution error with logging
	pub fn execution_error(msg: impl Into<String>) -> Self {
		let error = TriggerError::ExecutionError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new configuration error with logging
	pub fn configuration_error(msg: impl Into<String>) -> Self {
		let error = TriggerError::ConfigurationError(msg.into());
		error!("{}", error.format_message());
		error
	}
}

impl fmt::Display for TriggerError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.format_message())
	}
}

impl Error for TriggerError {}
