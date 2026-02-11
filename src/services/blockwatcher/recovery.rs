//! Missed block recovery module.
//!
//! This module provides functionality to recover blocks that were missed
//! during normal monitoring cycles. It runs as a separate scheduled job
//! to avoid adding RPC load during normal operations.

use anyhow::Context;
use futures::future::BoxFuture;
use std::sync::Arc;

use crate::{
	models::{BlockRecoveryConfig, BlockType, Network, ProcessedBlock},
	services::{
		blockchain::BlockChainClient,
		blockwatcher::{
			error::BlockWatcherError,
			storage::{BlockStorage, MissedBlockStatus},
			tracker::BlockTrackerTrait,
		},
	},
};

/// Result of a recovery job execution
#[derive(Debug, Clone, Default)]
pub struct RecoveryResult {
	/// Number of blocks attempted for recovery
	pub attempted: usize,
	/// Number of blocks successfully recovered
	pub recovered: usize,
	/// Number of blocks that failed recovery
	pub failed: usize,
	/// Number of old blocks pruned
	pub pruned: usize,
}

/// Processes missed blocks for recovery
///
/// This function runs as part of the recovery job and attempts to fetch
/// and process blocks that were previously missed.
///
/// # Algorithm
/// 1. Get current block number
/// 2. Prune blocks older than `max_block_age`
/// 3. Load missed blocks with `status == Pending` and `retry_count < max_retries`
/// 4. Limit to `max_blocks_per_run`, sorted by block number (oldest first)
/// 5. For each block:
///    - Mark as `Recovering`
///    - Fetch via RPC
///    - On success: process through handlers, mark `Recovered`, remove from file
///    - On failure: increment `retry_count`, record error, apply `retry_delay_ms`
///    - If `retry_count >= max_retries`: mark as `Failed`
/// 6. Return statistics
#[allow(clippy::too_many_arguments)]
pub async fn process_missed_blocks<S, C, H, T, TR>(
	network: &Network,
	recovery_config: &BlockRecoveryConfig,
	rpc_client: &C,
	block_storage: Arc<S>,
	block_handler: Arc<H>,
	trigger_handler: Arc<T>,
	_block_tracker: Arc<TR>,
) -> Result<RecoveryResult, BlockWatcherError>
where
	S: BlockStorage + Send + Sync,
	C: BlockChainClient + Send + Sync,
	H: Fn(BlockType, Network) -> BoxFuture<'static, ProcessedBlock> + Send + Sync,
	T: Fn(&ProcessedBlock) -> tokio::task::JoinHandle<()> + Send + Sync,
	TR: BlockTrackerTrait + Send + Sync,
{
	let mut result = RecoveryResult::default();

	// Get current block number
	let current_block = rpc_client
		.get_latest_block_number()
		.await
		.with_context(|| "Failed to get latest block number for recovery")?;

	let current_confirmed = current_block.saturating_sub(network.confirmation_blocks);

	// Prune old blocks first
	let pruned = block_storage
		.prune_old_missed_blocks(
			&network.slug,
			recovery_config.max_block_age,
			current_confirmed,
		)
		.await
		.with_context(|| "Failed to prune old missed blocks")?;

	result.pruned = pruned;

	if pruned > 0 {
		tracing::info!(
			network = %network.slug,
			pruned = pruned,
			"Pruned {} old missed blocks",
			pruned
		);
	}

	// Get eligible missed blocks (filtered by age, status, and max_retries)
	let mut missed_blocks = block_storage
		.get_missed_blocks(
			&network.slug,
			recovery_config.max_block_age,
			current_confirmed,
			recovery_config.max_retries,
		)
		.await
		.with_context(|| "Failed to get missed blocks for recovery")?;

	// Sort by block number (oldest first)
	missed_blocks.sort_by_key(|b| b.block_number);

	// Limit to max_blocks_per_run
	missed_blocks.truncate(recovery_config.max_blocks_per_run as usize);

	if missed_blocks.is_empty() {
		tracing::debug!(
			network = %network.slug,
			"No missed blocks eligible for recovery"
		);
		return Ok(result);
	}

	tracing::info!(
		network = %network.slug,
		count = missed_blocks.len(),
		"Attempting recovery of {} missed blocks",
		missed_blocks.len()
	);

	let mut recovered_blocks = Vec::new();

	for entry in missed_blocks {
		result.attempted += 1;
		let block_number = entry.block_number;

		// Mark as Recovering
		if let Err(e) = block_storage
			.update_missed_block_status(
				&network.slug,
				block_number,
				MissedBlockStatus::Recovering,
				None,
			)
			.await
		{
			tracing::warn!(
				network = %network.slug,
				block = block_number,
				error = %e,
				"Failed to update block status to Recovering"
			);
		}

		// Attempt to fetch the block
		match rpc_client
			.get_blocks(block_number, Some(block_number))
			.await
		{
			Ok(blocks) if !blocks.is_empty() => {
				let block = blocks.into_iter().next().unwrap();

				// Process through block handler
				let processed_block = (block_handler)(block, network.clone()).await;

				// Execute trigger handler
				let _handle = (trigger_handler)(&processed_block);

				// Mark as Recovered
				if let Err(e) = block_storage
					.update_missed_block_status(
						&network.slug,
						block_number,
						MissedBlockStatus::Recovered,
						None,
					)
					.await
				{
					tracing::warn!(
						network = %network.slug,
						block = block_number,
						error = %e,
						"Failed to update block status to Recovered"
					);
				}

				recovered_blocks.push(block_number);
				result.recovered += 1;

				tracing::info!(
					network = %network.slug,
					block = block_number,
					"Successfully recovered missed block"
				);
			}
			Ok(_) => {
				// Block not found (empty response)
				let new_retry_count = entry.retry_count + 1;
				let error_msg = "Block not found in RPC response".to_string();

				let new_status = if new_retry_count >= recovery_config.max_retries {
					MissedBlockStatus::Failed
				} else {
					MissedBlockStatus::Pending
				};

				if let Err(e) = block_storage
					.update_missed_block_status(
						&network.slug,
						block_number,
						new_status.clone(),
						Some(error_msg.clone()),
					)
					.await
				{
					tracing::warn!(
						network = %network.slug,
						block = block_number,
						error = %e,
						"Failed to update block status after empty response"
					);
				}

				if new_status == MissedBlockStatus::Failed {
					result.failed += 1;
					tracing::error!(
						network = %network.slug,
						block = block_number,
						retries = new_retry_count,
						"Block recovery failed after max retries: {}",
						error_msg
					);
				} else {
					tracing::warn!(
						network = %network.slug,
						block = block_number,
						retry = new_retry_count,
						"Block recovery attempt failed, will retry: {}",
						error_msg
					);
				}

				// Apply retry delay
				tokio::time::sleep(tokio::time::Duration::from_millis(
					recovery_config.retry_delay_ms,
				))
				.await;
			}
			Err(e) => {
				let new_retry_count = entry.retry_count + 1;
				let error_msg = e.to_string();

				let new_status = if new_retry_count >= recovery_config.max_retries {
					MissedBlockStatus::Failed
				} else {
					MissedBlockStatus::Pending
				};

				if let Err(update_err) = block_storage
					.update_missed_block_status(
						&network.slug,
						block_number,
						new_status.clone(),
						Some(error_msg.clone()),
					)
					.await
				{
					tracing::warn!(
						network = %network.slug,
						block = block_number,
						error = %update_err,
						"Failed to update block status after RPC error"
					);
				}

				if new_status == MissedBlockStatus::Failed {
					result.failed += 1;
					tracing::error!(
						network = %network.slug,
						block = block_number,
						retries = new_retry_count,
						"Block recovery failed after max retries: {}",
						error_msg
					);
				} else {
					tracing::warn!(
						network = %network.slug,
						block = block_number,
						retry = new_retry_count,
						"Block recovery attempt failed, will retry: {}",
						error_msg
					);
				}

				// Apply retry delay
				tokio::time::sleep(tokio::time::Duration::from_millis(
					recovery_config.retry_delay_ms,
				))
				.await;
			}
		}
	}

	// Remove recovered blocks from storage
	if !recovered_blocks.is_empty() {
		if let Err(e) = block_storage
			.remove_recovered_blocks(&network.slug, &recovered_blocks)
			.await
		{
			tracing::warn!(
				network = %network.slug,
				error = %e,
				"Failed to remove recovered blocks from storage"
			);
		}
	}

	tracing::info!(
		network = %network.slug,
		attempted = result.attempted,
		recovered = result.recovered,
		failed = result.failed,
		pruned = result.pruned,
		"Recovery job completed"
	);

	Ok(result)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{BlockChainType, RpcUrl, SecretString, SecretValue};
	use crate::services::blockwatcher::storage::{
		BlockStorage, FileBlockStorage, MissedBlockEntry,
	};
	use crate::services::blockwatcher::tracker::BlockTracker;
	use std::sync::atomic::{AtomicUsize, Ordering};
	use tempfile::tempdir;

	fn create_test_network() -> Network {
		Network {
			network_type: BlockChainType::EVM,
			slug: "test_network".to_string(),
			name: "Test Network".to_string(),
			rpc_urls: vec![RpcUrl {
				type_: "rpc".to_string(),
				url: SecretValue::Plain(SecretString::new("http://localhost:8545".to_string())),
				weight: 100,
			}],
			chain_id: Some(1),
			network_passphrase: None,
			block_time_ms: 12000,
			confirmation_blocks: 12,
			cron_schedule: "*/10 * * * * *".to_string(),
			max_past_blocks: Some(100),
			store_blocks: Some(true),
			recovery_config: Some(BlockRecoveryConfig {
				enabled: true,
				cron_schedule: "0 */5 * * * *".to_string(),
				max_blocks_per_run: 10,
				max_block_age: 1000,
				max_retries: 3,
				retry_delay_ms: 100,
			}),
		}
	}

	fn create_recovery_config() -> BlockRecoveryConfig {
		BlockRecoveryConfig {
			enabled: true,
			cron_schedule: "0 */5 * * * *".to_string(),
			max_blocks_per_run: 10,
			max_block_age: 1000,
			max_retries: 3,
			retry_delay_ms: 100,
		}
	}

	/// Mock RPC client that can return empty responses for specific blocks
	#[derive(Clone)]
	struct MockRpcClientWithEmptyResponse {
		latest_block: u64,
		fail_blocks: Vec<u64>,
		empty_response_blocks: Vec<u64>,
		call_count: Arc<AtomicUsize>,
	}

	impl MockRpcClientWithEmptyResponse {
		fn new(latest_block: u64, fail_blocks: Vec<u64>, empty_response_blocks: Vec<u64>) -> Self {
			Self {
				latest_block,
				fail_blocks,
				empty_response_blocks,
				call_count: Arc::new(AtomicUsize::new(0)),
			}
		}
	}

	#[async_trait::async_trait]
	impl BlockChainClient for MockRpcClientWithEmptyResponse {
		async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
			Ok(self.latest_block)
		}

		async fn get_blocks(
			&self,
			start: u64,
			_end: Option<u64>,
		) -> Result<Vec<BlockType>, anyhow::Error> {
			self.call_count.fetch_add(1, Ordering::SeqCst);

			if self.fail_blocks.contains(&start) {
				return Err(anyhow::anyhow!("Simulated RPC failure for block {}", start));
			}

			if self.empty_response_blocks.contains(&start) {
				// Return empty response (block not found)
				return Ok(vec![]);
			}

			// Return a mock EVM block
			Ok(vec![BlockType::EVM(Box::default())])
		}
	}

	// Mock RPC client for testing
	#[derive(Clone)]
	struct MockRpcClient {
		latest_block: u64,
		fail_blocks: Vec<u64>,
		call_count: Arc<AtomicUsize>,
	}

	impl MockRpcClient {
		fn new(latest_block: u64, fail_blocks: Vec<u64>) -> Self {
			Self {
				latest_block,
				fail_blocks,
				call_count: Arc::new(AtomicUsize::new(0)),
			}
		}
	}

	#[async_trait::async_trait]
	impl BlockChainClient for MockRpcClient {
		async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
			Ok(self.latest_block)
		}

		async fn get_blocks(
			&self,
			start: u64,
			_end: Option<u64>,
		) -> Result<Vec<BlockType>, anyhow::Error> {
			self.call_count.fetch_add(1, Ordering::SeqCst);

			if self.fail_blocks.contains(&start) {
				return Err(anyhow::anyhow!("Simulated RPC failure for block {}", start));
			}

			// Return a mock EVM block
			Ok(vec![BlockType::EVM(Box::default())])
		}
	}

	/// Mock storage that fails on specific operations
	#[derive(Clone)]
	struct MockFailingStorage {
		inner: FileBlockStorage,
		fail_update_status: bool,
		fail_remove_recovered: bool,
	}

	impl MockFailingStorage {
		fn new(storage_path: std::path::PathBuf) -> Self {
			Self {
				inner: FileBlockStorage::new(storage_path),
				fail_update_status: false,
				fail_remove_recovered: false,
			}
		}

		fn with_failing_update_status(mut self) -> Self {
			self.fail_update_status = true;
			self
		}

		fn with_failing_remove_recovered(mut self) -> Self {
			self.fail_remove_recovered = true;
			self
		}
	}

	#[async_trait::async_trait]
	impl BlockStorage for MockFailingStorage {
		async fn get_last_processed_block(
			&self,
			network_id: &str,
		) -> Result<Option<u64>, anyhow::Error> {
			self.inner.get_last_processed_block(network_id).await
		}

		async fn save_last_processed_block(
			&self,
			network_id: &str,
			block: u64,
		) -> Result<(), anyhow::Error> {
			self.inner
				.save_last_processed_block(network_id, block)
				.await
		}

		async fn save_blocks(
			&self,
			network_id: &str,
			blocks: &[BlockType],
		) -> Result<(), anyhow::Error> {
			self.inner.save_blocks(network_id, blocks).await
		}

		async fn delete_blocks(&self, network_id: &str) -> Result<(), anyhow::Error> {
			self.inner.delete_blocks(network_id).await
		}

		async fn save_missed_blocks(
			&self,
			network_id: &str,
			blocks: &[u64],
		) -> Result<(), anyhow::Error> {
			self.inner.save_missed_blocks(network_id, blocks).await
		}

		async fn get_missed_blocks(
			&self,
			network_id: &str,
			max_block_age: u64,
			current_block: u64,
			max_retries: u32,
		) -> Result<Vec<MissedBlockEntry>, anyhow::Error> {
			self.inner
				.get_missed_blocks(network_id, max_block_age, current_block, max_retries)
				.await
		}

		async fn update_missed_block_status(
			&self,
			network_id: &str,
			block_number: u64,
			status: MissedBlockStatus,
			error: Option<String>,
		) -> Result<(), anyhow::Error> {
			if self.fail_update_status {
				return Err(anyhow::anyhow!("Simulated update status failure"));
			}
			self.inner
				.update_missed_block_status(network_id, block_number, status, error)
				.await
		}

		async fn remove_recovered_blocks(
			&self,
			network_id: &str,
			block_numbers: &[u64],
		) -> Result<(), anyhow::Error> {
			if self.fail_remove_recovered {
				return Err(anyhow::anyhow!("Simulated remove recovered failure"));
			}
			self.inner
				.remove_recovered_blocks(network_id, block_numbers)
				.await
		}

		async fn prune_old_missed_blocks(
			&self,
			network_id: &str,
			max_block_age: u64,
			current_block: u64,
		) -> Result<usize, anyhow::Error> {
			self.inner
				.prune_old_missed_blocks(network_id, max_block_age, current_block)
				.await
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

	#[tokio::test]
	async fn test_recovery_with_no_missed_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		let network = create_test_network();
		let recovery_config = create_recovery_config();
		let rpc_client = MockRpcClient::new(1000, vec![]);
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		assert_eq!(result.attempted, 0);
		assert_eq!(result.recovered, 0);
		assert_eq!(result.failed, 0);
	}

	#[tokio::test]
	async fn test_recovery_processes_missed_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Add some missed blocks
		storage
			.save_missed_blocks("test_network", &[100, 101, 102])
			.await
			.unwrap();

		let network = create_test_network();
		let recovery_config = create_recovery_config();
		let rpc_client = MockRpcClient::new(1000, vec![]); // No failures
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		assert_eq!(result.attempted, 3);
		assert_eq!(result.recovered, 3);
		assert_eq!(result.failed, 0);

		// Verify blocks were removed from storage
		let remaining = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		assert!(remaining.is_empty());
	}

	#[tokio::test]
	async fn test_recovery_handles_rpc_failures() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Add a missed block that will fail
		storage
			.save_missed_blocks("test_network", &[100])
			.await
			.unwrap();

		let network = create_test_network();
		let mut recovery_config = create_recovery_config();
		recovery_config.max_retries = 1; // Only 1 retry so it fails quickly

		let rpc_client = MockRpcClient::new(1000, vec![100]); // Block 100 will fail
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		assert_eq!(result.attempted, 1);
		assert_eq!(result.recovered, 0);
		assert_eq!(result.failed, 1);
	}

	#[tokio::test]
	async fn test_recovery_respects_max_blocks_per_run() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Add more blocks than max_blocks_per_run
		storage
			.save_missed_blocks("test_network", &[100, 101, 102, 103, 104])
			.await
			.unwrap();

		let network = create_test_network();
		let mut recovery_config = create_recovery_config();
		recovery_config.max_blocks_per_run = 2; // Only process 2

		let rpc_client = MockRpcClient::new(1000, vec![]);
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		assert_eq!(result.attempted, 2);
		assert_eq!(result.recovered, 2);

		// Should have 3 blocks remaining
		let remaining = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		assert_eq!(remaining.len(), 3);
	}

	#[tokio::test]
	async fn test_recovery_prunes_old_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Add a very old block (outside max_block_age)
		storage
			.save_missed_blocks("test_network", &[10]) // Very old block
			.await
			.unwrap();

		let network = create_test_network();
		let mut recovery_config = create_recovery_config();
		recovery_config.max_block_age = 100; // Only keep blocks within 100 of current

		let rpc_client = MockRpcClient::new(1000, vec![]);
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		// Block should be pruned (current is 1000, block 10 is way older than 100 blocks)
		assert_eq!(result.pruned, 1);
		assert_eq!(result.attempted, 0); // No blocks to attempt after pruning
	}

	#[tokio::test]
	async fn test_recovery_handles_empty_rpc_response_with_retry() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Add a missed block that will return empty response
		storage
			.save_missed_blocks("test_network", &[100])
			.await
			.unwrap();

		let network = create_test_network();
		let mut recovery_config = create_recovery_config();
		recovery_config.max_retries = 3; // Multiple retries so it won't fail immediately
		recovery_config.retry_delay_ms = 10; // Short delay for testing

		// Block 100 will return empty response (block not found)
		let rpc_client = MockRpcClientWithEmptyResponse::new(1000, vec![], vec![100]);
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		// Block should be attempted but not recovered (empty response)
		assert_eq!(result.attempted, 1);
		assert_eq!(result.recovered, 0);
		assert_eq!(result.failed, 0); // Not failed yet, will retry

		// Verify block is still in storage with incremented retry count
		let remaining = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		assert_eq!(remaining.len(), 1);
		assert_eq!(remaining[0].retry_count, 1);
	}

	#[tokio::test]
	async fn test_recovery_handles_empty_rpc_response_max_retries() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Add a missed block that will return empty response
		storage
			.save_missed_blocks("test_network", &[100])
			.await
			.unwrap();

		let network = create_test_network();
		let mut recovery_config = create_recovery_config();
		recovery_config.max_retries = 1; // Only 1 retry so it fails immediately
		recovery_config.retry_delay_ms = 10;

		// Block 100 will return empty response (block not found)
		let rpc_client = MockRpcClientWithEmptyResponse::new(1000, vec![], vec![100]);
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		// Block should fail after max retries with empty response
		assert_eq!(result.attempted, 1);
		assert_eq!(result.recovered, 0);
		assert_eq!(result.failed, 1);
	}

	#[tokio::test]
	async fn test_recovery_handles_rpc_failure_with_retry() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Add a missed block that will fail
		storage
			.save_missed_blocks("test_network", &[100])
			.await
			.unwrap();

		let network = create_test_network();
		let mut recovery_config = create_recovery_config();
		recovery_config.max_retries = 3; // Multiple retries
		recovery_config.retry_delay_ms = 10;

		let rpc_client = MockRpcClient::new(1000, vec![100]); // Block 100 will fail
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		// Block should be attempted but not recovered, not failed (will retry)
		assert_eq!(result.attempted, 1);
		assert_eq!(result.recovered, 0);
		assert_eq!(result.failed, 0);

		// Verify retry count was incremented
		let remaining = storage
			.get_missed_blocks("test_network", 1000, 1000, 3)
			.await
			.unwrap();
		assert_eq!(remaining.len(), 1);
		assert_eq!(remaining[0].retry_count, 1);
	}

	#[tokio::test]
	async fn test_recovery_continues_on_update_status_error_recovering() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(
			MockFailingStorage::new(temp_dir.path().to_path_buf()).with_failing_update_status(),
		);

		// First save the missed block using the inner storage
		storage
			.inner
			.save_missed_blocks("test_network", &[100])
			.await
			.unwrap();

		let network = create_test_network();
		let recovery_config = create_recovery_config();
		let rpc_client = MockRpcClient::new(1000, vec![]);
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		// Recovery should continue despite status update errors
		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		// Recovery still proceeds even with status update failures
		assert_eq!(result.attempted, 1);
		// The block is still processed through handlers even if status update fails
		assert_eq!(result.recovered, 1);
	}

	#[tokio::test]
	async fn test_recovery_continues_on_remove_recovered_error() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(
			MockFailingStorage::new(temp_dir.path().to_path_buf()).with_failing_remove_recovered(),
		);

		// First save the missed block using the inner storage
		storage
			.inner
			.save_missed_blocks("test_network", &[100])
			.await
			.unwrap();

		let network = create_test_network();
		let recovery_config = create_recovery_config();
		let rpc_client = MockRpcClient::new(1000, vec![]);
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		// Recovery should complete even if remove_recovered fails
		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		assert_eq!(result.attempted, 1);
		assert_eq!(result.recovered, 1);
		assert_eq!(result.failed, 0);
	}

	#[tokio::test]
	async fn test_recovery_logs_pruned_blocks() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Add multiple old blocks that will be pruned
		storage
			.save_missed_blocks("test_network", &[10, 20, 30])
			.await
			.unwrap();

		let network = create_test_network();
		let mut recovery_config = create_recovery_config();
		recovery_config.max_block_age = 100;

		let rpc_client = MockRpcClient::new(1000, vec![]);
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage,
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		// All blocks should be pruned (they're all older than current - max_age)
		assert_eq!(result.pruned, 3);
		assert_eq!(result.attempted, 0);
	}

	#[tokio::test]
	async fn test_recovery_mixed_success_and_failures() {
		let temp_dir = tempdir().unwrap();
		let storage = Arc::new(FileBlockStorage::new(temp_dir.path().to_path_buf()));

		// Add multiple missed blocks
		storage
			.save_missed_blocks("test_network", &[100, 101, 102])
			.await
			.unwrap();

		let network = create_test_network();
		let mut recovery_config = create_recovery_config();
		recovery_config.max_retries = 1;
		recovery_config.retry_delay_ms = 10;

		// Block 101 will fail
		let rpc_client = MockRpcClient::new(1000, vec![101]);
		let block_tracker = Arc::new(BlockTracker::new(100));

		let block_handler = create_block_handler();
		let trigger_handler = create_trigger_handler();

		let result = process_missed_blocks(
			&network,
			&recovery_config,
			&rpc_client,
			storage.clone(),
			block_handler,
			trigger_handler,
			block_tracker,
		)
		.await
		.unwrap();

		assert_eq!(result.attempted, 3);
		assert_eq!(result.recovered, 2); // 100 and 102 succeed
		assert_eq!(result.failed, 1); // 101 fails

		// Verify only block 101 remains in storage
		let json_path = temp_dir.path().join("test_network_missed_blocks.json");
		let content = tokio::fs::read_to_string(&json_path).await.unwrap();
		let entries: Vec<MissedBlockEntry> = serde_json::from_str(&content).unwrap();
		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].block_number, 101);
		assert_eq!(entries[0].status, MissedBlockStatus::Failed);
	}
}
