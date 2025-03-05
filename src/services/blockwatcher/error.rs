//! Block watcher error types and handling.
//!
//! Provides a comprehensive error handling system for block watching operations,
//! including scheduling, network connectivity, and storage operations.

use crate::utils::ErrorContext;
use std::collections::HashMap;
use thiserror::Error as ThisError;

/// Represents possible errors that can occur during block watching operations
#[derive(ThisError, Debug)]
pub enum BlockWatcherError {
	/// Errors related to network connectivity issues
	#[error("Scheduler error: {0}")]
	SchedulerError(ErrorContext),

	/// Errors related to malformed requests or invalid responses
	#[error("Network error: {0}")]
	NetworkError(ErrorContext),

	/// When a requested block cannot be found on the blockchain
	#[error("Processing error: {0}")]
	ProcessingError(ErrorContext),

	/// Errors related to transaction processing
	#[error("Storage error: {0}")]
	StorageError(ErrorContext),

	/// Internal errors within the blockchain client
	#[error("Block tracker error: {0}")]
	BlockTrackerError(ErrorContext),

	/// Other errors that don't fit into the categories above
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

impl BlockWatcherError {
	// Scheduler error
	pub fn scheduler_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::SchedulerError(ErrorContext::new(msg, source, metadata))
	}

	// Network error
	pub fn network_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(ErrorContext::new(msg, source, metadata))
	}

	// Processing error
	pub fn processing_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ProcessingError(ErrorContext::new(msg, source, metadata))
	}

	// Storage error
	pub fn storage_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::StorageError(ErrorContext::new(msg, source, metadata))
	}

	// Block tracker error
	pub fn block_tracker_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::BlockTrackerError(ErrorContext::new(msg, source, metadata))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	#[test]
	fn test_scheduler_error_formatting() {
		let error = BlockWatcherError::scheduler_error("test error", None, None);
		assert_eq!(error.to_string(), "Scheduler error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockWatcherError::scheduler_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Scheduler error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_network_error_formatting() {
		let error = BlockWatcherError::network_error("test error", None, None);
		assert_eq!(error.to_string(), "Network error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockWatcherError::network_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Network error: test error [key1=value1]");
	}

	#[test]
	fn test_processing_error_formatting() {
		let error = BlockWatcherError::processing_error("test error", None, None);
		assert_eq!(error.to_string(), "Processing error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockWatcherError::processing_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Processing error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_storage_error_formatting() {
		let error = BlockWatcherError::storage_error("test error", None, None);
		assert_eq!(error.to_string(), "Storage error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockWatcherError::storage_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Storage error: test error [key1=value1]");
	}

	#[test]
	fn test_block_tracker_error_formatting() {
		let error = BlockWatcherError::block_tracker_error("test error", None, None);
		assert_eq!(error.to_string(), "Block tracker error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockWatcherError::block_tracker_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Block tracker error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_from_anyhow_error() {
		let anyhow_error = anyhow::anyhow!("test anyhow error");
		let block_watcher_error: BlockWatcherError = anyhow_error.into();
		assert!(matches!(block_watcher_error, BlockWatcherError::Other(_)));
		assert_eq!(block_watcher_error.to_string(), "test anyhow error");
	}

	#[test]
	fn test_error_source_chain() {
		let io_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = BlockWatcherError::scheduler_error(
			"Failed to initialize",
			Some(Box::new(io_error)),
			None,
		);

		// Just test the string representation instead of the source chain
		assert!(outer_error.to_string().contains("Failed to initialize"));

		// For BlockWatcherError::SchedulerError, we know the implementation details
		if let BlockWatcherError::SchedulerError(ctx) = &outer_error {
			// Check that the context has the right message
			assert_eq!(ctx.message, "Failed to initialize");

			// Check that the context has the source error
			assert!(ctx.source.is_some());

			if let Some(src) = &ctx.source {
				assert_eq!(src.to_string(), "while reading config");
			}
		} else {
			panic!("Expected SchedulerError variant");
		}
	}
}
