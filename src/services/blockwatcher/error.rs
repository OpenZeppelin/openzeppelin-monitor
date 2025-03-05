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
		Self::SchedulerError(ErrorContext::new(msg.into(), source, metadata))
	}

	// Network error
	pub fn network_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::NetworkError(ErrorContext::new(msg.into(), source, metadata))
	}

	// Processing error
	pub fn processing_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ProcessingError(ErrorContext::new(msg.into(), source, metadata))
	}

	// Storage error
	pub fn storage_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::StorageError(ErrorContext::new(msg.into(), source, metadata))
	}

	// Block tracker error
	pub fn block_tracker_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::BlockTrackerError(ErrorContext::new(msg.into(), source, metadata))
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
		assert_eq!(error.to_string(), "Scheduler error: test error");
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
		assert_eq!(error.to_string(), "Network error: test error");
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
		assert_eq!(error.to_string(), "Processing error: test error");
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
		assert_eq!(error.to_string(), "Storage error: test error");
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
		assert_eq!(error.to_string(), "Block tracker error: test error");
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
		use std::error::Error;
		let middle_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error = BlockWatcherError::scheduler_error(
			"Failed to initialize",
			Some(Box::new(middle_error) as Box<dyn std::error::Error + Send + Sync>),
			None,
		);

		// Test the source chain
		let source = outer_error.source();
		assert!(source.is_some());
		assert_eq!(source.unwrap().to_string(), "while reading config");
	}
}
