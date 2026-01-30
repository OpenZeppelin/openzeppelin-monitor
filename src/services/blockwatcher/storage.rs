//! Block storage implementations for the block watcher service.
//!
//! This module provides storage interfaces and implementations for persisting
//! blockchain blocks and tracking processing state. Currently supports:
//! - File-based storage with JSON serialization
//! - Last processed block tracking
//! - Block deletion for cleanup
//! - Missed block tracking and recovery

use async_trait::async_trait;
use chrono::Utc;
use glob::glob;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::models::BlockType;

/// Status of a missed block entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MissedBlockStatus {
	/// Block is pending recovery
	Pending,
	/// Block recovery is in progress
	Recovering,
	/// Block was successfully recovered
	Recovered,
	/// Block recovery failed after max retries
	Failed,
}

/// Entry tracking a missed block with recovery metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissedBlockEntry {
	/// The block number that was missed
	pub block_number: u64,
	/// Unix timestamp (seconds) when this block was first detected as missed
	pub first_missed_at: i64,
	/// Number of recovery attempts made
	pub retry_count: u32,
	/// Current status of the missed block
	pub status: MissedBlockStatus,
	/// Unix timestamp (seconds) of the last recovery attempt
	pub last_attempt_at: Option<i64>,
	/// Error message from the last failed attempt
	pub last_error: Option<String>,
}

impl MissedBlockEntry {
	/// Creates a new missed block entry
	pub fn new(block_number: u64) -> Self {
		Self {
			block_number,
			first_missed_at: Utc::now().timestamp(),
			retry_count: 0,
			status: MissedBlockStatus::Pending,
			last_attempt_at: None,
			last_error: None,
		}
	}
}

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
	/// * `Result<Option<u64>, anyhow::Error>` - Last processed block number or None if not found
	async fn get_last_processed_block(
		&self,
		network_id: &str,
	) -> Result<Option<u64>, anyhow::Error>;

	/// Saves the last processed block number for a network
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `block` - Block number to save
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn save_last_processed_block(
		&self,
		network_id: &str,
		block: u64,
	) -> Result<(), anyhow::Error>;

	/// Saves a collection of blocks for a network
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `blocks` - Collection of blocks to save
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn save_blocks(
		&self,
		network_id: &str,
		blocks: &[BlockType],
	) -> Result<(), anyhow::Error>;

	/// Deletes all stored blocks for a network
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn delete_blocks(&self, network_id: &str) -> Result<(), anyhow::Error>;

	/// Saves multiple missed blocks for a network in a single operation
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `blocks` - Slice of block numbers to save
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn save_missed_blocks(
		&self,
		network_id: &str,
		blocks: &[u64],
	) -> Result<(), anyhow::Error>;

	/// Retrieves missed blocks eligible for recovery
	///
	/// Returns blocks that are within the max_block_age range and have
	/// status Pending with retry_count below max_retries.
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `max_block_age` - Maximum age in blocks from current_block
	/// * `current_block` - The current block number
	/// * `max_retries` - Maximum retry attempts; blocks with retry_count >= max_retries are excluded
	///
	/// # Returns
	/// * `Result<Vec<MissedBlockEntry>, anyhow::Error>` - Eligible missed blocks
	async fn get_missed_blocks(
		&self,
		network_id: &str,
		max_block_age: u64,
		current_block: u64,
		max_retries: u32,
	) -> Result<Vec<MissedBlockEntry>, anyhow::Error>;

	/// Updates the status of a missed block
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `block_number` - Block number to update
	/// * `status` - New status for the block
	/// * `error` - Optional error message (for failed status)
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn update_missed_block_status(
		&self,
		network_id: &str,
		block_number: u64,
		status: MissedBlockStatus,
		error: Option<String>,
	) -> Result<(), anyhow::Error>;

	/// Removes recovered blocks from storage
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `block_numbers` - Block numbers to remove
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn remove_recovered_blocks(
		&self,
		network_id: &str,
		block_numbers: &[u64],
	) -> Result<(), anyhow::Error>;

	/// Prunes missed blocks older than max_block_age
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `max_block_age` - Maximum age in blocks
	/// * `current_block` - The current block number
	///
	/// # Returns
	/// * `Result<usize, anyhow::Error>` - Number of pruned blocks
	async fn prune_old_missed_blocks(
		&self,
		network_id: &str,
		max_block_age: u64,
		current_block: u64,
	) -> Result<usize, anyhow::Error>;
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
	) -> Result<Option<u64>, anyhow::Error> {
		let file_path = self
			.storage_path
			.join(format!("{}_last_block.txt", network_id));

		if !file_path.exists() {
			return Ok(None);
		}

		let content = tokio::fs::read_to_string(file_path)
			.await
			.map_err(|e| anyhow::anyhow!("Failed to read last processed block: {}", e))?;
		let block_number = content
			.trim()
			.parse::<u64>()
			.map_err(|e| anyhow::anyhow!("Failed to parse last processed block: {}", e))?;
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
	) -> Result<(), anyhow::Error> {
		let file_path = self
			.storage_path
			.join(format!("{}_last_block.txt", network_id));
		tokio::fs::write(file_path, block.to_string())
			.await
			.map_err(|e| anyhow::anyhow!("Failed to save last processed block: {}", e))?;
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
	) -> Result<(), anyhow::Error> {
		let file_path = self.storage_path.join(format!(
			"{}_blocks_{}.json",
			network_slug,
			chrono::Utc::now().timestamp()
		));
		let json = serde_json::to_string(blocks)
			.map_err(|e| anyhow::anyhow!("Failed to serialize blocks: {}", e))?;
		tokio::fs::write(file_path, json)
			.await
			.map_err(|e| anyhow::anyhow!("Failed to save blocks: {}", e))?;
		Ok(())
	}

	/// Deletes all block files for a network
	///
	/// # Note
	/// Uses glob pattern matching to find and delete all files matching:
	/// "{network_id}_blocks_*.json"
	async fn delete_blocks(&self, network_slug: &str) -> Result<(), anyhow::Error> {
		let pattern = self
			.storage_path
			.join(format!("{}_blocks_*.json", network_slug))
			.to_string_lossy()
			.to_string();

		for entry in glob(&pattern)
			.map_err(|e| anyhow::anyhow!("Failed to parse blocks: {}", e))?
			.flatten()
		{
			tokio::fs::remove_file(entry)
				.await
				.map_err(|e| anyhow::anyhow!("Failed to delete blocks: {}", e))?;
		}
		Ok(())
	}

	/// Saves multiple missed blocks for a network in a single operation
	///
	/// This method saves new missed blocks to the JSON file. It first loads
	/// existing entries, adds the new blocks (deduplicating), and saves back.
	///
	/// # Arguments
	/// * `network_id` - Unique identifier for the network
	/// * `blocks` - Slice of block numbers to save
	///
	/// # Returns
	/// * `Result<(), anyhow::Error>` - Success or error
	async fn save_missed_blocks(
		&self,
		network_id: &str,
		blocks: &[u64],
	) -> Result<(), anyhow::Error> {
		if blocks.is_empty() {
			return Ok(());
		}

		// Load existing entries (with migration if needed)
		let mut entries = self.load_missed_blocks_json(network_id).await?;

		// Create a set of existing block numbers for deduplication
		let existing_blocks: std::collections::HashSet<u64> =
			entries.iter().map(|e| e.block_number).collect();

		// Add new blocks that don't already exist
		for &block_number in blocks {
			if !existing_blocks.contains(&block_number) {
				entries.push(MissedBlockEntry::new(block_number));
			}
		}

		// Save back to JSON
		self.save_missed_blocks_json(network_id, &entries).await
	}

	async fn get_missed_blocks(
		&self,
		network_id: &str,
		max_block_age: u64,
		current_block: u64,
		max_retries: u32,
	) -> Result<Vec<MissedBlockEntry>, anyhow::Error> {
		let entries = self.load_missed_blocks_json(network_id).await?;

		// Calculate the minimum block number we'll consider
		let min_block = current_block.saturating_sub(max_block_age);

		// Filter to blocks within age range, with Pending status, and below max retries
		let eligible: Vec<MissedBlockEntry> = entries
			.into_iter()
			.filter(|e| {
				e.block_number >= min_block
					&& e.status == MissedBlockStatus::Pending
					&& e.retry_count < max_retries
			})
			.collect();

		Ok(eligible)
	}

	async fn update_missed_block_status(
		&self,
		network_id: &str,
		block_number: u64,
		status: MissedBlockStatus,
		error: Option<String>,
	) -> Result<(), anyhow::Error> {
		let mut entries = self.load_missed_blocks_json(network_id).await?;

		// Find and update the entry
		if let Some(entry) = entries.iter_mut().find(|e| e.block_number == block_number) {
			// Increment retry count when status transitions to Pending (retry) or Failed (gave up)
			if status == MissedBlockStatus::Pending || status == MissedBlockStatus::Failed {
				entry.retry_count += 1;
			}
			entry.status = status;
			entry.last_attempt_at = Some(Utc::now().timestamp());
			if error.is_some() {
				entry.last_error = error;
			}
		}

		self.save_missed_blocks_json(network_id, &entries).await
	}

	async fn remove_recovered_blocks(
		&self,
		network_id: &str,
		block_numbers: &[u64],
	) -> Result<(), anyhow::Error> {
		if block_numbers.is_empty() {
			return Ok(());
		}

		let entries = self.load_missed_blocks_json(network_id).await?;

		let block_set: std::collections::HashSet<u64> = block_numbers.iter().copied().collect();

		// Filter out the recovered blocks
		let remaining: Vec<MissedBlockEntry> = entries
			.into_iter()
			.filter(|e| !block_set.contains(&e.block_number))
			.collect();

		self.save_missed_blocks_json(network_id, &remaining).await
	}

	async fn prune_old_missed_blocks(
		&self,
		network_id: &str,
		max_block_age: u64,
		current_block: u64,
	) -> Result<usize, anyhow::Error> {
		let entries = self.load_missed_blocks_json(network_id).await?;
		let original_count = entries.len();

		// Calculate the minimum block number to keep
		let min_block = current_block.saturating_sub(max_block_age);

		// Filter to keep only blocks within the age range
		let remaining: Vec<MissedBlockEntry> = entries
			.into_iter()
			.filter(|e| e.block_number >= min_block)
			.collect();

		let pruned_count = original_count - remaining.len();

		if pruned_count > 0 {
			self.save_missed_blocks_json(network_id, &remaining).await?;
		}

		Ok(pruned_count)
	}
}

impl FileBlockStorage {
	/// Loads missed blocks from JSON file, migrating from text format if needed
	async fn load_missed_blocks_json(
		&self,
		network_id: &str,
	) -> Result<Vec<MissedBlockEntry>, anyhow::Error> {
		let json_path = self
			.storage_path
			.join(format!("{}_missed_blocks.json", network_id));
		let txt_path = self
			.storage_path
			.join(format!("{}_missed_blocks.txt", network_id));

		// Check if JSON file exists
		if json_path.exists() {
			let content = tokio::fs::read_to_string(&json_path)
				.await
				.map_err(|e| anyhow::anyhow!("Failed to read missed blocks JSON: {}", e))?;

			if content.trim().is_empty() {
				return Ok(Vec::new());
			}

			let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content)
				.map_err(|e| anyhow::anyhow!("Failed to parse missed blocks JSON: {}", e))?;

			return Ok(entries);
		}

		// Check if old text format exists and migrate
		if txt_path.exists() {
			let content = tokio::fs::read_to_string(&txt_path)
				.await
				.map_err(|e| anyhow::anyhow!("Failed to read missed blocks text file: {}", e))?;

			let mut entries = Vec::new();
			let mut seen_blocks = std::collections::HashSet::new();

			for line in content.lines() {
				let line = line.trim();
				if line.is_empty() {
					continue;
				}
				if let Ok(block_number) = line.parse::<u64>() {
					// Deduplicate during migration
					if seen_blocks.insert(block_number) {
						entries.push(MissedBlockEntry::new(block_number));
					}
				}
			}

			// Save to new JSON format
			self.save_missed_blocks_json(network_id, &entries).await?;

			// Remove old text file after successful migration
			if let Err(e) = tokio::fs::remove_file(&txt_path).await {
				tracing::warn!(
					"Failed to remove old missed blocks text file after migration: {}",
					e
				);
			}

			return Ok(entries);
		}

		// No file exists, return empty
		Ok(Vec::new())
	}

	/// Saves missed blocks to JSON file
	async fn save_missed_blocks_json(
		&self,
		network_id: &str,
		entries: &[MissedBlockEntry],
	) -> Result<(), anyhow::Error> {
		let json_path = self
			.storage_path
			.join(format!("{}_missed_blocks.json", network_id));

		let json = serde_json::to_string_pretty(entries)
			.map_err(|e| anyhow::anyhow!("Failed to serialize missed blocks: {}", e))?;

		tokio::fs::write(json_path, json)
			.await
			.map_err(|e| anyhow::anyhow!("Failed to save missed blocks JSON: {}", e))?;

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

		// Test 1: existing file
		let existing_file = temp_dir.path().join("existing_last_block.txt");
		tokio::fs::write(&existing_file, "100").await.unwrap();
		let result = storage.get_last_processed_block("existing").await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), Some(100));

		// Test 2: Non-existent file
		let result = storage.get_last_processed_block("non_existent").await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), None);

		// Test 3: Invalid content (not a number)
		let invalid_file = temp_dir.path().join("invalid_last_block.txt");
		tokio::fs::write(&invalid_file, "not a number")
			.await
			.unwrap();
		let result = storage.get_last_processed_block("invalid").await;
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err
			.to_string()
			.contains("Failed to parse last processed block"));
		assert!(err.to_string().contains("invalid"));

		// Test 4: Valid block number
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
			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(err
				.to_string()
				.contains("Failed to save last processed block"));
			assert!(err.to_string().contains("Permission denied"));
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
			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(err.to_string().contains("Failed to save blocks"));
			assert!(err.to_string().contains("Permission denied"));
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
			perms.set_mode(0o555); // Read-only directory with execute permission
			std::fs::set_permissions(&readonly_dir, perms).unwrap();

			let readonly_storage = FileBlockStorage::new(readonly_dir);
			let result = readonly_storage.delete_blocks("test").await;
			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(err.to_string().contains("Failed to delete blocks"));
			assert!(err.to_string().contains("Permission denied"));
		}
	}

	#[tokio::test]
	async fn test_save_missed_blocks() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Test 1: Normal save with single block
		let result = storage.save_missed_blocks("test", &[100]).await;
		assert!(result.is_ok());

		// Verify JSON file exists and has correct content
		let json_path = temp_dir.path().join("test_missed_blocks.json");
		assert!(json_path.exists());
		let content = tokio::fs::read_to_string(&json_path).await.unwrap();
		let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content).unwrap();
		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].block_number, 100);
		assert_eq!(entries[0].status, MissedBlockStatus::Pending);

		// Test 2: Save multiple blocks (should add to existing)
		let result = storage.save_missed_blocks("test", &[101, 102, 103]).await;
		assert!(result.is_ok());

		let content = tokio::fs::read_to_string(&json_path).await.unwrap();
		let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content).unwrap();
		assert_eq!(entries.len(), 4);

		// Test 3: Save duplicate block (should not add duplicate)
		let result = storage.save_missed_blocks("test", &[100]).await;
		assert!(result.is_ok());

		let content = tokio::fs::read_to_string(&json_path).await.unwrap();
		let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content).unwrap();
		assert_eq!(entries.len(), 4); // Still 4, no duplicate added

		// Test 4: Save empty slice (should be no-op)
		let result = storage.save_missed_blocks("test", &[]).await;
		assert!(result.is_ok());

		// Test 5: Save with invalid path
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let readonly_dir = temp_dir.path().join("readonly");
			tokio::fs::create_dir(&readonly_dir).await.unwrap();
			let mut perms = std::fs::metadata(&readonly_dir).unwrap().permissions();
			perms.set_mode(0o444); // Read-only
			std::fs::set_permissions(&readonly_dir, perms).unwrap();

			let readonly_storage = FileBlockStorage::new(readonly_dir);
			let result = readonly_storage.save_missed_blocks("test", &[100]).await;
			assert!(result.is_err());
		}
	}

	#[tokio::test]
	async fn test_migration_from_text_to_json() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Create an old-format text file
		let txt_path = temp_dir.path().join("test_missed_blocks.txt");
		tokio::fs::write(&txt_path, "100\n101\n102\n100\n") // Include duplicate
			.await
			.unwrap();

		// Call get_missed_blocks which should trigger migration
		let result = storage
			.get_missed_blocks("test", 1000, 1000, 3)
			.await
			.unwrap();

		// Should have 3 unique entries (deduplicated during migration)
		assert_eq!(result.len(), 3);

		// Verify JSON file was created
		let json_path = temp_dir.path().join("test_missed_blocks.json");
		assert!(json_path.exists());

		// Verify text file was removed
		assert!(!txt_path.exists());

		// Verify JSON content
		let content = tokio::fs::read_to_string(&json_path).await.unwrap();
		let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content).unwrap();
		assert_eq!(entries.len(), 3);
	}

	#[tokio::test]
	async fn test_get_missed_blocks() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Save some missed blocks
		storage
			.save_missed_blocks("test", &[100, 200, 300, 400, 500])
			.await
			.unwrap();

		// Get blocks with max_block_age=200, current_block=500, max_retries=3
		// Should return blocks >= 300 (500 - 200) with retry_count < 3
		let result = storage
			.get_missed_blocks("test", 200, 500, 3)
			.await
			.unwrap();

		assert_eq!(result.len(), 3);
		let block_numbers: Vec<u64> = result.iter().map(|e| e.block_number).collect();
		assert!(block_numbers.contains(&300));
		assert!(block_numbers.contains(&400));
		assert!(block_numbers.contains(&500));
	}

	#[tokio::test]
	async fn test_update_missed_block_status() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Save a missed block
		storage.save_missed_blocks("test", &[100]).await.unwrap();

		// Update status to Recovering
		storage
			.update_missed_block_status("test", 100, MissedBlockStatus::Recovering, None)
			.await
			.unwrap();

		// Verify the update
		let json_path = temp_dir.path().join("test_missed_blocks.json");
		let content = tokio::fs::read_to_string(&json_path).await.unwrap();
		let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content).unwrap();
		assert_eq!(entries[0].status, MissedBlockStatus::Recovering);
		assert!(entries[0].last_attempt_at.is_some());

		// Update status to Failed with error
		storage
			.update_missed_block_status(
				"test",
				100,
				MissedBlockStatus::Failed,
				Some("RPC error".to_string()),
			)
			.await
			.unwrap();

		let content = tokio::fs::read_to_string(&json_path).await.unwrap();
		let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content).unwrap();
		assert_eq!(entries[0].status, MissedBlockStatus::Failed);
		assert_eq!(entries[0].last_error, Some("RPC error".to_string()));
		assert_eq!(entries[0].retry_count, 1); // Incremented
	}

	#[tokio::test]
	async fn test_remove_recovered_blocks() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Save some missed blocks
		storage
			.save_missed_blocks("test", &[100, 101, 102, 103, 104])
			.await
			.unwrap();

		// Remove some recovered blocks
		storage
			.remove_recovered_blocks("test", &[101, 103])
			.await
			.unwrap();

		// Verify remaining blocks
		let json_path = temp_dir.path().join("test_missed_blocks.json");
		let content = tokio::fs::read_to_string(&json_path).await.unwrap();
		let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content).unwrap();

		assert_eq!(entries.len(), 3);
		let block_numbers: Vec<u64> = entries.iter().map(|e| e.block_number).collect();
		assert!(block_numbers.contains(&100));
		assert!(block_numbers.contains(&102));
		assert!(block_numbers.contains(&104));
		assert!(!block_numbers.contains(&101));
		assert!(!block_numbers.contains(&103));
	}

	#[tokio::test]
	async fn test_prune_old_missed_blocks() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Save some missed blocks
		storage
			.save_missed_blocks("test", &[100, 200, 300, 400, 500])
			.await
			.unwrap();

		// Prune with max_block_age=150, current_block=500
		// Should prune blocks < 350 (500 - 150)
		let pruned = storage
			.prune_old_missed_blocks("test", 150, 500)
			.await
			.unwrap();

		assert_eq!(pruned, 3); // Blocks 100, 200, 300 should be pruned

		// Verify remaining blocks
		let json_path = temp_dir.path().join("test_missed_blocks.json");
		let content = tokio::fs::read_to_string(&json_path).await.unwrap();
		let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content).unwrap();

		assert_eq!(entries.len(), 2);
		let block_numbers: Vec<u64> = entries.iter().map(|e| e.block_number).collect();
		assert!(block_numbers.contains(&400));
		assert!(block_numbers.contains(&500));
	}

	#[tokio::test]
	async fn test_get_missed_blocks_empty() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Get from non-existent network should return empty
		let result = storage
			.get_missed_blocks("nonexistent", 1000, 1000, 3)
			.await
			.unwrap();
		assert!(result.is_empty());
	}

	#[tokio::test]
	async fn test_get_missed_blocks_filters_by_status() {
		let temp_dir = tempfile::tempdir().unwrap();
		let storage = FileBlockStorage::new(temp_dir.path().to_path_buf());

		// Save some missed blocks
		storage
			.save_missed_blocks("test", &[100, 101, 102])
			.await
			.unwrap();

		// Update one to Failed status
		storage
			.update_missed_block_status("test", 101, MissedBlockStatus::Failed, None)
			.await
			.unwrap();

		// Get missed blocks - should only return Pending ones with retry_count < 3
		let result = storage
			.get_missed_blocks("test", 1000, 1000, 3)
			.await
			.unwrap();

		assert_eq!(result.len(), 2);
		for entry in &result {
			assert_eq!(entry.status, MissedBlockStatus::Pending);
		}
	}
}
