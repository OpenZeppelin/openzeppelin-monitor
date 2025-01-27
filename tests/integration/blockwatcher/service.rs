use futures::future::BoxFuture;
use mockall::predicate;
use std::sync::Arc;

use crate::integration::mocks::{
	create_test_block, create_test_network, MockBlockStorage, MockBlockTracker, MockEvmClientTrait,
};
use openzeppelin_monitor::{
	models::{BlockChainType, BlockType, Network, ProcessedBlock},
	services::blockwatcher::{process_new_blocks, BlockTrackerTrait, BlockWatcherError},
};

#[derive(Clone, Default)]
struct MockConfig {
	last_processed_block: Option<u64>,
	latest_block: u64,
	blocks_to_return: Vec<BlockType>,
	expected_save_block: Option<u64>,
	expected_block_range: Option<(u64, Option<u64>)>,
	expected_tracked_blocks: Vec<u64>,
	store_blocks: bool,
	history_size: usize,
}

/// Helper function to setup mock implementations with configurable expectations
fn setup_mocks(
	config: MockConfig,
) -> (
	Arc<MockBlockStorage>,
	MockBlockTracker<MockBlockStorage>,
	MockEvmClientTrait,
) {
	// Setup mock block storage
	let mut block_storage = MockBlockStorage::new();

	// Configure get_last_processed_block
	block_storage
		.expect_get_last_processed_block()
		.with(predicate::always())
		.returning(move |_| Ok(config.last_processed_block))
		.times(1);

	// Configure save_last_processed_block if expected
	if let Some(expected_block) = config.expected_save_block {
		block_storage
			.expect_save_last_processed_block()
			.with(predicate::always(), predicate::eq(expected_block))
			.returning(|_, _| Ok(()))
			.times(1);
	}

	// Configure block storage expectations based on store_blocks flag
	if config.store_blocks {
		block_storage
			.expect_delete_blocks()
			.with(predicate::always())
			.returning(|_| Ok(()))
			.times(1);

		block_storage
			.expect_save_blocks()
			.with(predicate::always(), predicate::always())
			.returning(|_, _| Ok(()))
			.times(1);
	} else {
		block_storage.expect_delete_blocks().times(0);
		block_storage.expect_save_blocks().times(0);
	}

	// Wrap the mock in an Arc to share the instance
	let block_storage_arc = Arc::new(block_storage);

	// Setup block tracker context for monitoring block processing
	let ctx = MockBlockTracker::<MockBlockStorage>::new_context();
	ctx.expect()
		.withf(|_, _| true)
		.returning(|_, _| MockBlockTracker::<MockBlockStorage>::default());

	// Setup mock RPC client
	let mut rpc_client = MockEvmClientTrait::new();

	// Configure get_latest_block_number
	rpc_client
		.expect_get_latest_block_number()
		.returning(move || Ok(config.latest_block))
		.times(1);

	// Configure get_blocks if range is specified
	if let Some((from, to)) = config.expected_block_range {
		rpc_client
			.expect_get_blocks()
			.with(predicate::eq(from), predicate::eq(to))
			.returning(move |_, _| Ok(config.blocks_to_return.clone()))
			.times(1);
	}

	// Setup mock block tracker with the same Arc<MockBlockStorage>
	let mut block_tracker = MockBlockTracker::<MockBlockStorage>::new(
		config.history_size,
		Some(block_storage_arc.clone()),
	);

	// Configure record_block expectations
	for &block_number in &config.expected_tracked_blocks {
		let block_num = block_number; // Create owned copy
		block_tracker
			.expect_record_block()
			.withf(move |network: &Network, num: &u64| {
				network.network_type == BlockChainType::EVM && *num == block_num
			})
			.returning(|_, _| ())
			.times(1);
	}

	(block_storage_arc, block_tracker, rpc_client)
}

#[tokio::test]
async fn test_normal_block_range() -> Result<(), BlockWatcherError> {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	let config = MockConfig {
		last_processed_block: Some(100),
		latest_block: 105,
		blocks_to_return: vec![
			create_test_block(BlockChainType::EVM, 101),
			create_test_block(BlockChainType::EVM, 102),
			create_test_block(BlockChainType::EVM, 103),
			create_test_block(BlockChainType::EVM, 104),
		],
		expected_save_block: Some(104),
		expected_block_range: Some((101, Some(104))),
		expected_tracked_blocks: vec![101, 102, 103, 104],
		store_blocks: false,
		history_size: 10,
	};

	let cloned_config = config.clone();

	let (block_storage, mut block_tracker, rpc_client) = setup_mocks(config);

	// Configure record_block expectations
	for block_number in cloned_config.expected_tracked_blocks {
		let block_num = block_number;
		block_tracker
			.expect_record_block()
			.withf(move |network: &Network, num: &u64| {
				network.network_type == BlockChainType::EVM && *num == block_num
			})
			.returning(|_, _| ());
	}

	// Create block processing handler that returns a ProcessedBlock
	let block_handler = Arc::new(|_: BlockType, network: Network| {
		Box::pin(async move {
			ProcessedBlock {
				block_number: 101,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	// Create trigger handler that spawns an empty task
	let trigger_handler = Arc::new(|_: &ProcessedBlock| tokio::spawn(async {}));

	// Process blocks
	process_new_blocks(
		&network,
		&rpc_client,
		block_storage.clone(),
		block_handler,
		trigger_handler,
		Arc::new(block_tracker),
	)
	.await?;

	Ok(())
}

#[tokio::test]
async fn test_fresh_start_processing() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	let config = MockConfig {
		last_processed_block: Some(0),
		latest_block: 100,
		blocks_to_return: vec![create_test_block(BlockChainType::EVM, 99)],
		expected_save_block: Some(99),
		expected_block_range: Some((99, None)),
		expected_tracked_blocks: vec![99],
		store_blocks: false,
		history_size: 10,
	};

	let (block_storage, block_tracker, rpc_client) = setup_mocks(config);

	// Create block processing handler that returns a ProcessedBlock
	let block_handler = Arc::new(|_: BlockType, network: Network| {
		Box::pin(async move {
			ProcessedBlock {
				block_number: 0,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let trigger_handler = Arc::new(|_processed_block: &ProcessedBlock| {
		tokio::spawn(async move { /* Handle trigger */ })
	});

	// Execute process_new_blocks
	let result = process_new_blocks(
		&network,
		&rpc_client,
		block_storage.clone(),
		block_handler,
		trigger_handler,
		Arc::new(block_tracker),
	)
	.await;

	assert!(result.is_ok(), "Process should complete successfully");
}

#[tokio::test]
async fn test_no_new_blocks() -> Result<(), BlockWatcherError> {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.store_blocks = Some(true);

	let config = MockConfig {
		last_processed_block: Some(100),
		latest_block: 100,        // Same as last_processed_block
		blocks_to_return: vec![], // No blocks should be returned
		expected_save_block: Some(99), /* We still store the last confirmed (latest_block - 1
		                           * confirmations) block */
		expected_block_range: None,      // No block range should be requested
		expected_tracked_blocks: vec![], // No blocks should be tracked
		store_blocks: true,
		history_size: 10,
	};

	let (block_storage, block_tracker, rpc_client) = setup_mocks(config);

	// Create block processing handler that should never be called
	let block_handler = Arc::new(|_: BlockType, network: Network| {
		Box::pin(async move {
			ProcessedBlock {
				block_number: 0,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let trigger_handler = Arc::new(|_: &ProcessedBlock| tokio::spawn(async {}));

	// Process blocks
	let result = process_new_blocks(
		&network,
		&rpc_client,
		block_storage.clone(),
		block_handler,
		trigger_handler,
		Arc::new(block_tracker),
	)
	.await;

	assert!(
		result.is_ok(),
		"Process should complete successfully even with no new blocks"
	);
	Ok(())
}

#[tokio::test]
async fn test_concurrent_processing() {
	// Test that blocks are processed concurrently
	// Verify the 32-block concurrent processing limit
}

#[tokio::test]
async fn test_ordered_trigger_handling() {
	// Test that blocks are triggered in correct order
	// Even when processed out of order
}

#[tokio::test]
async fn test_processing_error_handling() {
	// Test error scenarios during block processing
	// Verify proper error propagation
}

#[tokio::test]
async fn test_block_storage_enabled() -> Result<(), BlockWatcherError> {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.store_blocks = Some(true);

	let blocks_to_process = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 102),
	];

	let config = MockConfig {
		last_processed_block: Some(100),
		latest_block: 103,
		blocks_to_return: blocks_to_process.clone(),
		expected_save_block: Some(102),
		expected_block_range: Some((101, Some(102))),
		expected_tracked_blocks: vec![101, 102],
		store_blocks: true,
		history_size: 10,
	};

	let (block_storage, block_tracker, rpc_client) = setup_mocks(config);

	let block_handler = Arc::new(|_: BlockType, network: Network| {
		Box::pin(async move {
			ProcessedBlock {
				block_number: 0,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let trigger_handler = Arc::new(|_: &ProcessedBlock| tokio::spawn(async {}));

	let result = process_new_blocks(
		&network,
		&rpc_client,
		block_storage.clone(),
		block_handler,
		trigger_handler,
		Arc::new(block_tracker),
	)
	.await;

	assert!(
		result.is_ok(),
		"Block processing should succeed with storage enabled"
	);
	Ok(())
}

#[tokio::test]
async fn test_last_processed_block_update() {
	// Test proper updating of last processed block
	// Verify persistence
}

#[tokio::test]
async fn test_max_past_blocks_limit() {
	// Test different max_past_blocks configurations
	// Verify proper block range calculations
}

#[tokio::test]
async fn test_confirmation_blocks() {
	// Test different confirmation_blocks settings
	// Verify proper latest_confirmed_block calculation
}

#[tokio::test]
async fn test_rpc_client_errors() {
	// Test RPC client failure scenarios
	// Verify proper error handling
}

#[tokio::test]
async fn test_storage_errors() {
	// Test storage operation failures
	// Verify proper error handling
}

#[tokio::test]
async fn test_block_tracking() {
	// Test proper recording of blocks in tracker
	// Verify tracker state
}
