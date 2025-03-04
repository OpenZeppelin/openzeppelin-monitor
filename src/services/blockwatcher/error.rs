//! Block watcher error types and handling.
//!
//! Provides a comprehensive error handling system for block watching operations,
//! including scheduling, network connectivity, and storage operations.

use std::collections::HashMap;

use crate::utils::{new_error, new_error_with_source, ErrorContext, ErrorContextProvider};

/// Represents possible errors that can occur during block watching operations
#[derive(Debug)]
pub enum BlockWatcherError {
	/// Errors related to job scheduling operations
	///
	/// Examples include:
	/// - Failed to create scheduler
	/// - Failed to add/remove jobs
	/// - Failed to start/stop scheduler
	SchedulerError(ErrorContext<String>),

	/// Errors related to network operations
	///
	/// Examples include:
	/// - Failed to connect to blockchain node
	/// - Failed to retrieve blocks
	/// - RPC request failures
	NetworkError(ErrorContext<String>),

	/// Errors related to block processing
	///
	/// Examples include:
	/// - Failed to parse block data
	/// - Failed to process transactions
	/// - Handler execution failures
	ProcessingError(ErrorContext<String>),

	/// Errors related to block storage operations
	///
	/// Examples include:
	/// - Failed to save blocks
	/// - Failed to retrieve last processed block
	/// - File system errors
	StorageError(ErrorContext<String>),

	/// Errors related to block tracker operations
	///
	/// Examples include:
	/// - Failed to record block
	/// - Failed to retrieve last processed block
	/// - Errors related to ordered blocks
	BlockTrackerError(ErrorContext<String>),
}

impl ErrorContextProvider for BlockWatcherError {
	fn target() -> &'static str {
		"blockwatcher"
	}
	fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
		match self {
			Self::SchedulerError(ctx) => Some(ctx),
			Self::NetworkError(ctx) => Some(ctx),
			Self::ProcessingError(ctx) => Some(ctx),
			Self::StorageError(ctx) => Some(ctx),
			Self::BlockTrackerError(ctx) => Some(ctx),
		}
	}
}

impl BlockWatcherError {
	/// Creates a new scheduler error with logging
	pub fn scheduler_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::SchedulerError,
			"Scheduler Error",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new scheduler error with source
	pub fn scheduler_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::SchedulerError,
			"Scheduler Error",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new network error with logging
	pub fn network_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(Self::NetworkError, "Network Error", msg, metadata, target)
	}

	/// Creates a new network error with source
	pub fn network_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::NetworkError,
			"Network Error",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new processing error with logging
	pub fn processing_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::ProcessingError,
			"Processing Error",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new processing error with source
	pub fn processing_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::ProcessingError,
			"Processing Error",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new storage error with logging
	pub fn storage_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(Self::StorageError, "Storage Error", msg, metadata, target)
	}

	/// Creates a new storage error with source
	pub fn storage_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::StorageError,
			"Storage Error",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new block tracker error with logging
	pub fn block_tracker_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::BlockTrackerError,
			"Block Tracker Error",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new block tracker error with source
	pub fn block_tracker_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::BlockTrackerError,
			"Block Tracker Error",
			msg,
			source,
			metadata,
			target,
		)
	}
}

impl std::error::Error for BlockWatcherError {}

impl std::fmt::Display for BlockWatcherError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::SchedulerError(ctx) => ctx.fmt(f),
			Self::NetworkError(ctx) => ctx.fmt(f),
			Self::ProcessingError(ctx) => ctx.fmt(f),
			Self::StorageError(ctx) => ctx.fmt(f),
			Self::BlockTrackerError(ctx) => ctx.fmt(f),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_scheduler_error_formatting() {
		let error = BlockWatcherError::scheduler_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockWatcherError::scheduler_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");
	}

	#[test]
	fn test_network_error_formatting() {
		let error = BlockWatcherError::network_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockWatcherError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = BlockWatcherError::network_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_processing_error_formatting() {
		let error = BlockWatcherError::processing_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockWatcherError::processing_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = BlockWatcherError::processing_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_storage_error_formatting() {
		let error = BlockWatcherError::storage_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockWatcherError::storage_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = BlockWatcherError::storage_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_block_tracker_error_formatting() {
		let error = BlockWatcherError::block_tracker_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockWatcherError::block_tracker_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");
		let error = BlockWatcherError::block_tracker_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}
}
