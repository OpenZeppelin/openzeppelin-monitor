//! Error handling utilities for the application.
//!
//! This module provides a structured approach to error handling with context and metadata.
//! The primary type is [`ErrorContext`], which wraps errors with additional information
//! such as timestamps, trace IDs, and custom metadata.
//!
//! # Examples
//!
//! ```
//! use std::collections::HashMap;
//! use crate::utils::error::ErrorContext;
//!
//! // Create a basic error context
//! let error = ErrorContext::new("Failed to process request", None, None);
//!
//! // Add metadata to provide more context
//! let error_with_metadata = ErrorContext::new(
//!     "Database connection failed",
//!     None,
//!     None
//! ).with_metadata("db_host", "localhost")
//!  .with_metadata("retry_count", "3");
//!
//! // Get formatted error message with metadata
//! let message = error_with_metadata.format_with_metadata();
//! ```

use chrono::Utc;
use std::{collections::HashMap, fmt};
use uuid::Uuid;

/// A context wrapper for errors with additional metadata.
///
/// `ErrorContext` provides a way to enrich errors with contextual information,
/// making them more useful for debugging and logging. Each error context includes:
///
/// - A descriptive message
/// - An optional source error
/// - Optional key-value metadata
/// - A timestamp (automatically generated)
/// - A unique trace ID (automatically generated)
///
/// This structure implements both `Display` and `std::error::Error` traits,
/// making it suitable for use in error handling chains.
#[derive(Debug)]
pub struct ErrorContext {
	/// The error message
	pub message: String,
	/// The source error that caused this error
	pub source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
	/// Additional metadata about the error
	pub metadata: Option<HashMap<String, String>>,
	/// The timestamp of the error in RFC 3339 format
	pub timestamp: String,
	/// The unique identifier for the error (UUID v4)
	pub trace_id: String,
}

impl ErrorContext {
	/// Creates a new error context with the given message, source, and metadata.
	///
	/// # Arguments
	///
	/// * `message` - A descriptive error message
	/// * `source` - An optional source error that caused this error
	/// * `metadata` - Optional key-value pairs providing additional context
	///
	/// # Returns
	///
	/// A new `ErrorContext` instance with automatically generated timestamp and trace ID.
	pub fn new(
		message: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self {
			message: message.into(),
			source,
			metadata,
			timestamp: Utc::now().to_rfc3339(),
			trace_id: Uuid::new_v4().to_string(),
		}
	}

	/// Adds a single key-value metadata pair to the error context.
	///
	/// This method creates the metadata HashMap if it doesn't already exist.
	///
	/// # Arguments
	///
	/// * `key` - The metadata key
	/// * `value` - The metadata value
	///
	/// # Returns
	///
	/// The modified `ErrorContext` with the new metadata added.
	pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
		let metadata = self.metadata.get_or_insert_with(HashMap::new);
		metadata.insert(key.into(), value.into());
		self
	}

	/// Formats the error message with its metadata appended in a readable format.
	///
	/// The format is: `"message [key1=value1, key2=value2, ...]"`.
	/// Metadata keys are sorted alphabetically for consistent output.
	///
	/// # Returns
	///
	/// A formatted string containing the error message and its metadata.
	pub fn format_with_metadata(&self) -> String {
		let mut result = self.message.clone();

		if let Some(metadata) = &self.metadata {
			if !metadata.is_empty() {
				let mut parts = Vec::new();
				// Sort keys for consistent output
				let mut keys: Vec<_> = metadata.keys().collect();
				keys.sort();

				for key in keys {
					if let Some(value) = metadata.get(key) {
						parts.push(format!("{}={}", key, value));
					}
				}

				if !parts.is_empty() {
					result.push_str(&format!(" [{}]", parts.join(", ")));
				}
			}
		}

		result
	}
}

impl fmt::Display for ErrorContext {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		// TODO: Add metadata to the error message
		write!(f, "{}", self.message)
	}
}

impl std::error::Error for ErrorContext {}

// Helper function to format the complete error chain
pub fn format_error_chain(err: &anyhow::Error) -> String {
	let mut result = err.to_string();
	let mut source = err.source();

	while let Some(err) = source {
		result.push_str(&format!("\n  Caused by: {}", err));
		source = err.source();
	}

	result
}
