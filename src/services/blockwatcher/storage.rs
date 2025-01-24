//! Block storage implementations for the block watcher service.
//!
//! This module provides storage interfaces and implementations for persisting
//! blockchain blocks and tracking processing state. Currently supports:
//! - File-based storage with JSON serialization
//! - Last processed block tracking
//! - Block deletion for cleanup

use async_trait::async_trait;
use glob::glob;
use std::path::PathBuf;

use crate::{models::BlockType, services::blockwatcher::error::BlockWatcherError};

/// Interface for block storage implementations
///
/// Defines the required functionality for storing and retrieving blocks
/// and tracking the last processed block for each network.
#[async_trait]
pub trait BlockStorage: Clone + Send + Sync {
	/// Retrieves the last processed block number for a network
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	///
	/// # Returns
	/// * `Result<Option<u64>, BlockWatcherError>` - Last processed block number or None if not
	///   found
	async fn get_last_processed_block(
		&self,
		network_id: &str,
	) -> Result<Option<u64>, BlockWatcherError>;

	/// Saves the last processed block number for a network
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `block` - Block number to save
	///
	/// # Returns
	/// * `Result<(), BlockWatcherError>` - Success or error
	async fn save_last_processed_block(
		&self,
		network_id: &str,
		block: u64,
	) -> Result<(), BlockWatcherError>;

	/// Saves a collection of blocks for a network
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `blocks` - Collection of blocks to save
	///
	/// # Returns
	/// * `Result<(), BlockWatcherError>` - Success or error
	async fn save_blocks(
		&self,
		network_id: &str,
		blocks: &[BlockType],
	) -> Result<(), BlockWatcherError>;

	/// Deletes all stored blocks for a network
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	///
	/// # Returns
	/// * `Result<(), BlockWatcherError>` - Success or error
	async fn delete_blocks(&self, network_id: &str) -> Result<(), BlockWatcherError>;

	/// Saves a missed block for a network
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `block` - Block number to save
	///
	/// # Returns
	/// * `Result<(), BlockWatcherError>` - Success or error
	async fn save_missed_block(
		&self,
		network_id: &str,
		block: u64,
	) -> Result<(), BlockWatcherError>;
}

/// File-based implementation of block storage
///
/// Stores blocks and processing state in JSON files within a configured
/// directory structure.
#[derive(Clone)]
pub struct FileBlockStorage {
	/// Base path for all storage files
	storage_path: PathBuf,
}

impl FileBlockStorage {
	/// Creates a new file-based block storage instance
	///
	/// Initializes storage with the provided path
	pub fn new(storage_path: PathBuf) -> Self {
		FileBlockStorage { storage_path }
	}
}

impl Default for FileBlockStorage {
	/// Default implementation for FileBlockStorage
	///
	/// Initializes storage with the default path "data"
	fn default() -> Self {
		FileBlockStorage::new(PathBuf::from("data"))
	}
}

#[async_trait]
impl BlockStorage for FileBlockStorage {
	/// Retrieves the last processed block from a network-specific file
	///
	/// The file is named "{network_id}_last_block.txt"
	async fn get_last_processed_block(
		&self,
		network_id: &str,
	) -> Result<Option<u64>, BlockWatcherError> {
		let file_path = self
			.storage_path
			.join(format!("{}_last_block.txt", network_id));

		if !file_path.exists() {
			return Ok(None);
		}

		let content = tokio::fs::read_to_string(file_path)
			.await
			.map_err(|e| BlockWatcherError::storage_error(format!("Failed to read file: {}", e)))?;
		let block_number = content.trim().parse().map_err(|e| {
			BlockWatcherError::storage_error(format!("Failed to parse block number: {}", e))
		})?;
		Ok(Some(block_number))
	}

	/// Saves the last processed block to a network-specific file
	///
	/// # Note
	/// Overwrites any existing last block file for the network
	async fn save_last_processed_block(
		&self,
		network_id: &str,
		block: u64,
	) -> Result<(), BlockWatcherError> {
		let file_path = self
			.storage_path
			.join(format!("{}_last_block.txt", network_id));
		tokio::fs::write(file_path, block.to_string())
			.await
			.map_err(|e| {
				BlockWatcherError::storage_error(format!("Failed to write file: {}", e))
			})?;
		Ok(())
	}

	/// Saves blocks to a timestamped JSON file
	///
	/// # Note
	/// Creates a new file for each save operation, named:
	/// "{network_id}_blocks_{timestamp}.json"
	async fn save_blocks(
		&self,
		network_slug: &str,
		blocks: &[BlockType],
	) -> Result<(), BlockWatcherError> {
		let file_path = self.storage_path.join(format!(
			"{}_blocks_{}.json",
			network_slug,
			chrono::Utc::now().timestamp()
		));
		let json = serde_json::to_string(blocks).map_err(|e| {
			BlockWatcherError::storage_error(format!("Failed to serialize blocks: {}", e))
		})?;
		tokio::fs::write(file_path, json).await.map_err(|e| {
			BlockWatcherError::storage_error(format!("Failed to write file: {}", e))
		})?;
		Ok(())
	}

	/// Deletes all block files for a network
	///
	/// # Note
	/// Uses glob pattern matching to find and delete all files matching:
	/// "{network_id}_blocks_*.json"
	async fn delete_blocks(&self, network_slug: &str) -> Result<(), BlockWatcherError> {
		let pattern = self
			.storage_path
			.join(format!("{}_blocks_*.json", network_slug))
			.to_string_lossy()
			.to_string();

		for entry in glob(&pattern)
			.map_err(|e| BlockWatcherError::storage_error(format!("Failed to glob files: {}", e)))?
			.flatten()
		{
			tokio::fs::remove_file(entry).await.map_err(|e| {
				BlockWatcherError::storage_error(format!("Failed to remove file: {}", e))
			})?;
		}
		Ok(())
	}

	/// Saves a missed block for a network
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `block` - Block number to save
	///
	/// # Returns
	/// * `Result<(), BlockWatcherError>` - Success or error
	async fn save_missed_block(
		&self,
		network_id: &str,
		block: u64,
	) -> Result<(), BlockWatcherError> {
		let file_path = self
			.storage_path
			.join(format!("{}_missed_blocks.txt", network_id));

		// Open file in append mode, create if it doesn't exist
		let mut file = tokio::fs::OpenOptions::new()
			.create(true)
			.append(true)
			.open(file_path)
			.await
			.map_err(|e| BlockWatcherError::storage_error(format!("Failed to open file: {}", e)))?;

		// Write the block number followed by a newline
		tokio::io::AsyncWriteExt::write_all(&mut file, format!("{}\n", block).as_bytes())
			.await
			.map_err(|e| {
				BlockWatcherError::storage_error(format!("Failed to write file: {}", e))
			})?;

		Ok(())
	}
}
