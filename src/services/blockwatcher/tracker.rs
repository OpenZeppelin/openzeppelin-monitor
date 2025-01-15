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

use std::{
	collections::{HashMap, VecDeque},
	sync::Arc,
};
use tokio::sync::Mutex;

use crate::{
	models::Network,
	services::blockwatcher::{error::BlockWatcherError, storage::BlockStorage},
};
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
pub struct BlockTracker<S: BlockStorage> {
	/// Tracks the last N blocks processed for each network
	/// Key: network_slug, Value: Queue of block numbers
	block_history: Arc<Mutex<HashMap<String, VecDeque<u64>>>>,
	/// Maximum number of blocks to keep in history per network
	history_size: usize,
	/// Storage interface for persisting missed blocks
	storage: Option<Arc<S>>,
}

impl<S: BlockStorage> BlockTracker<S> {
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
	pub fn new(history_size: usize, storage: Option<Arc<S>>) -> Self {
		Self {
			block_history: Arc::new(Mutex::new(HashMap::new())),
			history_size,
			storage,
		}
	}

	/// Records a processed block and identifies any gaps in block sequence.
	///
	/// This method performs several checks:
	/// - Detects gaps between the last processed block and the current block
	/// - Identifies out-of-order or duplicate blocks
	/// - Stores information about missed blocks if storage is configured
	///
	/// # Arguments
	///
	/// * `network` - The network information for the processed block
	/// * `block_number` - The block number being recorded
	///
	/// # Warning
	///
	/// This method will log warnings for out-of-order blocks and errors for missed blocks.
	pub async fn record_block(&self, network: &Network, block_number: u64) {
		let mut history = self.block_history.lock().await;
		let network_history = history
			.entry(network.slug.clone())
			.or_insert_with(|| VecDeque::with_capacity(self.history_size));

		// Check for gaps if we have previous blocks
		if let Some(&last_block) = network_history.back() {
			if block_number > last_block + 1 {
				// Log each missed block number
				for missed in (last_block + 1)..block_number {
					BlockWatcherError::block_tracker_error(format!(
						"Missed block {} on network {}",
						missed, network.slug
					));

					if network.store_blocks.unwrap_or(false) {
						if let Some(storage) = &self.storage {
							// Store the missed block info
							if let Err(e) = storage.save_missed_block(&network.slug, missed).await {
								BlockWatcherError::storage_error(format!(
									"Failed to store missed block {} for network {}: {}",
									missed, network.slug, e
								));
							}
						}
					}
				}
			} else if block_number <= last_block {
				BlockWatcherError::block_tracker_error(format!(
					"Out of order or duplicate block detected for network {}: received {} after {}",
					network.slug, block_number, last_block
				));
			}
		}

		// Add the new block to history
		network_history.push_back(block_number);

		// Maintain history size
		while network_history.len() > self.history_size {
			network_history.pop_front();
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
	pub async fn get_last_block(&self, network_slug: &str) -> Option<u64> {
		self.block_history
			.lock()
			.await
			.get(network_slug)
			.and_then(|history| history.back().copied())
	}
}
