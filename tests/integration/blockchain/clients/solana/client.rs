use crate::integration::mocks::{MockSolanaClientTrait, MockSolanaTransportClient};
use mockall::predicate;
use openzeppelin_monitor::{
	models::{
		BlockType, SolanaBlock, SolanaConfirmedBlock, SolanaTransaction, SolanaTransactionInfo,
	},
	services::blockchain::{BlockChainClient, SignatureInfo, SolanaClientTrait},
};

#[tokio::test]
async fn test_get_transaction() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();
	let expected_transaction = SolanaTransaction::from(SolanaTransactionInfo {
		signature: "5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
			.to_string(),
		slot: 123456789,
		block_time: Some(1234567890),
		transaction: Default::default(),
		meta: None,
	});

	mock.expect_get_transaction()
		.with(predicate::eq(
			"5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F".to_string(),
		))
		.times(1)
		.returning(move |_| Ok(Some(expected_transaction.clone())));

	let result = mock
		.get_transaction(
			"5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F".to_string(),
		)
		.await;
	assert!(result.is_ok());
	let tx = result.unwrap();
	assert!(tx.is_some());
	assert_eq!(
		tx.unwrap().signature(),
		"5wHu1qwD7q5ifaN5nwdcDqNFF53GJqa7nLp2BLPASe7FPYoWZL3YBrJmVL6nrMtwKjNFin1F"
	);
}

#[tokio::test]
async fn test_get_transaction_not_found() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	mock.expect_get_transaction()
		.with(predicate::eq("invalid_signature".to_string()))
		.times(1)
		.returning(move |_| Ok(None));

	let result = mock.get_transaction("invalid_signature".to_string()).await;
	assert!(result.is_ok());
	assert!(result.unwrap().is_none());
}

#[tokio::test]
async fn test_get_transactions() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();
	let expected_transaction = SolanaTransaction::from(SolanaTransactionInfo {
		signature: "test_sig".to_string(),
		slot: 123456789,
		block_time: Some(1234567890),
		transaction: Default::default(),
		meta: None,
	});

	mock.expect_get_transactions()
		.with(predicate::eq(123456789u64))
		.times(1)
		.returning(move |_| Ok(vec![expected_transaction.clone()]));

	let result = mock.get_transactions(123456789).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_signatures_for_address_with_info() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	let sig_info1 = SignatureInfo {
		signature: "sig1".to_string(),
		slot: 123456789,
		err: None,
		block_time: Some(1234567890),
	};
	let sig_info2 = SignatureInfo {
		signature: "sig2".to_string(),
		slot: 123456790,
		err: None,
		block_time: Some(1234567891),
	};

	mock.expect_get_signatures_for_address_with_info()
		.with(
			predicate::eq("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string()),
			predicate::eq(Some(1000)),
			predicate::eq(Some(123456789)),
			predicate::eq(None::<String>),
		)
		.times(1)
		.returning(move |_, _, _, _| Ok(vec![sig_info1.clone(), sig_info2.clone()]));

	let result = mock
		.get_signatures_for_address_with_info(
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
			Some(1000),
			Some(123456789),
			None,
		)
		.await;
	assert!(result.is_ok());
	let sigs = result.unwrap();
	assert_eq!(sigs.len(), 2);
	assert_eq!(sigs[0].signature, "sig1");
	assert_eq!(sigs[1].signature, "sig2");
}

#[tokio::test]
async fn test_get_all_signatures_for_address_pagination() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	// Simulate pagination with multiple calls
	let sig_info1 = SignatureInfo {
		signature: "sig1".to_string(),
		slot: 123456789,
		err: None,
		block_time: Some(1234567890),
	};
	let sig_info2 = SignatureInfo {
		signature: "sig2".to_string(),
		slot: 123456790,
		err: None,
		block_time: Some(1234567891),
	};

	mock.expect_get_all_signatures_for_address()
		.with(
			predicate::eq("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string()),
			predicate::eq(123456789u64),
			predicate::eq(123456790u64),
		)
		.times(1)
		.returning(move |_, _, _| Ok(vec![sig_info1.clone(), sig_info2.clone()]));

	let result = mock
		.get_all_signatures_for_address(
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
			123456789,
			123456790,
		)
		.await;
	assert!(result.is_ok());
	let sigs = result.unwrap();
	assert_eq!(sigs.len(), 2);
}

#[tokio::test]
async fn test_get_transactions_for_addresses() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	let expected_transaction = SolanaTransaction::from(SolanaTransactionInfo {
		signature: "test_sig".to_string(),
		slot: 123456789,
		block_time: Some(1234567890),
		transaction: Default::default(),
		meta: None,
	});

	mock.expect_get_transactions_for_addresses()
		.with(
			predicate::eq(vec![
				"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string()
			]),
			predicate::eq(123456789u64),
			predicate::eq(Some(123456790u64)),
		)
		.times(1)
		.returning(move |_, _, _| Ok(vec![expected_transaction.clone()]));

	let result = mock
		.get_transactions_for_addresses(
			&["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string()],
			123456789,
			Some(123456790),
		)
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_latest_block_number() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();
	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Ok(123456789u64));

	let result = mock.get_latest_block_number().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 123456789u64);
}

#[tokio::test]
async fn test_get_blocks_standard_mode() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	let block = BlockType::Solana(Box::new(SolanaBlock::from(SolanaConfirmedBlock {
		slot: 123456789,
		blockhash: "ABC123".to_string(),
		previous_blockhash: "ZYX987".to_string(),
		parent_slot: 123456788,
		block_time: Some(1234567890),
		block_height: Some(123456789),
		transactions: vec![],
	})));

	let blocks = vec![block];

	mock.expect_get_blocks()
		.with(
			predicate::eq(123456789u64),
			predicate::eq(Some(123456790u64)),
		)
		.times(1)
		.returning(move |_, _| Ok(blocks.clone()));

	let result = mock.get_blocks(123456789, Some(123456790)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
	match &blocks[0] {
		BlockType::Solana(block) => assert_eq!(block.slot, 123456789),
		_ => panic!("Expected Solana block"),
	}
}

#[tokio::test]
async fn test_get_blocks_for_addresses_optimized() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	let block = BlockType::Solana(Box::new(SolanaBlock::from(SolanaConfirmedBlock {
		slot: 123456789,
		blockhash: String::new(),
		previous_blockhash: String::new(),
		parent_slot: 123456788,
		block_time: Some(1234567890),
		block_height: None,
		transactions: vec![],
	})));

	let blocks = vec![block];

	mock.expect_get_blocks_for_addresses()
		.with(
			predicate::eq(vec![
				"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string()
			]),
			predicate::eq(123456789u64),
			predicate::eq(Some(123456790u64)),
		)
		.times(1)
		.returning(move |_, _, _| Ok(blocks.clone()));

	let result = SolanaClientTrait::get_blocks_for_addresses(
		&mock,
		&["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string()],
		123456789,
		Some(123456790),
	)
	.await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
}

#[tokio::test]
async fn test_get_account_info() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	let account_info = serde_json::json!({
		"lamports": 1000000000,
		"owner": "11111111111111111111111111111111",
		"data": ["", "base64"],
		"executable": false,
		"rentEpoch": 361
	});

	mock.expect_get_account_info()
		.with(predicate::eq(
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
		))
		.times(1)
		.returning(move |_| Ok(account_info.clone()));

	let result = mock
		.get_account_info("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string())
		.await;
	assert!(result.is_ok());
}

#[tokio::test]
async fn test_get_program_accounts() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	let program_accounts = vec![
		serde_json::json!({"pubkey": "account1", "account": {}}),
		serde_json::json!({"pubkey": "account2", "account": {}}),
	];

	mock.expect_get_program_accounts()
		.with(predicate::eq(
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
		))
		.times(1)
		.returning(move |_| Ok(program_accounts.clone()));

	let result = mock
		.get_program_accounts("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string())
		.await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 2);
}

#[tokio::test]
async fn test_error_handling_invalid_slot() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	mock.expect_get_transactions()
		.with(predicate::eq(999999999u64))
		.times(1)
		.returning(move |_| Err(anyhow::anyhow!("Slot not available")));

	let result = mock.get_transactions(999999999).await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Slot not available"));
}

#[tokio::test]
async fn test_error_handling_network_failure() {
	let mut mock = MockSolanaClientTrait::<MockSolanaTransportClient>::new();

	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Err(anyhow::anyhow!("Network connection failed")));

	let result = mock.get_latest_block_number().await;
	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Network connection failed"));
}
