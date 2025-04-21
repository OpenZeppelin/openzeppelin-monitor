//! Error handling utilities for the application.
//!
//! This module provides a structured approach to error handling with context and metadata.
//! The primary type is [`ErrorContext`], which wraps errors with additional information
//! such as timestamps, trace IDs, and custom metadata.

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
		let trace_id = if let Some(ref src) = source {
			// Try to extract trace ID using the TraceableError trait
			TraceableError::trace_id(src.as_ref())
		} else {
			Uuid::new_v4().to_string()
		};

		Self {
			message: message.into(),
			source,
			metadata,
			timestamp: Utc::now().to_rfc3339(),
			trace_id,
		}
	}

	/// Creates a new error context and logs it with the given message, source, and metadata.
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
	pub fn new_with_log(
		message: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		// Create the error context
		let error_context = Self::new(message, source, metadata);

		// Log the error
		log_error(&error_context);

		error_context
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
		write!(f, "{}", self.format_with_metadata())
	}
}

// Add this implementation with Send + Sync bounds
impl std::error::Error for ErrorContext {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		self.source
			.as_ref()
			.map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
	}
}

// Ensure ErrorContext is Send + Sync
unsafe impl Send for ErrorContext {}
unsafe impl Sync for ErrorContext {}

/// A trait for errors that can provide a trace ID
pub trait TraceableError: std::error::Error + Send + Sync {
	/// Returns the trace ID for this error
	fn trace_id(&self) -> String;
}

impl TraceableError for dyn std::error::Error + Send + Sync + 'static {
	fn trace_id(&self) -> String {
		// First check if this error itself has a trace ID
		if let Some(id) = try_extract_trace_id(self) {
			return id;
		}

		// Then check the source chain to retain existing trace IDs
		let mut source = self.source();
		const MAX_DEPTH: usize = 3; // Limit the recursion depth
		let mut depth = 0;

		while let Some(err) = source {
			depth += 1;
			if depth > MAX_DEPTH {
				break;
			}

			// Try to extract trace ID from this error in the chain
			if let Some(id) = try_extract_trace_id(err) {
				return id;
			}

			// Continue with the next source
			source = err.source();
		}

		// If no trace ID found, generate a new one
		Uuid::new_v4().to_string()
	}
}

/// Helper function to try extracting a trace ID from an error
fn try_extract_trace_id(err: &(dyn std::error::Error + 'static)) -> Option<String> {
	// First check if this error is an ErrorContext
	if let Some(ctx) = err.downcast_ref::<ErrorContext>() {
		return Some(ctx.trace_id.clone());
	}

	// Define a macro to try downcasting to each error type
	macro_rules! try_downcast {
		($($ty:path),*) => {
			$(
				if let Some(e) = err.downcast_ref::<$ty>() {
					return Some(e.trace_id());
				}
			)*
		}
	}

	// Try all error types
	try_downcast!(
		crate::services::notification::NotificationError,
		crate::services::trigger::TriggerError,
		crate::services::filter::FilterError,
		crate::services::blockwatcher::BlockWatcherError,
		crate::services::blockchain::BlockChainError,
		crate::repositories::RepositoryError,
		crate::services::trigger::ScriptError,
		crate::models::ConfigError
	);

	// No match found
	None
}

/// Sanitize error messages to remove HTML content
fn sanitize_error_message(message: &str) -> String {
	if message.contains("<html>") || message.contains("<head>") || message.contains("<body>") {
		if let Some(pos) = message.find('<') {
			return message[..pos].trim().to_string();
		}
	}
	message.to_string()
}

/// Helper function to format the complete error chain
fn format_error_chain(err: &dyn std::error::Error) -> String {
	let mut result = sanitize_error_message(&err.to_string());
	let mut source = err.source();

	while let Some(err) = source {
		result.push_str("\n\tCaused by: ");
		result.push_str(&sanitize_error_message(&err.to_string()));
		source = err.source();
	}

	result
}

/// Extract structured fields from metadata for tracing
pub fn metadata_to_fields(metadata: &Option<HashMap<String, String>>) -> Vec<(&str, &str)> {
	let mut fields = Vec::new();
	if let Some(metadata) = metadata {
		for (key, value) in metadata {
			fields.push((key.as_str(), value.as_str()));
		}
	}
	fields
}

/// Log the error with structured fields
fn log_error(error: &ErrorContext) {
	if let Some(err) = &error.source {
		tracing::error!(
			message = error.format_with_metadata(),
			trace_id = %error.trace_id,
			timestamp = %error.timestamp,
			error.chain = %format_error_chain(&**err),
			"Error occurred"
		);
	} else {
		tracing::error!(
			message = error.format_with_metadata(),
			trace_id = %error.trace_id,
			timestamp = %error.timestamp,
			"Error occurred"
		);
	}
}

#[cfg(test)]
mod tests {
	use crate::services::notification::NotificationError;

	use super::*;
	use std::io;

	#[test]
	fn test_new_error_context() {
		let error = ErrorContext::new("Test error", None, None);

		assert_eq!(error.message, "Test error");
		assert!(error.source.is_none());
		assert!(error.metadata.is_none());
		assert!(!error.timestamp.is_empty());
		assert!(!error.trace_id.is_empty());
	}

	#[test]
	fn test_with_metadata() {
		let error = ErrorContext::new("Test error", None, None)
			.with_metadata("key1", "value1")
			.with_metadata("key2", "value2");

		let metadata = error.metadata.unwrap();
		assert_eq!(metadata.get("key1"), Some(&"value1".to_string()));
		assert_eq!(metadata.get("key2"), Some(&"value2".to_string()));
	}

	#[test]
	fn test_format_with_metadata() {
		let error = ErrorContext::new("Test error", None, None)
			.with_metadata("a", "1")
			.with_metadata("b", "2");

		// Keys are sorted alphabetically in the output
		assert_eq!(error.format_with_metadata(), "Test error [a=1, b=2]");
	}

	#[test]
	fn test_display_implementation() {
		let error = ErrorContext::new("Test error", None, None).with_metadata("key", "value");

		assert_eq!(format!("{}", error), "Test error [key=value]");
	}

	#[test]
	fn test_with_source_error() {
		let source_error = io::Error::new(io::ErrorKind::NotFound, "File not found");
		let boxed_source = Box::new(source_error) as Box<dyn std::error::Error + Send + Sync>;

		let error = ErrorContext::new("Failed to read config", Some(boxed_source), None);

		assert_eq!(error.message, "Failed to read config");
		assert!(error.source.is_some());
	}

	#[test]
	fn test_metadata_to_fields() {
		let mut metadata = HashMap::new();
		metadata.insert("key1".to_string(), "value1".to_string());
		metadata.insert("key2".to_string(), "value2".to_string());

		let metadata = Some(metadata);

		let fields = metadata_to_fields(&metadata);

		// Since HashMap doesn't guarantee order, we need to check contents without assuming order
		assert_eq!(fields.len(), 2);
		assert!(fields.contains(&("key1", "value1")));
		assert!(fields.contains(&("key2", "value2")));
	}

	#[test]
	fn test_format_error_chain() {
		// Create a chain of errors
		let inner_error = io::Error::new(io::ErrorKind::PermissionDenied, "Permission denied");
		let middle_error =
			ErrorContext::new("Failed to open file", Some(Box::new(inner_error)), None);
		let outer_error =
			ErrorContext::new("Config loading failed", Some(Box::new(middle_error)), None);

		let formatted = format_error_chain(&outer_error);

		assert!(formatted.contains("Config loading failed"));
		assert!(formatted.contains("Caused by: Failed to open file"));
		assert!(formatted.contains("Caused by: Permission denied"));
	}

	#[test]
	#[cfg_attr(not(feature = "test-ci-only"), ignore)]
	fn test_log_error() {
		use tracing_test::traced_test;

		#[traced_test]
		fn inner_test() {
			let error = ErrorContext::new("Test log error", None, None)
				.with_metadata("test_key", "test_value");

			log_error(&error);

			// Verify log contains our error information
			assert!(logs_contain("Test log error"));
			assert!(logs_contain(&error.trace_id));
			assert!(logs_contain(&error.timestamp));

			// Test with source error
			let source_error = std::io::Error::new(std::io::ErrorKind::Other, "Source error");
			let error_with_source =
				ErrorContext::new("Parent error", Some(Box::new(source_error)), None);

			log_error(&error_with_source);

			assert!(logs_contain("Parent error"));
			assert!(logs_contain("Source error"));
		}

		inner_test();
	}

	// Custom error type for testing
	#[derive(Debug)]
	struct TestError {
		message: String,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
	}

	impl fmt::Display for TestError {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			write!(f, "{}", self.message)
		}
	}

	impl std::error::Error for TestError {
		fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
			self.source
				.as_ref()
				.map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
		}
	}

	#[test]
	fn test_trace_id_propagation() {
		// Create an inner error with ErrorContext
		let inner_error = ErrorContext::new(
			"Inner error",
			None,
			Some(HashMap::from([("key".to_string(), "value".to_string())])),
		);

		// Get the trace ID from the inner error
		let inner_trace_id = inner_error.trace_id.clone();

		// Create a middle error that wraps the inner error
		let middle_error = TestError {
			message: "Middle error".to_string(),
			source: Some(Box::new(inner_error)),
		};

		// Create an outer error with ErrorContext that wraps the middle error
		let outer_error = ErrorContext::new("Outer error", Some(Box::new(middle_error)), None);

		// Get the trace ID from the outer error
		let outer_trace_id = outer_error.trace_id.clone();

		// Verify that the trace IDs match
		assert_eq!(
			inner_trace_id, outer_trace_id,
			"Trace IDs should match between inner and outer errors"
		);

		// Test the TraceableError implementation
		let dyn_error: &(dyn std::error::Error + Send + Sync) = &outer_error;
		let trace_id = TraceableError::trace_id(dyn_error);

		assert_eq!(
			inner_trace_id, trace_id,
			"Trace ID from TraceableError should match the original trace ID"
		);
	}

	#[test]
	fn test_error_sanitization() {
		// Test HTML sanitization
		let html_error = "Error occurred<html><body>Some HTML content</body></html>";
		let sanitized = sanitize_error_message(html_error);
		assert_eq!(
			sanitized, "Error occurred",
			"HTML content should be removed"
		);

		// Test normal error message
		let normal_error = "This is a normal error message";
		let sanitized = sanitize_error_message(normal_error);
		assert_eq!(
			sanitized, normal_error,
			"Normal error should remain unchanged"
		);
	}

	#[test]
	fn test_try_extract_trace_id() {
		// Test extracting from ErrorContext
		let error_ctx = ErrorContext::new("Test error", None, None);
		let expected_trace_id = error_ctx.trace_id.clone();

		let dyn_error: &(dyn std::error::Error + 'static) = &error_ctx;
		let extracted = try_extract_trace_id(dyn_error);

		assert_eq!(
			extracted,
			Some(expected_trace_id),
			"Should extract trace ID from ErrorContext"
		);

		// Test with non-traceable error
		let std_error = io::Error::new(io::ErrorKind::Other, "Standard error");
		let dyn_error: &(dyn std::error::Error + 'static) = &std_error;
		let extracted = try_extract_trace_id(dyn_error);

		assert_eq!(
			extracted, None,
			"Should return None for non-traceable errors"
		);
	}

	// Mock error types to test the try_downcast macro
	#[derive(Debug)]
	struct MockTraceableError {
		trace_id: String,
	}

	impl MockTraceableError {
		fn new() -> Self {
			Self {
				trace_id: Uuid::new_v4().to_string(),
			}
		}
	}

	impl fmt::Display for MockTraceableError {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			write!(f, "Mock traceable error")
		}
	}

	impl std::error::Error for MockTraceableError {}

	impl TraceableError for MockTraceableError {
		fn trace_id(&self) -> String {
			self.trace_id.clone()
		}
	}

	#[test]
	fn test_trace_id_extraction_with_custom_implementation() {
		// Create a mock error that implements TraceableError
		let mock_error = MockTraceableError::new();
		let expected_trace_id = mock_error.trace_id.clone();

		// We need to test the actual implementation of TraceableError for dyn Error
		let dyn_error: &(dyn std::error::Error + Send + Sync) = &mock_error;
		let extracted = TraceableError::trace_id(dyn_error);

		assert!(
			extracted != expected_trace_id,
			"Should not extract trace ID from custom error types since it's not in the \
			 try_downcast! macro list"
		);
	}

	#[test]
	fn test_trace_id_propagation_through_error_chain() {
		let mock_error = NotificationError::config_error("Test error", None, None);
		let expected_trace_id = mock_error.trace_id();

		// First, box our error
		let boxed_error: Box<dyn std::error::Error + Send + Sync> = Box::new(mock_error);

		// Now create an error context with this as the source
		let error_ctx = ErrorContext::new("Outer error", Some(boxed_error), None);

		// The trace ID should be extracted from our mock error
		assert_eq!(
			error_ctx.trace_id, expected_trace_id,
			"Trace ID should propagate through the error chain"
		);
	}
}
