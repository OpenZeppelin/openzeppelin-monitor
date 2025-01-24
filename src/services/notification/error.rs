//! Notification error types and handling.
//!
//! Provides error types for notification-related operations,
//! including network issues and configuration problems.

use log::error;
use std::{error::Error, fmt};

/// Represents possible errors during notification operations
#[derive(Debug)]
pub enum NotificationError {
	/// Network-related errors (e.g., webhook failures)
	NetworkError(String),
	/// Configuration-related errors
	ConfigError(String),
	/// Internal errors (e.g., failed to build email)
	InternalError(String),
}

use reqwest;

impl NotificationError {
	/// Formats the error message based on the error type
	fn format_message(&self) -> String {
		match self {
			Self::NetworkError(msg) => format!("Network error: {}", msg),
			Self::ConfigError(msg) => format!("Config error: {}", msg),
			Self::InternalError(msg) => format!("Internal error: {}", msg),
		}
	}

	/// Creates a new network error with logging
	pub fn network_error(msg: impl Into<String>) -> Self {
		let error = Self::NetworkError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new configuration error with logging
	pub fn config_error(msg: impl Into<String>) -> Self {
		let error = Self::ConfigError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new internal error with logging
	pub fn internal_error(msg: impl Into<String>) -> Self {
		let error = Self::InternalError(msg.into());
		error!("{}", error.format_message());
		error
	}
}

impl From<reqwest::Error> for NotificationError {
	fn from(error: reqwest::Error) -> Self {
		Self::network_error(error.to_string())
	}
}

impl fmt::Display for NotificationError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.format_message())
	}
}

impl Error for NotificationError {}
