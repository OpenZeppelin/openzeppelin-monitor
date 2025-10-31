//! Block tracking functionality for monitoring blockchain networks.
//!
//! This module provides tools for tracking processed blocks across different networks
//! and identifying potential issues such as:
//! - Missed blocks
//! - Out-of-order block processing
//! - Duplicate block processing
//!
//! The primary component is the [`BlockTracker`] which maintains a history of
//! recently processed blocks and can optionally persist information about missed
//! blocks using a storage implementation.

use async_trait::async_trait;
use std::{
	collections::{HashMap, HashSet, VecDeque},
	sync::Arc,
};
use tokio::sync::Mutex;

use crate::{
	models::Network,
	services::blockwatcher::{error::BlockWatcherError, storage::BlockStorage},
};

/// Trait for the BlockTracker
///
/// This trait defines the interface for the BlockTracker.
#[async_trait]
pub trait BlockTrackerTrait<S: BlockStorage> {
	fn new(history_size: usize, storage: Option<Arc<S>>) -> Self;
	async fn record_block(&self, network: &Network, block_number: u64)
		-> Result<(), anyhow::Error>;
	async fn get_last_block(&self, network_slug: &str) -> Option<u64>;
}

/// BlockTracker is responsible for monitoring the sequence of processed blocks
/// across different networks and identifying any gaps or irregularities in block processing.
///
/// It maintains a history of recently processed blocks for each network and can optionally
/// persist information about missed blocks using the provided storage implementation.
///
/// # Type Parameters
///
/// * `S` - A type that implements the `BlockStorage` trait for persisting missed block information
#[derive(Clone)]
pub struct BlockTracker<S> {
	/// Tracks the last N blocks processed for each network
	/// Key: network_slug, Value: Queue of block numbers
	block_history: Arc<Mutex<HashMap<String, VecDeque<u64>>>>,
	/// Maximum number of blocks to keep in history per network
	history_size: usize,
	/// Storage interface for persisting missed blocks
	storage: Option<Arc<S>>,
	/// Blocks received but not yet finalized (for buffered mode)
	/// Key: network_slug, Value: Set of block numbers
	pending_blocks: Arc<Mutex<HashMap<String, HashSet<u64>>>>,
	/// Highest block number watermark per network (for buffered mode)
	/// Key: network_slug, Value: highest block number seen
	watermarks: Arc<Mutex<HashMap<String, u64>>>,
}

#[async_trait]
impl<S: BlockStorage> BlockTrackerTrait<S> for BlockTracker<S> {
	/// Creates a new BlockTracker instance.
	///
	/// # Arguments
	///
	/// * `history_size` - The maximum number of recent blocks to track per network
	/// * `storage` - Optional storage implementation for persisting missed block information
	///
	/// # Returns
	///
	/// A new `BlockTracker` instance
	fn new(history_size: usize, storage: Option<Arc<S>>) -> Self {
		Self {
			block_history: Arc::new(Mutex::new(HashMap::new())),
			history_size,
			storage,
			pending_blocks: Arc::new(Mutex::new(HashMap::new())),
			watermarks: Arc::new(Mutex::new(HashMap::new())),
		}
	}

	/// Records a processed block and identifies any gaps in block sequence.
	///
	/// This method performs several checks:
	/// - Detects gaps between the last processed block and the current block
	/// - Identifies out-of-order or duplicate blocks
	/// - Stores information about missed blocks if storage is configured
	///
	/// Supports two modes based on network configuration:
	/// - Strict mode (tolerance = 0 or None): Enforces sequential ordering
	/// - Buffered mode (tolerance > 0): Allows blocks within tolerance window
	///
	/// # Arguments
	///
	/// * `network` - The network information for the processed block
	/// * `block_number` - The block number being recorded
	///
	/// # Warning
	///
	/// This method will log warnings for out-of-order blocks and errors for missed blocks.
	async fn record_block(
		&self,
		network: &Network,
		block_number: u64,
	) -> Result<(), anyhow::Error> {
		let tolerance = network.block_tracking_tolerance.unwrap_or(0);

		if tolerance == 0 {
			// Strict sequential mode (backward compatible)
			self.record_block_strict(network, block_number).await
		} else {
			// Buffered mode with tolerance window
			self.record_block_buffered(network, block_number, tolerance)
				.await
		}
	}

	/// Retrieves the most recently processed block number for a given network.
	///
	/// # Arguments
	///
	/// * `network_slug` - The unique identifier for the network
	///
	/// # Returns
	///
	/// Returns `Some(block_number)` if blocks have been processed for the network,
	/// otherwise returns `None`.
	async fn get_last_block(&self, network_slug: &str) -> Option<u64> {
		self.block_history
			.lock()
			.await
			.get(network_slug)
			.and_then(|history| history.back().copied())
	}
}

impl<S: BlockStorage> BlockTracker<S> {
	/// Records a block in strict sequential mode (tolerance = 0).
	///
	/// This is the original behavior: enforces strict sequential ordering
	/// and reports any gaps or out-of-order blocks immediately.
	async fn record_block_strict(
		&self,
		network: &Network,
		block_number: u64,
	) -> Result<(), anyhow::Error> {
		let mut history = self.block_history.lock().await;
		let network_history = history
			.entry(network.slug.clone())
			.or_insert_with(|| VecDeque::with_capacity(self.history_size));

		// Check for gaps if we have previous blocks
		if let Some(&last_block) = network_history.back() {
			if block_number > last_block + 1 {
				// Log each missed block number
				for missed in (last_block + 1)..block_number {
					BlockWatcherError::block_tracker_error(
						format!("Missed block {}", missed),
						None,
						None,
					);

					if network.store_blocks.unwrap_or(false) {
						if let Some(storage) = &self.storage {
							// Store the missed block info
							if (storage.save_missed_block(&network.slug, missed).await).is_err() {
								BlockWatcherError::storage_error(
									format!("Failed to store missed block {}", missed),
									None,
									None,
								);
							}
						}
					}
				}
			} else if block_number <= last_block {
				BlockWatcherError::block_tracker_error(
					format!(
						"Out of order or duplicate block detected: received {} after {}",
						block_number, last_block
					),
					None,
					None,
				);
			}
		}

		// Add the new block to history
		network_history.push_back(block_number);

		// Maintain history size
		while network_history.len() > self.history_size {
			network_history.pop_front();
		}
		Ok(())
	}

	/// Records a block in buffered mode with tolerance window.
	///
	/// Allows blocks to arrive slightly out of order within the tolerance window.
	/// Finalizes ranges and reports missed blocks once the window advances.
	async fn record_block_buffered(
		&self,
		network: &Network,
		block_number: u64,
		tolerance: u64,
	) -> Result<(), anyhow::Error> {
		let mut pending = self.pending_blocks.lock().await;
		let mut watermarks = self.watermarks.lock().await;
		let mut history = self.block_history.lock().await;

		let network_pending = pending
			.entry(network.slug.clone())
			.or_insert_with(HashSet::new);
		let watermark = watermarks.entry(network.slug.clone()).or_insert(0);
		let network_history = history
			.entry(network.slug.clone())
			.or_insert_with(|| VecDeque::with_capacity(self.history_size));

		// Check if block is too far behind the watermark (outside tolerance)
		if *watermark > 0 && block_number < watermark.saturating_sub(tolerance) {
			BlockWatcherError::block_tracker_error(
				format!(
					"Out of order block: {} is too far behind watermark {} (tolerance: {})",
					block_number, watermark, tolerance
				),
				None,
				None,
			);
			return Ok(());
		}

		// Check for duplicate
		if network_pending.contains(&block_number) {
			BlockWatcherError::block_tracker_error(
				format!("Duplicate block detected: {}", block_number),
				None,
				None,
			);
			return Ok(());
		}

		// Add block to pending set
		network_pending.insert(block_number);

		// Update watermark if this is a new high
		let old_watermark = *watermark;
		if block_number > *watermark {
			*watermark = block_number;

			// If we've advanced significantly, finalize the old range
			// We finalize when we've moved forward by at least half the tolerance
			// Skip finalization if old_watermark is 0 (initial state - no previous blocks to check)
			if old_watermark > 0 && block_number > old_watermark + (tolerance / 2) {
				self.finalize_range(
					network,
					network_pending,
					network_history,
					old_watermark,
					block_number,
					tolerance,
				)
				.await?;
			}
		}

		Ok(())
	}

	/// Finalizes a range of blocks and checks for gaps.
	///
	/// This is called when the watermark advances significantly in buffered mode.
	/// It validates that all blocks in the finalized range were received.
	async fn finalize_range(
		&self,
		network: &Network,
		pending: &mut HashSet<u64>,
		history: &mut VecDeque<u64>,
		start: u64,
		end: u64,
		tolerance: u64,
	) -> Result<(), anyhow::Error> {
		// The finalized range is from start to (end - tolerance)
		// We keep the last 'tolerance' blocks in pending as they might still arrive
		let finalize_up_to = end.saturating_sub(tolerance);

		// Nothing to finalize if finalize_up_to is 0 or if there are no blocks to finalize
		if finalize_up_to == 0 {
			return Ok(());
		}

		// Check for gaps in the finalized range only if there's a range to check
		// When finalize_up_to == start, there are no gaps to check (empty range)
		if finalize_up_to > start {
			for expected in (start + 1)..=finalize_up_to {
				if !pending.contains(&expected) {
					// This block is truly missed
					BlockWatcherError::block_tracker_error(
						format!("Missed block {}", expected),
						None,
						None,
					);

					if network.store_blocks.unwrap_or(false) {
						if let Some(storage) = &self.storage {
							if (storage.save_missed_block(&network.slug, expected).await).is_err() {
								BlockWatcherError::storage_error(
									format!("Failed to store missed block {}", expected),
									None,
									None,
								);
							}
						}
					}
				}
			}
		}

		// Move finalized blocks to history and remove from pending
		let mut finalized_blocks: Vec<u64> = pending
			.iter()
			.filter(|&&block| block <= finalize_up_to)
			.copied()
			.collect();
		finalized_blocks.sort_unstable();

		for block in finalized_blocks {
			history.push_back(block);
			pending.remove(&block);

			// Maintain history size
			while history.len() > self.history_size {
				history.pop_front();
			}
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use crate::{models::BlockType, utils::tests::network::NetworkBuilder};

	use super::*;
	use mockall::mock;

	// Create mock storage
	mock! {
		pub BlockStorage {}
		#[async_trait::async_trait]
		impl BlockStorage for BlockStorage {
			async fn save_missed_block(&self, network_slug: &str, block_number: u64) -> Result<(), anyhow::Error>;
			async fn save_last_processed_block(&self, network_slug: &str, block_number: u64) -> Result<(), anyhow::Error>;
			async fn get_last_processed_block(&self, network_slug: &str) -> Result<Option<u64>, anyhow::Error>;
			async fn save_blocks(&self, network_slug: &str, blocks: &[BlockType]) -> Result<(), anyhow::Error>;
			async fn delete_blocks(&self, network_slug: &str) -> Result<(), anyhow::Error>;
		}

		impl Clone for BlockStorage {
			fn clone(&self) -> Self {
				Self::new()
			}
		}
	}
	fn create_test_network(name: &str, slug: &str, store_blocks: bool) -> Network {
		NetworkBuilder::new()
			.name(name)
			.slug(slug)
			.store_blocks(store_blocks)
			.build()
	}

	#[tokio::test]
	async fn test_normal_block_sequence() {
		let mock_storage = MockBlockStorage::new();

		let tracker = BlockTracker::new(5, Some(Arc::new(mock_storage)));
		let network = create_test_network("test-net", "test_net", true);

		// Process blocks in sequence
		tracker.record_block(&network, 1).await.unwrap();
		tracker.record_block(&network, 2).await.unwrap();
		tracker.record_block(&network, 3).await.unwrap();

		assert_eq!(tracker.get_last_block("test_net").await, Some(3));
	}

	#[tokio::test]
	async fn test_history_size_limit() {
		let mock_storage = MockBlockStorage::new();

		let tracker = BlockTracker::new(3, Some(Arc::new(mock_storage)));
		let network = create_test_network("test-net", "test_net", true);

		// Process 5 blocks with a history limit of 3
		for i in 1..=5 {
			tracker.record_block(&network, i).await.unwrap();
		}

		let history = tracker.block_history.lock().await;
		let network_history = history
			.get(&network.slug)
			.expect("Network history should exist");

		// Verify we only kept the last 3 blocks
		assert_eq!(network_history.len(), 3);
		assert_eq!(network_history.front(), Some(&3)); // Oldest block
		assert_eq!(network_history.back(), Some(&5)); // Newest block
	}

	#[tokio::test]
	async fn test_missed_blocks_with_storage() {
		let mut mock_storage = MockBlockStorage::new();

		// Expect block 2 to be recorded as missed
		mock_storage
			.expect_save_missed_block()
			.with(
				mockall::predicate::eq("test_net"),
				mockall::predicate::eq(2),
			)
			.times(1)
			.returning(|_, _| Ok(()));

		let tracker = BlockTracker::new(5, Some(Arc::new(mock_storage)));
		let network = create_test_network("test-net", "test_net", true);

		// Process block 1
		tracker.record_block(&network, 1).await.unwrap();
		// Skip block 2 and process block 3
		tracker.record_block(&network, 3).await.unwrap();
	}

	#[tokio::test]
	async fn test_out_of_order_blocks() {
		let mock_storage = MockBlockStorage::new();

		let tracker = BlockTracker::new(5, Some(Arc::new(mock_storage)));
		let network = create_test_network("test-net", "test_net", true);

		// Process blocks out of order
		tracker.record_block(&network, 2).await.unwrap();
		tracker.record_block(&network, 1).await.unwrap();

		assert_eq!(tracker.get_last_block("test_net").await, Some(1));
	}

	#[tokio::test]
	async fn test_multiple_networks() {
		let mock_storage = MockBlockStorage::new();

		let tracker = BlockTracker::new(5, Some(Arc::new(mock_storage)));
		let network1 = create_test_network("net-1", "net_1", true);
		let network2 = create_test_network("net-2", "net_2", true);

		// Process blocks for both networks
		tracker.record_block(&network1, 1).await.unwrap();
		tracker.record_block(&network2, 100).await.unwrap();
		tracker.record_block(&network1, 2).await.unwrap();
		tracker.record_block(&network2, 101).await.unwrap();

		assert_eq!(tracker.get_last_block("net_1").await, Some(2));
		assert_eq!(tracker.get_last_block("net_2").await, Some(101));
	}

	#[tokio::test]
	async fn test_get_last_block_empty_network() {
		let tracker = BlockTracker::new(5, None::<Arc<MockBlockStorage>>);
		assert_eq!(tracker.get_last_block("nonexistent").await, None);
	}

	#[tokio::test]
	async fn test_save_missed_block_record() {
		let mut mock_storage = MockBlockStorage::new();

		mock_storage
			.expect_save_missed_block()
			.with(
				mockall::predicate::eq("test_network"),
				mockall::predicate::eq(2),
			)
			.times(1)
			.returning(|_, _| Ok(()));

		let tracker = BlockTracker::new(5, Some(Arc::new(mock_storage)));
		let network = create_test_network("test-network", "test_network", true);

		// This should trigger save_last_processed_block
		tracker.record_block(&network, 1).await.unwrap();
		// This should trigger save_missed_block for block 2
		tracker.record_block(&network, 3).await.unwrap();
	}

	#[tokio::test]
	async fn test_buffered_mode_initial_state() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(5, Some(Arc::new(mock_storage)));

		// Create network with tolerance
		let mut network = create_test_network("test-net", "test_net", false);
		network.block_tracking_tolerance = Some(10);

		// Simulate starting with a high block number (like in production)
		// This should NOT report millions of missed blocks
		tracker.record_block(&network, 210123279).await.unwrap();
		tracker.record_block(&network, 210123280).await.unwrap();
		tracker.record_block(&network, 210123281).await.unwrap();

		// Verify watermark is set
		let watermarks = tracker.watermarks.lock().await;
		assert_eq!(watermarks.get("test_net"), Some(&210123281));

		// Verify pending blocks contains our blocks
		let pending = tracker.pending_blocks.lock().await;
		let network_pending = pending.get("test_net").unwrap();
		assert!(network_pending.contains(&210123279));
		assert!(network_pending.contains(&210123280));
		assert!(network_pending.contains(&210123281));
	}

	#[tokio::test]
	async fn test_buffered_mode_finalization() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network = create_test_network("test-net", "test_net", false);
		network.block_tracking_tolerance = Some(10);

		// Start with block 100
		tracker.record_block(&network, 100).await.unwrap();

		// Add blocks in sequence
		for i in 101..=110 {
			tracker.record_block(&network, i).await.unwrap();
		}

		// Jump ahead to trigger finalization
		// Need to advance by more than tolerance to finalize
		// finalize_up_to = 121 - 10 = 111, which is > 110 (old_watermark)
		tracker.record_block(&network, 121).await.unwrap();

		// Verify blocks were finalized
		let pending = tracker.pending_blocks.lock().await;
		let network_pending = pending.get("test_net").unwrap();

		// Blocks 100-111 should be finalized (121 - 10 = 111)
		assert!(!network_pending.contains(&100));
		assert!(!network_pending.contains(&105));
		assert!(!network_pending.contains(&111));

		// Recent blocks should still be pending
		assert!(network_pending.contains(&121));
	}

	#[tokio::test]
	async fn test_buffered_mode_out_of_order_within_tolerance() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network = create_test_network("test-net", "test_net", false);
		network.block_tracking_tolerance = Some(10);

		// Process blocks out of order but within tolerance
		tracker.record_block(&network, 100).await.unwrap();
		tracker.record_block(&network, 102).await.unwrap();
		tracker.record_block(&network, 101).await.unwrap(); // Out of order but within tolerance
		tracker.record_block(&network, 103).await.unwrap();

		// All blocks should be in pending
		let pending = tracker.pending_blocks.lock().await;
		let network_pending = pending.get("test_net").unwrap();
		assert!(network_pending.contains(&100));
		assert!(network_pending.contains(&101));
		assert!(network_pending.contains(&102));
		assert!(network_pending.contains(&103));
	}

	#[tokio::test]
	async fn test_buffered_mode_duplicate_detection() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network = create_test_network("test-net", "test_net", false);
		network.block_tracking_tolerance = Some(10);

		tracker.record_block(&network, 100).await.unwrap();
		tracker.record_block(&network, 101).await.unwrap();

		// Try to record block 101 again - should detect duplicate
		tracker.record_block(&network, 101).await.unwrap();

		let pending = tracker.pending_blocks.lock().await;
		let network_pending = pending.get("test_net").unwrap();

		// Should only contain one instance of 101
		assert_eq!(network_pending.iter().filter(|&&b| b == 101).count(), 1);
	}

	#[tokio::test]
	async fn test_buffered_mode_block_too_far_behind() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network = create_test_network("test-net", "test_net", false);
		network.block_tracking_tolerance = Some(10);

		// Establish watermark with a sequence of blocks
		for i in 100..=110 {
			tracker.record_block(&network, i).await.unwrap();
		}

		// Advance watermark to trigger finalization
		tracker.record_block(&network, 120).await.unwrap();

		// Try to record a block that's too far behind (outside tolerance)
		// Watermark is 120, tolerance is 10, so blocks < 110 should be rejected
		tracker.record_block(&network, 105).await.unwrap();

		let pending = tracker.pending_blocks.lock().await;
		let network_pending = pending.get("test_net").unwrap();

		// Block 105 should NOT be in pending (too far behind watermark - tolerance = 110)
		assert!(!network_pending.contains(&105));
		// Blocks that were finalized (up to 120 - 10 = 110) should not be in pending
		assert!(!network_pending.contains(&100));
		assert!(!network_pending.contains(&110));
		// Recent blocks should still be pending
		assert!(network_pending.contains(&120));
	}

	#[tokio::test]
	async fn test_buffered_mode_missed_blocks_detection() {
		let mut mock_storage = MockBlockStorage::new();

		// We'll skip blocks 115 and 116, and detect them when we finalize
		mock_storage
			.expect_save_missed_block()
			.with(
				mockall::predicate::eq("test_net"),
				mockall::predicate::eq(115),
			)
			.times(1)
			.returning(|_, _| Ok(()));

		mock_storage
			.expect_save_missed_block()
			.with(
				mockall::predicate::eq("test_net"),
				mockall::predicate::eq(116),
			)
			.times(1)
			.returning(|_, _| Ok(()));

		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network = create_test_network("test-net", "test_net", true); // store_blocks = true
		network.block_tracking_tolerance = Some(10);

		// Process blocks with gaps at 115 and 116
		for i in 100..=114 {
			tracker.record_block(&network, i).await.unwrap();
		}
		// Skip 115 and 116 - these are the missed blocks we want to detect

		// Jump from 114 to 126
		// old_watermark = 114, new block = 126
		// Condition: 126 > 114 + 5? → 126 > 119? → YES ✓ (triggers finalization)
		// finalize_up_to = 126 - 10 = 116
		// Checks blocks (114+1)..=116, which is 115..=116
		// Both blocks 115 and 116 are missing and should be reported
		tracker.record_block(&network, 126).await.unwrap();

		// Blocks 115 and 116 should have been detected as missed and stored
	}

	#[tokio::test]
	async fn test_buffered_mode_watermark_advancement() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network = create_test_network("test-net", "test_net", false);
		network.block_tracking_tolerance = Some(10);

		tracker.record_block(&network, 100).await.unwrap();

		{
			let watermarks = tracker.watermarks.lock().await;
			assert_eq!(watermarks.get("test_net"), Some(&100));
		}

		tracker.record_block(&network, 105).await.unwrap();

		{
			let watermarks = tracker.watermarks.lock().await;
			assert_eq!(watermarks.get("test_net"), Some(&105));
		}

		// Lower block shouldn't change watermark
		tracker.record_block(&network, 103).await.unwrap();

		{
			let watermarks = tracker.watermarks.lock().await;
			assert_eq!(watermarks.get("test_net"), Some(&105));
		}
	}

	#[tokio::test]
	async fn test_buffered_mode_finalization_threshold() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network = create_test_network("test-net", "test_net", false);
		network.block_tracking_tolerance = Some(10);

		tracker.record_block(&network, 100).await.unwrap();

		// Advance by less than tolerance/2 (< 5) - should NOT finalize
		tracker.record_block(&network, 104).await.unwrap();

		{
			let pending = tracker.pending_blocks.lock().await;
			let network_pending = pending.get("test_net").unwrap();
			// Both blocks should still be pending (no finalization yet)
			assert!(network_pending.contains(&100));
			assert!(network_pending.contains(&104));
		}

		// Advance by more than tolerance/2 (>= 5) - SHOULD finalize
		tracker.record_block(&network, 110).await.unwrap();

		{
			let pending = tracker.pending_blocks.lock().await;
			let network_pending = pending.get("test_net").unwrap();

			// finalize_up_to = 110 - 10 = 100
			// So block 100 should be finalized (removed from pending)
			assert!(!network_pending.contains(&100));
			// But blocks after 100 should still be pending
			assert!(network_pending.contains(&104));
			assert!(network_pending.contains(&110));
		}
	}

	#[tokio::test]
	async fn test_buffered_mode_history_after_finalization() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network = create_test_network("test-net", "test_net", false);
		network.block_tracking_tolerance = Some(10);

		// Process a sequence of blocks
		for i in 100..=110 {
			tracker.record_block(&network, i).await.unwrap();
		}

		// Trigger finalization
		tracker.record_block(&network, 121).await.unwrap();

		// Check that finalized blocks are in history
		let history = tracker.block_history.lock().await;
		let network_history = history.get("test_net").unwrap();

		// Blocks up to finalize_up_to (121 - 10 = 111) should be in history
		assert!(network_history.contains(&100));
		assert!(network_history.contains(&105));
		assert!(network_history.contains(&110));

		// History should be sorted
		let history_vec: Vec<u64> = network_history.iter().copied().collect();
		let mut sorted = history_vec.clone();
		sorted.sort_unstable();
		assert_eq!(history_vec, sorted);
	}

	#[tokio::test]
	async fn test_buffered_mode_multiple_networks_independent() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network1 = create_test_network("net-1", "net_1", false);
		network1.block_tracking_tolerance = Some(10);

		let mut network2 = create_test_network("net-2", "net_2", false);
		network2.block_tracking_tolerance = Some(5);

		// Process blocks for both networks
		tracker.record_block(&network1, 100).await.unwrap();
		tracker.record_block(&network2, 200).await.unwrap();
		tracker.record_block(&network1, 110).await.unwrap();
		tracker.record_block(&network2, 210).await.unwrap();

		// Check watermarks are independent
		let watermarks = tracker.watermarks.lock().await;
		assert_eq!(watermarks.get("net_1"), Some(&110));
		assert_eq!(watermarks.get("net_2"), Some(&210));

		// Check pending blocks are independent
		let pending = tracker.pending_blocks.lock().await;
		let net1_pending = pending.get("net_1").unwrap();
		let net2_pending = pending.get("net_2").unwrap();

		// Network 1: finalize_up_to = 110 - 10 = 100, so block 100 is finalized
		assert!(!net1_pending.contains(&100));
		assert!(net1_pending.contains(&110));
		assert!(!net1_pending.contains(&200));

		// Network 2: finalize_up_to = 210 - 5 = 205, so block 200 is finalized
		assert!(!net2_pending.contains(&200));
		assert!(net2_pending.contains(&210));
		assert!(!net2_pending.contains(&100));
	}

	#[tokio::test]
	async fn test_buffered_mode_edge_case_tolerance_one() {
		let mock_storage = MockBlockStorage::new();
		let tracker = BlockTracker::new(100, Some(Arc::new(mock_storage)));

		let mut network = create_test_network("test-net", "test_net", false);
		network.block_tracking_tolerance = Some(1);

		tracker.record_block(&network, 100).await.unwrap();
		tracker.record_block(&network, 101).await.unwrap();

		// With tolerance=1, blocks should be finalized quickly
		tracker.record_block(&network, 102).await.unwrap();

		let pending = tracker.pending_blocks.lock().await;
		let network_pending = pending.get("test_net").unwrap();

		// Should have finalized some blocks
		// finalize_up_to = 102 - 1 = 101
		println!("network_pending: {:?}", network_pending);
		assert!(!network_pending.contains(&100));
	}
}
