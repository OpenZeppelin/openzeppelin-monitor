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
	models::{BlockFilterStatus, BlockType, MaxPastBlocks, Network, ProcessedBlock},
	services::{
		blockchain::{BlockChainClient, BlockFetchResult, FetchStreamKind},
		blockwatcher::{
			error::BlockWatcherError,
			recovery::process_missed_blocks,
			storage::BlockStorage,
			tracker::{BlockCheckResult, BlockTracker, BlockTrackerTrait},
		},
	},
	utils::metrics::BLOCK_CHECKPOINT_LAG,
};

/// Number of blocks fetched and processed per batch while catching up.
///
/// The checkpoint is saved after each batch, so a failure mid catch-up only
/// retries a single batch on the next tick instead of the whole backlog.
const CATCHUP_BATCH_SIZE: u64 = 30;

/// Maximum number of unlimited catch-up batches processed by a single tick.
///
/// This bounds tick duration so the per-network run_lock is released and
/// recovery can interleave. The next tick resumes from the per-batch checkpoint.
const MAX_BATCHES_PER_TICK: u64 = 100;

/// Maximum number of times in a row the checkpoint is held before a block whose filter
/// fails, when recovery is disabled.
///
/// Holding lets the next tick retry the block. A block that fails every time (for example a
/// ledger the RPC no longer serves) would otherwise stop the network for good, so after this
/// many holds the watcher logs the block, saves it as missed when `store_blocks` is on, and
/// moves past it.
const MAX_FILTER_FAILURE_HOLDS: u32 = 3;

struct BatchProcessSummary {
	block_count: usize,
	stream_kind: FetchStreamKind,
	/// Checkpoint saved for the batch. Lower than the batch end when the checkpoint is held
	/// before a block whose filter did not finish.
	checkpoint_block: u64,
}

/// What the trigger stage does with the rest of the batch after a block.
enum DispatchOutcome {
	/// Go on with the next block.
	Continue,
	/// Stop dispatching: the checkpoint is held before this block, so the next tick processes
	/// it and every later block again. Triggering those later blocks now would fire them twice.
	Hold,
}

/// Result of the trigger stage of a batch.
#[derive(Default)]
struct TriggerStageOutcome {
	/// Block the checkpoint is held before, if any.
	held_at: Option<u64>,
	/// Blocks whose filter failed and that the batch moves past, to be saved as missed.
	failed_blocks: Vec<u64>,
}

struct BatchProcessingContext<'a, S, H, T, TR> {
	network: &'a Network,
	block_storage: &'a Arc<S>,
	block_handler: &'a Arc<H>,
	trigger_handler: &'a Arc<T>,
	block_tracker: &'a Arc<TR>,
	latest_confirmed_block: u64,
}

struct BatchProcessingOptions {
	batch_start: u64,
	checkpoint_block: u64,
	delete_blocks_before_save: bool,
}

async fn save_checkpoint<S: BlockStorage>(
	block_storage: &Arc<S>,
	network: &Network,
	checkpoint_block: u64,
	latest_confirmed_block: u64,
) -> Result<(), BlockWatcherError> {
	block_storage
		.save_last_processed_block(&network.slug, checkpoint_block)
		.await
		.with_context(|| "Failed to save last processed block")?;

	BLOCK_CHECKPOINT_LAG
		.with_label_values(&[network.slug.as_str()])
		.set(latest_confirmed_block.saturating_sub(checkpoint_block) as f64);

	Ok(())
}

fn log_checkpoint_lag(network: &Network, checkpoint_block: u64, latest_confirmed_block: u64) {
	let lag = latest_confirmed_block.saturating_sub(checkpoint_block);
	tracing::info!(
		network = %network.slug,
		checkpoint_block,
		latest_confirmed_block,
		lag,
		"Block checkpoint lag is {} blocks",
		lag
	);
}

fn recovery_enabled(network: &Network) -> bool {
	network.recovery_config.as_ref().is_some_and(|c| c.enabled)
}

/// Saves missed blocks when `store_blocks` or recovery is enabled.
async fn save_missed_blocks_if_enabled<S: BlockStorage>(
	block_storage: &Arc<S>,
	network: &Network,
	missed_blocks: &[u64],
) -> Result<(), BlockWatcherError> {
	if missed_blocks.is_empty() {
		return Ok(());
	}
	if network.store_blocks.unwrap_or(false) || recovery_enabled(network) {
		block_storage
			.save_missed_blocks(&network.slug, missed_blocks)
			.await
			.with_context(|| format!("Failed to save {} missed blocks", missed_blocks.len()))?;
	}
	Ok(())
}

/// Logs a duplicate or out-of-order block. The trigger stage calls this in execution order.
async fn check_block_sequence<TR: BlockTrackerTrait>(
	block_tracker: &TR,
	network: &Network,
	block_number: u64,
) {
	match block_tracker
		.check_processed_block(network, block_number)
		.await
	{
		BlockCheckResult::Ok => {}
		BlockCheckResult::Duplicate { last_seen } => {
			tracing::error!(
				network = %network.slug,
				block_number,
				last_seen,
				"Duplicate block detected: received block {} again (last seen: {})",
				block_number,
				last_seen
			);
		}
		BlockCheckResult::OutOfOrder { expected, received } => {
			tracing::warn!(
				network = %network.slug,
				block_number = received,
				expected,
				"Out of order block detected: received {} but expected {}",
				received,
				expected
			);
		}
	}
}

/// Hands a processed block to the trigger handler, or decides what to do with a block whose
/// filter did not finish.
///
/// A block whose filter failed is retried by recovery when it is enabled, so the batch moves
/// past it and it is saved as missed. Without recovery the checkpoint is held before it, up to
/// [`MAX_FILTER_FAILURE_HOLDS`] times in a row, after which the block is skipped. A block cut
/// short by shutdown always holds the checkpoint and is not counted against that limit.
async fn dispatch_processed_block<T, TR>(
	network: &Network,
	block_tracker: &TR,
	trigger_handler: &T,
	block: &ProcessedBlock,
	outcome: &mut TriggerStageOutcome,
) -> DispatchOutcome
where
	T: Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()>,
	TR: BlockTrackerTrait,
{
	let block_number = block.block_number;
	match block.filter_status {
		BlockFilterStatus::Completed => {}
		BlockFilterStatus::Cancelled => {
			tracing::info!(
				network = %network.slug,
				block_number,
				"Filtering block {} was cancelled by shutdown; it will be processed again on the next run",
				block_number
			);
			return DispatchOutcome::Hold;
		}
		BlockFilterStatus::Failed if recovery_enabled(network) => {
			tracing::warn!(
				network = %network.slug,
				block_number,
				"Filtering block {} failed; saving it as missed for recovery",
				block_number
			);
			outcome.failed_blocks.push(block_number);
		}
		BlockFilterStatus::Failed => {
			let failures = block_tracker
				.record_filter_failure(network, block_number)
				.await;
			if failures <= MAX_FILTER_FAILURE_HOLDS {
				tracing::warn!(
					network = %network.slug,
					block_number,
					failures,
					max_holds = MAX_FILTER_FAILURE_HOLDS,
					"Filtering block {} failed ({} of {} holds); it will be processed again on the next run",
					block_number,
					failures,
					MAX_FILTER_FAILURE_HOLDS
				);
				return DispatchOutcome::Hold;
			}
			tracing::error!(
				network = %network.slug,
				block_number,
				failures,
				"Filtering block {} failed {} times in a row; skipping it, so its matches are lost",
				block_number,
				failures
			);
			outcome.failed_blocks.push(block_number);
		}
	}

	check_block_sequence(block_tracker, network, block_number).await;
	if block.filter_status == BlockFilterStatus::Completed {
		(trigger_handler)(block);
	}
	DispatchOutcome::Continue
}

async fn process_block_batch<
	S: BlockStorage,
	H: Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync + 'static,
	T: Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync + 'static,
	TR: BlockTrackerTrait + Send + Sync + 'static,
>(
	context: &BatchProcessingContext<'_, S, H, T, TR>,
	fetch_result: BlockFetchResult,
	options: BatchProcessingOptions,
) -> Result<BatchProcessSummary, BlockWatcherError> {
	let network = context.network;
	let block_storage = context.block_storage;
	let block_tracker = context.block_tracker;
	let batch_start = options.batch_start;
	let checkpoint_block = options.checkpoint_block;
	let delete_blocks_before_save = options.delete_blocks_before_save;
	let stream_kind = fetch_result.stream_kind;
	let blocks = fetch_result.blocks;

	// Reset expected_next to the batch start so long catch-ups can checkpoint
	// independently without false out-of-order warnings between batches.
	block_tracker
		.reset_expected_next(network, batch_start)
		.await;

	// Detect missing blocks based on the fetch stream kind.
	// Dense streams (sequential block retrieval) use gap detection.
	// Sparse streams (address-filtered) rely on explicitly reported failed blocks.
	let missed_blocks = match &stream_kind {
		FetchStreamKind::Dense => block_tracker.detect_missing_blocks(network, &blocks).await,
		FetchStreamKind::Sparse => fetch_result.failed_blocks,
	};

	// Log and save missed blocks if any
	if !missed_blocks.is_empty() {
		tracing::error!(
			network = %network.slug,
			count = missed_blocks.len(),
			"Missed {} blocks: {:?}",
			missed_blocks.len(),
			missed_blocks
		);

		save_missed_blocks_if_enabled(block_storage, network, &missed_blocks).await?;
	}

	// Create channels for our pipeline
	let channel_size = (blocks.len() * 2).max(1);
	let (process_tx, process_rx) = mpsc::channel::<(BlockType, u64)>(channel_size);
	let (trigger_tx, trigger_rx) = mpsc::channel::<ProcessedBlock>(channel_size);

	// Stage 1: Block Processing Pipeline
	let process_handle = tokio::spawn({
		let network = network.clone();
		let block_handler = Arc::clone(context.block_handler);
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
		let trigger_handler = Arc::clone(context.trigger_handler);
		let block_tracker = Arc::clone(context.block_tracker);

		async move {
			let mut trigger_rx = trigger_rx;
			let mut pending_blocks = BTreeMap::new();
			let mut next_block_number = batch_start;
			let mut outcome = TriggerStageOutcome::default();

			// Process all incoming blocks
			while let Some(processed_block) = trigger_rx.next().await {
				// Once held, keep receiving so the processing stage can finish, but dispatch
				// nothing more: every later block is processed again on the next run
				if outcome.held_at.is_some() {
					continue;
				}

				// Buffer the block - we'll check and execute in order
				pending_blocks.insert(processed_block.block_number, processed_block);

				// Dispatch blocks in order as long as we have the next expected block, so
				// duplicates and out-of-order blocks are checked in execution order
				while let Some(block) = pending_blocks.remove(&next_block_number) {
					match dispatch_processed_block(
						&network,
						&*block_tracker,
						&*trigger_handler,
						&block,
						&mut outcome,
					)
					.await
					{
						DispatchOutcome::Continue => next_block_number += 1,
						DispatchOutcome::Hold => {
							outcome.held_at = Some(block.block_number);
							break;
						}
					}
				}
			}

			// Dispatch any remaining blocks in order after the channel is closed
			if outcome.held_at.is_none() {
				while let Some((_, block)) = pending_blocks.pop_first() {
					if let DispatchOutcome::Hold = dispatch_processed_block(
						&network,
						&*block_tracker,
						&*trigger_handler,
						&block,
						&mut outcome,
					)
					.await
					{
						outcome.held_at = Some(block.block_number);
						break;
					}
				}
			}
			Ok::<TriggerStageOutcome, BlockWatcherError>(outcome)
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
	let (process_result, trigger_result) = tokio::join!(process_handle, trigger_handle);
	process_result.map_err(|e| anyhow::anyhow!("Block processing task failed: {}", e))??;
	let trigger_outcome =
		trigger_result.map_err(|e| anyhow::anyhow!("Trigger processing task failed: {}", e))??;

	save_missed_blocks_if_enabled(block_storage, network, &trigger_outcome.failed_blocks).await?;

	// Hold the checkpoint before a block whose filter did not finish, so the next run starts
	// there instead of counting the block as processed with no matches. It never goes below
	// the batch start: a held block numbered outside the batch (an unknown number reads as 0)
	// would otherwise save 0, which reads as a first run and jumps to the chain head
	let checkpoint_block = match trigger_outcome.held_at {
		Some(held_at) => held_at
			.saturating_sub(1)
			.max(batch_start.saturating_sub(1))
			.min(checkpoint_block),
		None => checkpoint_block,
	};

	if network.store_blocks.unwrap_or(false) {
		if delete_blocks_before_save {
			block_storage
				.delete_blocks(&network.slug)
				.await
				.with_context(|| "Failed to delete old blocks")?;
		}

		block_storage
			.save_blocks(&network.slug, &blocks)
			.await
			.with_context(|| "Failed to save blocks")?;
	}

	save_checkpoint(
		block_storage,
		network,
		checkpoint_block,
		context.latest_confirmed_block,
	)
	.await?;

	Ok(BatchProcessSummary {
		block_count: blocks.len(),
		stream_kind,
		checkpoint_block,
	})
}

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
	pub run_lock: Arc<tokio::sync::Mutex<()>>,
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
			run_lock: Arc::new(tokio::sync::Mutex::new(())),
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
		let run_lock = self.run_lock.clone();

		let job = Job::new_async(self.network.cron_schedule.as_str(), move |_uuid, _l| {
			let network = network.clone();
			let block_storage = block_storage.clone();
			let block_handler = block_handler.clone();
			let block_tracker = block_tracker.clone();
			let rpc_client = rpc_client.clone();
			let trigger_handler = trigger_handler.clone();
			let run_lock = run_lock.clone();
			Box::pin(async move {
				let _guard = run_lock.lock().await;
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
		let run_lock = self.run_lock.clone();

		let cron_schedule = recovery_config.cron_schedule.clone();
		let job = Job::new_async(cron_schedule.as_str(), move |_uuid, _l| {
			let network = network.clone();
			let recovery_config = recovery_config.clone();
			let block_storage = block_storage.clone();
			let block_handler = block_handler.clone();
			let block_tracker = block_tracker.clone();
			let rpc_client = rpc_client.clone();
			let trigger_handler = trigger_handler.clone();
			let run_lock = run_lock.clone();
			Box::pin(async move {
				let guard = run_lock.try_lock();
				if guard.is_err() {
					tracing::debug!(
						network = %network.slug,
						"Skipping recovery run: main watcher is currently processing"
					);
					return;
				}
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

	let max_past_blocks = network
		.max_past_blocks
		.unwrap_or(MaxPastBlocks::Limited(recommended_past_blocks));

	// Calculate the start block number, using the default if max_past_blocks is not set.
	// Unlimited networks never clamp: they always resume from the last processed block.
	let start_block = if last_processed_block == 0 {
		latest_confirmed_block
	} else {
		match max_past_blocks {
			MaxPastBlocks::Limited(max) => std::cmp::max(
				last_processed_block.saturating_add(1),
				latest_confirmed_block.saturating_sub(max),
			),
			MaxPastBlocks::Unlimited => last_processed_block.saturating_add(1),
		}
	};
	let skipped_blocks = match max_past_blocks {
		MaxPastBlocks::Limited(_) if last_processed_block != 0 => {
			let next_block = last_processed_block.saturating_add(1);
			(start_block > next_block).then_some(start_block - next_block)
		}
		_ => None,
	};

	tracing::info!(
		"Processing blocks:\n\tLast processed block: {}\n\tLatest confirmed block: {}\n\tStart \
		 block: {}{}\n\tConfirmations required: {}\n\tMax past blocks: {}",
		last_processed_block,
		latest_confirmed_block,
		start_block,
		skipped_blocks
			.map(|blocks| format!(" (skipped {} blocks)", blocks))
			.unwrap_or_default(),
		network.confirmation_blocks,
		max_past_blocks
	);

	let mut total_blocks_processed = 0;
	let mut stream_kind = FetchStreamKind::Dense;
	let mut checkpoint_block = last_processed_block;
	let mut blocks_deleted = false;
	let batch_context = BatchProcessingContext {
		network,
		block_storage: &block_storage,
		block_handler: &block_handler,
		trigger_handler: &trigger_handler,
		block_tracker: &block_tracker,
		latest_confirmed_block,
	};

	if last_processed_block == 0 {
		let fetch_result = rpc_client
			.get_blocks_with_meta(latest_confirmed_block, None)
			.await
			.with_context(|| format!("Failed to get block {}", latest_confirmed_block))?;
		let summary = process_block_batch(
			&batch_context,
			fetch_result,
			BatchProcessingOptions {
				batch_start: latest_confirmed_block,
				checkpoint_block: latest_confirmed_block,
				delete_blocks_before_save: true,
			},
		)
		.await?;
		total_blocks_processed += summary.block_count;
		stream_kind = summary.stream_kind;
		checkpoint_block = summary.checkpoint_block;
	} else if last_processed_block < latest_confirmed_block {
		// The per-network run_lock serializes cron ticks while long catch-ups are still running.
		let mut batch_start = start_block;
		let mut batches_processed_this_tick = 0;
		while batch_start <= latest_confirmed_block {
			if matches!(max_past_blocks, MaxPastBlocks::Unlimited)
				&& batches_processed_this_tick >= MAX_BATCHES_PER_TICK
			{
				let remaining_lag = latest_confirmed_block.saturating_sub(checkpoint_block);
				tracing::info!(
					network = %network.slug,
					checkpoint = checkpoint_block,
					latest_confirmed = latest_confirmed_block,
					remaining_lag,
					batches_processed = batches_processed_this_tick,
					"Reached unlimited catch-up batch cap; continuing next tick"
				);
				break;
			}

			let batch_end = std::cmp::min(
				batch_start.saturating_add(CATCHUP_BATCH_SIZE - 1),
				latest_confirmed_block,
			);
			let batch_result = async {
				let fetch_result = rpc_client
					.get_blocks_with_meta(batch_start, Some(batch_end))
					.await
					.with_context(|| {
						format!("Failed to get blocks from {} to {}", batch_start, batch_end)
					})?;

				process_block_batch(
					&batch_context,
					fetch_result,
					BatchProcessingOptions {
						batch_start,
						checkpoint_block: batch_end,
						delete_blocks_before_save: network.store_blocks.unwrap_or(false)
							&& !blocks_deleted,
					},
				)
				.await
			}
			.await;

			match batch_result {
				Ok(summary) => {
					total_blocks_processed += summary.block_count;
					stream_kind = summary.stream_kind;
					checkpoint_block = summary.checkpoint_block;
					blocks_deleted = blocks_deleted || network.store_blocks.unwrap_or(false);
					batches_processed_this_tick += 1;
					// The checkpoint is held inside this batch; a later batch would save a
					// checkpoint past the held block, so leave the rest to the next run
					if checkpoint_block < batch_end {
						tracing::warn!(
							network = %network.slug,
							checkpoint = checkpoint_block,
							batch_end,
							latest_confirmed = latest_confirmed_block,
							"Stopping catch-up at block {} so the next run can process it again",
							checkpoint_block.saturating_add(1)
						);
						break;
					}
				}
				Err(error) => {
					tracing::error!(
						network = %network.slug,
						batch_start,
						batch_end,
						error = %error,
						"Failed to process block batch"
					);
					log_checkpoint_lag(network, checkpoint_block, latest_confirmed_block);
					return Err(error);
				}
			}

			if batch_end == latest_confirmed_block {
				break;
			}
			batch_start = batch_end.saturating_add(1);
		}
	} else {
		let fetch_result = BlockFetchResult {
			blocks: Vec::new(),
			failed_blocks: Vec::new(),
			stream_kind: FetchStreamKind::Dense,
		};
		let summary = process_block_batch(
			&batch_context,
			fetch_result,
			BatchProcessingOptions {
				batch_start: start_block,
				checkpoint_block: latest_confirmed_block,
				delete_blocks_before_save: true,
			},
		)
		.await?;
		total_blocks_processed += summary.block_count;
		stream_kind = summary.stream_kind;
		checkpoint_block = summary.checkpoint_block;
	}

	log_checkpoint_lag(network, checkpoint_block, latest_confirmed_block);

	match stream_kind {
		FetchStreamKind::Sparse => {
			tracing::info!(
				"Processed {} slots with matching transactions (scanned slot range {}-{}) in {}ms for network {:?}",
				total_blocks_processed,
				start_block,
				latest_confirmed_block,
				start_time.elapsed().as_millis(),
				network.slug
			);
		}
		FetchStreamKind::Dense => {
			tracing::info!(
				"Processed {} blocks (range {}-{}) in {}ms for network {:?}",
				total_blocks_processed,
				start_block,
				latest_confirmed_block,
				start_time.elapsed().as_millis(),
				network.slug
			);
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{BlockChainType, BlockFilterStatus, BlockRecoveryConfig};
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
		fail_on_call: Arc<AtomicUsize>,
		call_count: Arc<AtomicUsize>,
	}

	impl MockRpcClient {
		fn new(latest_block: u64) -> Self {
			Self {
				latest_block: Arc::new(AtomicU64::new(latest_block)),
				blocks_to_return: Arc::new(std::sync::Mutex::new(Vec::new())),
				fail_get_blocks: Arc::new(AtomicBool::new(false)),
				fail_on_call: Arc::new(AtomicUsize::new(0)),
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

		fn with_failing_get_blocks_after(self, successful_calls: usize) -> Self {
			self.fail_on_call
				.store(successful_calls + 1, Ordering::SeqCst);
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
			let call_number = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;

			if self.fail_get_blocks.load(Ordering::SeqCst) {
				return Err(anyhow::anyhow!("Simulated RPC failure"));
			}

			let fail_on_call = self.fail_on_call.load(Ordering::SeqCst);
			if fail_on_call != 0 && call_number >= fail_on_call {
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
					filter_status: BlockFilterStatus::Completed,
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
		network.max_past_blocks = Some(MaxPastBlocks::Limited(50)); // Only process last 50 blocks

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
	async fn test_process_new_blocks_unlimited_processes_full_gap() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 10)
			.await
			.unwrap();

		let mut network = create_test_network();
		network.max_past_blocks = Some(MaxPastBlocks::Unlimited);
		network.store_blocks = Some(false);

		let rpc_client = MockRpcClient::new(1000);
		let block_tracker = Arc::new(BlockTracker::new(2000));
		let block_handler = create_block_handler();
		let trigger_count = Arc::new(AtomicUsize::new(0));
		let trigger_handler = {
			let trigger_count = trigger_count.clone();
			Arc::new(move |_block: &ProcessedBlock| {
				trigger_count.fetch_add(1, Ordering::SeqCst);
				tokio::spawn(async move {})
			})
		};

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

		let last_processed = storage
			.get_last_processed_block("test_network")
			.await
			.unwrap();
		assert_eq!(last_processed, Some(988));
		assert_eq!(trigger_count.load(Ordering::SeqCst), 978);
		assert_eq!(rpc_client.call_count.load(Ordering::SeqCst), 33);
	}

	#[tokio::test]
	async fn test_process_new_blocks_unlimited_caps_batches_per_tick_and_resumes() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let last_processed = 10;

		storage
			.save_last_processed_block("test_network", last_processed)
			.await
			.unwrap();

		let mut network = create_test_network();
		network.max_past_blocks = Some(MaxPastBlocks::Unlimited);
		network.store_blocks = Some(false);

		let first_tick_blocks = MAX_BATCHES_PER_TICK * CATCHUP_BATCH_SIZE;
		let remaining_blocks = CATCHUP_BATCH_SIZE + 20;
		let first_tick_checkpoint = last_processed + first_tick_blocks;
		let latest_confirmed_block = first_tick_checkpoint + remaining_blocks;
		let latest_block = latest_confirmed_block + network.confirmation_blocks;

		let rpc_client = MockRpcClient::new(latest_block);
		let block_tracker = Arc::new(BlockTracker::new(4000));
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler.clone(),
			trigger_handler.clone(),
			block_tracker.clone(),
		)
		.await;

		assert!(result.is_ok());
		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(first_tick_checkpoint)
		);
		assert_eq!(
			rpc_client.call_count.load(Ordering::SeqCst),
			usize::try_from(MAX_BATCHES_PER_TICK).unwrap()
		);

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
		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(latest_confirmed_block)
		);
		assert_eq!(
			rpc_client.call_count.load(Ordering::SeqCst),
			usize::try_from(MAX_BATCHES_PER_TICK + remaining_blocks.div_ceil(CATCHUP_BATCH_SIZE))
				.unwrap()
		);
	}

	#[tokio::test]
	async fn test_process_new_blocks_unlimited_failure_keeps_last_successful_batch_checkpoint() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 10)
			.await
			.unwrap();

		let mut network = create_test_network();
		network.max_past_blocks = Some(MaxPastBlocks::Unlimited);
		network.store_blocks = Some(false);

		let rpc_client = MockRpcClient::new(200).with_failing_get_blocks_after(2);
		let block_tracker = Arc::new(BlockTracker::new(2000));
		let block_handler = create_block_handler();
		let trigger_count = Arc::new(AtomicUsize::new(0));
		let trigger_handler = {
			let trigger_count = trigger_count.clone();
			Arc::new(move |_block: &ProcessedBlock| {
				trigger_count.fetch_add(1, Ordering::SeqCst);
				tokio::spawn(async move {})
			})
		};

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		assert!(result.is_err());

		let last_processed = storage
			.get_last_processed_block("test_network")
			.await
			.unwrap();
		assert_eq!(last_processed, Some(70));
		assert_eq!(trigger_count.load(Ordering::SeqCst), 60);
		assert_eq!(rpc_client.call_count.load(Ordering::SeqCst), 3);
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

	// ============ Filter failure tests (#517) ============

	/// Block handler whose filter status follows a script: each call for a block takes the
	/// next status from that block's queue, and `Completed` once the queue is empty.
	fn create_scripted_block_handler(
		script: Vec<(u64, Vec<BlockFilterStatus>)>,
	) -> Arc<
		impl Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync + 'static,
	> {
		let script: Arc<std::sync::Mutex<HashMap<u64, std::collections::VecDeque<_>>>> =
			Arc::new(std::sync::Mutex::new(
				script
					.into_iter()
					.map(|(block, statuses)| (block, statuses.into()))
					.collect(),
			));
		Arc::new(move |block: BlockType, network: Network| {
			let block_number = block.number().unwrap_or(0);
			let filter_status = script
				.lock()
				.unwrap()
				.get_mut(&block_number)
				.and_then(|statuses| statuses.pop_front())
				.unwrap_or(BlockFilterStatus::Completed);
			Box::pin(async move {
				ProcessedBlock {
					network_slug: network.slug,
					block_number,
					processing_results: vec![],
					filter_status,
				}
			}) as BoxFuture<'static, ProcessedBlock>
		})
	}

	/// Trigger handler that records the number of every block it is called for, in call order.
	fn create_recording_trigger_handler(
		triggered: Arc<std::sync::Mutex<Vec<u64>>>,
	) -> Arc<impl Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync + 'static> {
		Arc::new(move |block: &ProcessedBlock| {
			triggered.lock().unwrap().push(block.block_number);
			tokio::spawn(async move {})
		})
	}

	/// Network as in #517: no recovery and no stored blocks.
	fn create_network_without_recovery() -> Network {
		let mut network = create_test_network();
		network.store_blocks = Some(false);
		network
	}

	async fn missed_block_numbers(storage: &FileBlockStorage) -> Vec<u64> {
		storage
			.get_missed_blocks("test_network", 1000, 1000, 10)
			.await
			.unwrap()
			.iter()
			.map(|entry| entry.block_number)
			.collect()
	}

	#[tokio::test]
	async fn test_filter_failure_holds_checkpoint_and_skips_later_triggers() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		storage
			.save_last_processed_block("test_network", 80)
			.await
			.unwrap();

		let network = create_network_without_recovery();
		// Blocks 81-88 (latest 100, 12 confirmations)
		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler =
			create_scripted_block_handler(vec![(84, vec![BlockFilterStatus::Failed])]);
		let triggered = Arc::new(std::sync::Mutex::new(Vec::new()));
		let trigger_handler = create_recording_trigger_handler(triggered.clone());

		let result = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler.clone(),
			trigger_handler.clone(),
			block_tracker.clone(),
		)
		.await;
		assert!(result.is_ok());

		// The checkpoint stops before the failed block, and no block from it on is triggered,
		// so retrying them on the next tick cannot fire a trigger twice
		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(83)
		);
		assert_eq!(*triggered.lock().unwrap(), vec![81, 82, 83]);

		// The next tick retries from the failed block, and this time its filter succeeds
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

		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(88)
		);
		assert_eq!(
			*triggered.lock().unwrap(),
			(81..=88).collect::<Vec<_>>(),
			"each block should be triggered exactly once"
		);
	}

	#[tokio::test]
	async fn test_filter_failure_after_gap_skips_later_triggers_on_drain() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		storage
			.save_last_processed_block("test_network", 80)
			.await
			.unwrap();

		let network = create_network_without_recovery();
		// Range 81-84 with block 82 missing from the response, so blocks 83 and 84 are only
		// dispatched by the drain that runs once the processing channel closes
		let rpc_client = MockRpcClient::new(96).with_blocks(vec![
			create_evm_block(81),
			create_evm_block(83),
			create_evm_block(84),
		]);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler =
			create_scripted_block_handler(vec![(83, vec![BlockFilterStatus::Failed])]);
		let triggered = Arc::new(std::sync::Mutex::new(Vec::new()));
		let trigger_handler = create_recording_trigger_handler(triggered.clone());

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

		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(82)
		);
		assert_eq!(*triggered.lock().unwrap(), vec![81]);
	}

	#[tokio::test]
	async fn test_filter_failure_hold_is_capped() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		storage
			.save_last_processed_block("test_network", 80)
			.await
			.unwrap();

		// store_blocks on, so the block given up on is saved as missed
		let network = create_test_network();
		assert!(network.recovery_config.is_none());
		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let failures = usize::try_from(MAX_FILTER_FAILURE_HOLDS).unwrap() + 1;
		let block_handler =
			create_scripted_block_handler(vec![(84, vec![BlockFilterStatus::Failed; failures])]);
		let triggered = Arc::new(std::sync::Mutex::new(Vec::new()));
		let trigger_handler = create_recording_trigger_handler(triggered.clone());

		for tick in 1..=MAX_FILTER_FAILURE_HOLDS {
			let result = process_new_blocks(
				&network,
				&rpc_client,
				storage.clone(),
				block_handler.clone(),
				trigger_handler.clone(),
				block_tracker.clone(),
			)
			.await;
			assert!(result.is_ok());
			assert_eq!(
				storage
					.get_last_processed_block("test_network")
					.await
					.unwrap(),
				Some(83),
				"tick {tick} should hold the checkpoint before the failed block"
			);
		}
		assert!(missed_block_numbers(&storage).await.is_empty());

		// One more failure than the cap: the watcher gives up on the block and moves on
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

		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(88)
		);
		assert_eq!(
			*triggered.lock().unwrap(),
			vec![81, 82, 83, 85, 86, 87, 88],
			"the skipped block is never triggered and the others are triggered once"
		);
		assert_eq!(missed_block_numbers(&storage).await, vec![84]);
	}

	#[tokio::test]
	async fn test_filter_failure_of_block_without_number_keeps_checkpoint_in_batch() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		storage
			.save_last_processed_block("test_network", 80)
			.await
			.unwrap();

		let network = create_network_without_recovery();
		// A block whose number is unknown is processed as block 0
		let rpc_client = MockRpcClient::new(100).with_blocks(vec![create_evm_block(0)]);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler =
			create_scripted_block_handler(vec![(0, vec![BlockFilterStatus::Failed])]);
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

		// A checkpoint of 0 would read as a first run and jump to the chain head
		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(80)
		);
	}

	#[tokio::test]
	async fn test_filter_failure_with_recovery_saves_missed_block_and_advances() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		storage
			.save_last_processed_block("test_network", 80)
			.await
			.unwrap();

		let mut network = create_test_network_with_recovery();
		network.store_blocks = Some(false);
		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler =
			create_scripted_block_handler(vec![(84, vec![BlockFilterStatus::Failed])]);
		let triggered = Arc::new(std::sync::Mutex::new(Vec::new()));
		let trigger_handler = create_recording_trigger_handler(triggered.clone());

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

		// Recovery retries the block, so the main watcher does not hold the checkpoint
		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(88)
		);
		assert_eq!(missed_block_numbers(&storage).await, vec![84]);
		assert_eq!(*triggered.lock().unwrap(), vec![81, 82, 83, 85, 86, 87, 88]);
	}

	#[tokio::test]
	async fn test_cancelled_filter_holds_checkpoint_even_with_recovery() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		storage
			.save_last_processed_block("test_network", 80)
			.await
			.unwrap();

		let network = create_test_network_with_recovery();
		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler =
			create_scripted_block_handler(vec![(84, vec![BlockFilterStatus::Cancelled])]);
		let triggered = Arc::new(std::sync::Mutex::new(Vec::new()));
		let trigger_handler = create_recording_trigger_handler(triggered.clone());

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

		// A filter cut short by shutdown did not fail, so it is not handed to recovery: the
		// checkpoint stays before it and the restarted watcher processes it first
		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(83)
		);
		assert!(missed_block_numbers(&storage).await.is_empty());
		assert_eq!(*triggered.lock().unwrap(), vec![81, 82, 83]);
	}

	#[tokio::test]
	async fn test_cancelled_filter_does_not_count_toward_hold_cap() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		storage
			.save_last_processed_block("test_network", 80)
			.await
			.unwrap();

		let network = create_network_without_recovery();
		let rpc_client = MockRpcClient::new(100);
		let block_tracker = Arc::new(BlockTracker::new(100));
		let cancellations = usize::try_from(MAX_FILTER_FAILURE_HOLDS).unwrap() + 1;
		let block_handler = create_scripted_block_handler(vec![(
			84,
			vec![BlockFilterStatus::Cancelled; cancellations],
		)]);
		let trigger_handler = create_trigger_handler();

		for tick in 1..=cancellations {
			let result = process_new_blocks(
				&network,
				&rpc_client,
				storage.clone(),
				block_handler.clone(),
				trigger_handler.clone(),
				block_tracker.clone(),
			)
			.await;
			assert!(result.is_ok());
			assert_eq!(
				storage
					.get_last_processed_block("test_network")
					.await
					.unwrap(),
				Some(83),
				"tick {tick} should hold the checkpoint before the cancelled block"
			);
		}
	}

	#[tokio::test]
	async fn test_filter_failure_stops_unlimited_catch_up() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		storage
			.save_last_processed_block("test_network", 10)
			.await
			.unwrap();

		let mut network = create_network_without_recovery();
		network.max_past_blocks = Some(MaxPastBlocks::Unlimited);
		// Blocks 11-188, in batches of CATCHUP_BATCH_SIZE; block 25 is in the first batch
		let rpc_client = MockRpcClient::new(200);
		let block_tracker = Arc::new(BlockTracker::new(2000));
		let block_handler =
			create_scripted_block_handler(vec![(25, vec![BlockFilterStatus::Failed])]);
		let triggered = Arc::new(std::sync::Mutex::new(Vec::new()));
		let trigger_handler = create_recording_trigger_handler(triggered.clone());

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

		// A later batch would move the checkpoint past the failed block, so the tick stops
		assert_eq!(
			storage
				.get_last_processed_block("test_network")
				.await
				.unwrap(),
			Some(24)
		);
		assert_eq!(rpc_client.call_count.load(Ordering::SeqCst), 1);
		assert_eq!(*triggered.lock().unwrap(), (11..=24).collect::<Vec<_>>());
	}

	// ============ Solana missed block detection tests ============

	/// Helper to create a Solana block with a specific slot number
	fn create_solana_block(slot: u64) -> BlockType {
		use crate::models::{SolanaBlock, SolanaConfirmedBlock};

		let confirmed_block = SolanaConfirmedBlock {
			slot,
			blockhash: String::new(),
			previous_blockhash: String::new(),
			parent_slot: slot.saturating_sub(1),
			block_time: None,
			block_height: None,
			transactions: Vec::new(),
		};
		BlockType::Solana(Box::new(SolanaBlock::from(confirmed_block)))
	}

	#[allow(dead_code)]
	fn create_solana_test_network() -> Network {
		NetworkBuilder::new()
			.name("Solana Test")
			.slug("test_network")
			.network_type(BlockChainType::Solana)
			.store_blocks(true)
			.confirmation_blocks(32)
			.max_past_blocks(100)
			.build()
	}

	fn create_solana_test_network_with_recovery() -> Network {
		NetworkBuilder::new()
			.name("Solana Test")
			.slug("test_network")
			.network_type(BlockChainType::Solana)
			.store_blocks(true)
			.confirmation_blocks(32)
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

	/// Mock Solana RPC client that returns BlockFetchResult via get_blocks_with_meta
	#[derive(Clone)]
	struct MockSolanaRpcClient {
		latest_block: Arc<AtomicU64>,
		blocks_to_return: Arc<std::sync::Mutex<Vec<BlockType>>>,
		failed_blocks: Arc<std::sync::Mutex<Vec<u64>>>,
		stream_kind: FetchStreamKind,
	}

	impl MockSolanaRpcClient {
		fn new(latest_block: u64) -> Self {
			Self {
				latest_block: Arc::new(AtomicU64::new(latest_block)),
				blocks_to_return: Arc::new(std::sync::Mutex::new(Vec::new())),
				failed_blocks: Arc::new(std::sync::Mutex::new(Vec::new())),
				stream_kind: FetchStreamKind::Sparse,
			}
		}

		fn with_blocks(self, blocks: Vec<BlockType>) -> Self {
			*self.blocks_to_return.lock().unwrap() = blocks;
			self
		}

		fn with_failed_blocks(self, failed: Vec<u64>) -> Self {
			*self.failed_blocks.lock().unwrap() = failed;
			self
		}

		fn with_stream_kind(mut self, kind: FetchStreamKind) -> Self {
			self.stream_kind = kind;
			self
		}
	}

	#[async_trait::async_trait]
	impl BlockChainClient for MockSolanaRpcClient {
		async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
			Ok(self.latest_block.load(Ordering::SeqCst))
		}

		async fn get_blocks(
			&self,
			_start: u64,
			_end: Option<u64>,
		) -> Result<Vec<BlockType>, anyhow::Error> {
			let blocks = self.blocks_to_return.lock().unwrap();
			Ok(blocks.clone())
		}

		async fn get_blocks_with_meta(
			&self,
			_start_block: u64,
			_end_block: Option<u64>,
		) -> Result<BlockFetchResult, anyhow::Error> {
			let blocks = self.blocks_to_return.lock().unwrap().clone();
			let failed_blocks = self.failed_blocks.lock().unwrap().clone();
			Ok(BlockFetchResult {
				blocks,
				failed_blocks,
				stream_kind: self.stream_kind.clone(),
			})
		}
	}

	#[tokio::test]
	async fn test_solana_skips_gap_detection_no_false_missed_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let network = create_solana_test_network_with_recovery();

		// Return Solana blocks with gaps (slots 96 and 98, missing 97)
		// This is normal for Solana — slot 97 may have been skipped or had no matching txs
		// Sparse stream kind means gap detection should NOT fire
		let rpc_client = MockSolanaRpcClient::new(130)
			.with_blocks(vec![create_solana_block(96), create_solana_block(98)]);

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

		// No missed blocks should be recorded — gaps are expected on Sparse streams
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		assert!(
			missed.is_empty(),
			"Sparse stream should not flag gaps as missed blocks, but found: {:?}",
			missed.iter().map(|e| e.block_number).collect::<Vec<_>>()
		);
	}

	#[tokio::test]
	async fn test_solana_saves_failed_slots_as_missed_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let network = create_solana_test_network_with_recovery();

		// Blocks returned successfully, but slot 97 had a failed transaction fetch
		let rpc_client = MockSolanaRpcClient::new(130)
			.with_blocks(vec![create_solana_block(96), create_solana_block(98)])
			.with_failed_blocks(vec![97]);

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

		// Slot 97 should be saved as missed (actual RPC failure)
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		let missed_numbers: Vec<u64> = missed.iter().map(|e| e.block_number).collect();
		assert!(
			missed_numbers.contains(&97),
			"Failed slot 97 should be recorded as missed, got: {:?}",
			missed_numbers
		);
	}

	#[tokio::test]
	async fn test_solana_dense_path_uses_gap_detection() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let network = create_solana_test_network_with_recovery();

		// Dense stream (non-optimized Solana path) with a gap — should detect as missed
		let rpc_client = MockSolanaRpcClient::new(130)
			.with_blocks(vec![create_solana_block(96), create_solana_block(98)])
			.with_stream_kind(FetchStreamKind::Dense);

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

		// Dense stream should detect block 97 as missed via gap detection
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		let missed_numbers: Vec<u64> = missed.iter().map(|e| e.block_number).collect();
		assert!(
			missed_numbers.contains(&97),
			"Dense stream should detect gaps as missed blocks, got: {:?}",
			missed_numbers
		);
	}

	#[tokio::test]
	async fn test_evm_still_uses_gap_detection() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let network = create_test_network_with_recovery();

		// EVM blocks with a gap (missing block 97) — should detect as missed
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

		// EVM should still detect block 97 as missed
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		let missed_numbers: Vec<u64> = missed.iter().map(|e| e.block_number).collect();
		assert!(
			missed_numbers.contains(&97),
			"EVM should still detect gaps as missed blocks"
		);
	}

	#[tokio::test]
	async fn test_sparse_with_blocks_and_failed_blocks_simultaneously() {
		// Verify that when Sparse stream returns both blocks and failed_blocks,
		// blocks are still processed AND failed_blocks are saved as missed
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let network = create_solana_test_network_with_recovery();

		// Return blocks for slots 96 and 98, with slot 97 as failed
		let rpc_client = MockSolanaRpcClient::new(130)
			.with_blocks(vec![create_solana_block(96), create_solana_block(98)])
			.with_failed_blocks(vec![97]);

		let block_tracker = Arc::new(BlockTracker::new(100));
		let block_handler = create_block_handler();

		let trigger_count = Arc::new(AtomicUsize::new(0));
		let trigger_handler = create_counting_trigger_handler(trigger_count.clone());

		let _ = process_new_blocks(
			&network,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await;

		// Wait for async triggers to complete
		tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

		// Blocks 96 and 98 should have been processed (triggers fired)
		assert_eq!(
			trigger_count.load(Ordering::SeqCst),
			2,
			"Both blocks should have been processed even though slot 97 failed"
		);

		// Slot 97 should be saved as missed
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		let missed_numbers: Vec<u64> = missed.iter().map(|e| e.block_number).collect();
		assert!(
			missed_numbers.contains(&97),
			"Failed slot 97 should be recorded as missed"
		);
	}

	#[tokio::test]
	async fn test_dense_stream_ignores_failed_blocks_field() {
		// When Dense stream has a non-empty failed_blocks (hypothetical), process_new_blocks
		// should use gap detection instead and ignore the failed_blocks field.
		// This is a behavioral test: Dense path uses detect_missing_blocks, not failed_blocks.
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let network = create_solana_test_network_with_recovery();

		// Dense stream with failed_blocks=[999] set but blocks have no gap 96,97,98
		let rpc_client = MockSolanaRpcClient::new(130)
			.with_blocks(vec![
				create_solana_block(96),
				create_solana_block(97),
				create_solana_block(98),
			])
			.with_failed_blocks(vec![999])
			.with_stream_kind(FetchStreamKind::Dense);

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

		// Gap detection should find no missed blocks (96,97,98 are contiguous)
		// The failed_blocks=[999] from the fetch result should be IGNORED
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		assert!(
			missed.is_empty(),
			"Dense stream should use gap detection, not failed_blocks field. Got: {:?}",
			missed.iter().map(|e| e.block_number).collect::<Vec<_>>()
		);
	}

	#[tokio::test]
	async fn test_process_new_blocks_sparse_no_recovery_config() {
		// Sparse stream with failed blocks but recovery NOT enabled —
		// failed blocks should NOT be saved when neither store_blocks nor recovery is enabled
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		storage
			.save_last_processed_block("test_network", 95)
			.await
			.unwrap();

		let mut network = create_solana_test_network_with_recovery();
		network.store_blocks = Some(false);
		network.recovery_config = None; // Disable recovery

		let rpc_client = MockSolanaRpcClient::new(130)
			.with_blocks(vec![create_solana_block(96)])
			.with_failed_blocks(vec![97]);

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

		// No missed blocks should be saved (recovery is disabled)
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		assert!(
			missed.is_empty(),
			"Missed blocks should not be saved when recovery is disabled"
		);
	}

	#[tokio::test]
	async fn test_run_lock_prevents_overlap() {
		// Verify that the recovery job skips when the run lock is held
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let network = create_test_network_with_recovery();
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let watcher: NetworkBlockWatcher<_, _, _, MockJobScheduler> = NetworkBlockWatcher::new(
			network,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		let run_lock = watcher.run_lock.clone();

		// Acquire the lock (simulating main watcher running)
		let _guard = run_lock.lock().await;

		// try_lock should fail (recovery would skip)
		let try_result = run_lock.try_lock();
		assert!(
			try_result.is_err(),
			"try_lock should fail when lock is held by main watcher"
		);

		// Drop the guard — lock should be acquirable again
		drop(_guard);
		let try_result = run_lock.try_lock();
		assert!(
			try_result.is_ok(),
			"try_lock should succeed after main watcher releases lock"
		);
	}

	#[tokio::test]
	async fn test_run_lock_shared_between_main_and_recovery() {
		// Verify that the run_lock is the same Arc between start_main_watcher and start_recovery_job
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));
		let network = create_test_network_with_recovery();
		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();
		let block_tracker = Arc::new(BlockTracker::new(100));

		let watcher: NetworkBlockWatcher<_, _, _, MockJobScheduler> = NetworkBlockWatcher::new(
			network,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		// The run_lock should be initialized and accessible
		assert!(
			watcher.run_lock.try_lock().is_ok(),
			"Freshly created watcher should have an unlocked run_lock"
		);

		// Verify Arc strong count is 1 (only the watcher holds it initially)
		assert_eq!(
			Arc::strong_count(&watcher.run_lock),
			1,
			"Only the watcher should initially hold the run_lock"
		);
	}

	#[tokio::test]
	async fn test_process_new_blocks_first_run_uses_get_blocks_with_meta() {
		// Verify that first run (last_processed_block == 0) uses get_blocks_with_meta
		// by using a MockSolanaRpcClient with Sparse stream
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		let network = create_solana_test_network_with_recovery();

		// No previous run — last_processed_block defaults to 0
		// Mock returns blocks via get_blocks_with_meta (Sparse)
		let rpc_client = MockSolanaRpcClient::new(130)
			.with_blocks(vec![create_solana_block(98)])
			.with_failed_blocks(vec![97]);

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

		// Should have saved latest_confirmed_block (130 - 32 = 98)
		let last_processed = storage
			.get_last_processed_block("test_network")
			.await
			.unwrap();
		assert_eq!(last_processed, Some(98));

		// Slot 97 should be saved as missed
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		let missed_numbers: Vec<u64> = missed.iter().map(|e| e.block_number).collect();
		assert!(missed_numbers.contains(&97));
	}

	#[tokio::test]
	async fn test_process_new_blocks_no_new_blocks_returns_empty_dense() {
		// When last_processed >= latest_confirmed, should return empty Dense result
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Already processed up to confirmed block
		storage
			.save_last_processed_block("test_network", 98)
			.await
			.unwrap();

		let network = create_solana_test_network_with_recovery();

		let rpc_client = MockSolanaRpcClient::new(130).with_failed_blocks(vec![999]); // Should be ignored

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

		// No missed blocks should be saved (empty Dense result, no gaps)
		let missed = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		assert!(
			missed.is_empty(),
			"No missed blocks when there are no new blocks to process"
		);
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
