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

use crate::models::{BlockType, Network};

/// Result of checking a processed block for issues
#[derive(Debug, Clone, PartialEq)]
pub enum BlockCheckResult {
	/// Block is valid
	Ok,
	/// Duplicate block detected
	Duplicate { last_seen: u64 },
	/// Block received out of order
	OutOfOrder { expected: u64, received: u64 },
}

/// Trait for the BlockTracker
///
/// This trait defines the interface for the BlockTracker.
#[async_trait]
pub trait BlockTrackerTrait {
	fn new(history_size: usize) -> Self;
	async fn get_last_block(&self, network_slug: &str) -> Option<u64>;
	/// Detects missing blocks in a batch of fetched blocks
	///
	/// Takes the entire fetched block set, detects gaps using optimized min/max approach,
	/// records all fetched blocks to history in batch, and returns list of missed block numbers.
	async fn detect_missing_blocks(
		&self,
		network: &Network,
		fetched_blocks: &[BlockType],
	) -> Vec<u64>;
	/// Checks a processed block for duplicates or out-of-order issues
	///
	/// Tracks processed sequence separately from fetched sequence, detects duplicates and
	/// out-of-order blocks, and returns result enum.
	async fn check_processed_block(&self, network: &Network, block_number: u64)
		-> BlockCheckResult;

	/// Resets the expected next block number for a network to a new starting point.
	/// This should be called at the start of each process_new_blocks execution to
	/// synchronize expected_next with the start_block.
	async fn reset_expected_next(&self, network: &Network, start_block: u64);
}

/// BlockTracker is responsible for monitoring the sequence of processed blocks
/// across different networks and identifying any gaps or irregularities in block processing.
///
/// Gap detection is per-execution and doesn't require shared state, so we don't track
/// fetched blocks in shared history. Only processed blocks are tracked for duplicate/out-of-order detection.
#[derive(Clone)]
pub struct BlockTracker {
	/// Tracks the last N processed blocks for each network
	/// Key: network_slug, Value: Queue of block numbers
	processed_history: Arc<Mutex<HashMap<String, VecDeque<u64>>>>,
	/// Expected next processed block number for each network
	/// Key: network_slug, Value: Expected next block number
	expected_next: Arc<Mutex<HashMap<String, u64>>>,
	/// Maximum number of blocks to keep in history per network
	history_size: usize,
}

#[async_trait]
impl BlockTrackerTrait for BlockTracker {
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
	fn new(history_size: usize) -> Self {
		Self {
			processed_history: Arc::new(Mutex::new(HashMap::new())),
			expected_next: Arc::new(Mutex::new(HashMap::new())),
			history_size,
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
		self.processed_history
			.lock()
			.await
			.get(network_slug)
			.and_then(|history| history.iter().max().copied())
	}

	async fn detect_missing_blocks(
		&self,
		_network: &Network,
		fetched_blocks: &[BlockType],
	) -> Vec<u64> {
		// Extract block numbers from fetched blocks
		let fetched_block_numbers: HashSet<u64> = fetched_blocks
			.iter()
			.filter_map(|block| block.number())
			.collect();

		if fetched_block_numbers.is_empty() {
			return Vec::new();
		}

		// Find min and max without sorting
		let first = *fetched_block_numbers
			.iter()
			.min()
			.expect("fetched_block_numbers is guaranteed to be non-empty");
		let last = *fetched_block_numbers
			.iter()
			.max()
			.expect("fetched_block_numbers is guaranteed to be non-empty");

		// Collect missed blocks
		// Note: Gap detection is per-execution and doesn't require shared state.
		// Each execution only looks at its own fetched blocks, so concurrent executions
		// won't cause false positives.
		let missed_blocks: Vec<u64> = (first..=last)
			.filter(|&num| !fetched_block_numbers.contains(&num))
			.collect();

		missed_blocks
	}

	async fn check_processed_block(
		&self,
		network: &Network,
		block_number: u64,
	) -> BlockCheckResult {
		let mut processed_history = self.processed_history.lock().await;
		let mut expected_next = self.expected_next.lock().await;

		let network_history = processed_history
			.entry(network.slug.clone())
			.or_insert_with(|| VecDeque::with_capacity(self.history_size));

		let expected = expected_next
			.entry(network.slug.clone())
			.or_insert(block_number);

		// Check for duplicate
		if network_history.contains(&block_number) {
			let last_seen = *network_history.back().unwrap_or(&block_number);
			return BlockCheckResult::Duplicate { last_seen };
		}

		// Check for out-of-order (if block is less than expected, it's out of order)
		let result = if block_number < *expected {
			BlockCheckResult::OutOfOrder {
				expected: *expected,
				received: block_number,
			}
		} else {
			BlockCheckResult::Ok
		};

		// Always record the block (even if out of order, we still process it)
		network_history.push_back(block_number);

		// Only update expected_next when the block is in-order or ahead
		// If it's out-of-order (behind), don't advance expected_next as we're still
		// waiting for the missing blocks in between
		if block_number >= *expected {
			*expected = block_number + 1;
		}

		// Maintain history size
		while network_history.len() > self.history_size {
			network_history.pop_front();
		}

		result
	}

	async fn reset_expected_next(&self, network: &Network, start_block: u64) {
		let mut expected_next = self.expected_next.lock().await;
		let entry = expected_next.entry(network.slug.clone());

		// Reset expected_next to start_block if it's higher than start_block
		// This handles cases where we're reprocessing blocks or restarting from an earlier point
		match entry {
			std::collections::hash_map::Entry::Occupied(mut e) => {
				if *e.get() > start_block {
					*e.get_mut() = start_block;
				}
			}
			std::collections::hash_map::Entry::Vacant(e) => {
				e.insert(start_block);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::utils::tests::network::NetworkBuilder;

	use super::*;

	fn create_test_network(name: &str, slug: &str, store_blocks: bool) -> Network {
		NetworkBuilder::new()
			.name(name)
			.slug(slug)
			.store_blocks(store_blocks)
			.build()
	}

	#[tokio::test]
	async fn test_normal_block_sequence() {
		let tracker = BlockTracker::new(5);
		let network = create_test_network("test-net", "test_net", true);

		// Process blocks in sequence
		assert_eq!(
			tracker.check_processed_block(&network, 1).await,
			BlockCheckResult::Ok
		);
		assert_eq!(
			tracker.check_processed_block(&network, 2).await,
			BlockCheckResult::Ok
		);
		assert_eq!(
			tracker.check_processed_block(&network, 3).await,
			BlockCheckResult::Ok
		);

		assert_eq!(tracker.get_last_block("test_net").await, Some(3));
	}

	#[tokio::test]
	async fn test_history_size_limit() {
		let tracker = BlockTracker::new(3);
		let network = create_test_network("test-net", "test_net", true);

		// Process 5 blocks with a history limit of 3
		for i in 1..=5 {
			assert_eq!(
				tracker.check_processed_block(&network, i).await,
				BlockCheckResult::Ok
			);
		}

		let history = tracker.processed_history.lock().await;
		let network_history = history
			.get(&network.slug)
			.expect("Network history should exist");

		// Verify we only kept the last 3 blocks
		assert_eq!(network_history.len(), 3);
		assert_eq!(network_history.front(), Some(&3)); // Oldest block
		assert_eq!(network_history.back(), Some(&5)); // Newest block
	}

	#[tokio::test]
	async fn test_check_processed_block_maintains_history() {
		let tracker = BlockTracker::new(5);
		let network = create_test_network("test-net", "test_net", true);

		// Process block 1 - should add to history
		assert_eq!(
			tracker.check_processed_block(&network, 1).await,
			BlockCheckResult::Ok
		);
		assert_eq!(tracker.get_last_block("test_net").await, Some(1));

		// Process block 3 - should be Ok (ahead of expected, advances expected)
		assert_eq!(
			tracker.check_processed_block(&network, 3).await,
			BlockCheckResult::Ok
		);
		assert_eq!(tracker.get_last_block("test_net").await, Some(3));
	}

	#[tokio::test]
	async fn test_out_of_order_blocks() {
		let tracker = BlockTracker::new(5);
		let network = create_test_network("test-net", "test_net", true);

		// Process blocks out of order - should detect out-of-order
		assert_eq!(
			tracker.check_processed_block(&network, 2).await,
			BlockCheckResult::Ok
		);
		assert_eq!(
			tracker.check_processed_block(&network, 1).await,
			BlockCheckResult::OutOfOrder {
				expected: 3,
				received: 1
			}
		);

		// Both blocks are recorded, but last is the higher one
		// Note: After processing block 2, expected becomes 3, so block 1 is OutOfOrder
		assert_eq!(tracker.get_last_block("test_net").await, Some(2));
	}

	#[tokio::test]
	async fn test_multiple_networks() {
		let tracker = BlockTracker::new(5);
		let network1 = create_test_network("net-1", "net_1", true);
		let network2 = create_test_network("net-2", "net_2", true);

		// Process blocks for both networks
		assert_eq!(
			tracker.check_processed_block(&network1, 1).await,
			BlockCheckResult::Ok
		);
		assert_eq!(
			tracker.check_processed_block(&network2, 100).await,
			BlockCheckResult::Ok
		);
		assert_eq!(
			tracker.check_processed_block(&network1, 2).await,
			BlockCheckResult::Ok
		);
		assert_eq!(
			tracker.check_processed_block(&network2, 101).await,
			BlockCheckResult::Ok
		);

		assert_eq!(tracker.get_last_block("net_1").await, Some(2));
		assert_eq!(tracker.get_last_block("net_2").await, Some(101));
	}

	#[tokio::test]
	async fn test_get_last_block_empty_network() {
		let tracker = BlockTracker::new(5);
		assert_eq!(tracker.get_last_block("nonexistent").await, None);
	}

	#[tokio::test]
	async fn test_check_processed_block_with_gaps() {
		let tracker = BlockTracker::new(5);
		let network = create_test_network("test-network", "test_network", true);

		// Process block 1
		assert_eq!(
			tracker.check_processed_block(&network, 1).await,
			BlockCheckResult::Ok
		);
		assert_eq!(tracker.get_last_block("test_network").await, Some(1));

		// Process block 3 (gap detection happens at service layer via detect_missing_blocks)
		// Block 3 is ahead of expected (2), so it's Ok and advances expected to 4
		assert_eq!(
			tracker.check_processed_block(&network, 3).await,
			BlockCheckResult::Ok
		);
		assert_eq!(tracker.get_last_block("test_network").await, Some(3));
	}
}
