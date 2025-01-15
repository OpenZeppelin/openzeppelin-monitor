//! Block watcher error types and handling.
//!
//! Provides a comprehensive error handling system for block watching operations,
//! including scheduling, network connectivity, and storage operations.

use log::error;
use std::{error::Error, fmt};

/// Represents possible errors that can occur during block watching operations
#[derive(Debug)]
pub enum BlockWatcherError {
	/// Errors related to job scheduling operations
	///
	/// Examples include:
	/// - Failed to create scheduler
	/// - Failed to add/remove jobs
	/// - Failed to start/stop scheduler
	SchedulerError(String),

	/// Errors related to network operations
	///
	/// Examples include:
	/// - Failed to connect to blockchain node
	/// - Failed to retrieve blocks
	/// - RPC request failures
	NetworkError(String),

	/// Errors related to block processing
	///
	/// Examples include:
	/// - Failed to parse block data
	/// - Failed to process transactions
	/// - Handler execution failures
	ProcessingError(String),

	/// Errors related to block storage operations
	///
	/// Examples include:
	/// - Failed to save blocks
	/// - Failed to retrieve last processed block
	/// - File system errors
	StorageError(String),

	/// Errors related to block tracker operations
	///
	/// Examples include:
	/// - Failed to record block
	/// - Failed to retrieve last processed block
	/// - Errors related to ordered blocks
	BlockTrackerError(String),
}

impl BlockWatcherError {
	/// Formats the error message based on the error type
	fn format_message(&self) -> String {
		match self {
			Self::SchedulerError(msg) => format!("Scheduler error: {}", msg),
			Self::NetworkError(msg) => format!("Network error: {}", msg),
			Self::ProcessingError(msg) => format!("Processing error: {}", msg),
			Self::StorageError(msg) => format!("Storage error: {}", msg),
			Self::BlockTrackerError(msg) => format!("Block tracker error: {}", msg),
		}
	}

	/// Creates a new scheduler error with logging
	pub fn scheduler_error(msg: impl Into<String>) -> Self {
		let error = Self::SchedulerError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new network error with logging
	pub fn network_error(msg: impl Into<String>) -> Self {
		let error = Self::NetworkError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new processing error with logging
	pub fn processing_error(msg: impl Into<String>) -> Self {
		let error = Self::ProcessingError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new storage error with logging
	pub fn storage_error(msg: impl Into<String>) -> Self {
		let error = Self::StorageError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new missed block error with logging
	pub fn block_tracker_error(msg: impl Into<String>) -> Self {
		let error = Self::BlockTrackerError(msg.into());
		error!("{}", error.format_message());
		error
	}
}

impl fmt::Display for BlockWatcherError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.format_message())
	}
}

impl Error for BlockWatcherError {}
