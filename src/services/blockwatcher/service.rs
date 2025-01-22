//! Block watcher service implementation.
//!
//! Provides functionality to watch and process blockchain blocks across multiple networks,
//! managing individual watchers for each network and coordinating block processing.

use log::{error, info};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::{
	models::{BlockType, Network},
	services::{
		blockchain::BlockChainClient,
		blockwatcher::{error::BlockWatcherError, storage::BlockStorage, BlockTracker},
	},
};

/// Watcher implementation for a single network
///
/// Manages block watching and processing for a specific blockchain network,
/// including scheduling and block handling.
pub struct NetworkBlockWatcher<S, H> {
	network: Network,
	block_storage: Arc<S>,
	block_handler: Arc<H>,
	scheduler: JobScheduler,
	block_tracker: Arc<BlockTracker<S>>,
}

/// Service for managing multiple network watchers
///
/// Coordinates block watching across multiple networks, managing individual
/// watchers and their lifecycles.
pub struct BlockWatcherService<S, H> {
	block_storage: Arc<S>,
	block_handler: Arc<H>,
	active_watchers: Arc<RwLock<HashMap<String, NetworkBlockWatcher<S, H>>>>,
	block_tracker: Arc<BlockTracker<S>>,
}

impl<S, H> NetworkBlockWatcher<S, H>
where
	S: BlockStorage + Send + Sync + 'static,
	H: Fn(&BlockType, &Network) + Send + Sync + 'static,
{
	/// Creates a new network watcher instance
	///
	/// # Arguments
	/// * `network` - Network configuration
	/// * `block_storage` - Storage implementation for blocks
	/// * `block_handler` - Handler function for processed blocks
	///
	/// # Returns
	/// * `Result<Self, BlockWatcherError>` - New watcher instance or error
	pub async fn new(
		network: Network,
		block_storage: Arc<S>,
		block_handler: Arc<H>,
		block_tracker: Arc<BlockTracker<S>>,
	) -> Result<Self, BlockWatcherError> {
		let scheduler = JobScheduler::new().await.map_err(|e| {
			BlockWatcherError::scheduler_error(format!("Failed to create scheduler: {}", e))
		})?;
		Ok(Self {
			network,
			block_storage,
			block_handler,
			scheduler,
			block_tracker,
		})
	}

	/// Starts the network watcher
	///
	/// Initializes the scheduler and begins watching for new blocks according
	/// to the network's cron schedule.
	pub async fn start<C: BlockChainClient + Clone + Send + 'static>(
		&mut self,
		rpc_client: C,
	) -> Result<(), BlockWatcherError> {
		let network = self.network.clone();
		let block_storage = self.block_storage.clone();
		let block_handler = self.block_handler.clone();
		let block_tracker = self.block_tracker.clone();

		let job = Job::new_async(self.network.cron_schedule.as_str(), move |_uuid, _l| {
			let network = network.clone();
			let block_storage = block_storage.clone();
			let block_handler = block_handler.clone();
			let block_tracker = block_tracker.clone();
			let rpc_client = rpc_client.clone();

			Box::pin(async move {
				match process_new_blocks(
					&network,
					&rpc_client,
					block_storage,
					block_handler,
					block_tracker,
				)
				.await
				{
					Ok(_) => info!(
						"Successfully processed blocks for network: {}",
						network.slug
					),
					Err(e) => error!(
						"Error processing blocks for network {}: {}",
						network.slug, e
					),
				}
			})
		})
		.map_err(|e| BlockWatcherError::scheduler_error(format!("Failed to create job: {}", e)))?;

		self.scheduler
			.add(job)
			.await
			.map_err(|e| BlockWatcherError::scheduler_error(format!("Failed to add job: {}", e)))?;

		self.scheduler.start().await.map_err(|e| {
			BlockWatcherError::scheduler_error(format!("Failed to start scheduler: {}", e))
		})?;

		info!("Started block watcher for network: {}", self.network.slug);
		Ok(())
	}

	/// Stops the network watcher
	///
	/// Shuts down the scheduler and stops watching for new blocks.
	pub async fn stop(&mut self) -> Result<(), BlockWatcherError> {
		self.scheduler.shutdown().await.map_err(|e| {
			BlockWatcherError::scheduler_error(format!("Failed to stop scheduler: {}", e))
		})?;

		info!("Stopped block watcher for network: {}", self.network.slug);
		Ok(())
	}
}

impl<S, H> BlockWatcherService<S, H>
where
	S: BlockStorage + Send + Sync + 'static,
	H: Fn(&BlockType, &Network) + Send + Sync + 'static,
{
	/// Creates a new block watcher service
	///
	/// # Arguments
	/// * `network_service` - Service for network operations
	/// * `block_storage` - Storage implementation for blocks
	/// * `block_handler` - Handler function for processed blocks
	pub async fn new(
		block_storage: Arc<S>,
		block_handler: Arc<H>,
		block_tracker: Arc<BlockTracker<S>>,
	) -> Result<Self, BlockWatcherError> {
		Ok(BlockWatcherService {
			block_storage,
			block_handler,
			active_watchers: Arc::new(RwLock::new(HashMap::new())),
			block_tracker,
		})
	}

	/// Starts a watcher for a specific network
	///
	/// # Arguments
	/// * `network` - Network configuration to start watching
	pub async fn start_network_watcher<C: BlockChainClient + Send + Clone + 'static>(
		&self,
		network: &Network,
		rpc_client: C,
	) -> Result<(), BlockWatcherError> {
		let mut watchers = self.active_watchers.write().await;

		if watchers.contains_key(&network.slug) {
			info!(
				"Block watcher already running for network: {}",
				network.slug
			);
			return Ok(());
		}

		let mut watcher = NetworkBlockWatcher::new(
			network.clone(),
			self.block_storage.clone(),
			self.block_handler.clone(),
			self.block_tracker.clone(),
		)
		.await?;

		watcher.start(rpc_client).await?;
		watchers.insert(network.slug.clone(), watcher);

		Ok(())
	}

	/// Stops a watcher for a specific network
	///
	/// # Arguments
	/// * `network_slug` - Identifier of the network to stop watching
	pub async fn stop_network_watcher(&self, network_slug: &str) -> Result<(), BlockWatcherError> {
		let mut watchers = self.active_watchers.write().await;

		if let Some(mut watcher) = watchers.remove(network_slug) {
			watcher.stop().await?;
		}

		Ok(())
	}
}

/// Processes new blocks for a network
///
/// # Arguments
/// * `network` - Network configuration
/// * `block_storage` - Storage implementation for blocks
/// * `block_handler` - Handler function for processed blocks
///
/// # Returns
/// * `Result<(), BlockWatcherError>` - Success or error
async fn process_new_blocks<
	S: BlockStorage,
	C: BlockChainClient + Send + Clone + 'static,
	H: Fn(&BlockType, &Network) + Send + Sync + 'static,
>(
	network: &Network,
	rpc_client: &C,
	block_storage: Arc<S>,
	block_handler: Arc<H>,
	block_tracker: Arc<BlockTracker<S>>,
) -> Result<(), BlockWatcherError> {
	let last_processed_block = block_storage
		.get_last_processed_block(&network.slug)
		.await
		.map_err(|e| {
			BlockWatcherError::storage_error(format!("Failed to get last processed block: {}", e))
		})?
		.unwrap_or(0);

	let latest_block = rpc_client.get_latest_block_number().await.map_err(|e| {
		BlockWatcherError::network_error(format!("Failed to get latest block number: {}", e))
	})?;

	let latest_confirmed_block = latest_block.saturating_sub(network.confirmation_blocks);

	let recommended_past_blocks = network.get_recommended_past_blocks();

	let max_past_blocks = network.max_past_blocks.unwrap_or(recommended_past_blocks);

	info!(
		"Processing blocks for network {} ({}). Last processed: {}, Latest confirmed: {} (waiting \
		 {} confirmations, max past blocks: {})",
		network.name,
		network.slug,
		last_processed_block,
		latest_confirmed_block,
		network.confirmation_blocks,
		max_past_blocks
	);

	let mut blocks = Vec::new();
	if last_processed_block == 0 {
		blocks = rpc_client
			.get_blocks(latest_confirmed_block, None)
			.await
			.map_err(|e| {
				BlockWatcherError::network_error(format!(
					"Failed to get block {}: {}",
					latest_confirmed_block, e
				))
			})?;
	} else if last_processed_block < latest_confirmed_block {
		// Calculate the start block number, using the default if max_past_blocks is not set
		let start_block = std::cmp::max(
			last_processed_block + 1,
			latest_confirmed_block.saturating_sub(max_past_blocks.saturating_sub(1)),
		);

		blocks = rpc_client
			.get_blocks(start_block, Some(latest_confirmed_block))
			.await
			.map_err(|e| {
				BlockWatcherError::network_error(format!(
					"Failed to get blocks from {} to {}: {}",
					start_block, latest_confirmed_block, e
				))
			})?;
	}

	for block in &blocks {
		let block_number = match block {
			BlockType::EVM(block) => block.number(),
			BlockType::Stellar(block) => Some(block.number()),
		};
		// record the block number in the block tracker service
		// so that if a block is missed, we can log it
		if let Some(number) = block_number {
			block_tracker.record_block(network, number).await;
		}

		// process the block
		(block_handler)(block, network);
	}

	if network.store_blocks.unwrap_or(false) {
		// Delete old blocks before saving new ones
		block_storage
			.delete_blocks(&network.slug)
			.await
			.map_err(|e| {
				BlockWatcherError::storage_error(format!("Failed to delete old blocks: {}", e))
			})?;

		block_storage
			.save_blocks(&network.slug, &blocks)
			.await
			.map_err(|e| {
				BlockWatcherError::storage_error(format!("Failed to save blocks: {}", e))
			})?;
	}
	// Update the last processed block
	block_storage
		.save_last_processed_block(&network.slug, latest_confirmed_block)
		.await
		.map_err(|e| {
			BlockWatcherError::storage_error(format!("Failed to save last processed block: {}", e))
		})?;

	Ok(())
}
