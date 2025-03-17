//! Block watcher service implementation.
//!
//! Provides functionality to watch and process blockchain blocks across multiple networks,
//! managing individual watchers for each network and coordinating block processing.

use anyhow::Context;
use futures::{channel::mpsc, future::BoxFuture, stream::StreamExt, SinkExt};
use std::{
	collections::{BTreeMap, HashMap},
	sync::Arc,
};
use tokio::sync::RwLock;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::instrument;

use crate::{
	models::{BlockType, Network, ProcessedBlock},
	services::{
		blockchain::BlockChainClient,
		blockwatcher::{
			error::BlockWatcherError,
			storage::BlockStorage,
			tracker::{BlockTracker, BlockTrackerTrait},
		},
	},
};

/// Trait for job scheduler
///
/// This trait is used to abstract the job scheduler implementation.
/// It is used to allow the block watcher service to be used with different job scheduler
/// implementations.
#[async_trait::async_trait]
pub trait JobSchedulerTrait: Send + Sync + Sized {
	async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>>;
	async fn add(&self, job: Job) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
	async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
	async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Implementation of the job scheduler trait for the JobScheduler struct
#[async_trait::async_trait]
impl JobSchedulerTrait for JobScheduler {
	async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
		Self::new().await.map_err(Into::into)
	}

	async fn add(&self, job: Job) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		self.add(job).await.map(|_| ()).map_err(Into::into)
	}

	async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		self.start().await.map(|_| ()).map_err(Into::into)
	}

	async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
		self.shutdown().await.map(|_| ()).map_err(Into::into)
	}
}

/// Watcher implementation for a single network
///
/// Manages block watching and processing for a specific blockchain network,
/// including scheduling and block handling.
///
/// # Type Parameters
/// * `S` - Storage implementation for blocks
/// * `H` - Handler function for processed blocks
/// * `T` - Trigger handler function for processed blocks
/// * `J` - Job scheduler implementation (must implement JobSchedulerTrait)
pub struct NetworkBlockWatcher<S, H, T, J>
where
	J: JobSchedulerTrait,
{
	pub network: Network,
	pub block_storage: Arc<S>,
	pub block_handler: Arc<H>,
	pub trigger_handler: Arc<T>,
	pub scheduler: J,
	pub block_tracker: Arc<BlockTracker<S>>,
}

/// Map of active block watchers
type BlockWatchersMap<S, H, T, J> = HashMap<String, NetworkBlockWatcher<S, H, T, J>>;

/// Service for managing multiple network watchers
///
/// Coordinates block watching across multiple networks, managing individual
/// watchers and their lifecycles.
///
/// # Type Parameters
/// * `S` - Storage implementation for blocks
/// * `H` - Handler function for processed blocks
/// * `T` - Trigger handler function for processed blocks
/// * `J` - Job scheduler implementation (must implement JobSchedulerTrait)
pub struct BlockWatcherService<S, H, T, J>
where
	J: JobSchedulerTrait,
{
	pub block_storage: Arc<S>,
	pub block_handler: Arc<H>,
	pub trigger_handler: Arc<T>,
	pub active_watchers: Arc<RwLock<BlockWatchersMap<S, H, T, J>>>,
	pub block_tracker: Arc<BlockTracker<S>>,
}

impl<S, H, T, J> NetworkBlockWatcher<S, H, T, J>
where
	S: BlockStorage + Send + Sync + 'static,
	H: Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync + 'static,
	T: Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
	J: JobSchedulerTrait,
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
		trigger_handler: Arc<T>,
		block_tracker: Arc<BlockTracker<S>>,
	) -> Result<Self, BlockWatcherError> {
		let scheduler = J::new().await.map_err(|e| {
			BlockWatcherError::scheduler_error(
				e.to_string(),
				Some(e),
				Some(HashMap::from([(
					"network".to_string(),
					network.slug.clone(),
				)])),
			)
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
				let _ = process_new_blocks(
					&network,
					&rpc_client,
					block_storage,
					block_handler,
					trigger_handler,
					block_tracker,
				)
				.await
				.map_err(|e| {
					BlockWatcherError::processing_error(
						"Failed to process blocks".to_string(),
						Some(e.into()),
						Some(HashMap::from([(
							"network".to_string(),
							network.slug.clone(),
						)])),
					)
				});
			})
		})
		.with_context(|| "Failed to create job")?;

		self.scheduler.add(job).await.map_err(|e| {
			BlockWatcherError::scheduler_error(
				e.to_string(),
				Some(e),
				Some(HashMap::from([(
					"network".to_string(),
					self.network.slug.clone(),
				)])),
			)
		})?;

		self.scheduler.start().await.map_err(|e| {
			BlockWatcherError::scheduler_error(
				e.to_string(),
				Some(e),
				Some(HashMap::from([(
					"network".to_string(),
					self.network.slug.clone(),
				)])),
			)
		})?;

		tracing::info!("Started block watcher for network: {}", self.network.slug);
		Ok(())
	}

	/// Stops the network watcher
	///
	/// Shuts down the scheduler and stops watching for new blocks.
	pub async fn stop(&mut self) -> Result<(), BlockWatcherError> {
		self.scheduler.shutdown().await.map_err(|e| {
			BlockWatcherError::scheduler_error(
				e.to_string(),
				Some(e),
				Some(HashMap::from([(
					"network".to_string(),
					self.network.slug.clone(),
				)])),
			)
		})?;

		tracing::info!("Stopped block watcher for network: {}", self.network.slug);
		Ok(())
	}
}

impl<S, H, T, J> BlockWatcherService<S, H, T, J>
where
	S: BlockStorage + Send + Sync + 'static,
	H: Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync + 'static,
	T: Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
	J: JobSchedulerTrait,
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
		trigger_handler: Arc<T>,
		block_tracker: Arc<BlockTracker<S>>,
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
			tracing::info!(
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
/// * `rpc_client` - RPC client for the network
/// * `block_storage` - Storage implementation for blocks
/// * `block_handler` - Handler function for processed blocks
/// * `trigger_handler` - Handler function for processed blocks
/// * `block_tracker` - Tracker implementation for block processing
///
/// # Returns
/// * `Result<(), BlockWatcherError>` - Success or error
#[instrument(skip_all, fields(network = network.slug))]
pub async fn process_new_blocks<
	S: BlockStorage,
	C: BlockChainClient + Send + Clone + 'static,
	H: Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync + 'static,
	T: Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
	TR: BlockTrackerTrait<S>,
>(
	network: &Network,
	rpc_client: &C,
	block_storage: Arc<S>,
	block_handler: Arc<H>,
	trigger_handler: Arc<T>,
	block_tracker: Arc<TR>,
) -> Result<(), BlockWatcherError> {
	let start_time = std::time::Instant::now();

	let last_processed_block = block_storage
		.get_last_processed_block(&network.slug)
		.await
		.with_context(|| "Failed to get last processed block")?
		.unwrap_or(0);

	let latest_block = rpc_client
		.get_latest_block_number()
		.await
		.with_context(|| "Failed to get latest block number")?;

	let latest_confirmed_block = latest_block.saturating_sub(network.confirmation_blocks);

	let recommended_past_blocks = network.get_recommended_past_blocks();

	let max_past_blocks = network.max_past_blocks.unwrap_or(recommended_past_blocks);

	// Calculate the start block number, using the default if max_past_blocks is not set
	let start_block = std::cmp::max(
		last_processed_block + 1,
		latest_confirmed_block.saturating_sub(max_past_blocks),
	);

	tracing::info!(
		"Processing blocks:\n\tLast processed block: {}\n\tLatest confirmed block: {}\n\tStart \
		 block: {}{}\n\tConfirmations required: {}\n\tMax past blocks: {}",
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
			.with_context(|| format!("Failed to get block {}", latest_confirmed_block))?;
	} else if last_processed_block < latest_confirmed_block {
		blocks = rpc_client
			.get_blocks(start_block, Some(latest_confirmed_block))
			.await
			.with_context(|| {
				format!(
					"Failed to get blocks from {} to {}",
					start_block, latest_confirmed_block
				)
			})?;
	}

	// Create channels for our pipeline
	let (process_tx, process_rx) = mpsc::channel::<(BlockType, u64)>(blocks.len() * 2);
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
				.buffer_unordered(32);

			// Process all results and send them to trigger channel
			while let Some(result) = results.next().await {
				trigger_tx
					.send(result)
					.await
					.with_context(|| "Failed to send processed block")?;
			}

			Ok::<(), BlockWatcherError>(())
		}
	});

	// Stage 2: Trigger Pipeline
	let trigger_handle = tokio::spawn({
		let trigger_handler = trigger_handler.clone();

		async move {
			let mut trigger_rx = trigger_rx;
			let mut pending_blocks = BTreeMap::new();
			let mut next_block_number = Some(start_block);

			// Process all incoming blocks
			while let Some(processed_block) = trigger_rx.next().await {
				let block_number = processed_block.block_number;
				pending_blocks.insert(block_number, processed_block);

				// Process blocks in order as long as we have the next expected block
				while let Some(expected) = next_block_number {
					if let Some(block) = pending_blocks.remove(&expected) {
						(trigger_handler)(&block);
						next_block_number = Some(expected + 1);
					} else {
						break;
					}
				}
			}

			// Process any remaining blocks in order after the channel is closed
			while let Some(min_block) = pending_blocks.keys().next().copied() {
				if let Some(block) = pending_blocks.remove(&min_block) {
					(trigger_handler)(&block);
				}
			}
			Ok::<(), BlockWatcherError>(())
		}
	});

	// Feed blocks into the pipeline
	futures::future::join_all(blocks.iter().map(|block| {
		let network = network.clone();
		let block_tracker = block_tracker.clone();
		let mut process_tx = process_tx.clone();
		async move {
			let block_number = block.number().unwrap_or(0);

			// Record block in tracker
			block_tracker.record_block(&network, block_number).await?;

			// Send block to processing pipeline
			process_tx
				.send((block.clone(), block_number))
				.await
				.with_context(|| "Failed to send block to pipeline")?;

			Ok::<(), BlockWatcherError>(())
		}
	}))
	.await
	.into_iter()
	.collect::<Result<Vec<_>, _>>()
	.with_context(|| format!("Failed to process blocks for network {}", network.slug))?;

	// Drop the sender after all blocks are sent
	drop(process_tx);
	drop(trigger_tx);

	// Wait for both pipeline stages to complete
	let (_process_result, _trigger_result) = tokio::join!(process_handle, trigger_handle);

	if network.store_blocks.unwrap_or(false) {
		// Delete old blocks before saving new ones
		block_storage
			.delete_blocks(&network.slug)
			.await
			.with_context(|| "Failed to delete old blocks")?;

		block_storage
			.save_blocks(&network.slug, &blocks)
			.await
			.with_context(|| "Failed to save blocks")?;
	}
	// Update the last processed block
	block_storage
		.save_last_processed_block(&network.slug, latest_confirmed_block)
		.await
		.with_context(|| "Failed to save last processed block")?;

	tracing::info!(
		"Processed {} blocks in {}ms",
		blocks.len(),
		start_time.elapsed().as_millis()
	);

	Ok(())
}
