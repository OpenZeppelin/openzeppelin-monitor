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
	collections::{HashMap, VecDeque},
	sync::Arc,
};
use tokio::sync::Mutex;

use crate::models::Network;

/// Trait for the BlockTracker
///
/// This trait defines the interface for the BlockTracker.
#[async_trait]
pub trait BlockTrackerTrait {
	fn new(history_size: usize) -> Self;
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
pub struct BlockTracker {
	/// Tracks the last N blocks processed for each network
	/// Key: network_slug, Value: Queue of block numbers
	block_history: Arc<Mutex<HashMap<String, VecDeque<u64>>>>,
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
			block_history: Arc::new(Mutex::new(HashMap::new())),
			history_size,
		}
	}

	/// Records a processed block and maintains block history.
	///
	/// This method:
	/// - Adds the block to the network's history
	/// - Maintains the history size limit
	///
	/// Note: This method does not persist to storage. Storage persistence
	/// is handled separately by save_last_processed_block in the service layer.
	///
	/// # Arguments
	///
	/// * `network` - The network information for the processed block
	/// * `block_number` - The block number being recorded
	/// * `fetched_blocks` - Set of block numbers that were fetched from RPC in this execution (unused, kept for trait compatibility)
	async fn record_block(
		&self,
		network: &Network,
		block_number: u64,
	) -> Result<(), anyhow::Error> {
		let mut history = self.block_history.lock().await;
		let network_history = history
			.entry(network.slug.clone())
			.or_insert_with(|| VecDeque::with_capacity(self.history_size));

		// Add the new block to history
		network_history.push_back(block_number);

		// Maintain history size
		while network_history.len() > self.history_size {
			network_history.pop_front();
		}

		Ok(())
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
		tracker.record_block(&network, 1).await.unwrap();
		tracker.record_block(&network, 2).await.unwrap();
		tracker.record_block(&network, 3).await.unwrap();

		assert_eq!(tracker.get_last_block("test_net").await, Some(3));
	}

	#[tokio::test]
	async fn test_history_size_limit() {
		let tracker = BlockTracker::new(3);
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
	async fn test_record_block_maintains_history() {
		let tracker = BlockTracker::new(5);
		let network = create_test_network("test-net", "test_net", true);

		// Process block 1 - should add to history
		tracker.record_block(&network, 1).await.unwrap();
		assert_eq!(tracker.get_last_block("test_net").await, Some(1));

		// Process block 3 - should add to history
		tracker.record_block(&network, 3).await.unwrap();
		assert_eq!(tracker.get_last_block("test_net").await, Some(3));
	}

	#[tokio::test]
	async fn test_out_of_order_blocks() {
		let tracker = BlockTracker::new(5);
		let network = create_test_network("test-net", "test_net", true);

		// Process blocks out of order - record_block no longer detects this
		tracker.record_block(&network, 2).await.unwrap();
		tracker.record_block(&network, 1).await.unwrap();

		assert_eq!(tracker.get_last_block("test_net").await, Some(1));
	}

	#[tokio::test]
	async fn test_multiple_networks() {
		let tracker = BlockTracker::new(5);
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
		let tracker = BlockTracker::new(5);
		assert_eq!(tracker.get_last_block("nonexistent").await, None);
	}

	#[tokio::test]
	async fn test_record_block_with_gaps() {
		let tracker = BlockTracker::new(5);
		let network = create_test_network("test-network", "test_network", true);

		// Process block 1
		tracker.record_block(&network, 1).await.unwrap();
		assert_eq!(tracker.get_last_block("test_network").await, Some(1));

		// Process block 3 (gap detection should happen at service layer)
		tracker.record_block(&network, 3).await.unwrap();
		assert_eq!(tracker.get_last_block("test_network").await, Some(3));
	}
}
