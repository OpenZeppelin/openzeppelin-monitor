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

/// A context for logging errors with additional information
#[derive(Debug)]
pub struct EnhancedContext {
	// The description of the context
	pub description: String,

	// The metadata of the context
	pub metadata: Option<HashMap<String, String>>,

	// The timestamp of the context
	pub timestamp: i64,
}

impl EnhancedContext {
	pub fn new(description: &str) -> Self {
		Self {
			description: description.to_string(),
			metadata: None,
			timestamp: Utc::now().timestamp_millis(),
		}
	}

	pub fn with_metadata(mut self, metadata: Option<HashMap<String, String>>) -> Self {
		self.metadata = metadata;
		self
	}

	pub fn format(&self) -> String {
		let mut parts: Vec<String> = vec![];
		if let Some(metadata) = &self.metadata {
			for (key, value) in metadata {
				parts.push(format!("{}={}", key, value));
			}
		}
		parts.push(format!("timestamp={}", self.timestamp));
		format!("[{}]", parts.join(", "))
	}
}

impl std::fmt::Display for EnhancedContext {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{} {}", self.description, self.format())
	}
}

/// A context for logging errors with additional information
///
/// This struct wraps an error and provides methods to log it with context
/// and track whether it has been logged.
#[derive(Debug)]
pub struct ErrorContext<T> {
	pub message: T,
	pub source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
	pub context: EnhancedContext,
	pub target: Option<String>,
	logged: AtomicBool,
}

impl<T: std::fmt::Display> ErrorContext<T> {
	/// Create a new `ErrorContext` with the given error and context
	///
	/// # Arguments
	/// * `message` - The error to wrap
	/// * `context` - The context to log with the error
	///
	/// # Returns
	pub fn new(message: T, context: EnhancedContext) -> Self {
		Self {
			message,
			source: None,
			context,
			target: None,
			logged: AtomicBool::new(false),
		}
	}

	/// Add a source error to the context
	///
	/// # Arguments
	/// * `source` - The source error to add
	///
	/// # Returns
	pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
		self.source = Some(Box::new(source));
		self
	}

	/// Add a target to the context
	///
	/// # Arguments
	/// * `target` - The target to log the error to
	///
	/// # Returns
	pub fn with_target(mut self, target: impl Into<String>) -> Self {
		self.target = Some(target.into());
		self
	}

	/// Log the error if it hasn't been logged yet
	pub fn log_once(&self) {
		if !self.logged.swap(true, Ordering::SeqCst) {
			let log_message = if let Some(source) = &self.source {
				format!(
					"{}: {} ({}) {}",
					self.context.description,
					self.message,
					source,
					self.context.format()
				)
			} else {
				format!(
					"{}: {} {}",
					self.context.description,
					self.message,
					self.context.format()
				)
			};

			if let Some(target) = &self.target {
				error!(
					target: format!("openzeppelin_monitor::{}", target).as_str(),
					"{}",
					log_message
				);
			} else {
				error!("{}", log_message);
			}
		}
	}

	/// Get the message of the error
	pub fn message(&self) -> &T {
		&self.message
	}
}

impl<T: std::fmt::Display> std::fmt::Display for ErrorContext<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		if let Some(source) = &self.source {
			write!(
				f,
				"{}: {} ({}) {}",
				self.context.description,
				self.message,
				source,
				self.context.format()
			)
		} else {
			write!(
				f,
				"{}: {} {}",
				self.context.description,
				self.message,
				self.context.format()
			)
		}
	}
}

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
		let error = ErrorContext::new("test error", EnhancedContext::new("test context"));
		assert!(error.to_string().contains("test context"));
		assert!(error.to_string().contains("test error"));
		assert!(error.to_string().contains("timestamp="));
	}

	#[test]
	fn test_error_context_log_once() {
		let error = ErrorContext::new("test error", EnhancedContext::new("test context"));
		error.log_once();
		assert!(error.logged.load(Ordering::SeqCst));
	}

	#[test]
	fn test_error_context_log_once_with_source() {
		let error =
			ErrorContext::new("test error", EnhancedContext::new("test context")).with_source(
				std::io::Error::new(std::io::ErrorKind::Other, "test source"),
			);
		error.log_once();
		assert!(error.logged.load(Ordering::SeqCst));
	}

	#[test]
	fn test_error_context_log_once_with_target() {
		let error = ErrorContext::new("test error", EnhancedContext::new("test context"))
			.with_target("test target");
		error.log_once();
		assert!(error.logged.load(Ordering::SeqCst));
	}

	#[test]
	fn test_error_context_log_once_with_source_and_target() {
		let error = ErrorContext::new("test error", EnhancedContext::new("test context"))
			.with_source(std::io::Error::new(
				std::io::ErrorKind::Other,
				"test source",
			))
			.with_target("test target");
		error.log_once();
		assert!(error.logged.load(Ordering::SeqCst));
	}

	#[test]
	fn test_error_context_log_once_called_twice() {
		let logger = setup_test_logger();

		let error = ErrorContext::new("test error", EnhancedContext::new("test context"))
			.with_target("test_specific_target");
		error.log_once();
		error.log_once();

		let messages = logger.get_messages();
		assert_eq!(messages.len(), 1, "Expected 1 message, got: {:?}", messages);
		assert!(messages[0].contains("test context"));
		assert!(messages[0].contains("test error"));
		assert!(messages[0].contains("timestamp="));
	}
}
