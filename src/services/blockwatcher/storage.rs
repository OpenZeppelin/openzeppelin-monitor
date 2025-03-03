//! Block storage implementations for the block watcher service.
//!
//! This module provides storage interfaces and implementations for persisting
//! blockchain blocks and tracking processing state. Currently supports:
//! - File-based storage with JSON serialization
//! - Last processed block tracking
//! - Block deletion for cleanup

use async_trait::async_trait;
use glob::glob;
use std::{collections::HashMap, path::PathBuf};

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

		let content = tokio::fs::read_to_string(file_path).await.map_err(|e| {
			BlockWatcherError::storage_error(e.to_string(), None, Some("get_last_processed_block"))
		})?;
		let block_number = content.trim().parse::<u64>().map_err(|e| {
			BlockWatcherError::storage_error(e.to_string(), None, Some("get_last_processed_block"))
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
		let context = HashMap::from([
			("network".to_string(), network_id.to_string()),
			("block".to_string(), block.to_string()),
		]);
		let file_path = self
			.storage_path
			.join(format!("{}_last_block.txt", network_id));
		tokio::fs::write(file_path, block.to_string())
			.await
			.map_err(|e| {
				BlockWatcherError::storage_error(
					e.to_string(),
					Some(context),
					Some("save_last_processed_block"),
				)
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
		let context = HashMap::from([("network".to_string(), network_slug.to_string())]);
		let file_path = self.storage_path.join(format!(
			"{}_blocks_{}.json",
			network_slug,
			chrono::Utc::now().timestamp()
		));
		let json = serde_json::to_string(blocks).map_err(|e| {
			BlockWatcherError::storage_error(
				e.to_string(),
				Some(context.clone()),
				Some("save_blocks"),
			)
		})?;
		tokio::fs::write(file_path, json).await.map_err(|e| {
			BlockWatcherError::storage_error(
				e.to_string(),
				Some(context.clone()),
				Some("save_blocks"),
			)
		})?;
		Ok(())
	}

	/// Deletes all block files for a network
	///
	/// # Note
	/// Uses glob pattern matching to find and delete all files matching:
	/// "{network_id}_blocks_*.json"
	async fn delete_blocks(&self, network_slug: &str) -> Result<(), BlockWatcherError> {
		let context = HashMap::from([("network".to_string(), network_slug.to_string())]);
		let pattern = self
			.storage_path
			.join(format!("{}_blocks_*.json", network_slug))
			.to_string_lossy()
			.to_string();

		for entry in glob(&pattern)
			.map_err(|e| {
				BlockWatcherError::storage_error(
					e.to_string(),
					Some(context.clone()),
					Some("delete_blocks"),
				)
			})?
			.flatten()
		{
			tokio::fs::remove_file(entry).await.map_err(|e| {
				BlockWatcherError::storage_error(
					e.to_string(),
					Some(context.clone()),
					Some("delete_blocks"),
				)
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
		let context = HashMap::from([
			("network_id".to_string(), network_id.to_string()),
			("block".to_string(), block.to_string()),
		]);
		let file_path = self
			.storage_path
			.join(format!("{}_missed_blocks.txt", network_id));

		// Open file in append mode, create if it doesn't exist
		let mut file = tokio::fs::OpenOptions::new()
			.create(true)
			.append(true)
			.open(file_path)
			.await
			.map_err(|e| {
				BlockWatcherError::storage_error(
					e.to_string(),
					Some(context.clone()),
					Some("save_missed_block"),
				)
			})?;

		// Write the block number followed by a newline
		tokio::io::AsyncWriteExt::write_all(&mut file, format!("{}\n", block).as_bytes())
			.await
			.map_err(|e| {
				BlockWatcherError::storage_error(
					e.to_string(),
					Some(context.clone()),
					Some("save_missed_block"),
				)
			})?;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile;

	#[tokio::test]
	async fn test_get_last_processed_block() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Test 1: Non-existent file
		let result = storage.get_last_processed_block("non_existent").await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), None);

		// Test 2: Invalid content (not a number)
		let invalid_file = temp_dir.path().join("invalid_last_block.txt");
		tokio::fs::write(&invalid_file, "not a number")
			.await
			.unwrap();
		let result = storage.get_last_processed_block("invalid").await;
		assert!(matches!(
			result,
			Err(BlockWatcherError::StorageError { .. })
		));

		// Test 3: Valid block number
		let valid_file = temp_dir.path().join("valid_last_block.txt");
		tokio::fs::write(&valid_file, "123").await.unwrap();
		let result = storage.get_last_processed_block("valid").await;
		assert_eq!(result.unwrap(), Some(123));
	}

	#[tokio::test]
	async fn test_save_last_processed_block() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Test 1: Normal save
		let result = storage.save_last_processed_block("test", 100).await;
		assert!(result.is_ok());

		// Verify the content
		let content = tokio::fs::read_to_string(temp_dir.path().join("test_last_block.txt"))
			.await
			.unwrap();
		assert_eq!(content, "100");

		// Test 2: Save with invalid path (create a readonly directory)
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let readonly_dir = temp_dir.path().join("readonly");
			tokio::fs::create_dir(&readonly_dir).await.unwrap();
			let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
			perms.set_mode(0o444); // Read-only
			std::fs::set_permissions(&readonly_dir, perms).unwrap();

			let readonly_storage = FileBlockStorage::new(readonly_dir);
			let result = readonly_storage
				.save_last_processed_block("test", 100)
				.await;
			assert!(matches!(
				result,
				Err(BlockWatcherError::StorageError { .. })
			));
		}
	}

	#[tokio::test]
	async fn test_save_blocks() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Test 1: Save empty blocks array
		let result = storage.save_blocks("test", &[]).await;
		assert!(result.is_ok());

		// Test 2: Save with invalid path
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let readonly_dir = temp_dir.path().join("readonly");
			tokio::fs::create_dir(&readonly_dir).await.unwrap();
			let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
			perms.set_mode(0o444); // Read-only
			std::fs::set_permissions(&readonly_dir, perms).unwrap();

			let readonly_storage = FileBlockStorage::new(readonly_dir);
			let result = readonly_storage.save_blocks("test", &[]).await;
			assert!(matches!(
				result,
				Err(BlockWatcherError::StorageError { .. })
			));
		}
	}

	#[tokio::test]
	async fn test_delete_blocks() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Create some test block files
		tokio::fs::write(temp_dir.path().join("test_blocks_1.json"), "[]")
			.await
			.unwrap();
		tokio::fs::write(temp_dir.path().join("test_blocks_2.json"), "[]")
			.await
			.unwrap();

		// Test 1: Normal delete
		let result = storage.delete_blocks("test").await;
		assert!(result.is_ok());

		// Test 2: Delete with invalid path
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let readonly_dir = temp_dir.path().join("readonly");
			tokio::fs::create_dir(&readonly_dir).await.unwrap();

			// Create test files first
			tokio::fs::write(readonly_dir.join("test_blocks_1.json"), "[]")
				.await
				.unwrap();

			// Then make directory readonly
			let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
			perms.set_mode(0o444); // Read-only
			std::fs::set_permissions(&readonly_dir, perms).unwrap();

			let readonly_storage = FileBlockStorage::new(readonly_dir);
			let result = readonly_storage.delete_blocks("test").await;
			assert!(matches!(
				result,
				Err(BlockWatcherError::StorageError { .. })
			));
		}
	}

	#[tokio::test]
	async fn test_save_missed_block() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Test 1: Normal save
		let result = storage.save_missed_block("test", 100).await;
		assert!(result.is_ok());

		// Verify the content
		let content = tokio::fs::read_to_string(temp_dir.path().join("test_missed_blocks.txt"))
			.await
			.unwrap();
		assert_eq!(content, "100\n");

		// Test 2: Save with invalid path
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let readonly_dir = temp_dir.path().join("readonly");
			tokio::fs::create_dir(&readonly_dir).await.unwrap();
			let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
			perms.set_mode(0o444); // Read-only
			std::fs::set_permissions(&readonly_dir, perms).unwrap();

			let readonly_storage = FileBlockStorage::new(readonly_dir);
			let result = readonly_storage.save_missed_block("test", 100).await;
			assert!(matches!(
				result,
				Err(BlockWatcherError::StorageError { .. })
			));
		}
	}
}
