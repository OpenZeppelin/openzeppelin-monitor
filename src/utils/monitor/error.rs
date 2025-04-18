//! Monitor execution error types and handling.
//!
//! Provides error types for monitor execution against a specific block,
use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error as ThisError;
use uuid::Uuid;

/// Represents possible errors during monitor execution
#[derive(ThisError, Debug)]
pub enum MonitorExecutionError {
	/// Errors related to not found errors
	#[error("Not found error: {0}")]
	NotFound(ErrorContext),

	/// Errors related to execution failures
	#[error("Execution error: {0}")]
	ExecutionError(ErrorContext),

	/// Other errors that don't fit into the categories above
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

impl MonitorExecutionError {
	// Not found error
	pub fn not_found(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NotFound(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Execution error
	pub fn execution_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ExecutionError(ErrorContext::new_with_log(msg, source, metadata))
	}
}

impl TraceableError for MonitorExecutionError {
	fn trace_id(&self) -> String {
		match self {
			Self::NotFound(ctx) => ctx.trace_id.clone(),
			Self::ExecutionError(ctx) => ctx.trace_id.clone(),
			Self::Other(_) => Uuid::new_v4().to_string(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	#[test]
	fn test_not_found_error_formatting() {
		let error = MonitorExecutionError::not_found("test error", None, None);
		assert_eq!(error.to_string(), "Not found error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = MonitorExecutionError::not_found(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Not found error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_execution_error_formatting() {
		let error = MonitorExecutionError::execution_error("test error", None, None);
		assert_eq!(error.to_string(), "Execution error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = MonitorExecutionError::execution_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Execution error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_from_anyhow_error() {
		let anyhow_error = anyhow::anyhow!("test anyhow error");
		let script_error: MonitorExecutionError = anyhow_error.into();
		assert!(matches!(script_error, MonitorExecutionError::Other(_)));
		assert_eq!(script_error.to_string(), "test anyhow error");
	}
}
