use crate::integration::mocks::{
	create_default_method_responses, start_test_websocket_server, MockMidnightWsTransportClient,
};
use openzeppelin_monitor::services::blockchain::{WsConfig, WsEndpointManager};

use mockall::predicate;
use std::time::Duration;

#[tokio::test]
async fn test_endpoint_rotation() {
	// Start test servers
	let (url1, shutdown_tx1) =
		start_test_websocket_server(Some(create_default_method_responses())).await;
	let (url2, shutdown_tx2) =
		start_test_websocket_server(Some(create_default_method_responses())).await;
	let (url3, shutdown_tx3) =
		start_test_websocket_server(Some(create_default_method_responses())).await;

	let config = WsConfig::single_attempt();
	let manager = WsEndpointManager::new(&config, &url1, vec![url2.clone(), url3.clone()]);
	let mut transport = MockMidnightWsTransportClient::new();
	transport.expect_try_connect().returning(|_| Ok(()));
	transport.expect_update_client().returning(|_| Ok(()));

	// Test initial state
	assert_eq!(&*manager.active_url.read().await, &url1);
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![url2.clone(), url3.clone()]
	);

	// Test rotation
	manager.rotate_url(&transport).await.unwrap();
	assert_eq!(&*manager.active_url.read().await, &url2);

	manager.rotate_url(&transport).await.unwrap();
	assert_eq!(&*manager.active_url.read().await, &url3);

	// Cleanup
	let _ = shutdown_tx1.send(());
	let _ = shutdown_tx2.send(());
	let _ = shutdown_tx3.send(());
}

#[tokio::test]
async fn test_rotate_url_no_fallbacks() {
	let (url, shutdown_tx) =
		start_test_websocket_server(Some(create_default_method_responses())).await;

	// Create manager with no fallback URLs
	let config = WsConfig::single_attempt();
	let manager = WsEndpointManager::new(&config, &url, vec![]);
	let transport = MockMidnightWsTransportClient::new();

	// Attempt to rotate
	let result = manager.rotate_url(&transport).await;

	// Verify we get the expected error
	let err = result.unwrap_err();
	assert!(err.to_string().contains("No fallback URLs available"));

	// Verify the active URL hasn't changed
	assert_eq!(&*manager.active_url.read().await, &url);

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_rotate_url_connection_failure() {
	let (url, shutdown_tx) =
		start_test_websocket_server(Some(create_default_method_responses())).await;

	// Create manager with an invalid fallback URL that will fail to connect
	let invalid_url = "ws://invalid";
	let config = WsConfig::new()
		.with_max_reconnect_attempts(1)
		.with_connection_timeout(Duration::from_secs(1))
		.with_reconnect_timeout(Duration::from_secs(1))
		.with_message_timeout(Duration::from_secs(1))
		.build();

	let manager = WsEndpointManager::new(&config, &url, vec![invalid_url.to_string()]);

	// Create a mock that fails to connect
	let mut transport = MockMidnightWsTransportClient::new();
	transport
		.expect_try_connect()
		.with(predicate::eq(invalid_url))
		.returning(|_| Err(anyhow::anyhow!("Failed to connect to fallback URL")));

	// Attempt to rotate
	let result = manager.rotate_url(&transport).await;

	// Verify we get the expected error
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to reconnect"));

	// Verify the active URL hasn't changed
	assert_eq!(&*manager.active_url.read().await, &url);

	// Verify the failed URL was pushed back to fallback_urls
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![invalid_url.to_string()]
	);

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_should_rotate() {
	let (url1, shutdown_tx1) =
		start_test_websocket_server(Some(create_default_method_responses())).await;
	let (url2, shutdown_tx2) =
		start_test_websocket_server(Some(create_default_method_responses())).await;

	let config = WsConfig::single_attempt();
	let manager = WsEndpointManager::new(&config, &url1, vec![url2.clone()]);

	// Should rotate when fallbacks are available
	assert!(manager.should_rotate().await);

	// Create manager with no fallbacks
	let manager = WsEndpointManager::new(&config, &url1, vec![]);

	// Should not rotate when no fallbacks are available
	assert!(!manager.should_rotate().await);

	// Cleanup
	let _ = shutdown_tx1.send(());
	let _ = shutdown_tx2.send(());
}

#[tokio::test]
async fn test_endpoint_manager_configuration() {
	let (url1, shutdown_tx1) =
		start_test_websocket_server(Some(create_default_method_responses())).await;
	let (url2, shutdown_tx2) =
		start_test_websocket_server(Some(create_default_method_responses())).await;

	// Test with custom configuration
	let config = WsConfig::new()
		.with_max_reconnect_attempts(2)
		.with_connection_timeout(Duration::from_millis(100))
		.with_reconnect_timeout(Duration::from_millis(50))
		.build();

	let manager = WsEndpointManager::new(&config, &url1, vec![url2.clone()]);
	let mut transport = MockMidnightWsTransportClient::new();

	transport.expect_try_connect().returning(|_| Ok(()));
	transport.expect_update_client().returning(|_| Ok(()));

	// Test rotation with custom config
	let result = manager.rotate_url(&transport).await;
	assert!(result.is_ok(), "Should rotate with custom configuration");

	// Cleanup
	let _ = shutdown_tx1.send(());
	let _ = shutdown_tx2.send(());
}

#[tokio::test]
async fn test_endpoint_manager_thread_safety() {
	let (url1, shutdown_tx1) =
		start_test_websocket_server(Some(create_default_method_responses())).await;
	let (url2, shutdown_tx2) =
		start_test_websocket_server(Some(create_default_method_responses())).await;
	let (url3, shutdown_tx3) =
		start_test_websocket_server(Some(create_default_method_responses())).await;

	let manager = WsEndpointManager::new(
		&WsConfig::single_attempt(),
		&url1,
		vec![url2.clone(), url3.clone()],
	);
	let mut transport = MockMidnightWsTransportClient::new();
	transport.expect_clone().returning(|| {
		let mut transport: MockMidnightWsTransportClient = MockMidnightWsTransportClient::new();
		transport.expect_try_connect().returning(|_| Ok(()));
		transport.expect_update_client().returning(|_| Ok(()));
		transport
	});

	// Test concurrent rotation attempts
	let mut handles = vec![];
	for _ in 0..3 {
		let manager = manager.clone();
		let transport = transport.clone();
		handles.push(tokio::spawn(
			async move { manager.rotate_url(&transport).await },
		));
	}

	// Wait for all rotations to complete
	let results = futures::future::join_all(handles).await;
	let success_count = results
		.iter()
		.filter(|r| r.as_ref().unwrap().is_ok())
		.count();
	assert!(success_count > 0, "At least one rotation should succeed");

	// Cleanup
	let _ = shutdown_tx1.send(());
	let _ = shutdown_tx2.send(());
	let _ = shutdown_tx3.send(());
}

#[tokio::test]
async fn test_endpoint_manager_error_handling() {
	let (url, shutdown_tx) =
		start_test_websocket_server(Some(create_default_method_responses())).await;

	// Test with invalid URL
	let invalid_url = "ws://invalid-domain-that-does-not-exist:12345";
	let manager = WsEndpointManager::new(
		&WsConfig::single_attempt(),
		&url,
		vec![invalid_url.to_string()],
	);
	let mut transport = MockMidnightWsTransportClient::new();
	transport
		.expect_try_connect()
		.with(predicate::always())
		.returning(|_| Err(anyhow::anyhow!("Failed to connect to fallback URL")));

	// Test rotation with invalid URL
	let result = manager.rotate_url(&transport).await;
	assert!(result.is_err(), "Should fail with invalid URL");
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Failed to reconnect after 1 attempts"));

	// Test with malformed URL
	let malformed_url = "not-a-url";
	let manager = WsEndpointManager::new(
		&WsConfig::single_attempt(),
		&url,
		vec![malformed_url.to_string()],
	);
	let result = manager.rotate_url(&transport).await;
	assert!(result.is_err(), "Should fail with malformed URL");

	// Cleanup
	let _ = shutdown_tx.send(());
}

#[tokio::test]
async fn test_endpoint_manager_state_management() {
	let (url1, shutdown_tx1) =
		start_test_websocket_server(Some(create_default_method_responses())).await;
	let (url2, shutdown_tx2) =
		start_test_websocket_server(Some(create_default_method_responses())).await;
	let invalid_url = "ws://invalid-domain-that-does-not-exist:12345";

	// Create a config with minimal retry attempts
	let config = WsConfig::single_attempt();

	// Test successful rotation
	let manager = WsEndpointManager::new(&config, &url1, vec![url2.clone()]);
	let mut transport = MockMidnightWsTransportClient::new();
	transport
		.expect_try_connect()
		.with(predicate::eq(url2.clone()))
		.times(1)
		.returning(|_| Ok(()));
	transport
		.expect_update_client()
		.with(predicate::eq(url2.clone()))
		.times(1)
		.returning(|_| Ok(()));

	// Test initial state
	assert_eq!(&*manager.active_url.read().await, &url1);
	assert_eq!(&*manager.fallback_urls.read().await, &vec![url2.clone()]);

	// Test state after successful rotation
	manager.rotate_url(&transport).await.unwrap();
	assert_eq!(&*manager.active_url.read().await, &url2);
	assert_eq!(&*manager.fallback_urls.read().await, &vec![url1.clone()]);

	// Test failed rotation with a new mock instance
	let manager = WsEndpointManager::new(&config, &url1, vec![invalid_url.to_string()]);
	let mut transport = MockMidnightWsTransportClient::new();
	transport
		.expect_try_connect()
		.with(predicate::eq(invalid_url))
		.times(1)
		.returning(|_| Err(anyhow::anyhow!("Failed to connect to fallback URL")));

	// Test state after failed rotation
	let result = manager.rotate_url(&transport).await;
	assert!(result.is_err());
	assert_eq!(&*manager.active_url.read().await, &url1);
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![invalid_url.to_string()]
	);

	// Cleanup
	let _ = shutdown_tx1.send(());
	let _ = shutdown_tx2.send(());
}

#[tokio::test]
async fn test_endpoint_manager_retry_settings() {
	let (url, shutdown_tx) =
		start_test_websocket_server(Some(create_default_method_responses())).await;

	// Create manager with custom retry settings
	let config = WsConfig::new()
		.with_max_reconnect_attempts(2)  // Only 2 retry attempts
		.with_connection_timeout(Duration::from_millis(100))  // Short connection timeout
		.with_reconnect_timeout(Duration::from_millis(50))  // Short reconnect timeout
		.build();

	let invalid_url = "ws://invalid-domain-that-does-not-exist:12345";
	let manager = WsEndpointManager::new(&config, &url, vec![invalid_url.to_string()]);
	let mut transport = MockMidnightWsTransportClient::new();

	// Set up mock to fail connection attempts
	transport
		.expect_try_connect()
		.with(predicate::eq(invalid_url))
		.times(2)  // Expect exactly 2 attempts
		.returning(|_| Err(anyhow::anyhow!("Failed to connect to fallback URL")));

	// Attempt to rotate
	let result = manager.rotate_url(&transport).await;

	// Verify we get the expected error after exactly 2 attempts
	let err = result.unwrap_err();
	assert!(err.to_string().contains("Failed to reconnect"));
	assert!(err.to_string().contains("2 attempts")); // Verify number of attempts in error message

	// Verify the active URL hasn't changed
	assert_eq!(&*manager.active_url.read().await, &url);

	// Verify the failed URL was pushed back to fallback_urls
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![invalid_url.to_string()]
	);

	// Cleanup
	let _ = shutdown_tx.send(());
}
