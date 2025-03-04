//! Utility for logging errors with additional context
//!
//! This module provides a `ErrorContext` struct that wraps an error and
//! provides methods to log it with context and track whether it has been logged.
//!
//! The `ErrorContext` struct is useful for logging errors in a structured way,
//! with additional context about the error.

use chrono::Utc;
use log::error;
use std::{
	collections::HashMap,
	sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

/// Helper function to format the target of the error
///
/// # Arguments
/// * `target` - The target of the error
///
/// # Returns
/// * `String` - The formatted target
fn format_target<T: ErrorContextProvider>(target: Option<&str>) -> String {
	if let Some(target) = target {
		format!("{}::{}", T::target(), target)
	} else {
		T::target().to_string()
	}
}

/// Helper function to format the target of the error with the source
///
/// # Arguments
/// * `target` - The target of the error
/// * `context` - The context of the error
///
/// # Returns
/// * `String` - The formatted target
pub fn format_target_with_source(
	target: Option<&str>,
	context: Option<&ErrorContext<String>>,
) -> String {
	match (target, context) {
		(Some(target), Some(ctx)) => {
			let source_target = ctx.get_recursive_source_target();
			if source_target.is_empty() {
				target.to_string()
			} else {
				format!("{}::{}", target, source_target)
			}
		}
		(Some(target), None) => target.to_string(),
		(None, Some(ctx)) => ctx.get_recursive_source_target(),
		(None, None) => String::new(),
	}
}

/// Helper function to create a new error of the specified type
///
/// # Arguments
/// * `error_type` - The type of the error
/// * `error_name` - The name of the error
/// * `msg` - The message of the error
/// * `metadata` - The metadata of the error
/// * `target` - The target of the error
///
/// # Returns
/// * `T` - The new error of the specified type
pub fn new_error<T: ErrorContextProvider>(
	error_type: fn(ErrorContext<String>) -> T,
	error_name: &str,
	msg: impl Into<String>,
	metadata: Option<HashMap<String, String>>,
	target: Option<&str>,
) -> T {
	let message = msg.into();
	let target = format_target::<T>(target);
	let context = ErrorContext::new(
		error_name,
		message,
		EnhancedContext::new(None).with_metadata(metadata),
	)
	.with_target(target);

	// Don't log here, we log in at higher level with `new_error_with_source`
	// context.log_once();

	error_type(context)
}

/// Helper function to create a new error with source of the specified type
///
/// # Arguments
/// * `error_type` - The type of the error
/// * `error_name` - The name of the error
/// * `msg` - The message of the error
/// * `source` - The source of the error
/// * `metadata` - The metadata of the error
/// * `target` - The target of the error
///
/// # Returns
/// * `T` - The new error of the specified type
pub fn new_error_with_source<T: ErrorContextProvider>(
	error_type: fn(ErrorContext<String>) -> T,
	error_name: &str,
	msg: impl Into<String>,
	source: impl ErrorContextProvider + 'static,
	metadata: Option<HashMap<String, String>>,
	target: Option<&str>,
) -> T {
	let message = msg.into();
	let target = format_target::<T>(target);
	let context = ErrorContext::new(
		error_name,
		message,
		EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
	)
	.with_target(target);

	context.log_once();

	error_type(context)
}

/// A trait for types that can provide error context and serve as an error source
pub trait ErrorContextProvider: std::error::Error + Send + Sync {
	fn target() -> &'static str
	where
		Self: Sized;
	fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
		None
	}
}

impl ErrorContextProvider for std::io::Error {
	fn target() -> &'static str {
		"std::io::Error"
	}
}
impl<T: std::fmt::Display + std::fmt::Debug + Send + Sync> ErrorContextProvider
	for ErrorContext<T>
{
	fn target() -> &'static str {
		""
	}
	fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
		None
	}
}

/// A context for logging errors with additional information
#[derive(Debug)]
pub struct EnhancedContext {
	// The source of the error (e.g. "std::io::Error")
	pub source: Option<Box<dyn ErrorContextProvider + 'static>>,

	// The metadata of the context
	pub metadata: Option<HashMap<String, String>>,
}

impl EnhancedContext {
	/// Create a new `EnhancedContext` with the given source
	///
	/// # Arguments
	/// * `source` - The source of the error
	///
	/// # Returns
	pub fn new(source: Option<Box<dyn ErrorContextProvider + 'static>>) -> Self {
		Self {
			source,
			metadata: None,
		}
	}

	/// Add metadata to the context
	///
	/// # Arguments
	/// * `metadata` - The metadata to add
	///
	/// # Returns
	pub fn with_metadata(mut self, metadata: Option<HashMap<String, String>>) -> Self {
		self.metadata = metadata;
		self
	}

	/// Format the context
	///
	/// # Returns
	pub fn format(&self) -> String {
		let source = self.format_source();
		let metadata = self.format_metadata();
		if !source.is_empty() && !metadata.is_empty() {
			format!("{} {}", source, metadata)
		} else if !source.is_empty() {
			source
		} else {
			metadata
		}
	}

	/// Format the source
	///
	/// # Returns
	fn format_source(&self) -> String {
		if let Some(source) = &self.source {
			source.to_string()
		} else {
			"".to_string()
		}
	}

	/// Format the metadata
	///
	/// # Returns
	fn format_metadata(&self) -> String {
		let mut parts: Vec<String> = vec![];
		if let Some(metadata) = &self.metadata {
			// Collect keys into a vector and sort them
			let mut keys: Vec<_> = metadata.keys().collect();
			keys.sort();

			// Build parts using sorted keys
			for key in keys {
				parts.push(format!("{}={}", key, metadata.get(key).unwrap()));
			}
			format!("[{}]", parts.join(", "))
		} else {
			"".to_string()
		}
	}
}

impl std::fmt::Display for EnhancedContext {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.format())
	}
}

/// A context for logging errors with additional information
///
/// This struct wraps an error and provides methods to log it with context
/// and track whether it has been logged.
#[derive(Debug)]
pub struct ErrorContext<T> {
	// The message of the error (e.g. "Failed to fetch data")
	pub message: T,

	// The type of the error (e.g. "FilterError")
	pub error_type: String,

	// The context of the error
	pub context: EnhancedContext,

	// The target of the error (e.g. "filter::handle_match")
	pub target: Option<String>,

	// The timestamp of the error (e.g. 1714435200000)
	pub timestamp: i64,

	// The trace ID of the error (e.g. 123e4567-e89b-12d3-a456-426614174000)
	pub trace_id: String,

	// Whether the error has been logged
	logged: AtomicBool,
}

impl<T: std::fmt::Display> ErrorContext<T> {
	/// Create a new `ErrorContext` with the given error and context
	///
	/// # Arguments
	/// * `error_type` - The type of the error
	/// * `message` - The error to wrap
	/// * `context` - The context to log with the error
	///
	/// # Returns
	pub fn new(error_type: &str, message: T, context: EnhancedContext) -> Self {
		Self {
			message,
			error_type: error_type.to_string(),
			context,
			target: None,
			timestamp: Utc::now().timestamp_millis(),
			trace_id: Uuid::new_v4().to_string(),
			logged: AtomicBool::new(false),
		}
	}

	/// Add a target to the context, including any recursive source targets
	///
	/// # Arguments
	/// * `target` - The target to log the error to
	///
	/// # Returns
	pub fn with_target(mut self, target: impl Into<String>) -> Self {
		self.target = Some(target.into());
		self
	}

	/// Format the error message
	///
	/// # Returns
	pub fn format_message(&self) -> String {
		let mut message = format!("{}", self.message);

		// Remove HTML tags and their content using a regex in case of server response errors
		if message.contains('<') {
			let re = regex::Regex::new(r"<[^>]*>.*?</[^>]*>|<[^>]*>").unwrap();
			message = re
				.replace_all(&message, "")
				.to_string()
				.replace('\n', " ")
				.split_whitespace()
				.collect::<Vec<&str>>()
				.join(" ");
		}

		// Get the context formatting
		let context = self.context.format();
		if !context.is_empty() {
			message.push_str(&format!(" ({})", context));
		}

		message
	}

	/// Log the error if it hasn't been logged yet
	pub fn log_once(&self) {
		if !self.logged.swap(true, Ordering::SeqCst) {
			let full_target = if self.target.is_some() {
				format!(
					"openzeppelin_monitor::{}",
					self.get_recursive_source_target()
				)
			} else {
				"openzeppelin_monitor".to_string()
			};

			error!(
				target: &full_target,
				"{}",
				self.format_message()
			);
		}
	}

	pub fn target(&self) -> Option<String> {
		self.target.clone()
	}

	/// Get the target of the error from the source in a recursive manner
	///
	/// # Arguments
	/// * `source` - The source of the error
	///
	/// # Returns
	pub fn get_recursive_source_target(&self) -> String {
		// Start with the current target if it exists
		let mut parts = Vec::new();
		if let Some(target) = &self.target {
			if !target.is_empty() {
				parts.push(target.to_string());
			}
		}

		// Get source targets
		let mut current_error = self.context.source.as_ref();
		let mut depth = 0;
		const MAX_DEPTH: usize = 8;

		while let Some(err) = current_error {
			if depth >= MAX_DEPTH {
				break;
			}

			if let Some(err_ctx) = err.provide_error_context() {
				if let Some(target) = err_ctx.target() {
					if !target.is_empty() {
						parts.push(target.to_string());
					}
				}
				current_error = err_ctx.context.source.as_ref();
			} else {
				break;
			}
			depth += 1;
		}

		// Join non-empty parts with "::"
		parts.join("::")
	}
}

impl<T: std::fmt::Display> std::fmt::Display for ErrorContext<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.format_message())
	}
}
impl<T: std::fmt::Display + std::fmt::Debug> std::error::Error for ErrorContext<T> {}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::{Arc, Mutex};

	#[derive(Clone)]
	struct TestLogger {
		messages: Arc<Mutex<Vec<String>>>,
	}

	impl TestLogger {
		fn new() -> Self {
			Self {
				messages: Arc::new(Mutex::new(Vec::new())),
			}
		}

		fn get_messages(&self) -> Vec<String> {
			self.messages.lock().unwrap().clone()
		}
	}

	impl log::Log for TestLogger {
		fn enabled(&self, metadata: &log::Metadata) -> bool {
			// Only log messages with the target "test_specific_target"
			// This is to ensure that the error is logged only once when tests are run in parallel
			metadata.target().contains("test_specific_target")
		}

		fn log(&self, record: &log::Record) {
			if self.enabled(record.metadata()) {
				self.messages
					.lock()
					.unwrap()
					.push(record.args().to_string());
			}
		}

		fn flush(&self) {}
	}

	// Helper function to setup logger for each test
	fn setup_test_logger() -> Box<TestLogger> {
		let logger = Box::new(TestLogger::new());
		// Reset the log level to ensure no logging occurs before we set up
		log::set_max_level(log::LevelFilter::Off);
		let boxed_logger: Box<dyn log::Log> = Box::new(logger.clone());
		let _ = log::set_boxed_logger(boxed_logger);
		log::set_max_level(log::LevelFilter::Error);
		logger
	}

	#[test]
	fn test_error_context_display() {
		let error = ErrorContext::new(
			"test type",
			"test error",
			EnhancedContext::new(Some(Box::new(std::io::Error::new(
				std::io::ErrorKind::Other,
				"test context",
			)))),
		);
		assert!(error.to_string().contains("test error (test context)"));
	}

	#[test]
	fn test_error_context_log_once() {
		let error = ErrorContext::new(
			"test type",
			"test error",
			EnhancedContext::new(Some(Box::new(std::io::Error::new(
				std::io::ErrorKind::Other,
				"test context",
			)))),
		);
		error.log_once();
		assert!(error.logged.load(Ordering::SeqCst));
	}

	#[test]
	fn test_error_context_log_once_with_source() {
		let error = ErrorContext::new(
			"test type",
			"test error",
			EnhancedContext::new(Some(Box::new(std::io::Error::new(
				std::io::ErrorKind::Other,
				"test context",
			)))),
		);
		error.log_once();
		assert!(error.logged.load(Ordering::SeqCst));
	}

	#[test]
	fn test_error_context_log_once_with_target() {
		let error = ErrorContext::new(
			"test type",
			"test error",
			EnhancedContext::new(Some(Box::new(std::io::Error::new(
				std::io::ErrorKind::Other,
				"test context",
			)))),
		)
		.with_target("test target");
		error.log_once();
		assert!(error.logged.load(Ordering::SeqCst));
	}

	#[test]
	fn test_error_context_log_once_with_source_and_target() {
		let error = ErrorContext::new(
			"test type",
			"test error",
			EnhancedContext::new(Some(Box::new(std::io::Error::new(
				std::io::ErrorKind::Other,
				"test context",
			)))),
		)
		.with_target("test target");
		error.log_once();
		assert!(error.logged.load(Ordering::SeqCst));
	}

	#[test]
	fn test_error_context_log_once_called_twice() {
		let logger = setup_test_logger();

		let error = ErrorContext::new(
			"test type",
			"test error",
			EnhancedContext::new(Some(Box::new(std::io::Error::new(
				std::io::ErrorKind::Other,
				"test context",
			)))),
		)
		.with_target("test_specific_target");
		error.log_once();
		error.log_once();

		let messages = logger.get_messages();
		assert_eq!(messages.len(), 1, "Expected 1 message, got: {:?}", messages);
		assert!(messages[0].contains("test error (test context)"));
	}

	// Add this mock struct
	#[derive(Debug)]
	struct MockError {
		context: Option<ErrorContext<String>>,
	}

	impl std::fmt::Display for MockError {
		fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			write!(f, "mock error")
		}
	}

	impl std::error::Error for MockError {}

	impl ErrorContextProvider for MockError {
		fn target() -> &'static str {
			"MockError"
		}
		fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
			self.context.as_ref()
		}
	}

	#[test]
	fn test_get_recursive_source_target() {
		// Create the innermost error (base)
		let base_context = ErrorContext::new(
			"base type",
			"base message".to_string(),
			EnhancedContext::new(None),
		)
		.with_target("target1");

		// Create first level mock
		let mock1 = MockError {
			context: Some(base_context),
		};

		// Create second level error
		let error2_context = ErrorContext::new(
			"error2 type",
			"error2 message".to_string(),
			EnhancedContext::new(Some(Box::new(mock1))),
		)
		.with_target("target2");

		// Create second level mock
		let mock2 = MockError {
			context: Some(error2_context),
		};

		// Create the top-level error without setting its target
		let error3 = ErrorContext::new(
			"error3 type",
			"error3 message",
			EnhancedContext::new(Some(Box::new(mock2))),
		);

		assert_eq!(
			error3.get_recursive_source_target(),
			// Double entry for target1 as we use with_target which recursively adds the target as
			// well
			"target2::target1"
		);
	}

	#[test]
	fn test_error_context_html_stripping() {
		let html_error = r#"Failed to get transaction receipts: HTTP error 429 Too Many Requests: <html>
	<head><title>429 Too Many Requests</title></head>
    <body>
    <center><h1>429 Too Many Requests</h1></center>
    <hr><center>nginx</center>
    </body>
    </html>"#;
		let error = ErrorContext::new("test type", html_error, EnhancedContext::new(None));
		let formatted = error.format_message();
		assert!(!formatted.contains('<'));
		assert!(!formatted.contains('>'));
		assert!(formatted
			.contains("Failed to get transaction receipts: HTTP error 429 Too Many Requests"));
	}
}
