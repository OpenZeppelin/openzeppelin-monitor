use crate::integration::mocks::{
	create_stellar_test_network_with_urls, create_stellar_valid_server_mock_network_response,
	MockStellarClientTrait, MockStellarTransportClient,
};
use mockall::predicate;
use mockito::Server;
use openzeppelin_monitor::{
	models::{
		BlockType, ContractSpec, StellarBlock, StellarEvent, StellarFormattedContractSpec,
		StellarLedgerInfo, StellarTransaction, StellarTransactionInfo,
	},
	services::blockchain::{BlockChainClient, StellarClient, StellarClientTrait},
};
use serde_json::json;

#[tokio::test]
async fn test_get_transactions() {
	let mut mock = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let expected_transaction = StellarTransaction::from(StellarTransactionInfo {
		status: "SUCCESS".to_string(),
		transaction_hash: "test_hash".to_string(),
		..Default::default()
	});

	mock.expect_get_transactions()
		.with(predicate::eq(1u32), predicate::eq(Some(2u32)))
		.times(1)
		.returning(move |_, _| Ok(vec![expected_transaction.clone()]));

	let result = mock.get_transactions(1, Some(2)).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_events() {
	let mut mock = MockStellarClientTrait::<MockStellarTransportClient>::new();
	let expected_event = StellarEvent {
		..Default::default()
	};

	mock.expect_get_events()
		.with(predicate::eq(1u32), predicate::eq(Some(2u32)))
		.times(1)
		.returning(move |_, _| Ok(vec![expected_event.clone()]));

	let result = mock.get_events(1, Some(2)).await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap().len(), 1);
}

#[tokio::test]
async fn test_get_latest_block_number() {
	let mut mock = MockStellarClientTrait::<MockStellarTransportClient>::new();
	mock.expect_get_latest_block_number()
		.times(1)
		.returning(|| Ok(100u64));

	let result = mock.get_latest_block_number().await;
	assert!(result.is_ok());
	assert_eq!(result.unwrap(), 100u64);
}

#[tokio::test]
async fn test_get_blocks() {
	let mut mock = MockStellarClientTrait::<MockStellarTransportClient>::new();

	let block = BlockType::Stellar(Box::new(StellarBlock::from(StellarLedgerInfo {
		sequence: 1,
		..Default::default()
	})));

	let blocks = vec![block];

	mock.expect_get_blocks()
		.with(predicate::eq(1u64), predicate::eq(Some(2u64)))
		.times(1)
		.returning(move |_, _| Ok(blocks.clone()));

	let result = mock.get_blocks(1, Some(2)).await;
	assert!(result.is_ok());
	let blocks = result.unwrap();
	assert_eq!(blocks.len(), 1);
	match &blocks[0] {
		BlockType::Stellar(block) => assert_eq!(block.sequence, 1),
		_ => panic!("Expected Stellar block"),
	}
}

#[tokio::test]
async fn test_new_client() {
	let mut server = Server::new_async().await;

	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	// Create a test network
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Test successful client creation
	let result = StellarClient::new(&network).await;
	assert!(result.is_ok(), "Client creation should succeed");
	mock.assert();
}

#[tokio::test]
async fn test_get_transactions_pagination() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Mock first request (current_iteration == 0)
	let first_response = json!({
		"result": {
			"transactions": [
				{
				"status": "SUCCESS",
					"txHash": "1723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d",
					"applicationOrder": 1,
					"feeBump": false,
					"envelopeXdr": "CCC",
					"resultXdr": "BBB",
					"resultMetaXdr": "AAA",
					"ledger": 1,
					"createdAt": 1735440610
				}
			],
			"cursor": "next_page"
		}
	});

	// Mock second request (with cursor)
	let second_response = json!({
		"result": {
			"transactions": [
				{
					"status": "SUCCESS",
					"txHash": "2723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d",
					"applicationOrder": 1,
					"feeBump": false,
					"envelopeXdr": "CCC",
					"resultXdr": "BBB=",
					"resultMetaXdr": "AAA",
					"ledger": 2,
					"createdAt": 1735440610
				}
			],
			"cursor": null
		}
	});

	let first_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(first_response.to_string())
		.create_async()
		.await;

	let second_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(second_response.to_string())
		.create_async()
		.await;

	let client = StellarClient::new(&network).await.unwrap();
	let result = client.get_transactions(1, Some(2)).await.unwrap();

	assert_eq!(result.len(), 2);
	assert_eq!(
		result[0].transaction_hash,
		"1723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d"
	);
	assert_eq!(
		result[1].transaction_hash,
		"2723ef4c6f11aba528eea5b0cd57676a651333bfd57c2fead949999a3183304d"
	);

	mock.assert_async().await;
	first_mock.assert_async().await;
	second_mock.assert_async().await;
}

#[tokio::test]
async fn test_get_events_pagination() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Mock first request (current_iteration == 0)
	let first_response = json!({
		"result": {
			"events": [
				{
					"type": "contract",
					"ledger": 1,
					"ledgerClosedAt": "2024-12-29T02:50:10Z",
					"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
					"id": "0001364073023291392-0000000001",
					"pagingToken": "0001364073023291392-0000000001",
					"inSuccessfulContractCall": true,
					"txHash": "5a7bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d8",
					"topic": [
					  "AAAADwAAAA9jb250cmFjdF9jYWxsZWQA",
					  "AAAAEgAAAAAAAAAACMEAtVPau/0s+2y4o3aWt1MAtjmdqWNzPmy6MRVcdfo=",
					  "AAAADgAAAAlnYW5hY2hlLTAAAAA=",
					  "AAAADgAAACoweDY4QjkzMDQ1ZmU3RDg3OTRhN2NBRjMyN2U3Zjg1NUNENkNkMDNCQjgAAA==",
					  "AAAADQAAACAaemkIzyqB6sH3VVev7iSjYHderf04InYUVZQLYhCsdg=="
					],
					"value": "AAA"
				}
			],
			"cursor": "next_page"
		}
	});

	// Mock second request (with cursor)
	let second_response = json!({
		"result": {
			"events": [
				{
					"type": "contract",
					"ledger": 2,
					"ledgerClosedAt": "2024-12-29T02:50:10Z",
					"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
					"id": "0001364073023291392-0000000001",
					"pagingToken": "0001364073023291392-0000000001",
					"inSuccessfulContractCall": true,
					"txHash": "5a7bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d8",
					"topic": [
					  "AAAADwAAAA9jb250cmFjdF9jYWxsZWQA",
					  "AAAAEgAAAAAAAAAACMEAtVPau/0s+2y4o3aWt1MAtjmdqWNzPmy6MRVcdfo=",
					  "AAAADgAAAAlnYW5hY2hlLTAAAAA=",
					  "AAAADgAAACoweDY4QjkzMDQ1ZmU3RDg3OTRhN2NBRjMyN2U3Zjg1NUNENkNkMDNCQjgAAA==",
					  "AAAADQAAACAaemkIzyqB6sH3VVev7iSjYHderf04InYUVZQLYhCsdg=="
					],
					"value": "AAA"
				}
			],
			"cursor": null
		}
	});

	let first_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(first_response.to_string())
		.create_async()
		.await;

	let second_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(second_response.to_string())
		.create_async()
		.await;

	let client = StellarClient::new(&network).await.unwrap();
	let result = client.get_events(1, Some(2)).await.unwrap();

	assert_eq!(result.len(), 2);
	assert_eq!(result[0].ledger, 1);
	assert_eq!(result[1].ledger, 2);

	mock.assert_async().await;
	first_mock.assert_async().await;
	second_mock.assert_async().await;
}

#[tokio::test]
async fn test_get_blocks_pagination() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Mock first request (current_iteration == 0)
	let first_response = json!({
		"result": {
			"ledgers": [
				{
					"hash": "eeb74bcdfd4de1a0b2753ef37ed76a5f696a6f22d5be68b4d7db7a972b728c8f",
					"sequence": 1,
					"ledgerCloseTime": "1734715051",
					"headerXdr": "AAA",
					"metadataXdr": "BBB"
				}
			],
			"cursor": "next_page"
		}
	});

	// Mock second request (with cursor)
	let second_response = json!({
		"result": {
			"ledgers": [
				{
					"hash": "eeb74bcdfd4de1a0b2753ef37ed76a5f696a6f22d5be68b4d7db7a972b728c8f",
					"sequence": 2,
					"ledgerCloseTime": "1734715051",
					"headerXdr": "AAA",
					"metadataXdr": "BBB"
				}
			],
			"cursor": null
		}
	});

	let first_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(first_response.to_string())
		.create_async()
		.await;

	let second_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(second_response.to_string())
		.create_async()
		.await;

	let client = StellarClient::new(&network).await.unwrap();
	let result = client.get_blocks(1, Some(2)).await.unwrap();

	assert_eq!(result.len(), 2);
	match &result[0] {
		BlockType::Stellar(block) => assert_eq!(block.sequence, 1),
		_ => panic!("Expected Stellar block"),
	}
	match &result[1] {
		BlockType::Stellar(block) => assert_eq!(block.sequence, 2),
		_ => panic!("Expected Stellar block"),
	}

	mock.assert_async().await;
	first_mock.assert_async().await;
	second_mock.assert_async().await;
}

#[tokio::test]
async fn test_get_contract_spec() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	let contract_data_xdr = "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";
	let contract_code_xdr = "AAAABwAAAAEAAAAAAAAAAAAAAEAAAAAFAAAAAwAAAAAAAAAEAAAAAAAAAAAAAAAEAAAABQAAAAAK2r5DjlOc9ad6/YGX+OJcgiyi0nupnY4OMbgLdADJAwAAAkYAYXNtAQAAAAEVBGACfn4BfmADfn5+AX5gAAF+YAAAAhkEAWwBMAAAAWwBMQAAAWwBXwABAWwBOAAAAwYFAgIDAwMFAwEAEAYZA38BQYCAwAALfwBBgIDAAAt/AEGAgMAACwc1BQZtZW1vcnkCAAlpbmNyZW1lbnQABQFfAAgKX19kYXRhX2VuZAMBC19faGVhcF9iYXNlAwIKpAEFCgBCjrrQr4bUOQuFAQIBfwJ+QQAhAAJAAkACQBCEgICAACIBQgIQgICAgABCAVINACABQgIQgYCAgAAiAkL/AYNCBFINASACQiCIpyEACyAAQQFqIgBFDQEgASAArUIghkIEhCICQgIQgoCAgAAaQoSAgICgBkKEgICAwAwQg4CAgAAaIAIPCwALEIaAgIAAAAsJABCHgICAAAALAwAACwIACwBzDmNvbnRyYWN0c3BlY3YwAAAAAAAAAEBJbmNyZW1lbnQgaW5jcmVtZW50cyBhbiBpbnRlcm5hbCBjb3VudGVyLCBhbmQgcmV0dXJucyB0aGUgdmFsdWUuAAAACWluY3JlbWVudAAAAAAAAAAAAAABAAAABAAeEWNvbnRyYWN0ZW52bWV0YXYwAAAAAAAAABYAAAAAAG8OY29udHJhY3RtZXRhdjAAAAAAAAAABXJzdmVyAAAAAAAABjEuODYuMAAAAAAAAAAAAAhyc3Nka3ZlcgAAAC8yMi4wLjcjMjExNTY5YWE0OWM4ZDg5Njg3N2RmY2ExZjJlYjRmZTkwNzExMjFjOAAAAA==";

	// Mock first response for contract instance
	let instance_response = json!({
		"result": {
			"entries": [{
				"xdr": contract_data_xdr
			}]
		}
	});

	// Mock second response for contract code
	let code_response = json!({
		"result": {
			"entries": [{
				"xdr": contract_code_xdr
			}]
		}
	});

	let instance_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(instance_response.to_string())
		.create_async()
		.await;

	let code_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_body(code_response.to_string())
		.create_async()
		.await;

	let client = StellarClient::new(&network).await.unwrap();
	let result = client
		.get_contract_spec("CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK")
		.await;

	assert!(result.is_ok(), "Should successfully get contract spec");
	let spec = result.unwrap();
	match spec {
		ContractSpec::Stellar(spec) => {
			let stellar_spec = StellarFormattedContractSpec::from(spec);
			assert!(
				!stellar_spec.functions.is_empty(),
				"Contract spec should have at least one function"
			);
		}
		_ => panic!("Expected Stellar contract spec"),
	}

	mock.assert_async().await;
	instance_mock.assert_async().await;
	code_mock.assert_async().await;
}

#[tokio::test]
async fn test_get_contract_spec_invalid_response() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);
	let contract_data_xdr = "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";

	// Mock invalid response
	let invalid_response = json!({
		"result": {
			"entries": []
		}
	});

	let valid_response = json!({
		"result": {
			"entries": [{
				"xdr": contract_data_xdr
			}]
		}
	});

	// First return invalid response for contract data
	let contract_invalid_data_mock = server
		.mock("POST", "/")
		.with_status(200)
		.expect(1)
		.with_body(invalid_response.to_string())
		.create_async()
		.await;

	// Then return valid response for contract data
	let contract_valid_data_mock = server
		.mock("POST", "/")
		.with_status(200)
		.expect(1)
		.with_body(valid_response.to_string())
		.create_async()
		.await;

	// Then return invalid response for contract code
	let contract_invalid_code_mock = server
		.mock("POST", "/")
		.with_status(200)
		.expect(1)
		.with_body(invalid_response.to_string())
		.create_async()
		.await;

	let client = StellarClient::new(&network).await.unwrap();
	let result = client
		.get_contract_spec("CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK")
		.await;

	assert!(result.is_err(), "Should fail with invalid contract spec");
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get contract data XDR"));

	mock.assert_async().await;
	contract_invalid_data_mock.assert_async().await;

	let result = client
		.get_contract_spec("CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK")
		.await;

	assert!(result.is_err(), "Should fail with invalid contract spec");
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to get contract code XDR"));
	contract_valid_data_mock.assert_async().await;
	contract_invalid_code_mock.assert_async().await;

	// Failed to get contract code XDR
}

#[tokio::test]
async fn test_get_events_sparse_pagination() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Define all responses in sequence
	let responses = vec![
		// First request - empty events but has cursor
		json!({
			"result": {
				"events": [], // Empty due to LedgerScanLimit
				"cursor": "page_2_cursor"
			}
		}),
		// Second request - empty events again but has cursor
		json!({
			"result": {
				"events": [], // Empty again due to sparse events
				"cursor": "page_3_cursor"
			}
		}),
		// Third request - empty events again but has cursor
		json!({
			"result": {
				"events": [], // Empty again due to sparse events
				"cursor": "page_4_cursor"
			}
		}),
		// Fourth request - finally returns some events
		json!({
			"result": {
				"events": [
					{
						"type": "contract",
						"ledger": 100,
						"ledgerClosedAt": "2024-12-29T02:50:10Z",
						"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
						"id": "0001364073023291392-0000000001",
						"pagingToken": "0001364073023291392-0000000001",
						"inSuccessfulContractCall": true,
						"txHash": "5a7bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d8",
						"topic": [
						  "AAAADwAAAA9jb250cmFjdF9jYWxsZWQA"
						],
						"value": "AAA"
					}
				],
				"cursor": "page_5_cursor"
			}
		}),
		// Fifth request - another event
		json!({
			"result": {
				"events": [
					{
						"type": "contract",
						"ledger": 105,
						"ledgerClosedAt": "2024-12-29T02:50:15Z",
						"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
						"id": "0001364073023291393-0000000002",
						"pagingToken": "0001364073023291393-0000000002",
						"inSuccessfulContractCall": true,
						"txHash": "6b8cf297f2db4bc67089de60985bbf5a7d4e1e7b5672cd92e01680c0fff261e9",
						"topic": [
						  "AAAADwAAAA9jb250cmFjdF9jYWxsZWQA"
						],
						"value": "BBB"
					}
				],
				"cursor": "page_6_cursor"
			}
		}),
		// Sixth request - empty events with no cursor (end)
		json!({
			"result": {
				"events": [], // Empty and no more pages
				"cursor": null
			}
		}),
	];

	let mut mocks = Vec::new();
	for response in &responses {
		let mock = server
			.mock("POST", "/")
			.with_status(200)
			.with_body(response.to_string())
			.create_async()
			.await;
		mocks.push(mock);
	}

	let client = StellarClient::new(&network).await.unwrap();
	let result = client.get_events(1, Some(150)).await.unwrap();

	// Should find 2 events despite empty intermediate pages
	assert_eq!(result.len(), 2);
	assert_eq!(result[0].ledger, 100);
	assert_eq!(
		result[0].contract_id,
		"CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK"
	);
	assert_eq!(result[1].ledger, 105);
	assert_eq!(
		result[1].contract_id,
		"CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK"
	);

	mock.assert_async().await;
	for mock in mocks {
		mock.assert_async().await;
	}
}

#[tokio::test]
async fn test_get_events_complex_sparse_pagination_with_boundaries() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server);
	let network = create_stellar_test_network_with_urls(vec![&server.url()]);

	// Define all responses in sequence - testing complex scenarios
	let responses = vec![
		// Page 1: Empty start
		json!({
			"result": {
				"events": [],
				"cursor": "page_2_cursor"
			}
		}),
		// Page 2: Event exactly at start_sequence (should be included)
		json!({
			"result": {
				"events": [
					{
						"type": "contract",
						"ledger": 10, // Exactly at start_sequence
						"ledgerClosedAt": "2024-12-29T02:50:10Z",
						"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
						"id": "0001364073023291390-0000000001",
						"pagingToken": "0001364073023291390-0000000001",
						"inSuccessfulContractCall": true,
						"txHash": "1a1bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d1",
						"topic": ["AAAADwAAAA9jb250cmFjdF9jYWxsZWQA"],
						"value": "START"
					}
				],
				"cursor": "page_3_cursor"
			}
		}),
		// Page 3: Multiple sparse empty pages
		json!({
			"result": {
				"events": [],
				"cursor": "page_4_cursor"
			}
		}),
		json!({
			"result": {
				"events": [],
				"cursor": "page_5_cursor"
			}
		}),
		json!({
			"result": {
				"events": [],
				"cursor": "page_6_cursor"
			}
		}),
		// Page 6: Multiple events in one page (dense region)
		json!({
			"result": {
				"events": [
					{
						"type": "contract",
						"ledger": 45,
						"ledgerClosedAt": "2024-12-29T02:50:20Z",
						"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
						"id": "0001364073023291391-0000000001",
						"pagingToken": "0001364073023291391-0000000001",
						"inSuccessfulContractCall": true,
						"txHash": "2b2bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d2",
						"topic": ["AAAADwAAAA9jb250cmFjdF9jYWxsZWQA"],
						"value": "MID1"
					},
					{
						"type": "contract",
						"ledger": 47,
						"ledgerClosedAt": "2024-12-29T02:50:22Z",
						"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
						"id": "0001364073023291392-0000000002",
						"pagingToken": "0001364073023291392-0000000002",
						"inSuccessfulContractCall": true,
						"txHash": "3c3bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d3",
						"topic": ["AAAADwAAAA9jb250cmFjdF9jYWxsZWQA"],
						"value": "MID2"
					}
				],
				"cursor": "page_7_cursor"
			}
		}),
		// Page 7: Another sparse region
		json!({
			"result": {
				"events": [],
				"cursor": "page_8_cursor"
			}
		}),
		json!({
			"result": {
				"events": [],
				"cursor": "page_9_cursor"
			}
		}),
		// Page 9: Event exactly at end_sequence (should be included)
		json!({
			"result": {
				"events": [
					{
						"type": "contract",
						"ledger": 100, // Exactly at end_sequence
						"ledgerClosedAt": "2024-12-29T02:50:30Z",
						"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
						"id": "0001364073023291393-0000000001",
						"pagingToken": "0001364073023291393-0000000001",
						"inSuccessfulContractCall": true,
						"txHash": "4d4bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d4",
						"topic": ["AAAADwAAAA9jb250cmFjdF9jYWxsZWQA"],
						"value": "END"
					}
				],
				"cursor": "page_10_cursor"
			}
		}),
		// Page 10: Event beyond end_sequence (should be filtered out and stop pagination)
		json!({
			"result": {
				"events": [
					{
						"type": "contract",
						"ledger": 105, // Beyond end_sequence, should trigger early return
						"ledgerClosedAt": "2024-12-29T02:50:35Z",
						"contractId": "CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK",
						"id": "0001364073023291394-0000000001",
						"pagingToken": "0001364073023291394-0000000001",
						"inSuccessfulContractCall": true,
						"txHash": "5e5bf196f1db3ab56089de59985bbf5a6c3e0e6a4672cd91e01680b0fff260d5",
						"topic": ["AAAADwAAAA9jb250cmFjdF9jYWxsZWQA"],
						"value": "BEYOND"
					}
				],
				"cursor": null // This shouldn't matter as we should return early
			}
		}),
	];

	// Create mocks using a loop
	let mut mocks = Vec::new();
	for response in &responses {
		let mock = server
			.mock("POST", "/")
			.with_status(200)
			.with_body(response.to_string())
			.create_async()
			.await;
		mocks.push(mock);
	}

	let client = StellarClient::new(&network).await.unwrap();
	// Query range: ledger 10 to 100 (inclusive)
	let result = client.get_events(10, Some(100)).await.unwrap();

	// Should find 4 events: at ledger 10, 45, 47, and 100
	// Event at ledger 105 should be excluded (beyond range)
	assert_eq!(result.len(), 4);

	// Verify events are in order and within range
	assert_eq!(result[0].ledger, 10);
	assert_eq!(result[1].ledger, 45);
	assert_eq!(result[2].ledger, 47);
	assert_eq!(result[3].ledger, 100);

	// Verify all events are from the expected contract
	for event in &result {
		assert_eq!(
			event.contract_id,
			"CC5WP4L2CXUBZXZY3ZHK2XURV4H7VS6GKYF7K7WIHQSMEUDJYQ2E5TLK"
		);
		// Ensure all events are within the requested range
		assert!(event.ledger >= 10 && event.ledger <= 100);
	}

	mock.assert_async().await;
	for mock in mocks {
		mock.assert_async().await;
	}
}
