use futures::future::BoxFuture;
use mockall::predicate;
use std::sync::Arc;
use tokio_cron_scheduler::JobScheduler;

use crate::integration::mocks::{
	create_test_block, create_test_network, MockBlockStorage, MockBlockTracker,
	MockEVMTransportClient, MockEvmClientTrait, MockJobScheduler,
};
use openzeppelin_monitor::{
	models::{BlockChainType, BlockType, Network, ProcessedBlock},
	services::blockwatcher::{
		process_new_blocks, BlockCheckResult, BlockTracker, BlockTrackerTrait, BlockWatcherError,
		BlockWatcherService, NetworkBlockWatcher,
	},
	utils::get_cron_interval_ms,
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
}

/// Helper function to setup mock implementations with network-specific configuration
fn setup_mocks_with_network(
	config: MockConfig,
	network: Option<&Network>,
) -> (
	Arc<MockBlockStorage>,
	MockBlockTracker,
	MockEvmClientTrait<MockEVMTransportClient>,
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
	let mut block_tracker = MockBlockTracker::default();

	// Calculate start_block based on config (matches logic in process_new_blocks)
	// start_block = max(last_processed_block + 1, latest_confirmed_block - max_past_blocks)
	// Use network configuration if provided, otherwise use defaults
	let last_processed = config.last_processed_block.unwrap_or(0);
	let confirmation_blocks = network.map(|n| n.confirmation_blocks).unwrap_or(1); // default confirmation_blocks = 1
	let latest_confirmed = config.latest_block.saturating_sub(confirmation_blocks);
	let max_past_blocks = network
		.and_then(|n| n.max_past_blocks)
		.or_else(|| network.map(|n| n.get_recommended_past_blocks()))
		.unwrap_or(50); // default max_past_blocks = 50
	let start_block = std::cmp::max(
		last_processed + 1,
		latest_confirmed.saturating_sub(max_past_blocks),
	);

	// Configure reset_expected_next to be called at the start of each execution
	block_tracker
		.expect_reset_expected_next()
		.withf(move |network: &Network, block: &u64| {
			network.network_type == BlockChainType::EVM && *block == start_block
		})
		.returning(|_, _| ())
		.times(1);

	// Configure detect_missing_blocks to return empty vec (no gaps expected in most tests)
	let expected_blocks = config.expected_tracked_blocks.clone();
	block_tracker
		.expect_detect_missing_blocks()
		.withf(move |_, blocks: &[BlockType]| {
			// Verify blocks match expected
			let block_numbers: Vec<u64> = blocks.iter().filter_map(|b| b.number()).collect();
			block_numbers == expected_blocks
		})
		.returning(|_, _| Vec::new())
		.times(1);

	// Configure check_processed_block to return Ok for all expected blocks
	for &block_number in &config.expected_tracked_blocks {
		let block_num = block_number; // Create owned copy
		block_tracker
			.expect_check_processed_block()
			.withf(move |network: &Network, num: &u64| {
				network.network_type == BlockChainType::EVM && *num == block_num
			})
			.returning(|_, _| BlockCheckResult::Ok)
			.times(1);
	}

	(block_storage_arc, block_tracker, rpc_client)
}

#[tokio::test]
async fn test_normal_block_range() {
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
	};

	let (block_storage, block_tracker, rpc_client) =
		setup_mocks_with_network(config, Some(&network));

	// Create block processing handler that returns a ProcessedBlock
	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	// Create trigger handler that spawns an empty task
	let trigger_handler = Arc::new(|_: &ProcessedBlock| tokio::spawn(async {}));

	let block_tracker_arc = Arc::new(block_tracker);

	// Process blocks
	let result = process_new_blocks(
		&network,
		&rpc_client,
		block_storage.clone(),
		block_handler,
		trigger_handler,
		block_tracker_arc,
	)
	.await;

	assert!(result.is_ok(), "Process should complete successfully");
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
	};

	let (block_storage, block_tracker, rpc_client) =
		setup_mocks_with_network(config, Some(&network));

	// Create block processing handler that returns a ProcessedBlock
	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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
async fn test_no_new_blocks() {
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
	};

	let (block_storage, block_tracker, rpc_client) =
		setup_mocks_with_network(config, Some(&network));

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
}

#[tokio::test]
async fn test_concurrent_processing() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.max_past_blocks = Some(51); // match processing limit

	// Create 50 blocks to test the pipeline
	let blocks_to_process: Vec<u64> = (101..151).collect();

	let config = MockConfig {
		last_processed_block: Some(100),
		latest_block: 151,
		blocks_to_return: blocks_to_process
			.iter()
			.map(|&num| create_test_block(BlockChainType::EVM, num))
			.collect(),
		expected_save_block: Some(150),
		expected_block_range: Some((101, Some(150))),
		expected_tracked_blocks: blocks_to_process.clone(),
		store_blocks: false,
	};

	let (block_storage, block_tracker, rpc_client) =
		setup_mocks_with_network(config, Some(&network));

	// Track when each block starts and finishes processing
	let processing_records = Arc::new(tokio::sync::Mutex::new(Vec::new()));

	let block_handler = {
		let processing_records = processing_records.clone();

		Arc::new(move |block: BlockType, network: Network| {
			let processing_records = processing_records.clone();

			Box::pin(async move {
				let block_number = block.number().unwrap_or(0);
				let start_time = std::time::Instant::now();

				// Simulate varying processing times
				let sleep_duration = match block_number % 3 {
					0 => 100,
					1 => 150,
					_ => 200,
				};
				tokio::time::sleep(tokio::time::Duration::from_millis(sleep_duration)).await;

				processing_records.lock().await.push((
					block_number,
					start_time,
					std::time::Instant::now(),
				));

				ProcessedBlock {
					block_number,
					network_slug: network.slug,
					processing_results: vec![],
				}
			}) as BoxFuture<'static, ProcessedBlock>
		})
	};

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

	assert!(result.is_ok(), "Block processing should succeed");

	let records = processing_records.lock().await;

	// Verify concurrent processing through timing analysis
	let mut _concurrent_blocks = 0;
	let mut max_concurrent = 0;

	for (i, &(_, start1, end1)) in records.iter().enumerate() {
		_concurrent_blocks = 1;
		for &(_, start2, end2) in records.iter().skip(i + 1) {
			// Check if the processing times overlap
			if start2 < end1 && start1 < end2 {
				_concurrent_blocks += 1;
			}
		}
		max_concurrent = std::cmp::max(max_concurrent, _concurrent_blocks);
	}

	assert!(
		max_concurrent > 1,
		"Should process multiple blocks concurrently"
	);
	assert!(
		max_concurrent <= 32,
		"Should not exceed buffer_unordered(32) limit"
	);
}

#[tokio::test]
async fn test_ordered_trigger_handling() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	// Create blocks with varying processing times to ensure out-of-order processing
	let blocks_to_process: Vec<u64> = (101..106).collect();

	let config = MockConfig {
		last_processed_block: Some(100),
		latest_block: 106,
		blocks_to_return: blocks_to_process
			.iter()
			.map(|&num| create_test_block(BlockChainType::EVM, num))
			.collect(),
		expected_save_block: Some(105),
		expected_block_range: Some((101, Some(105))),
		expected_tracked_blocks: blocks_to_process.clone(),
		store_blocks: false,
	};

	let (block_storage, block_tracker, rpc_client) =
		setup_mocks_with_network(config, Some(&network));

	// Track the order of triggered blocks
	let triggered_blocks = Arc::new(tokio::sync::Mutex::new(Vec::new()));

	// Create block handler that processes blocks with varying delays
	let block_handler = Arc::new(move |block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);

			// Intentionally delay processing of even-numbered blocks
			if block_number.is_multiple_of(2) {
				tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
			} else {
				tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
			}

			ProcessedBlock {
				block_number,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	// Create trigger handler that records the order of triggered blocks
	let trigger_handler = {
		let triggered_blocks = triggered_blocks.clone();

		Arc::new(move |block: &ProcessedBlock| {
			let triggered_blocks = triggered_blocks.clone();
			let block_number = block.block_number;

			tokio::spawn(async move {
				triggered_blocks.lock().await.push(block_number);
			})
		})
	};

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

	assert!(result.is_ok(), "Block processing should succeed");

	// Verify blocks were triggered in order
	let final_order = triggered_blocks.lock().await;

	// Check that blocks were triggered in ascending order
	let expected_order: Vec<u64> = (101..106).collect();
	assert_eq!(
		*final_order, expected_order,
		"Blocks should be triggered in sequential order regardless of processing time. Expected: \
		 {:?}, Got: {:?}",
		expected_order, *final_order
	);

	// Verify all blocks were triggered
	assert_eq!(
		final_order.len(),
		blocks_to_process.len(),
		"All blocks should be triggered"
	);
}

#[tokio::test]
async fn test_block_storage_enabled() {
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
	};

	let (block_storage, block_tracker, rpc_client) =
		setup_mocks_with_network(config, Some(&network));

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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
}

#[tokio::test]
async fn test_max_past_blocks_limit() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.max_past_blocks = Some(3); // Only process last 3 blocks max

	let config = MockConfig {
		last_processed_block: Some(100),
		latest_block: 110,
		blocks_to_return: vec![
			create_test_block(BlockChainType::EVM, 106),
			create_test_block(BlockChainType::EVM, 107),
			create_test_block(BlockChainType::EVM, 108),
			create_test_block(BlockChainType::EVM, 109),
		],
		expected_save_block: Some(109),
		// Should start at 106 (110 - 1 confirmation - 3 past blocks) instead of 101
		expected_block_range: Some((106, Some(109))),
		expected_tracked_blocks: vec![106, 107, 108, 109],
		store_blocks: false,
	};

	let (block_storage, block_tracker, rpc_client) =
		setup_mocks_with_network(config, Some(&network));

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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
		"Block processing should succeed with max_past_blocks limit"
	);
}

#[tokio::test]
async fn test_max_past_blocks_limit_recommended() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.max_past_blocks = None; // Use recommended past blocks
	network.block_time_ms = 12000;
	network.cron_schedule = "*/5 * * * * *".to_string(); // Every 5 seconds
	network.confirmation_blocks = 12;

	// (cron_interval_ms/block_time_ms) + confirmation_blocks + 1
	let recommended_max_past_blocks =
		(get_cron_interval_ms(&network.cron_schedule).unwrap() as u64 / 12000) + 12 + 1;

	assert_eq!(
		network.get_recommended_past_blocks(),
		recommended_max_past_blocks
	);

	let config = MockConfig {
		last_processed_block: Some(100),
		latest_block: 150,
		blocks_to_return: vec![
			create_test_block(BlockChainType::EVM, 125),
			create_test_block(BlockChainType::EVM, 126),
			create_test_block(BlockChainType::EVM, 127),
			create_test_block(BlockChainType::EVM, 128),
			create_test_block(BlockChainType::EVM, 129),
			create_test_block(BlockChainType::EVM, 130),
			create_test_block(BlockChainType::EVM, 131),
			create_test_block(BlockChainType::EVM, 132),
			create_test_block(BlockChainType::EVM, 133),
			create_test_block(BlockChainType::EVM, 134),
			create_test_block(BlockChainType::EVM, 135),
			create_test_block(BlockChainType::EVM, 136),
			create_test_block(BlockChainType::EVM, 137),
			create_test_block(BlockChainType::EVM, 138),
		],
		expected_save_block: Some(138),
		expected_block_range: Some((125, Some(138))), /* start at 125 (150 - 12 (confirmations) - 13 (max_past_blocks)
													  stop at 138 (150 - 12 (confirmations) */
		expected_tracked_blocks: vec![
			125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138,
		],
		store_blocks: false,
	};

	let (block_storage, block_tracker, rpc_client) =
		setup_mocks_with_network(config, Some(&network));

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let trigger_handler = Arc::new(|_: &ProcessedBlock| tokio::spawn(async {}));

	// Process blocks without limit
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
		"Block processing should succeed without max_past_blocks limit"
	);
}

#[tokio::test]
async fn test_confirmation_blocks() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.confirmation_blocks = 2;

	let config = MockConfig {
		last_processed_block: Some(100),
		latest_block: 105,
		blocks_to_return: vec![
			create_test_block(BlockChainType::EVM, 101),
			create_test_block(BlockChainType::EVM, 102),
			create_test_block(BlockChainType::EVM, 103),
		],
		expected_save_block: Some(103), /* We expect this to be saved as the last processed block
		                                 * with 2 confirmations */
		expected_block_range: Some((101, Some(103))),
		expected_tracked_blocks: vec![101, 102, 103],
		store_blocks: false,
	};

	let (block_storage, block_tracker, rpc_client) =
		setup_mocks_with_network(config, Some(&network));

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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

	assert!(result.is_ok(), "Block processing should succeed");
}

#[tokio::test]
async fn test_process_new_blocks_storage_error() {
	let network = create_test_network("Ethereum", "ethereum_mainnet", BlockChainType::EVM);

	// Create mock block storage that returns an error
	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.with(predicate::always())
		.returning(|_| Err(anyhow::anyhow!("Storage error")))
		.times(1);

	let block_storage = Arc::new(block_storage);

	// Setup other required mocks
	let ctx = MockBlockTracker::new_context();
	ctx.expect()
		.withf(|_| true)
		.returning(|_| MockBlockTracker::default());

	let rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();

	let block_handler = Arc::new(|_: BlockType, network: Network| {
		Box::pin(async move {
			ProcessedBlock {
				block_number: 101,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let trigger_handler = Arc::new(|_: &ProcessedBlock| tokio::spawn(async {}));

	// Process blocks - should fail with storage error
	let result = process_new_blocks(
		&network,
		&rpc_client,
		block_storage.clone(),
		block_handler,
		trigger_handler,
		Arc::new(MockBlockTracker::default()),
	)
	.await;

	assert!(result.is_err());
	if let Err(e) = result {
		assert!(matches!(e, BlockWatcherError::Other { .. }));
	}
}

#[tokio::test]
async fn test_process_new_blocks_network_errors() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	// Setup mock block storage
	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	let block_storage = Arc::new(block_storage);

	// Setup mock RPC client that fails
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Err(anyhow::anyhow!("RPC error")))
		.times(1);

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

	// Process blocks - should fail with network error
	let result = process_new_blocks(
		&network,
		&rpc_client,
		block_storage.clone(),
		block_handler,
		trigger_handler,
		Arc::new(MockBlockTracker::default()),
	)
	.await;

	assert!(result.is_err());
	if let Err(e) = result {
		assert!(matches!(e, BlockWatcherError::Other { .. }));
	}
}

#[tokio::test]
async fn test_process_new_blocks_get_blocks_error() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	// Setup mock block storage
	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	let block_storage = Arc::new(block_storage);

	// Setup mock RPC client that fails on get_blocks
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(|_, _| Err(anyhow::anyhow!("Failed to fetch blocks")))
		.times(1);

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
		Arc::new(MockBlockTracker::default()),
	)
	.await;

	assert!(result.is_err());
	if let Err(e) = result {
		assert!(matches!(e, BlockWatcherError::Other { .. }));
	}
}

#[tokio::test]
async fn test_process_new_blocks_storage_save_error() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.store_blocks = Some(true);

	// Setup mock block storage that fails on save
	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	block_storage
		.expect_delete_blocks()
		.returning(|_| Ok(()))
		.times(1);
	block_storage
		.expect_save_blocks()
		.returning(|_, _| Err(anyhow::anyhow!("Failed to save blocks")))
		.times(1);
	let block_storage = Arc::new(block_storage);

	// Setup block tracker expectations
	let mut block_tracker = MockBlockTracker::default();
	// start_block = max(100 + 1, 105 - 1 - 50) = max(101, 54) = 101
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	block_tracker
		.expect_detect_missing_blocks()
		.withf(|_, blocks: &[BlockType]| {
			let block_numbers: Vec<u64> = blocks.iter().filter_map(|b| b.number()).collect();
			block_numbers == vec![101]
		})
		.returning(|_, _| Vec::new())
		.times(1);
	block_tracker
		.expect_check_processed_block()
		.withf(|_, block_number| *block_number == 101)
		.returning(|_, _| BlockCheckResult::Ok)
		.times(1);

	// Setup mock RPC client
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(|_, _| Ok(vec![create_test_block(BlockChainType::EVM, 101)]))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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

	assert!(result.is_err());
	if let Err(e) = result {
		assert!(matches!(e, BlockWatcherError::Other { .. }));
	}
}

#[tokio::test]
async fn test_process_new_blocks_save_last_processed_error() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	// Setup mock block storage that fails on save_last_processed_block
	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	block_storage
		.expect_save_last_processed_block()
		.returning(|_, _| Err(anyhow::anyhow!("Failed to save last processed block")))
		.times(1);
	let block_storage = Arc::new(block_storage);

	// Setup block tracker expectations
	let mut block_tracker = MockBlockTracker::default();
	// start_block = max(100 + 1, 105 - 1 - 50) = max(101, 54) = 101
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	block_tracker
		.expect_detect_missing_blocks()
		.withf(|_, blocks: &[BlockType]| {
			let block_numbers: Vec<u64> = blocks.iter().filter_map(|b| b.number()).collect();
			block_numbers == vec![101]
		})
		.returning(|_, _| Vec::new())
		.times(1);
	block_tracker
		.expect_check_processed_block()
		.withf(|_, block_number| *block_number == 101)
		.returning(|_, _| BlockCheckResult::Ok)
		.times(1);

	// Setup mock RPC client
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(|_, _| Ok(vec![create_test_block(BlockChainType::EVM, 101)]))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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

	assert!(result.is_err());
	if let Err(e) = result {
		assert!(matches!(e, BlockWatcherError::Other { .. }));
	}
}

#[tokio::test]
async fn test_process_new_blocks_storage_delete_error() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.store_blocks = Some(true);

	// Setup mock block storage that fails on delete
	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	block_storage
		.expect_delete_blocks()
		.returning(|_| Err(anyhow::anyhow!("Failed to delete blocks")))
		.times(1);
	// save_blocks should not be called if delete fails
	block_storage.expect_save_blocks().times(0);
	let block_storage = Arc::new(block_storage);

	// Setup block tracker expectations
	let mut block_tracker = MockBlockTracker::default();
	// start_block = max(100 + 1, 105 - 1 - 50) = max(101, 54) = 101
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	block_tracker
		.expect_detect_missing_blocks()
		.withf(|_, blocks: &[BlockType]| {
			let block_numbers: Vec<u64> = blocks.iter().filter_map(|b| b.number()).collect();
			block_numbers == vec![101]
		})
		.returning(|_, _| Vec::new())
		.times(1);
	block_tracker
		.expect_check_processed_block()
		.withf(|_, block_number| *block_number == 101)
		.returning(|_, _| BlockCheckResult::Ok)
		.times(1);

	// Setup mock RPC client
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(|_, _| Ok(vec![create_test_block(BlockChainType::EVM, 101)]))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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

	assert!(result.is_err());
	if let Err(e) = result {
		assert!(matches!(e, BlockWatcherError::Other { .. }));
	}
}

#[tokio::test]
async fn test_network_block_watcher_new() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	let block_storage = Arc::new(MockBlockStorage::new());
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
	let block_tracker = Arc::new(BlockTracker::new(10));

	let watcher = NetworkBlockWatcher::<_, _, _, JobScheduler>::new(
		network,
		block_storage,
		block_handler,
		trigger_handler,
		block_tracker,
	)
	.await;

	assert!(watcher.is_ok());

	// Not expected to be initialized since we haven't started the watcher
	assert!(!watcher
		.unwrap()
		.scheduler
		.inited
		.load(std::sync::atomic::Ordering::Relaxed));
}

#[tokio::test]
async fn test_network_block_watcher_start_stop() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	let block_storage = Arc::new(MockBlockStorage::new());
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
	let block_tracker = Arc::new(BlockTracker::new(10));

	let watcher = NetworkBlockWatcher::<_, _, _, JobScheduler>::new(
		network.clone(),
		block_storage.clone(),
		block_handler,
		trigger_handler,
		block_tracker,
	)
	.await;

	// Setup mock RPC client
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(100))
		.times(0);

	let mut watcher = watcher.unwrap();
	// Test start
	let started_result = watcher.start(rpc_client).await;
	assert!(started_result.is_ok());
	assert!(watcher.scheduler.inited().await);

	// Test stop
	let stopped_result = watcher.stop().await;
	assert!(stopped_result.is_ok());
}

#[tokio::test]
async fn test_block_watcher_service_start_stop_network() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	let block_storage = Arc::new(MockBlockStorage::new());
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
	let block_tracker = Arc::new(BlockTracker::new(10));

	let service = BlockWatcherService::<_, _, _, JobScheduler>::new(
		block_storage.clone(),
		block_handler,
		trigger_handler,
		block_tracker,
	)
	.await;

	// Setup mock RPC client
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(100))
		.times(0);

	rpc_client
		.expect_clone()
		.times(2)
		.returning(MockEvmClientTrait::new);

	let service = service.unwrap();

	// Test starting a network watcher
	let started_result = service
		.start_network_watcher(&network, rpc_client.clone())
		.await;
	assert!(started_result.is_ok());
	{
		let watchers = service.active_watchers.read().await;
		assert!(watchers.contains_key(&network.slug));
	}

	// Test starting the same network watcher again (should be idempotent)
	let started_result = service
		.start_network_watcher(&network, rpc_client.clone())
		.await;
	assert!(started_result.is_ok());
	{
		let watchers = service.active_watchers.read().await;
		assert_eq!(watchers.len(), 1);
	}

	// Test stopping the network watcher
	let stopped_result = service.stop_network_watcher(&network.slug).await;
	assert!(stopped_result.is_ok());
	{
		let watchers = service.active_watchers.read().await;
		assert!(!watchers.contains_key(&network.slug));
	}

	// Test stopping a non-existent network watcher (should not error)
	let stopped_result = service.stop_network_watcher("non-existent").await;
	assert!(stopped_result.is_ok());
}

#[tokio::test]
async fn test_block_watcher_service_new() {
	let block_storage = Arc::new(MockBlockStorage::new());
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
	let block_tracker = Arc::new(BlockTracker::new(10));

	let service = BlockWatcherService::<_, _, _, JobScheduler>::new(
		block_storage.clone(),
		block_handler,
		trigger_handler,
		block_tracker,
	)
	.await;

	assert!(service.is_ok());
	assert!(service.unwrap().active_watchers.read().await.is_empty());
}

#[tokio::test]
async fn test_process_new_blocks_get_blocks_error_fresh_start() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	// Setup mock block storage that returns 0 as last processed block
	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(0)))
		.times(1);
	let block_storage = Arc::new(block_storage);

	// Setup mock RPC client that succeeds for latest block but fails for get_blocks
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(100))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.with(predicate::eq(99), predicate::eq(None))
		.returning(|_, _| Err(anyhow::anyhow!("Failed to fetch block")))
		.times(1);

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
		Arc::new(MockBlockTracker::default()),
	)
	.await;

	assert!(result.is_err());
	if let Err(e) = result {
		assert!(matches!(e, BlockWatcherError::Other { .. }));
	}
}

#[tokio::test]
async fn test_scheduler_errors() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	let block_storage = Arc::new(MockBlockStorage::new());
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
	let block_tracker = Arc::new(BlockTracker::new(10));

	// Test case 1: Scheduler fails to initialize
	{
		let ctx = MockJobScheduler::new_context();
		ctx.expect()
			.returning(|| Err("Failed to initialize scheduler".into()));

		let service = BlockWatcherService::<_, _, _, MockJobScheduler>::new(
			block_storage.clone(),
			block_handler.clone(),
			trigger_handler.clone(),
			block_tracker.clone(),
		)
		.await
		.unwrap();

		let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
		rpc_client
			.expect_clone()
			.returning(MockEvmClientTrait::<MockEVMTransportClient>::new);

		let result = service.start_network_watcher(&network, rpc_client).await;

		assert!(matches!(
			result.unwrap_err(),
			BlockWatcherError::SchedulerError { .. }
		));
	}

	// Test case 2: Scheduler fails to add job
	{
		let ctx = MockJobScheduler::new_context();
		ctx.expect().returning(|| {
			let mut scheduler = MockJobScheduler::default();
			scheduler
				.expect_add()
				.returning(|_| Err("Failed to add job".into()));
			Ok(scheduler)
		});

		let service = BlockWatcherService::<_, _, _, MockJobScheduler>::new(
			block_storage.clone(),
			block_handler.clone(),
			trigger_handler.clone(),
			block_tracker.clone(),
		)
		.await
		.unwrap();

		let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
		rpc_client
			.expect_clone()
			.returning(MockEvmClientTrait::<MockEVMTransportClient>::new);

		let result = service.start_network_watcher(&network, rpc_client).await;

		assert!(matches!(
			result.unwrap_err(),
			BlockWatcherError::SchedulerError { .. }
		));
	}

	// Test case 3: Scheduler fails to start
	{
		let ctx = MockJobScheduler::new_context();
		ctx.expect().returning(|| {
			let mut scheduler = MockJobScheduler::default();
			scheduler.expect_add().returning(|_| Ok(()));

			scheduler
				.expect_start()
				.times(1)
				.returning(|| Err("Failed to start scheduler".into()));
			Ok(scheduler)
		});

		let service = BlockWatcherService::<_, _, _, MockJobScheduler>::new(
			block_storage.clone(),
			block_handler.clone(),
			trigger_handler.clone(),
			block_tracker.clone(),
		)
		.await
		.unwrap();

		let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
		rpc_client
			.expect_clone()
			.returning(MockEvmClientTrait::<MockEVMTransportClient>::new);

		let result = service.start_network_watcher(&network, rpc_client).await;

		assert!(matches!(
			result.unwrap_err(),
			BlockWatcherError::SchedulerError { .. }
		));
	}

	// Test case 4: Scheduler fails to shutdown
	{
		let ctx = MockJobScheduler::new_context();
		ctx.expect().returning(|| {
			let mut scheduler = MockJobScheduler::default();

			scheduler.expect_add().returning(|_| Ok(()));
			scheduler.expect_start().returning(|| Ok(()));
			scheduler
				.expect_shutdown()
				.returning(|| Err("Failed to shutdown scheduler".into()));
			Ok(scheduler)
		});

		let service = BlockWatcherService::<_, _, _, MockJobScheduler>::new(
			block_storage.clone(),
			block_handler.clone(),
			trigger_handler.clone(),
			block_tracker.clone(),
		)
		.await
		.unwrap();

		let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
		rpc_client
			.expect_clone()
			.returning(MockEvmClientTrait::<MockEVMTransportClient>::new);

		let _ = service.start_network_watcher(&network, rpc_client).await;

		assert!(service
			.active_watchers
			.read()
			.await
			.contains_key(&network.slug));

		let result = service.stop_network_watcher(&network.slug).await;

		assert!(matches!(
			result.unwrap_err(),
			BlockWatcherError::SchedulerError { .. }
		));
	}
}

#[tokio::test]
async fn test_missed_block_detection_and_saving() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.store_blocks = Some(true);

	// Create blocks with gaps (101, 102, 104, 106, 107) - missing 103 and 105
	let blocks_with_gaps = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 102),
		create_test_block(BlockChainType::EVM, 104),
		create_test_block(BlockChainType::EVM, 106),
		create_test_block(BlockChainType::EVM, 107),
	];

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.with(predicate::always())
		.returning(|_| Ok(Some(100)))
		.times(1);

	// Expect save_missed_blocks to be called for blocks 103 and 105
	block_storage
		.expect_save_missed_blocks()
		.with(
			predicate::eq("test-network"),
			predicate::function(|blocks: &[u64]| blocks == [103, 105]),
		)
		.returning(|_, _| Ok(()))
		.times(1);

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

	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::eq(107))
		.returning(|_, _| Ok(()))
		.times(1);

	let block_storage = Arc::new(block_storage);

	// Setup block tracker expectations
	let mut block_tracker = MockBlockTracker::default();
	// start_block = max(100 + 1, 108 - 1 - 50) = max(101, 57) = 101
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	// Configure detect_missing_blocks to return missed blocks 103 and 105
	block_tracker
		.expect_detect_missing_blocks()
		.withf(|_, blocks: &[BlockType]| {
			let block_numbers: Vec<u64> = blocks.iter().filter_map(|b| b.number()).collect();
			block_numbers == vec![101, 102, 104, 106, 107]
		})
		.returning(|_, _| vec![103, 105])
		.times(1);

	// Configure check_processed_block for all fetched blocks
	for &block_num in &[101, 102, 104, 106, 107] {
		block_tracker
			.expect_check_processed_block()
			.withf(move |_, num: &u64| *num == block_num)
			.returning(|_, _| BlockCheckResult::Ok)
			.times(1);
	}

	// Setup mock RPC client
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(108))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.with(predicate::eq(101), predicate::eq(Some(107)))
		.returning(move |_, _| Ok(blocks_with_gaps.clone()))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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
		"Process should succeed and save missed blocks"
	);
}

#[tokio::test]
async fn test_missed_block_save_error() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.store_blocks = Some(true);

	// Create blocks with a gap (101, 103) - missing 102
	let blocks_with_gap = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 103),
	];

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);

	// Expect save_missed_blocks to be called and fail
	block_storage
		.expect_save_missed_blocks()
		.with(
			predicate::eq("test-network"),
			predicate::function(|blocks: &[u64]| blocks == [102]),
		)
		.returning(|_, _| Err(anyhow::anyhow!("Failed to save missed blocks")))
		.times(1);

	let block_storage = Arc::new(block_storage);

	// Setup block tracker expectations
	let mut block_tracker = MockBlockTracker::default();
	// start_block = max(100 + 1, 105 - 1 - 10) = max(101, 94) = 101
	// max_past_blocks = 10 (from create_test_network default)
	block_tracker
		.expect_reset_expected_next()
		.with(predicate::always(), predicate::eq(101u64))
		.returning(|_, _| ())
		.times(1);
	// Configure detect_missing_blocks to return missed block 102
	block_tracker
		.expect_detect_missing_blocks()
		.withf(|_, blocks: &[BlockType]| {
			let block_numbers: Vec<u64> = blocks.iter().filter_map(|b| b.number()).collect();
			block_numbers == vec![101, 103]
		})
		.returning(|_, _| vec![102])
		.times(1);

	// Note: check_processed_block won't be called because save_missed_blocks fails
	// and the function returns early before processing blocks

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(move |_, _| Ok(blocks_with_gap.clone()))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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

	assert!(result.is_err());
	if let Err(e) = result {
		assert!(matches!(e, BlockWatcherError::Other { .. }));
	}
}

#[tokio::test]
async fn test_missed_block_detection_store_disabled() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.store_blocks = Some(false);

	// Create blocks with gaps (101, 103) - missing 102
	let blocks_with_gaps = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 103),
	];

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);

	// save_missed_blocks should NOT be called when store_blocks is false
	block_storage.expect_save_missed_blocks().times(0);

	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::eq(104))
		.returning(|_, _| Ok(()))
		.times(1);

	let block_storage = Arc::new(block_storage);

	let mut block_tracker = MockBlockTracker::default();
	// start_block = max(100 + 1, 105 - 1 - 50) = max(101, 54) = 101
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	// Configure detect_missing_blocks to return missed block 102
	block_tracker
		.expect_detect_missing_blocks()
		.withf(|_, blocks: &[BlockType]| {
			let block_numbers: Vec<u64> = blocks.iter().filter_map(|b| b.number()).collect();
			block_numbers == vec![101, 103]
		})
		.returning(|_, _| vec![102])
		.times(1);

	// Configure check_processed_block for fetched blocks
	for &block_num in &[101, 103] {
		block_tracker
			.expect_check_processed_block()
			.withf(move |_, num: &u64| *num == block_num)
			.returning(|_, _| BlockCheckResult::Ok)
			.times(1);
	}

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(move |_, _| Ok(blocks_with_gaps.clone()))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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
		"Process should succeed without saving missed blocks when store_blocks is false"
	);
}

#[tokio::test]
async fn test_scheduled_job_execution_success() {
	// Test that the scheduled job actually executes and calls process_new_blocks
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	// Use a very frequent schedule (every second) to ensure the job runs during the test
	network.cron_schedule = "* * * * * *".to_string();

	let mut block_storage = MockBlockStorage::new();
	// The job will call get_last_processed_block when it runs
	block_storage
		.expect_get_last_processed_block()
		.with(predicate::always())
		.returning(|_| Ok(Some(100)))
		.times(1..=10); // Allow multiple calls in case job runs multiple times

	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::always())
		.returning(|_, _| Ok(()))
		.times(1..=10);

	let block_storage = Arc::new(block_storage);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let trigger_handler = Arc::new(|_: &ProcessedBlock| tokio::spawn(async {}));

	// Use real BlockTracker since NetworkBlockWatcher::new requires it
	let block_tracker = Arc::new(BlockTracker::new(10));

	let mut watcher = NetworkBlockWatcher::<_, _, _, JobScheduler>::new(
		network.clone(),
		block_storage.clone(),
		block_handler,
		trigger_handler,
		block_tracker,
	)
	.await
	.unwrap();

	// Setup mock RPC client that will be called by the scheduled job
	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();

	// Expect clone to be called multiple times (when job is created and each time it runs)
	rpc_client.expect_clone().times(1..=20).returning(|| {
		let mut client = MockEvmClientTrait::<MockEVMTransportClient>::new();
		client
			.expect_get_latest_block_number()
			.returning(|| Ok(105))
			.times(1..=10);
		// get_blocks should be called since last_processed (100) < latest_confirmed (104)
		// Return blocks 101-104 (latest_confirmed = 105 - 1 = 104)
		client
			.expect_get_blocks()
			.returning(|_, _| {
				Ok(vec![
					create_test_block(BlockChainType::EVM, 101),
					create_test_block(BlockChainType::EVM, 102),
					create_test_block(BlockChainType::EVM, 103),
					create_test_block(BlockChainType::EVM, 104),
				])
			})
			.times(1..=10);
		client
			.expect_clone()
			.times(0..=10)
			.returning(MockEvmClientTrait::<MockEVMTransportClient>::new);
		client
	});

	// Start the watcher - this schedules the job
	let start_result = watcher.start(rpc_client).await;
	assert!(start_result.is_ok());

	// Wait for the job to execute (give it up to 2 seconds to ensure it runs at least once)
	tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

	// Stop the watcher
	let stop_result = watcher.stop().await;
	assert!(stop_result.is_ok());
}

#[tokio::test]
async fn test_scheduled_job_execution_with_processing_error() {
	// Test that the scheduled job handles errors from process_new_blocks
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	// Use a very frequent schedule (every second)
	network.cron_schedule = "* * * * * *".to_string();

	let mut block_storage = MockBlockStorage::new();
	// Make get_last_processed_block fail
	block_storage
		.expect_get_last_processed_block()
		.with(predicate::always())
		.returning(|_| Err(anyhow::anyhow!("Storage error")))
		.times(1..=10);

	let block_storage = Arc::new(block_storage);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let trigger_handler = Arc::new(|_: &ProcessedBlock| tokio::spawn(async {}));

	// Use real BlockTracker since NetworkBlockWatcher::new requires it
	let block_tracker = Arc::new(BlockTracker::new(10));

	let mut watcher = NetworkBlockWatcher::<_, _, _, JobScheduler>::new(
		network.clone(),
		block_storage.clone(),
		block_handler,
		trigger_handler,
		block_tracker,
	)
	.await
	.unwrap();

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();

	// Expect clone to be called multiple times (when job is created and each time it runs)
	rpc_client.expect_clone().times(1..=20).returning(|| {
		let mut client = MockEvmClientTrait::<MockEVMTransportClient>::new();
		client
			.expect_get_latest_block_number()
			.returning(|| Ok(105))
			.times(0..=10);
		client
			.expect_clone()
			.times(0..=10)
			.returning(MockEvmClientTrait::<MockEVMTransportClient>::new);
		client
	});

	// Start the watcher
	let start_result = watcher.start(rpc_client).await;
	assert!(start_result.is_ok());

	// Wait for the job to execute and encounter the error (give it 2 seconds)
	tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

	// The watcher should still be running despite the error
	// (errors in the job don't stop the scheduler)
	let stop_result = watcher.stop().await;
	assert!(stop_result.is_ok());
}

#[tokio::test]
async fn test_duplicate_block_detection() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	// Create blocks with intentional duplicate (101, 102, 102, 103)
	// Note: In reality, the RPC shouldn't return duplicates, but this tests the detection logic
	let blocks = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 102),
		create_test_block(BlockChainType::EVM, 102), // duplicate
		create_test_block(BlockChainType::EVM, 103),
	];

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::eq(104))
		.returning(|_, _| Ok(()))
		.times(1);
	let block_storage = Arc::new(block_storage);

	let mut block_tracker = MockBlockTracker::default();
	// start_block = max(100 + 1, 105 - 1 - 50) = max(101, 54) = 101
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	// Configure detect_missing_blocks
	block_tracker
		.expect_detect_missing_blocks()
		.withf(|_, blocks: &[BlockType]| {
			let block_numbers: Vec<u64> = blocks.iter().filter_map(|b| b.number()).collect();
			block_numbers == vec![101, 102, 102, 103]
		})
		.returning(|_, _| Vec::new())
		.times(1);
	// Expect check_processed_block to be called for each block including the duplicate
	// Track call order to properly simulate duplicate detection
	let call_order = Arc::new(std::sync::Mutex::new(vec![]));
	let call_order_clone = call_order.clone();
	block_tracker
		.expect_check_processed_block()
		.withf(move |_, num: &u64| {
			let mut order = call_order_clone.lock().unwrap();
			order.push(*num);
			true
		})
		.returning(move |_, num| {
			let order = call_order.lock().unwrap();
			// Check if this block number has been seen before
			let count = order.iter().filter(|&&x| x == num).count();
			if count > 1 {
				BlockCheckResult::Duplicate { last_seen: num }
			} else {
				BlockCheckResult::Ok
			}
		})
		.times(4); // 101, 102, 102 (duplicate), 103

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(move |_, _| Ok(blocks.clone()))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	// Track triggered blocks to verify duplicate detection
	let triggered_blocks = Arc::new(tokio::sync::Mutex::new(Vec::new()));
	let trigger_handler = {
		let triggered_blocks = triggered_blocks.clone();
		Arc::new(move |block: &ProcessedBlock| {
			let triggered_blocks = triggered_blocks.clone();
			let block_number = block.block_number;
			tokio::spawn(async move {
				triggered_blocks.lock().await.push(block_number);
			})
		})
	};

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
		"Process should succeed even with duplicate blocks"
	);
}

#[tokio::test]
async fn test_missed_blocks_with_store_blocks_none() {
	let mut network = create_test_network("Test Network", "test-network", BlockChainType::EVM);
	network.store_blocks = None; // Test the None case specifically

	// Create blocks with gaps
	let blocks_with_gaps = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 103), // missing 102
	];

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);

	// save_missed_blocks should NOT be called when store_blocks is None
	block_storage.expect_save_missed_blocks().times(0);

	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::eq(104))
		.returning(|_, _| Ok(()))
		.times(1);

	let block_storage = Arc::new(block_storage);

	let mut block_tracker = MockBlockTracker::default();
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	block_tracker
		.expect_detect_missing_blocks()
		.returning(|_, _| vec![102]) // Missing block detected
		.times(1);

	for &block_num in &[101, 103] {
		block_tracker
			.expect_check_processed_block()
			.withf(move |_, num: &u64| *num == block_num)
			.returning(|_, _| BlockCheckResult::Ok)
			.times(1);
	}

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(move |_, _| Ok(blocks_with_gaps.clone()))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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
		"Process should succeed without saving missed blocks when store_blocks is None"
	);
}

#[tokio::test]
async fn test_empty_blocks_from_rpc_despite_having_new_blocks() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	// Even with no blocks processed, we still save last_processed_block
	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::eq(104))
		.returning(|_, _| Ok(()))
		.times(1);
	let block_storage = Arc::new(block_storage);

	let mut block_tracker = MockBlockTracker::default();
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	block_tracker
		.expect_detect_missing_blocks()
		.withf(|_, blocks: &[BlockType]| blocks.is_empty())
		.returning(|_, _| Vec::new())
		.times(1);

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	// RPC returns empty despite last_processed_block < latest_confirmed_block
	rpc_client
		.expect_get_blocks()
		.with(predicate::eq(101), predicate::eq(Some(104)))
		.returning(|_, _| Ok(vec![])) // Empty despite condition matching
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			ProcessedBlock {
				block_number,
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
		"Process should handle empty blocks from RPC gracefully"
	);
}

#[tokio::test]
async fn test_out_of_order_in_cleanup_phase() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	let blocks = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 102),
		create_test_block(BlockChainType::EVM, 103),
		create_test_block(BlockChainType::EVM, 100), // Out of order - arrives last
	];

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(99)))
		.times(1);
	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::eq(104))
		.returning(|_, _| Ok(()))
		.times(1);
	let block_storage = Arc::new(block_storage);

	let mut block_tracker = MockBlockTracker::default();
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 100)
		.returning(|_, _| ())
		.times(1);
	block_tracker
		.expect_detect_missing_blocks()
		.returning(|_, _| Vec::new())
		.times(1);

	// Track the order of calls to simulate cleanup phase detection
	let call_count = Arc::new(std::sync::Mutex::new(0));
	let call_count_clone = call_count.clone();

	block_tracker
		.expect_check_processed_block()
		.returning(move |_, num| {
			let mut count = call_count_clone.lock().unwrap();
			*count += 1;

			// First 3 calls (100, 101, 102) are Ok
			// Last call (103) detects out-of-order for block 100 in cleanup
			if *count <= 3 {
				BlockCheckResult::Ok
			} else if num == 100 {
				// Block 100 comes after 103, so it's out of order
				BlockCheckResult::OutOfOrder {
					expected: 104,
					received: 100,
				}
			} else {
				BlockCheckResult::Ok
			}
		})
		.times(4);

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(move |_, _| Ok(blocks.clone()))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);

			// Add delay for block 100 to ensure it's processed last and buffered
			if block_number == 100 {
				tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
			}

			ProcessedBlock {
				block_number,
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
		"Process should handle out-of-order blocks in cleanup phase"
	);
}

#[tokio::test]
async fn test_duplicate_detection_main_loop() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	// Process blocks with a duplicate - block 102 appears twice
	let blocks = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 102),
		create_test_block(BlockChainType::EVM, 103),
		create_test_block(BlockChainType::EVM, 102), // Duplicate - appears again
	];

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::eq(104))
		.returning(|_, _| Ok(()))
		.times(1);
	let block_storage = Arc::new(block_storage);

	let mut block_tracker = MockBlockTracker::default();
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	block_tracker
		.expect_detect_missing_blocks()
		.returning(|_, _| Vec::new())
		.times(1);

	// Track which blocks have been seen to detect duplicates
	let seen_blocks = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
	let seen_blocks_clone = seen_blocks.clone();

	block_tracker
		.expect_check_processed_block()
		.returning(move |_, num| {
			let mut seen = seen_blocks_clone.lock().unwrap();

			if seen.contains(&num) {
				// This is a duplicate
				BlockCheckResult::Duplicate { last_seen: num }
			} else {
				seen.insert(num);
				BlockCheckResult::Ok
			}
		})
		.times(4); // 101, 102, 103, 102 (duplicate)

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(move |_, _| Ok(blocks.clone()))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			// Add delay for the duplicate block to ensure it arrives after the original blocks
			if block_number == 102 {
				// Check if this is the duplicate by checking if we've already seen it
				// We'll add a delay to ensure it processes after the original sequence
				tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
			}
			ProcessedBlock {
				block_number,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let triggered_blocks = Arc::new(tokio::sync::Mutex::new(Vec::new()));
	let trigger_handler = {
		let triggered_blocks = triggered_blocks.clone();
		Arc::new(move |block: &ProcessedBlock| {
			let triggered_blocks = triggered_blocks.clone();
			let block_number = block.block_number;
			tokio::spawn(async move {
				triggered_blocks.lock().await.push(block_number);
			})
		})
	};

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
		"Process should succeed even with duplicate detection in main loop"
	);

	// All blocks should still be triggered despite duplicate detection
	// The duplicate block 102 will still be triggered even though it's a duplicate
	let final_order = triggered_blocks.lock().await;
	assert_eq!(*final_order, vec![101, 102, 103, 102]);
}

#[tokio::test]
async fn test_out_of_order_detection_main_loop() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	// Blocks arrive out of order: 101, 103, 104, then 102 (which will be out-of-order)
	let blocks = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 103),
		create_test_block(BlockChainType::EVM, 104),
		create_test_block(BlockChainType::EVM, 102),
	];

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::eq(104))
		.returning(|_, _| Ok(()))
		.times(1);
	let block_storage = Arc::new(block_storage);

	let mut block_tracker = MockBlockTracker::default();
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	block_tracker
		.expect_detect_missing_blocks()
		.returning(|_, _| Vec::new())
		.times(1);

	// The trigger phase processes blocks sequentially, so even if blocks arrive out of order
	// from processing, they'll be buffered and triggered in order. The test verifies that
	// out-of-order detection works when blocks arrive late from processing.
	// However, since the trigger phase ensures sequential processing, block 102 will be
	// checked when expected_next is 102, so it will be Ok, not OutOfOrder.
	// To test OutOfOrder detection, we need to simulate a scenario where the tracker's
	// expected_next has advanced beyond the block being checked.

	// Track which blocks have been checked to simulate tracker state
	let checked_blocks = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
	let checked_clone = checked_blocks.clone();

	// Simulate tracker state: if blocks 103 and 104 are checked before 102,
	// the tracker's expected_next would be 105, making 102 appear out-of-order
	// But in reality, the trigger phase ensures sequential checking, so this scenario
	// tests the robustness of the code when tracker state might be inconsistent.

	// First block (101) is Ok
	block_tracker
		.expect_check_processed_block()
		.withf(|_, num| *num == 101)
		.returning(move |_, _| {
			checked_clone.lock().unwrap().insert(101);
			BlockCheckResult::Ok
		})
		.times(1);

	// Block 102 is checked - but simulate that tracker thinks 103 and 104 were already seen
	// This tests the OutOfOrder detection logic
	let checked_clone2 = checked_blocks.clone();
	block_tracker
		.expect_check_processed_block()
		.withf(|_, num| *num == 102)
		.returning(move |_, _| {
			// Simulate that tracker's expected_next is 105 (blocks 103, 104 were already processed)
			checked_clone2.lock().unwrap().insert(102);
			BlockCheckResult::OutOfOrder {
				expected: 105,
				received: 102,
			}
		})
		.times(1);

	// Block 103 is Ok
	let checked_clone3 = checked_blocks.clone();
	block_tracker
		.expect_check_processed_block()
		.withf(|_, num| *num == 103)
		.returning(move |_, _| {
			checked_clone3.lock().unwrap().insert(103);
			BlockCheckResult::Ok
		})
		.times(1);

	// Block 104 is Ok
	let checked_clone4 = checked_blocks.clone();
	block_tracker
		.expect_check_processed_block()
		.withf(|_, num| *num == 104)
		.returning(move |_, _| {
			checked_clone4.lock().unwrap().insert(104);
			BlockCheckResult::Ok
		})
		.times(1);

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(move |_, _| Ok(blocks.clone()))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);
			// Add delay for block 102 to ensure it arrives after 103 and 104
			if block_number == 102 {
				tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
			}
			ProcessedBlock {
				block_number,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let triggered_blocks = Arc::new(tokio::sync::Mutex::new(Vec::new()));
	let trigger_handler = {
		let triggered_blocks = triggered_blocks.clone();
		Arc::new(move |block: &ProcessedBlock| {
			let triggered_blocks = triggered_blocks.clone();
			let block_number = block.block_number;
			tokio::spawn(async move {
				triggered_blocks.lock().await.push(block_number);
			})
		})
	};

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
		"Process should succeed even with out-of-order detection in main loop"
	);

	// All blocks should still be triggered despite out-of-order detection
	// The trigger phase processes blocks sequentially, so they're triggered in order
	// even if they arrive out of order from processing: 101, 102 (detected as out-of-order), 103, 104
	let final_order = triggered_blocks.lock().await;
	assert_eq!(*final_order, vec![101, 102, 103, 104]);
}

#[tokio::test]
async fn test_duplicate_detection_in_cleanup_phase() {
	let network = create_test_network("Test Network", "test-network", BlockChainType::EVM);

	let blocks = vec![
		create_test_block(BlockChainType::EVM, 101),
		create_test_block(BlockChainType::EVM, 102),
		create_test_block(BlockChainType::EVM, 103),
		create_test_block(BlockChainType::EVM, 102), // Duplicate - arrives last
	];

	let mut block_storage = MockBlockStorage::new();
	block_storage
		.expect_get_last_processed_block()
		.returning(|_| Ok(Some(100)))
		.times(1);
	block_storage
		.expect_save_last_processed_block()
		.with(predicate::always(), predicate::eq(104))
		.returning(|_, _| Ok(()))
		.times(1);
	let block_storage = Arc::new(block_storage);

	let mut block_tracker = MockBlockTracker::default();
	block_tracker
		.expect_reset_expected_next()
		.withf(|_, block: &u64| *block == 101)
		.returning(|_, _| ())
		.times(1);
	block_tracker
		.expect_detect_missing_blocks()
		.returning(|_, _| Vec::new())
		.times(1);

	// Track which blocks have been seen to detect duplicates
	let seen_blocks = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
	let seen_blocks_clone = seen_blocks.clone();

	block_tracker
		.expect_check_processed_block()
		.returning(move |_, num| {
			let mut seen = seen_blocks_clone.lock().unwrap();

			if seen.contains(&num) {
				// This is a duplicate
				BlockCheckResult::Duplicate { last_seen: num }
			} else {
				seen.insert(num);
				BlockCheckResult::Ok
			}
		})
		.times(4); // 101, 102, 103, 102 (duplicate)

	let mut rpc_client = MockEvmClientTrait::<MockEVMTransportClient>::new();
	rpc_client
		.expect_get_latest_block_number()
		.returning(|| Ok(105))
		.times(1);
	rpc_client
		.expect_get_blocks()
		.returning(move |_, _| Ok(blocks.clone()))
		.times(1);

	let block_handler = Arc::new(|block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);

			// Add delay for the duplicate block to ensure it's processed last and enters cleanup
			if block_number == 102 {
				tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
			}

			ProcessedBlock {
				block_number,
				network_slug: network.slug,
				processing_results: vec![],
			}
		}) as BoxFuture<'static, ProcessedBlock>
	});

	let triggered_blocks = Arc::new(tokio::sync::Mutex::new(Vec::new()));
	let trigger_handler = {
		let triggered_blocks = triggered_blocks.clone();
		Arc::new(move |block: &ProcessedBlock| {
			let triggered_blocks = triggered_blocks.clone();
			let block_number = block.block_number;
			tokio::spawn(async move {
				triggered_blocks.lock().await.push(block_number);
			})
		})
	};

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
		"Process should succeed even with duplicate blocks in cleanup phase"
	);
}
