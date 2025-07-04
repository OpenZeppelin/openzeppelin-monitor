use openzeppelin_monitor::{
	models::BlockChainType,
	services::blockchain::{
		ClientPool, ClientPoolTrait, EVMTransportClient, EvmClient, StellarClient,
		StellarTransportClient,
	},
	utils::{tests::network::NetworkBuilder, RetryConfig},
};

use std::sync::Arc;

use crate::integration::mocks::{
	create_evm_test_network_with_urls, create_evm_valid_server_mock_network_response,
	create_stellar_test_network_with_urls, create_stellar_valid_server_mock_network_response,
};

#[tokio::test]
async fn test_new_pool_is_empty() {
	let pool = ClientPool::new();

	// Test EVM clients
	let result = pool
		.get_evm_client(&create_evm_test_network_with_urls(vec!["http://dummy"]))
		.await;
	assert!(result.is_err()); // Should error since no client exists yet

	// Test Stellar clients
	let result = pool
		.get_stellar_client(&create_stellar_test_network_with_urls(vec!["http://dummy"]))
		.await;
	assert!(result.is_err()); // Should error since no client exists yet
}

#[tokio::test]
async fn test_get_evm_client_creates_and_caches() {
	let mut mock_server = mockito::Server::new_async().await;
	let mock = create_evm_valid_server_mock_network_response(&mut mock_server);
	let pool = ClientPool::new();
	let network = create_evm_test_network_with_urls(vec![&mock_server.url()]);

	// First request should create new client
	let client1 = pool.get_evm_client(&network).await.unwrap();
	assert_eq!(pool.storages.len(), 2);
	assert_eq!(
		pool.get_client_count::<EvmClient<EVMTransportClient>>(BlockChainType::EVM)
			.await,
		1
	); // But only one EVM client
	assert_eq!(
		pool.get_client_count::<StellarClient<StellarTransportClient>>(BlockChainType::Stellar)
			.await,
		0
	); // And no Stellar clients

	// Second request should return cached client
	let client2 = pool.get_evm_client(&network).await.unwrap();
	assert_eq!(
		pool.get_client_count::<EvmClient<EVMTransportClient>>(BlockChainType::EVM)
			.await,
		1
	); // Still only one EVM client

	// Clients should be the same instance
	assert!(Arc::ptr_eq(&client1, &client2));

	mock.assert();
}

#[tokio::test]
async fn test_get_stellar_client_creates_and_caches() {
	let mut mock_server = mockito::Server::new_async().await;

	let mock = create_stellar_valid_server_mock_network_response(&mut mock_server);

	let pool = ClientPool::new();
	let network = create_stellar_test_network_with_urls(vec![&mock_server.url()]);

	// First request should create new client
	let client1 = pool.get_stellar_client(&network).await.unwrap();
	assert_eq!(pool.storages.len(), 2);
	assert_eq!(
		pool.get_client_count::<StellarClient<StellarTransportClient>>(BlockChainType::Stellar)
			.await,
		1
	);

	// Second request should return cached client
	let client2 = pool.get_stellar_client(&network).await.unwrap();
	assert_eq!(pool.storages.len(), 2);
	assert_eq!(
		pool.get_client_count::<StellarClient<StellarTransportClient>>(BlockChainType::Stellar)
			.await,
		1
	);

	// Clients should be the same instance
	assert!(Arc::ptr_eq(&client1, &client2));

	mock.assert();
}

#[tokio::test]
async fn test_different_evm_networks_get_different_clients() {
	let pool = ClientPool::new();
	let mut mock_server = mockito::Server::new_async().await;
	let mut mock_server_2 = mockito::Server::new_async().await;

	let mock = create_evm_valid_server_mock_network_response(&mut mock_server);
	let mock_2 = create_evm_valid_server_mock_network_response(&mut mock_server_2);

	let network1 = create_evm_test_network_with_urls(vec![&mock_server.url()]);
	let network2 = NetworkBuilder::new()
		.name("test-2")
		.slug("test-2")
		.network_type(BlockChainType::EVM)
		.rpc_urls(vec![&mock_server_2.url()])
		.build();

	let client1 = pool.get_evm_client(&network1).await.unwrap();
	let client2 = pool.get_evm_client(&network2).await.unwrap();

	// Should have different clients
	assert_eq!(pool.storages.len(), 2);
	assert_eq!(
		pool.get_client_count::<EvmClient<EVMTransportClient>>(BlockChainType::EVM)
			.await,
		2
	);
	assert!(!Arc::ptr_eq(&client1, &client2));

	mock.assert();
	mock_2.assert();
}

#[tokio::test]
async fn test_different_stellar_networks_get_different_clients() {
	let pool = ClientPool::new();
	let mut mock_server = mockito::Server::new_async().await;
	let mut mock_server_2 = mockito::Server::new_async().await;

	let mock = create_stellar_valid_server_mock_network_response(&mut mock_server);
	let mock_2 = create_stellar_valid_server_mock_network_response(&mut mock_server_2);

	let network1 = create_stellar_test_network_with_urls(vec![&mock_server.url()]);
	let network2 = NetworkBuilder::new()
		.name("test-2")
		.slug("test-2")
		.network_type(BlockChainType::EVM)
		.rpc_urls(vec![&mock_server_2.url()])
		.build();

	let client1 = pool.get_stellar_client(&network1).await.unwrap();
	let client2 = pool.get_stellar_client(&network2).await.unwrap();

	// Should have different clients
	assert_eq!(pool.storages.len(), 2);
	assert_eq!(
		pool.get_client_count::<StellarClient<StellarTransportClient>>(BlockChainType::Stellar)
			.await,
		2
	);
	assert!(!Arc::ptr_eq(&client1, &client2));

	mock.assert();
	mock_2.assert();
}

#[tokio::test]
async fn test_concurrent_access() {
	let pool = Arc::new(ClientPool::new());
	let mut mock_server = mockito::Server::new_async().await;

	let mock = create_evm_valid_server_mock_network_response(&mut mock_server);

	let network = create_evm_test_network_with_urls(vec![&mock_server.url()]);

	// Spawn multiple tasks trying to get the same client
	let mut handles = vec![];
	for _ in 0..10 {
		let pool = pool.clone();
		let network = network.clone();
		handles.push(tokio::spawn(async move {
			pool.get_evm_client(&network).await.unwrap()
		}));
	}

	// Wait for all tasks to complete
	let clients: Vec<Arc<EvmClient<EVMTransportClient>>> = futures::future::join_all(handles)
		.await
		.into_iter()
		.map(|r| r.unwrap())
		.collect();

	// Should only have created one client
	assert_eq!(pool.storages.len(), 2);
	assert_eq!(
		pool.get_client_count::<EvmClient<EVMTransportClient>>(BlockChainType::EVM)
			.await,
		1
	);

	// All clients should be the same instance
	let first = &clients[0];
	for client in &clients[1..] {
		assert!(Arc::ptr_eq(first, client));
	}

	mock.assert();
}

#[tokio::test]
async fn test_default_creates_empty_pool() {
	let pool: ClientPool = Default::default();

	assert_eq!(pool.storages.len(), 2);
	assert_eq!(
		pool.get_client_count::<EvmClient<EVMTransportClient>>(BlockChainType::EVM)
			.await,
		0
	);
	assert_eq!(
		pool.get_client_count::<StellarClient<StellarTransportClient>>(BlockChainType::Stellar)
			.await,
		0
	);
}

#[tokio::test]
async fn test_get_evm_client_handles_errors() {
	let mut mock_server = mockito::Server::new_async().await;

	// Use the default retry config to determine expected attempts
	let expected_attempts = 1 + RetryConfig::default().max_retries;

	// Setup mock to return an error response
	let mock = mock_server
		.mock("POST", "/")
		.with_status(500)
		.with_header("content-type", "application/json")
		.with_body(r#"{"error": "Internal Server Error"}"#)
		.expect(expected_attempts as usize)
		.create_async()
		.await;

	let pool = ClientPool::new();
	let network = create_evm_test_network_with_urls(vec![&mock_server.url()]);

	// Attempt to get client should result in error
	let result = pool.get_evm_client(&network).await;
	if let Err(err) = result {
		assert!(
			err.to_string()
				.contains("Failed to get or create EVM client"),
			"Expected ClientPoolError, got: {}",
			err
		);
	} else {
		panic!("Expected error, got success");
	}

	// Pool should remain empty after failed client creation
	assert_eq!(pool.storages.len(), 2);
	assert_eq!(
		pool.get_client_count::<EvmClient<EVMTransportClient>>(BlockChainType::EVM)
			.await,
		0
	);
	assert_eq!(
		pool.get_client_count::<StellarClient<StellarTransportClient>>(BlockChainType::Stellar)
			.await,
		0
	);
	mock.assert();
}

#[tokio::test]
async fn test_get_stellar_client_handles_errors() {
	let mut mock_server = mockito::Server::new_async().await;

	// Use the default retry config to determine expected attempts
	let expected_attempts = 1 + RetryConfig::default().max_retries;

	// Setup mock to return an error response
	let mock = mock_server
		.mock("POST", "/")
		.with_status(500)
		.with_header("content-type", "application/json")
		.with_body(r#"{"error": "Internal Server Error"}"#)
		.expect(expected_attempts as usize)
		.create_async()
		.await;

	let pool = ClientPool::new();
	let network = create_stellar_test_network_with_urls(vec![&mock_server.url()]);

	// Attempt to get client should result in error
	let result = pool.get_stellar_client(&network).await;
	if let Err(err) = result {
		assert!(
			err.to_string()
				.contains("Failed to get or create Stellar client"),
			"Expected ClientPoolError, got: {}",
			err
		);
	} else {
		panic!("Expected error, got success");
	}

	// Pool should remain empty after failed client creation
	assert_eq!(pool.storages.len(), 2);
	assert_eq!(
		pool.get_client_count::<EvmClient<EVMTransportClient>>(BlockChainType::EVM)
			.await,
		0
	);
	assert_eq!(
		pool.get_client_count::<StellarClient<StellarTransportClient>>(BlockChainType::Stellar)
			.await,
		0
	);
	mock.assert();
}
