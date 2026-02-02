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
			recovery::process_missed_blocks,
			storage::BlockStorage,
			tracker::{BlockCheckResult, BlockTracker, BlockTrackerTrait},
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
	pub block_tracker: Arc<BlockTracker>,
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
	pub block_tracker: Arc<BlockTracker>,
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
	/// * `trigger_handler` - Trigger handler function
	/// * `block_tracker` - Block tracker instance
	///
	/// # Returns
	/// * `Result<Self, BlockWatcherError>` - New watcher instance or error
	pub async fn new(
		network: Network,
		block_storage: Arc<S>,
		block_handler: Arc<H>,
		trigger_handler: Arc<T>,
		block_tracker: Arc<BlockTracker>,
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
	/// to the network's cron schedule. Also starts the recovery job if enabled.
	pub async fn start<C: BlockChainClient + Clone + Send + 'static>(
		&mut self,
		rpc_client: C,
	) -> Result<(), BlockWatcherError> {
		// Start main block watcher job
		self.start_main_watcher(rpc_client.clone()).await?;

		// Start recovery job if enabled
		if let Some(ref config) = self.network.recovery_config {
			if config.enabled {
				self.start_recovery_job(rpc_client).await?;
			}
		}

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

	/// Starts the main block watcher job
	async fn start_main_watcher<C: BlockChainClient + Clone + Send + 'static>(
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
		.with_context(|| "Failed to create main watcher job")?;

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

		Ok(())
	}

	/// Starts the recovery job for missed blocks
	async fn start_recovery_job<C: BlockChainClient + Clone + Send + 'static>(
		&mut self,
		rpc_client: C,
	) -> Result<(), BlockWatcherError> {
		let recovery_config = self
			.network
			.recovery_config
			.as_ref()
			.ok_or_else(|| {
				BlockWatcherError::recovery_error(
					"Recovery config is required but not found".to_string(),
					None,
					Some(HashMap::from([(
						"network".to_string(),
						self.network.slug.clone(),
					)])),
				)
			})?
			.clone();

		let network = self.network.clone();
		let block_storage = self.block_storage.clone();
		let block_handler = self.block_handler.clone();
		let trigger_handler = self.trigger_handler.clone();
		let block_tracker = self.block_tracker.clone();

		let cron_schedule = recovery_config.cron_schedule.clone();
		let job = Job::new_async(cron_schedule.as_str(), move |_uuid, _l| {
			let network = network.clone();
			let recovery_config = recovery_config.clone();
			let block_storage = block_storage.clone();
			let block_handler = block_handler.clone();
			let block_tracker = block_tracker.clone();
			let rpc_client = rpc_client.clone();
			let trigger_handler = trigger_handler.clone();
			Box::pin(async move {
				let _ = process_missed_blocks(
					&network,
					&recovery_config,
					&rpc_client,
					block_storage,
					block_handler,
					trigger_handler,
					block_tracker,
				)
				.await
				.map_err(|e| {
					BlockWatcherError::recovery_error(
						"Failed to process missed blocks".to_string(),
						Some(e.into()),
						Some(HashMap::from([(
							"network".to_string(),
							network.slug.clone(),
						)])),
					)
				});
			})
		})
		.with_context(|| "Failed to create recovery job")?;

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

		tracing::info!(
			"Started recovery job for network: {} with schedule: {}",
			self.network.slug,
			self.network
				.recovery_config
				.as_ref()
				.map(|c| c.cron_schedule.as_str())
				.unwrap_or("unknown")
		);
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
		block_tracker: Arc<BlockTracker>,
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
	/// * `rpc_client` - RPC client for the network
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
	TR: BlockTrackerTrait + Send + Sync + 'static,
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

	// Reset expected_next to start_block to ensure synchronization with this execution
	// This prevents false out-of-order warnings when reprocessing blocks or restarting
	block_tracker
		.reset_expected_next(network, start_block)
		.await;

	// Detect missing blocks using BlockTracker
	let missed_blocks = block_tracker.detect_missing_blocks(network, &blocks).await;

	// Log and save missed blocks if any
	if !missed_blocks.is_empty() {
		tracing::error!(
			network = %network.slug,
			count = missed_blocks.len(),
			"Missed {} blocks: {:?}",
			missed_blocks.len(),
			missed_blocks
		);

		// Save missed blocks in batch (enabled if store_blocks OR recovery is enabled)
		let recovery_enabled = network.recovery_config.as_ref().is_some_and(|c| c.enabled);
		if network.store_blocks.unwrap_or(false) || recovery_enabled {
			block_storage
				.save_missed_blocks(&network.slug, &missed_blocks)
				.await
				.with_context(|| format!("Failed to save {} missed blocks", missed_blocks.len()))?;
		}
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
		let network = network.clone();
		let trigger_handler = trigger_handler.clone();
		let block_tracker = block_tracker.clone();

		async move {
			let mut trigger_rx = trigger_rx;
			let mut pending_blocks = BTreeMap::new();
			let mut next_block_number = Some(start_block);
			let block_tracker = block_tracker.clone();

			// Process all incoming blocks
			while let Some(processed_block) = trigger_rx.next().await {
				let block_number = processed_block.block_number;

				// Buffer the block - we'll check and execute in order
				pending_blocks.insert(block_number, processed_block);

				// Process blocks in order as long as we have the next expected block
				while let Some(expected) = next_block_number {
					if let Some(block) = pending_blocks.remove(&expected) {
						// Check for duplicate or out-of-order blocks when actually executing
						// This ensures we're checking the execution order, not arrival order
						match block_tracker
							.check_processed_block(&network, expected)
							.await
						{
							BlockCheckResult::Ok => {
								// Block is valid, execute it
							}
							BlockCheckResult::Duplicate { last_seen } => {
								tracing::error!(
									network = %network.slug,
									block_number = expected,
									last_seen = last_seen,
									"Duplicate block detected: received block {} again (last seen: {})",
									expected,
									last_seen
								);
							}
							BlockCheckResult::OutOfOrder {
								expected: exp,
								received,
							} => {
								tracing::warn!(
									network = %network.slug,
									block_number = received,
									expected = exp,
									"Out of order block detected: received {} but expected {}",
									received,
									exp
								);
							}
						}

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
					// Check for duplicate or out-of-order blocks when executing
					match block_tracker
						.check_processed_block(&network, min_block)
						.await
					{
						BlockCheckResult::Ok => {
							// Block is valid, execute it
						}
						BlockCheckResult::Duplicate { last_seen } => {
							tracing::error!(
								network = %network.slug,
								block_number = min_block,
								last_seen = last_seen,
								"Duplicate block detected: received block {} again (last seen: {})",
								min_block,
								last_seen
							);
						}
						BlockCheckResult::OutOfOrder {
							expected: exp,
							received,
						} => {
							tracing::warn!(
								network = %network.slug,
								block_number = received,
								expected = exp,
								"Out of order block detected: received {} but expected {}",
								received,
								exp
							);
						}
					}

					(trigger_handler)(&block);
				}
			}
			Ok::<(), BlockWatcherError>(())
		}
	});

	// Feed blocks into the pipeline
	futures::future::join_all(blocks.iter().map(|block| {
		let mut process_tx = process_tx.clone();
		async move {
			let block_number = block.number().unwrap_or(0);

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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::BlockRecoveryConfig;
	use crate::services::blockwatcher::storage::FileBlockStorage;
	use crate::utils::tests::network::NetworkBuilder;
	use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
	use tempfile::tempdir;

	/// Helper to create an EVM block with a specific block number
	fn create_evm_block(block_number: u64) -> BlockType {
		use crate::models::EVMBlock;
		use alloy::rpc::types::{Block, Header};

		let alloy_block = Block {
			header: Header {
				inner: alloy::consensus::Header {
					number: block_number,
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		};
		let evm_block: EVMBlock = alloy_block.into();
		BlockType::EVM(Box::new(evm_block))
	}

	fn create_test_network() -> Network {
		NetworkBuilder::new()
			.name("Test Network")
			.slug("test_network")
			.store_blocks(true)
			.confirmation_blocks(12)
			.max_past_blocks(100)
			.build()
	}

	fn create_test_network_with_recovery() -> Network {
		NetworkBuilder::new()
			.name("Test Network")
			.slug("test_network")
			.store_blocks(true)
			.confirmation_blocks(12)
			.max_past_blocks(100)
			.recovery_config(BlockRecoveryConfig {
				enabled: true,
				cron_schedule: "0 */5 * * * *".to_string(),
				max_blocks_per_run: 10,
				max_block_age: 1000,
				max_retries: 3,
				retry_delay_ms: 100,
			})
			.build()
	}

	/// Mock RPC client for testing
	#[derive(Clone)]
	struct MockRpcClient {
		latest_block: Arc<AtomicU64>,
		blocks_to_return: Arc<std::sync::Mutex<Vec<BlockType>>>,
		fail_get_blocks: Arc<AtomicBool>,
		call_count: Arc<AtomicUsize>,
	}

	impl MockRpcClient {
		fn new(latest_block: u64) -> Self {
			Self {
				latest_block: Arc::new(AtomicU64::new(latest_block)),
				blocks_to_return: Arc::new(std::sync::Mutex::new(Vec::new())),
				fail_get_blocks: Arc::new(AtomicBool::new(false)),
				call_count: Arc::new(AtomicUsize::new(0)),
			}
		}

		fn with_blocks(self, blocks: Vec<BlockType>) -> Self {
			*self.blocks_to_return.lock().unwrap() = blocks;
			self
		}

		#[allow(dead_code)]
		fn with_failing_get_blocks(self) -> Self {
			self.fail_get_blocks.store(true, Ordering::SeqCst);
			self
		}
	}

	#[async_trait::async_trait]
	impl BlockChainClient for MockRpcClient {
		async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
			Ok(self.latest_block.load(Ordering::SeqCst))
		}

		async fn get_blocks(
			&self,
			start: u64,
			end: Option<u64>,
		) -> Result<Vec<BlockType>, anyhow::Error> {
			self.call_count.fetch_add(1, Ordering::SeqCst);

			if self.fail_get_blocks.load(Ordering::SeqCst) {
				return Err(anyhow::anyhow!("Simulated RPC failure"));
			}

			let blocks = self.blocks_to_return.lock().unwrap();
			if !blocks.is_empty() {
				return Ok(blocks.clone());
			}

			// Generate mock blocks for the requested range
			let end_block = end.unwrap_or(start);
			let mut result = Vec::new();
			for block_num in start..=end_block {
				result.push(create_evm_block(block_num));
			}
			Ok(result)
		}
	}

	fn create_block_handler() -> Arc<
		impl Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync + 'static,
	> {
		Arc::new(|block: BlockType, network: Network| {
			Box::pin(async move {
				ProcessedBlock {
					network_slug: network.slug,
					block_number: block.number().unwrap_or(0),
					processing_results: vec![],
				}
			}) as BoxFuture<'static, ProcessedBlock>
		})
	}

	fn create_trigger_handler(
	) -> Arc<impl Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync + 'static> {
		Arc::new(|_block: &ProcessedBlock| tokio::spawn(async move {}))
	}

	fn create_counting_trigger_handler(
		counter: Arc<AtomicUsize>,
	) -> Arc<impl Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync + 'static> {
		Arc::new(move |_block: &ProcessedBlock| {
			let counter = counter.clone();
			tokio::spawn(async move {
				counter.fetch_add(1, Ordering::SeqCst);
			})
		})
	}

	// ============ process_new_blocks tests ============

	#[tokio::test]
	async fn test_process_new_blocks_first_run() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let network = create_test_network();
		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_ok());

		// First run should save latest_confirmed_block (100 - 12 = 88)
		let last_processed = storage
			.get_last_processed_block("test_network")
			.await
			.unwrap();
		assert_eq!(last_processed, Some(88));
	}

	#[tokio::test]
	async fn test_process_new_blocks_subsequent_run() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Simulate a previous run
		storage
			.save_last_processed_block("test_network", 80)
			.await
			.unwrap();

		let network = create_test_network();
		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_ok());

		// Should process blocks from 81 to 88
		let last_processed = storage
			.get_last_processed_block("test_network")
			.await
			.unwrap();
		assert_eq!(last_processed, Some(88));
	}

	#[tokio::test]
	async fn test_process_new_blocks_with_store_blocks_enabled() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		let mut network = create_test_network();
		network.store_blocks = Some(true);

		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_ok());

		// Check that blocks were saved (file should exist)
		let pattern = format!("{}/test_network_blocks_*.json", temp_dir.path().display());
		let files: Vec<_> = glob::glob(&pattern).unwrap().collect();
		assert!(
			!files.is_empty(),
			"Block files should be created when store_blocks is enabled"
		);
	}

	#[tokio::test]
	async fn test_process_new_blocks_with_store_blocks_disabled() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		let mut network = create_test_network();
		network.store_blocks = Some(false);

		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_ok());

		// Check that no block files were created
		let pattern = format!("{}/test_network_blocks_*.json", temp_dir.path().display());
		let files: Vec<_> = glob::glob(&pattern).unwrap().collect();
		assert!(
			files.is_empty(),
			"Block files should not be created when store_blocks is disabled"
		);
	}

	#[tokio::test]
	async fn test_process_new_blocks_skips_old_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Last processed was very long ago
		storage
			.save_last_processed_block("test_network", 10)
			.await
			.unwrap();

		let mut network = create_test_network();
		network.max_past_blocks = Some(50); // Only process last 50 blocks

		let rpc_client = MockRpcClient::new(1000);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_ok());

		// Should skip to recent blocks (current 1000 - confirmations 12 - max_past 50 = 938)
		let last_processed = storage
			.get_last_processed_block("test_network")
			.await
			.unwrap();
		assert_eq!(last_processed, Some(988)); // 1000 - 12 confirmations
	}

	#[tokio::test]
	async fn test_process_new_blocks_detects_missed_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let network = create_test_network_with_recovery();

		// Create blocks with a gap (missing block 97)
		let rpc_client =
			MockRpcClient::new(110).with_blocks(vec![create_evm_block(96), create_evm_block(98)]);

		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let _ = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		// Verify missed block was saved
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();

		// Block 97 should be recorded as missed
		let missed_numbers: Vec<u64> = missed.iter().map(|e| e.block_number).collect();
		assert!(missed_numbers.contains(&97));
	}

	#[tokio::test]
	async fn test_process_new_blocks_no_new_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Already processed up to current confirmed block
		storage
			.save_last_processed_block("test_network", 88)
			.await
			.unwrap();

		let network = create_test_network();
		let rpc_client = MockRpcClient::new(100); // 100 - 12 = 88 confirmed
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_process_new_blocks_triggers_handlers() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 85)
			.await
			.unwrap();

		let network = create_test_network();
		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();

		let trigger_count = Arc::new(AtomicUsize::new(0));
		let trigger_handler = create_counting_trigger_handler(trigger_count.clone());

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_ok());

		// Wait a bit for async triggers to complete
		tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

		// Should have triggered for blocks 86, 87, 88 (3 blocks)
		assert_eq!(trigger_count.load(Ordering::SeqCst), 3);
	}

	#[tokio::test]
	async fn test_process_new_blocks_handles_duplicate_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let network = create_test_network();

		// Create blocks with duplicates
		let rpc_client = MockRpcClient::new(110).with_blocks(vec![
			create_evm_block(96),
			create_evm_block(97),
			create_evm_block(96), // Duplicate
		]);

		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		// Should handle duplicates gracefully (BlockTracker detects them)
		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_process_new_blocks_handles_out_of_order_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let network = create_test_network();

		// Create blocks in reverse order
		let rpc_client = MockRpcClient::new(110).with_blocks(vec![
			create_evm_block(98),
			create_evm_block(97),
			create_evm_block(96),
		]);

		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		// Should handle out-of-order blocks (they get reordered in the trigger pipeline)
		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_process_new_blocks_saves_missed_blocks_when_recovery_enabled() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		// Network with store_blocks=false but recovery enabled
		let mut network = create_test_network_with_recovery();
		network.store_blocks = Some(false);

		// Create blocks with a gap
		let rpc_client =
			MockRpcClient::new(110).with_blocks(vec![create_evm_block(96), create_evm_block(98)]);

		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let _ = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		// Missed blocks should be saved even when store_blocks is false (because recovery is enabled)
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		assert!(!missed.is_empty());
	}

	// ============ BlockWatcherService tests ============

	/// Mock job scheduler for testing
	#[derive(Clone)]
	struct MockJobScheduler {
		started: Arc<AtomicBool>,
		shutdown_called: Arc<AtomicBool>,
		jobs_added: Arc<AtomicUsize>,
		fail_new: bool,
		fail_add: bool,
		fail_start: bool,
		fail_shutdown: bool,
	}

	impl MockJobScheduler {
		fn new() -> Self {
			Self {
				started: Arc::new(AtomicBool::new(false)),
				shutdown_called: Arc::new(AtomicBool::new(false)),
				jobs_added: Arc::new(AtomicUsize::new(0)),
				fail_new: false,
				fail_add: false,
				fail_start: false,
				fail_shutdown: false,
			}
		}

		#[allow(dead_code)]
		fn with_failing_new() -> Self {
			Self {
				fail_new: true,
				..Self::new()
			}
		}

		#[allow(dead_code)]
		fn with_failing_add() -> Self {
			Self {
				fail_add: true,
				..Self::new()
			}
		}

		#[allow(dead_code)]
		fn with_failing_start() -> Self {
			Self {
				fail_start: true,
				..Self::new()
			}
		}

		#[allow(dead_code)]
		fn with_failing_shutdown() -> Self {
			Self {
				fail_shutdown: true,
				..Self::new()
			}
		}
	}

	#[async_trait::async_trait]
	impl JobSchedulerTrait for MockJobScheduler {
		async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
			let scheduler = MockJobScheduler::new();
			if scheduler.fail_new {
				return Err("Simulated scheduler creation failure".into());
			}
			Ok(scheduler)
		}

		async fn add(&self, _job: Job) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
			if self.fail_add {
				return Err("Simulated job add failure".into());
			}
			self.jobs_added.fetch_add(1, Ordering::SeqCst);
			Ok(())
		}

		async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
			if self.fail_start {
				return Err("Simulated scheduler start failure".into());
			}
			self.started.store(true, Ordering::SeqCst);
			Ok(())
		}

		async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
			if self.fail_shutdown {
				return Err("Simulated scheduler shutdown failure".into());
			}
			self.shutdown_called.store(true, Ordering::SeqCst);
			Ok(())
		}
	}

	#[tokio::test]
	async fn test_block_watcher_service_new() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let service: Result<BlockWatcherService<_, _, _, MockJobScheduler>, _> =
			BlockWatcherService::new(storage, block_handler, trigger_handler, block_tracker).await;

		assert!(service.is_ok());
	}

	#[tokio::test]
	async fn test_network_block_watcher_new() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let network = create_test_network();
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let watcher: Result<NetworkBlockWatcher<_, _, _, MockJobScheduler>, _> =
			NetworkBlockWatcher::new(
				network,
				storage,
				block_handler,
				trigger_handler,
				block_tracker,
			)
			.await;

		assert!(watcher.is_ok());
	}

	#[tokio::test]
	async fn test_start_network_watcher_already_running() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let network = create_test_network();
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let service: BlockWatcherService<_, _, _, MockJobScheduler> =
			BlockWatcherService::new(storage, block_handler, trigger_handler, block_tracker)
				.await
				.unwrap();

		let rpc_client = MockRpcClient::new(100);

		// Start watcher first time
		let result = service
			.start_network_watcher(&network, rpc_client.clone())
			.await;
		assert!(result.is_ok());

		// Start watcher second time - should return early without error
		let result = service.start_network_watcher(&network, rpc_client).await;
		assert!(result.is_ok());

		// Verify only one watcher exists
		let watchers = service.active_watchers.read().await;
		assert_eq!(watchers.len(), 1);
	}

	#[tokio::test]
	async fn test_stop_network_watcher() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let network = create_test_network();
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let service: BlockWatcherService<_, _, _, MockJobScheduler> =
			BlockWatcherService::new(storage, block_handler, trigger_handler, block_tracker)
				.await
				.unwrap();

		let rpc_client = MockRpcClient::new(100);

		// Start watcher
		service
			.start_network_watcher(&network, rpc_client)
			.await
			.unwrap();

		// Verify watcher is running
		{
			let watchers = service.active_watchers.read().await;
			assert_eq!(watchers.len(), 1);
		}

		// Stop watcher
		let result = service.stop_network_watcher("test_network").await;
		assert!(result.is_ok());

		// Verify watcher was removed
		let watchers = service.active_watchers.read().await;
		assert_eq!(watchers.len(), 0);
	}

	#[tokio::test]
	async fn test_stop_network_watcher_not_running() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let service: BlockWatcherService<_, _, _, MockJobScheduler> =
			BlockWatcherService::new(storage, block_handler, trigger_handler, block_tracker)
				.await
				.unwrap();

		// Stop watcher that doesn't exist - should not error
		let result = service.stop_network_watcher("nonexistent").await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_network_watcher_start_with_recovery() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let network = create_test_network_with_recovery();
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let mut watcher: NetworkBlockWatcher<_, _, _, MockJobScheduler> = NetworkBlockWatcher::new(
			network,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		let rpc_client = MockRpcClient::new(100);
		let result = watcher.start(rpc_client).await;

		assert!(result.is_ok());
		// With recovery enabled, 2 jobs should be added (main watcher + recovery)
		assert_eq!(watcher.scheduler.jobs_added.load(Ordering::SeqCst), 2);
	}

	#[tokio::test]
	async fn test_network_watcher_start_without_recovery() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let network = create_test_network(); // No recovery config
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let mut watcher: NetworkBlockWatcher<_, _, _, MockJobScheduler> = NetworkBlockWatcher::new(
			network,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		let rpc_client = MockRpcClient::new(100);
		let result = watcher.start(rpc_client).await;

		assert!(result.is_ok());
		// Without recovery, only 1 job should be added (main watcher)
		assert_eq!(watcher.scheduler.jobs_added.load(Ordering::SeqCst), 1);
	}

	#[tokio::test]
	async fn test_network_watcher_stop() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let network = create_test_network();
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let mut watcher: NetworkBlockWatcher<_, _, _, MockJobScheduler> = NetworkBlockWatcher::new(
			network,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		let rpc_client = MockRpcClient::new(100);
		watcher.start(rpc_client).await.unwrap();

		let result = watcher.stop().await;
		assert!(result.is_ok());
		assert!(watcher.scheduler.shutdown_called.load(Ordering::SeqCst));
	}
}
