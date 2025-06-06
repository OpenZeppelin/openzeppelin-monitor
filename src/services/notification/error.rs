//! Notification error types and handling.
//!
//! Provides error types for notification-related operations,
//! including network issues and configuration problems.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error as ThisError;

/// Represents errors that can occur during notification operations
#[derive(ThisError, Debug)]
pub enum NotificationError {
	/// Errors related to network connectivity issues
	#[error("Network error: {0}")]
	NetworkError(Box<ErrorContext>),

	/// Errors related to malformed requests or invalid responses
	#[error("Config error: {0}")]
	ConfigError(Box<ErrorContext>),

	/// Errors related to internal processing errors
	#[error("Internal error: {0}")]
	InternalError(Box<ErrorContext>),

	/// Errors related to script execution
	#[error("Script execution error: {0}")]
	ExecutionError(Box<ErrorContext>),

	/// Error when Notifier `notify`` method fails (e.g., webhook failure, parsing error, invalid signature)
	#[error("Notification failed: {0}")]
	NotifyFailed(Box<ErrorContext>),
}

impl NotificationError {
	// Network error
	pub fn network_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(Box::new(ErrorContext::new_with_log(msg, source, metadata)))
	}

	// Config error
	pub fn config_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConfigError(Box::new(ErrorContext::new_with_log(msg, source, metadata)))
	}

	// Internal error
	pub fn internal_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(Box::new(ErrorContext::new_with_log(msg, source, metadata)))
	}

	// Execution error
	pub fn execution_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ExecutionError(Box::new(ErrorContext::new_with_log(msg, source, metadata)))
	}

	// Notify failed error
	pub fn notify_failed(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NotifyFailed(Box::new(ErrorContext::new_with_log(msg, source, metadata)))
	}
}

impl TraceableError for NotificationError {
	fn trace_id(&self) -> String {
		match self {
			Self::NetworkError(ctx) => ctx.trace_id.clone(),
			Self::ConfigError(ctx) => ctx.trace_id.clone(),
			Self::InternalError(ctx) => ctx.trace_id.clone(),
			Self::ExecutionError(ctx) => ctx.trace_id.clone(),
			Self::NotifyFailed(ctx) => ctx.trace_id.clone(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	#[test]
	fn test_network_error_formatting() {
		let error = NotificationError::network_error("test error", None, None);
		assert_eq!(error.to_string(), "Network error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = NotificationError::network_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Network error: test error [key1=value1]");
	}

	#[test]
	fn test_config_error_formatting() {
		let error = NotificationError::config_error("test error", None, None);
		assert_eq!(error.to_string(), "Config error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = NotificationError::config_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Config error: test error [key1=value1]");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = NotificationError::internal_error("test error", None, None);
		assert_eq!(error.to_string(), "Internal error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = NotificationError::internal_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Internal error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_execution_error_formatting() {
		let error = NotificationError::execution_error("test error", None, None);
		assert_eq!(error.to_string(), "Script execution error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = NotificationError::execution_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Script execution error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_notify_failed_error_formatting() {
		let error = NotificationError::notify_failed("test error", None, None);
		assert_eq!(error.to_string(), "Notification failed: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = NotificationError::notify_failed(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Notification failed: test error [key1=value1]"
		);
	}

	#[test]
	fn test_error_source_chain() {
		let io_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = NotificationError::network_error(
			"Failed to initialize",
			Some(Box::new(io_error)),
			None,
		);

		// Just test the string representation instead of the source chain
		assert!(outer_error.to_string().contains("Failed to initialize"));

		// For NotificationError::NetworkError, we know the implementation details
		if let NotificationError::NetworkError(ctx) = &outer_error {
			// Check that the context has the right message
			assert_eq!(ctx.message, "Failed to initialize");

			// Check that the context has the source error
			assert!(ctx.source.is_some());

			if let Some(src) = &ctx.source {
				assert_eq!(src.to_string(), "while reading config");
			}
		} else {
			panic!("Expected NetworkError variant");
		}
	}

	#[test]
	fn test_all_error_variants_have_and_propagate_consistent_trace_id() {
		let create_context_with_id = || {
			let context = ErrorContext::new("test message", None, None);
			let original_id = context.trace_id.clone();
			(Box::new(context), original_id)
		};

		let errors_with_ids: Vec<(NotificationError, String)> = vec![
			{
				let (ctx, id) = create_context_with_id();
				(NotificationError::NetworkError(ctx), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(NotificationError::ConfigError(ctx), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(NotificationError::InternalError(ctx), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(NotificationError::ExecutionError(ctx), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(NotificationError::NotifyFailed(ctx), id)
			},
		];

		for (error, original_id) in errors_with_ids {
			let propagated_id = error.trace_id();
			assert!(
				!propagated_id.is_empty(),
				"Error {:?} should have a non-empty trace_id",
				error
			);
			assert_eq!(
				propagated_id, original_id,
				"Trace ID for {:?} was not propagated consistently. Expected: {}, Got: {}",
				error, original_id, propagated_id
			);
		}
	}
}
