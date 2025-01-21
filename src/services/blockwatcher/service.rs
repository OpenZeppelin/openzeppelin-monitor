//! Block watcher service implementation.
//!
//! Provides functionality to watch and process blockchain blocks across multiple networks,
//! managing individual watchers for each network and coordinating block processing.

use futures::{channel::mpsc, future::BoxFuture, stream::StreamExt, SinkExt};
use log::{error, info};
use std::{
	collections::{BTreeMap, HashMap},
	sync::Arc,
};
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};

use crate::{
	models::{BlockType, Network, ProcessedBlock},
	services::{
		blockchain::BlockChainClient,
		blockwatcher::{error::BlockWatcherError, storage::BlockStorage, BlockTracker},
	},
};

type BlockHandler =
	Arc<dyn Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync>;
type TriggerHandler = Arc<dyn Fn(&ProcessedBlock) + Send + Sync>;

/// Watcher implementation for a single network
///
/// Manages block watching and processing for a specific blockchain network,
/// including scheduling and block handling.
pub struct NetworkBlockWatcher<B>
where
	B: BlockStorage + Send + Sync + 'static,
{
	network: Network,
	block_storage: Arc<B>,
	block_handler: BlockHandler,
	trigger_handler: TriggerHandler,
	scheduler: JobScheduler,
	block_tracker: Arc<BlockTracker<B>>,
}

/// Service for managing multiple network watchers
///
/// Coordinates block watching across multiple networks, managing individual
/// watchers and their lifecycles.
pub struct BlockWatcherService<B>
where
	B: BlockStorage + Send + Sync + 'static,
{
	block_storage: Arc<B>,
	block_handler: BlockHandler,
	trigger_handler: TriggerHandler,
	active_watchers: Arc<RwLock<HashMap<String, NetworkBlockWatcher<B>>>>,
	block_tracker: Arc<BlockTracker<B>>,
}

impl<B> NetworkBlockWatcher<B>
where
	B: BlockStorage + Send + Sync + 'static,
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
		block_storage: Arc<B>,
		block_handler: BlockHandler,
		trigger_handler: TriggerHandler,
		block_tracker: Arc<BlockTracker<B>>,
	) -> Result<Self, BlockWatcherError> {
		let scheduler = JobScheduler::new().await.map_err(|e| {
			BlockWatcherError::scheduler_error(format!("Failed to create scheduler: {}", e))
		})?;
		Ok(Self {
			network,
			block_storage,
			block_handler,
			trigger_handler,
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
		let trigger_handler = self.trigger_handler.clone();
		let block_tracker = self.block_tracker.clone();

		let job = Job::new_async(self.network.cron_schedule.as_str(), move |_uuid, _l| {
			let network = network.clone();
			let block_storage = block_storage.clone();
			let block_handler = block_handler.clone();
			let block_tracker = block_tracker.clone();
			let rpc_client = rpc_client.clone();
			let trigger_handler = trigger_handler.clone();
			Box::pin(async move {
				match process_new_blocks(
					&network,
					&rpc_client,
					block_storage,
					block_handler,
					trigger_handler,
					block_tracker,
				)
				.await
				{
					Ok(_) => info!(
						"Network {} ({}) processed blocks successfully",
						network.name, network.slug
					),
					Err(e) => error!(
						"Network {} ({}) error processing blocks: {}",
						network.name, network.slug, e
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

impl<B> BlockWatcherService<B>
where
	B: BlockStorage + Send + Sync + 'static,
{
	/// Creates a new block watcher service
	///
	/// # Arguments
	/// * `network_service` - Service for network operations
	/// * `block_storage` - Storage implementation for blocks
	/// * `block_handler` - Handler function for processed blocks
	pub async fn new(
		block_storage: Arc<B>,
		block_handler: BlockHandler,
		trigger_handler: TriggerHandler,
		block_tracker: Arc<BlockTracker<B>>,
	) -> Result<Self, BlockWatcherError> {
		Ok(BlockWatcherService {
			block_storage,
			block_handler,
			trigger_handler,
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
			self.trigger_handler.clone(),
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
async fn process_new_blocks<B: BlockStorage, C: BlockChainClient + Send + Clone + 'static>(
	network: &Network,
	rpc_client: &C,
	block_storage: Arc<B>,
	block_handler: BlockHandler,
	trigger_handler: TriggerHandler,
	block_tracker: Arc<BlockTracker<B>>,
) -> Result<(), BlockWatcherError> {
	let start_time = std::time::Instant::now();

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

	// Calculate the start block number, using the default if max_past_blocks is not set
	let start_block = std::cmp::max(
		last_processed_block + 1,
		latest_confirmed_block.saturating_sub(max_past_blocks.saturating_sub(1)),
	);

	info!(
		"Network {} ({}) processing blocks:\n\tLast processed block: {}\n\tLatest confirmed \
		 block: {}\n\tStart block: {}{}\n\tConfirmations required: {}\n\tMax past blocks: {}",
		network.name,
		network.slug,
		last_processed_block,
		latest_confirmed_block,
		start_block,
		if start_block > last_processed_block + 1 {
			format!(
				" (skipped {} blocks)",
				start_block - (last_processed_block + 1)
			)
		} else {
			String::new()
		},
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

	// Create channels for our pipeline
	let (mut process_tx, process_rx) = mpsc::channel::<(BlockType, u64)>(blocks.len() * 2);
	let (trigger_tx, trigger_rx) = mpsc::channel::<ProcessedBlock>(blocks.len() * 2);

	// Stage 1: Block Processing Pipeline
	let process_handle = tokio::spawn({
		let network = network.clone();
		let block_handler = block_handler.clone();
		let mut trigger_tx = trigger_tx.clone();

		async move {
			// Process blocks concurrently, up to 32 at a time
			let mut results = process_rx
				.map(|(block, _)| {
					let network = network.clone();
					let block_handler = block_handler.clone();
					async move { (block_handler)(block, network).await }
				})
				.buffer_unordered(32); // TODO: This is an arbitrary number. Make this configurable

			// Process all results and send them to trigger channel
			while let Some(result) = results.next().await {
				trigger_tx.send(result).await.map_err(|e| {
					BlockWatcherError::processing_error(format!(
						"Failed to send processed block: {}",
						e
					))
				})?;
			}

			Ok::<(), BlockWatcherError>(())
		}
	});

	// Stage 2: Trigger Pipeline (maintains order)
	let trigger_handle = tokio::spawn({
		let trigger_handler = trigger_handler.clone();

		async move {
			use futures::StreamExt;
			let mut trigger_rx = trigger_rx;
			let mut expected_block = None;
			let mut pending_blocks = BTreeMap::new();

			while let Some(processed_block) = trigger_rx.next().await {
				let block_number = processed_block.block_number;

				// Initialize expected_block if this is the first block
				if expected_block.is_none() {
					expected_block = Some(block_number);
				}

				// Store block in pending map if it's not the next expected block
				if Some(block_number) != expected_block {
					pending_blocks.insert(block_number, processed_block);
					continue;
				}

				// Process the current block
				(trigger_handler)(&processed_block);

				// Process any subsequent pending blocks that are now ready
				expected_block = Some(block_number + 1);
				while let Some(expected) = expected_block {
					if let Some(next_block) = pending_blocks.remove(&expected) {
						(trigger_handler)(&next_block);
						expected_block = Some(expected + 1);
					} else {
						break;
					}
				}
			}
			Ok::<(), BlockWatcherError>(())
		}
	});

	// Feed blocks into the pipeline
	for block in &blocks {
		// Record block in tracker
		block_tracker
			.record_block(network, block.number().unwrap_or(0))
			.await;

		// Send block to processing pipeline
		process_tx
			.send((block.clone(), block.number().unwrap_or(0)))
			.await
			.map_err(|e| {
				BlockWatcherError::processing_error(format!(
					"Failed to send block to pipeline: {}",
					e
				))
			})?;
	}

	// Drop the sender after all blocks are sent
	drop(process_tx);
	drop(trigger_tx);

	// Wait for both pipeline stages to complete
	let (process_result, trigger_result) = tokio::join!(process_handle, trigger_handle);
	process_result.map_err(|e| BlockWatcherError::processing_error(e.to_string()))??;
	trigger_result.map_err(|e| BlockWatcherError::processing_error(e.to_string()))??;

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

	let duration = start_time.elapsed();
	info!(
		"Network {} ({}) processed {} blocks in {:.2?}",
		network.name,
		network.slug,
		blocks.len(),
		duration
	);

	Ok(())
}
