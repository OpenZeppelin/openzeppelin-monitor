use futures::future::BoxFuture;
use mockall::predicate;
use std::sync::Arc;

use crate::integration::mocks::{
	create_test_block, create_test_network, MockBlockStorage, MockBlockTracker, MockEvmClientTrait,
	MockWeb3TransportClient,
};
use openzeppelin_monitor::{
	models::{BlockChainType, BlockType, Network, ProcessedBlock},
	services::blockwatcher::{process_new_blocks, BlockTrackerTrait, BlockWatcherError},
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
	history_size: usize,
}

/// Helper function to setup mock implementations with configurable expectations
fn setup_mocks(
	config: MockConfig,
) -> (
	Arc<MockBlockStorage>,
	MockBlockTracker<MockBlockStorage>,
	MockEvmClientTrait<MockWeb3TransportClient>,
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

	let block_tracker_arc = Arc::new(block_tracker);

	// Process blocks
	process_new_blocks(
		&network,
		&rpc_client,
		block_storage.clone(),
		block_handler,
		trigger_handler,
		block_tracker_arc,
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
async fn test_concurrent_processing() -> Result<(), BlockWatcherError> {
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
		history_size: 50,
	};

	let (block_storage, block_tracker, rpc_client) = setup_mocks(config);

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

	Ok(())
}

#[tokio::test]
async fn test_ordered_trigger_handling() -> Result<(), BlockWatcherError> {
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
		history_size: 10,
	};

	let (block_storage, block_tracker, rpc_client) = setup_mocks(config);

	// Track the order of triggered blocks
	let triggered_blocks = Arc::new(tokio::sync::Mutex::new(Vec::new()));

	// Create block handler that processes blocks with varying delays
	let block_handler = Arc::new(move |block: BlockType, network: Network| {
		Box::pin(async move {
			let block_number = block.number().unwrap_or(0);

			// Intentionally delay processing of even-numbered blocks
			if block_number % 2 == 0 {
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

	Ok(())
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
async fn test_max_past_blocks_limit() -> Result<(), BlockWatcherError> {
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
		"Block processing should succeed with max_past_blocks limit"
	);
	Ok(())
}

#[tokio::test]
async fn test_max_past_blocks_limit_recommended() -> Result<(), BlockWatcherError> {
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

	Ok(())
}

#[tokio::test]
async fn test_confirmation_blocks() -> Result<(), BlockWatcherError> {
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
		history_size: 10,
	};

	let (block_storage, block_tracker, rpc_client) = setup_mocks(config);

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

	Ok(())
}
