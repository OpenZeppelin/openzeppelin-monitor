use mockito::Server;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;

use openzeppelin_monitor::services::blockchain::{
	BlockchainTransport, HttpEndpointManager, TransportError, ROTATE_ON_ERROR_CODES,
};

use crate::integration::mocks::{AlwaysFailsToUpdateClientTransport, MockTransport};

/// Default network slug for test cases
const TEST_NETWORK_SLUG: &str = "test-network";

fn get_mock_client_builder() -> ClientWithMiddleware {
	ClientBuilder::new(reqwest::Client::new()).build()
}

#[tokio::test]
async fn test_endpoint_rotation() {
	// Set up mock servers
	let server1 = Server::new_async().await;
	let mut server2 = Server::new_async().await;
	let server3 = Server::new_async().await;

	let mock2 = server2
		.mock("GET", "/")
		.with_status(200)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server1.url().as_ref(),
		vec![server2.url(), server3.url()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	// Test initial state
	assert_eq!(&*manager.active_url.read().await, &server1.url());
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![server2.url(), server3.url()]
	);

	// Test rotation
	let new_url = manager.try_rotate_url(&transport).await.unwrap();
	assert_eq!(new_url, server2.url());
	assert_eq!(&*manager.active_url.read().await, &server2.url());

	mock2.assert();
}

#[tokio::test]
async fn test_send_raw_request() {
	let mut server = Server::new_async().await;

	// Mock successful response
	let mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "success", "id": 1}"#)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();

	assert_eq!(result["result"], "success");
	mock.assert();
}

#[tokio::test]
async fn test_rotation_on_error() {
	let mut primary_server = Server::new_async().await;
	let mut fallback_server = Server::new_async().await;

	// Primary server returns 429 (Too Many Requests)
	let primary_mock = primary_server
		.mock("POST", "/")
		.with_status(429)
		.with_body("Rate limited")
		.expect(1) // Expect 1 request due to 429 error which is not retried
		.create_async()
		.await;

	// Fallback server returns success
	let fallback_mock = fallback_server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "success", "id": 1}"#)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		primary_server.url().as_ref(),
		vec![fallback_server.url()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();

	assert_eq!(result["result"], "success");
	primary_mock.assert();
	fallback_mock.assert();

	// Verify rotation occurred
	assert_eq!(&*manager.active_url.read().await, &fallback_server.url());
}

#[tokio::test]
async fn test_no_fallback_urls_available() {
	let mut server = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.with_status(429)
		.with_body("Rate limited")
		.expect(1) // Expect 1 request due to 429 error which is not retried
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	match err {
		TransportError::Http {
			status_code,
			url,
			body,
			..
		} => {
			assert_eq!(status_code, 429);
			assert_eq!(url, server.url());
			assert_eq!(body, "Rate limited");
		}
		_ => panic!("Expected Http error with status code 429"),
	}
	mock.assert();
}

#[tokio::test]
async fn test_customize_request() {
	let transport = MockTransport::new();

	// Test with parameters
	let result = transport
		.customize_request("test_method", Some(json!(["param1", "param2"])))
		.await;

	assert_eq!(
		result,
		json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": "test_method",
			"params": ["param1", "param2"]
		})
	);

	// Test without parameters
	let result = transport
		.customize_request::<Value>("test_method", None)
		.await;

	assert_eq!(
		result,
		json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": "test_method",
			"params": null
		})
	);
}

#[tokio::test]
async fn test_rotate_url_no_fallbacks() {
	let server = Server::new_async().await;

	// Create manager with no fallback URLs
	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	// Attempt to rotate
	let result = manager.try_rotate_url(&transport).await;

	// Verify we get the expected error
	let err = result.unwrap_err();

	match err {
		TransportError::UrlRotation(ctx) => {
			assert!(ctx.to_string().contains("No fallback URLs available"));
		}
		_ => panic!("Expected UrlRotation error"),
	}

	// Verify the active URL hasn't changed
	assert_eq!(&*manager.active_url.read().await, &server.url());
}

#[tokio::test]
async fn test_rotate_url_all_urls_match_active() {
	let server = Server::new_async().await;

	// Create manager with fallback URLs that are identical to the active URL
	let active_url = server.url();
	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		active_url.as_ref(),
		vec![active_url.clone(), active_url.clone()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	// Attempt to rotate
	let result = manager.try_rotate_url(&transport).await;

	// Verify we get the expected error
	let err = result.unwrap_err();

	match err {
		TransportError::UrlRotation(ctx) => {
			assert!(ctx.to_string().contains("No fallback URLs available"));
			assert!(ctx.to_string().contains(&active_url));
		}
		_ => panic!("Expected UrlRotation error"),
	}

	// Verify the active URL hasn't changed
	assert_eq!(&*manager.active_url.read().await, &active_url);

	// Verify fallback URLs are unchanged
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![active_url.clone(), active_url.clone()]
	);
}

#[tokio::test]
async fn test_rotate_url_connection_failure() {
	let server = Server::new_async().await;

	// Create manager with an invalid fallback URL that will fail to connect
	let invalid_url = "http://invalid-domain-that-does-not-exist:12345";
	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![invalid_url.to_string()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	// Attempt to rotate
	let result = manager.try_rotate_url(&transport).await;

	// Verify we get the expected error
	let err = result.unwrap_err();

	match err {
		TransportError::UrlRotation(ctx) => {
			assert!(ctx.to_string().contains("Failed to connect to new URL"));
			assert!(ctx.to_string().contains(invalid_url));
		}
		_ => panic!("Expected UrlRotation error"),
	}

	// Verify the active URL hasn't changed
	assert_eq!(&*manager.active_url.read().await, &server.url());

	// Verify the failed URL was pushed back to fallback_urls
	assert_eq!(
		&*manager.fallback_urls.read().await,
		&vec![invalid_url.to_string()]
	);
}

#[tokio::test]
async fn test_rotate_url_update_client_failure() {
	let server1 = Server::new_async().await;
	let server2 = Server::new_async().await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server1.url().as_ref(),
		vec![server2.url()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = AlwaysFailsToUpdateClientTransport {
		current_url: Arc::new(RwLock::new(server1.url())),
	};

	let result = manager.try_rotate_url(&transport).await;

	assert!(result.is_err());
	match result.unwrap_err() {
		TransportError::UrlRotation(ctx) => {
			assert!(ctx
				.to_string()
				.contains("Failed to update transport client with new URL"));
		}
		_ => panic!("Expected UrlRotation error"),
	}
	// The active URL should not have changed
	assert_eq!(&*manager.active_url.read().await, &server1.url());
}

#[tokio::test]
async fn test_rotate_url_all_urls_fail_returns_url_rotation_error() {
	let invalid_url1 = "http://invalid-domain-that-will-fail-1:12345";
	let invalid_url2 = "http://invalid-domain-that-will-fail-2:12345";

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		invalid_url1,
		vec![invalid_url2.to_string()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager.try_rotate_url(&transport).await;

	assert!(result.is_err());
	assert!(matches!(
		result.unwrap_err(),
		TransportError::UrlRotation(_)
	));
}

#[tokio::test]
async fn test_update_client() {
	let mut server = Server::new_async().await;

	// Set up two different responses to differentiate between clients
	let initial_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "initial_client", "id": 1}"#)
		.expect(1)
		.create_async()
		.await;

	let mut manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);

	// Test initial client
	let transport = MockTransport::new();
	let initial_result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();
	assert_eq!(initial_result["result"], "initial_client");
	initial_mock.assert();

	// Set up mock for new client
	let updated_mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "updated_client", "id": 1}"#)
		.expect(1)
		.create_async()
		.await;

	// Create and update to new client with different configuration
	let new_client = ClientBuilder::new(reqwest::Client::new())
		.with(RetryTransientMiddleware::new_with_policy(
			ExponentialBackoff::builder().build_with_max_retries(3),
		))
		.build();
	manager.update_client(new_client);

	// Test updated client
	let updated_result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();
	assert_eq!(updated_result["result"], "updated_client");
	updated_mock.assert();
}

#[tokio::test]
async fn test_send_raw_request_network_error() {
	// Set up with an invalid primary URL that will cause a network error
	let invalid_url = "http://invalid-domain-that-will-fail:12345";
	let mut valid_server = Server::new_async().await;

	// Set up mock for fallback server
	let success_mock = valid_server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "success", "id": 1}"#)
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		invalid_url,
		vec![valid_server.url()], // Add valid fallback URL
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	// Send request - should fail first with network error, then rotate and succeed
	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	// Verify success after rotation
	assert!(result.is_ok());
	let response = result.unwrap();
	assert_eq!(response["result"], "success");
	success_mock.assert();

	// Verify URL rotation occurred
	assert_eq!(&*manager.active_url.read().await, &valid_server.url());
}

#[tokio::test]
async fn test_send_raw_request_network_error_no_fallback() {
	// Set up with an invalid URL and no fallbacks
	let invalid_url = "http://invalid-domain-that-will-fail:12345";
	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		invalid_url,
		vec![], // No fallback URLs
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	// Send request - should fail with network error and no rotation possible
	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	// Verify error
	assert!(result.is_err());
	assert!(matches!(result.unwrap_err(), TransportError::Network(_)));

	// Verify URL didn't change
	assert_eq!(&*manager.active_url.read().await, invalid_url);
}

#[tokio::test]
async fn test_send_raw_request_response_parse_error() {
	let mut server = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "invalid_json"#) // Missing closing brace
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	// Send request - should fail with parse error
	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	assert!(result.is_err());
	assert!(matches!(
		result.unwrap_err(),
		TransportError::ResponseParse(_)
	));

	mock.assert();
}

#[tokio::test]
async fn test_send_raw_request_all_urls_fail_returns_network_error() {
	let invalid_url1 = "http://invalid-domain-that-will-fail-1:12345";
	let invalid_url2 = "http://invalid-domain-that-will-fail-2:12345";
	let invalid_url3 = "http://invalid-domain-that-will-fail-3:12345";

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		invalid_url1,
		vec![invalid_url2.to_string(), invalid_url3.to_string()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	assert!(result.is_err());
	assert!(matches!(result.unwrap_err(), TransportError::Network(_)));
}

#[tokio::test]
async fn test_send_raw_request_returns_http_error_if_non_transient() {
	let mut server = Server::new_async().await;

	// Mock a non-transient HTTP error (e.g., 404 Not Found)
	let mock = server
		.mock("POST", "/")
		.with_status(404)
		.with_body("Not Found")
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	assert!(result.is_err());
	match result.unwrap_err() {
		TransportError::Http {
			status_code,
			url,
			body,
			..
		} => {
			assert_eq!(status_code, 404);
			assert_eq!(url, server.url());
			assert_eq!(body, "Not Found");
		}
		_ => panic!("Expected Http error with status code 404"),
	}

	mock.assert();
}

#[tokio::test]
async fn test_rotation_on_5xx_error() {
	let mut primary_server = Server::new_async().await;
	let mut fallback_server = Server::new_async().await;

	// Primary server returns 502 (Bad Gateway)
	let primary_mock = primary_server
		.mock("POST", "/")
		.with_status(502)
		.with_body("Bad Gateway")
		.expect(1)
		.create_async()
		.await;

	// Fallback server returns success
	let fallback_mock = fallback_server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc": "2.0", "result": "success", "id": 1}"#)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		primary_server.url().as_ref(),
		vec![fallback_server.url()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await
		.unwrap();

	assert_eq!(result["result"], "success");
	primary_mock.assert();
	fallback_mock.assert();

	// Verify rotation occurred
	assert_eq!(&*manager.active_url.read().await, &fallback_server.url());
}

#[tokio::test]
async fn test_rotation_on_all_new_error_codes() {
	// Test each error code in ROTATE_ON_ERROR_CODES triggers rotation
	for &status_code in ROTATE_ON_ERROR_CODES.iter() {
		let mut primary_server = Server::new_async().await;
		let mut fallback_server = Server::new_async().await;

		let primary_mock = primary_server
			.mock("POST", "/")
			.with_status(status_code as usize)
			.with_body(format!("Error {}", status_code))
			.expect(1)
			.create_async()
			.await;

		let fallback_mock = fallback_server
			.mock("POST", "/")
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(r#"{"jsonrpc": "2.0", "result": "success", "id": 1}"#)
			.create_async()
			.await;

		let manager = HttpEndpointManager::new(
			get_mock_client_builder(),
			primary_server.url().as_ref(),
			vec![fallback_server.url()],
			TEST_NETWORK_SLUG.to_string(),
			&[],
		);
		let transport = MockTransport::new();

		let result = manager
			.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
			.await;

		assert!(
			result.is_ok(),
			"Expected rotation and success for status code {}, got error: {:?}",
			status_code,
			result.err()
		);
		assert_eq!(result.unwrap()["result"], "success");
		primary_mock.assert();
		fallback_mock.assert();

		// Verify rotation occurred
		assert_eq!(
			&*manager.active_url.read().await,
			&fallback_server.url(),
			"Expected rotation to fallback for status code {}",
			status_code
		);
	}
}

#[tokio::test]
async fn test_no_fallback_urls_available_5xx() {
	let mut server = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.with_status(503)
		.with_body("Service Unavailable")
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
		.await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	match err {
		TransportError::Http {
			status_code,
			url,
			body,
			..
		} => {
			assert_eq!(status_code, 503);
			assert_eq!(url, server.url());
			assert_eq!(body, "Service Unavailable");
		}
		_ => panic!("Expected Http error with status code 503"),
	}
	mock.assert();
}

#[tokio::test]
async fn test_non_rotation_error_codes_do_not_rotate() {
	// These codes should NOT trigger rotation
	let non_rotation_codes: [u16; 4] = [401, 403, 404, 405];

	for &status_code in non_rotation_codes.iter() {
		let mut primary_server = Server::new_async().await;
		let fallback_server = Server::new_async().await;

		let primary_mock = primary_server
			.mock("POST", "/")
			.with_status(status_code as usize)
			.with_body(format!("Error {}", status_code))
			.expect(1)
			.create_async()
			.await;

		let manager = HttpEndpointManager::new(
			get_mock_client_builder(),
			primary_server.url().as_ref(),
			vec![fallback_server.url()],
			TEST_NETWORK_SLUG.to_string(),
			&[],
		);
		let transport = MockTransport::new();

		let result = manager
			.send_raw_request(&transport, "test_method", Some(json!(["param1"])))
			.await;

		assert!(
			result.is_err(),
			"Expected error for status code {}",
			status_code
		);
		match result.unwrap_err() {
			TransportError::Http {
				status_code: code, ..
			} => {
				assert_eq!(code, status_code);
			}
			_ => panic!("Expected Http error for status code {}", status_code),
		}
		primary_mock.assert();

		// Verify NO rotation occurred - still on primary
		assert_eq!(
			&*manager.active_url.read().await,
			&primary_server.url(),
			"Should NOT have rotated for status code {}",
			status_code
		);
	}
}

// ============================================================
// JSON-RPC envelope tests
// ============================================================

/// HTTP 200 with a JSON-RPC error envelope must trigger rotation to a fallback. Mirrors
/// the upstream behavior where some providers (e.g. 1rpc.io) signal rate limiting via
/// `{"error": {"code": 15, ...}}` instead of HTTP 429.
#[tokio::test]
async fn test_rotation_on_jsonrpc_error() {
	let mut primary_server = Server::new_async().await;
	let mut fallback_server = Server::new_async().await;

	let primary_mock = primary_server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(
			r#"{"id":1,"jsonrpc":"2.0","error":{"message":"Too many request, try again later","code":15}}"#,
		)
		.expect(1)
		.create_async()
		.await;

	let fallback_mock = fallback_server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc":"2.0","result":"0x1","id":1}"#)
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		primary_server.url().as_ref(),
		vec![fallback_server.url()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "eth_blockNumber", None::<Value>)
		.await
		.unwrap();

	assert_eq!(result["result"], "0x1");
	primary_mock.assert();
	fallback_mock.assert();

	// Active URL should now be the fallback
	assert_eq!(&*manager.active_url.read().await, &fallback_server.url());
}

/// When no fallback is available and the primary returns a JSON-RPC error envelope, we
/// surface a `TransportError::RpcError` carrying the upstream code and message.
#[tokio::test]
async fn test_jsonrpc_error_no_fallback() {
	let mut server = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(
			r#"{"id":1,"jsonrpc":"2.0","error":{"message":"Too many request, try again later","code":15}}"#,
		)
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec![],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let err = manager
		.send_raw_request(&transport, "eth_blockNumber", None::<Value>)
		.await
		.unwrap_err();

	match err {
		TransportError::RpcError {
			code, message, url, ..
		} => {
			assert_eq!(code, 15);
			assert_eq!(message, "Too many request, try again later");
			assert_eq!(url, server.url());
		}
		other => panic!("Expected RpcError, got {:?}", other),
	}
	mock.assert();
}

/// JSON-RPC error codes in the per-transport skip-list (e.g. Solana's skipped-slot codes)
/// must be passed through to the caller without rotating, so per-client handlers can
/// distinguish "legitimate chain state" from "broken endpoint".
#[tokio::test]
async fn test_jsonrpc_passthrough_for_skip_listed_code() {
	let mut server = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(
			r#"{"id":1,"jsonrpc":"2.0","error":{"message":"Slot was skipped","code":-32007}}"#,
		)
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec!["http://other-fallback.example.invalid".to_string()],
		TEST_NETWORK_SLUG.to_string(),
		&[-32004, -32007, -32009],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "getBlock", Some(json!([12345])))
		.await
		.unwrap();

	// Response is passed through unchanged so per-client error handlers can run.
	assert_eq!(result["error"]["code"], -32007);
	assert_eq!(result["error"]["message"], "Slot was skipped");
	mock.assert();

	// No rotation should have occurred.
	assert_eq!(&*manager.active_url.read().await, &server.url());
}

/// `result: null` is a legitimate response for some methods (e.g. `eth_getBlockByNumber`
/// for a future block). It must not trigger rotation and must be returned as-is.
#[tokio::test]
async fn test_null_result_passes_through() {
	let mut server = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc":"2.0","result":null,"id":1}"#)
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server.url().as_ref(),
		vec!["http://other-fallback.example.invalid".to_string()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(
			&transport,
			"eth_getBlockByNumber",
			Some(json!(["0xffffff", true])),
		)
		.await
		.unwrap();

	assert!(result["result"].is_null());
	mock.assert();
	assert_eq!(&*manager.active_url.read().await, &server.url());
}

/// A JSON body with neither `result` nor `error` is malformed and treated like a
/// rotatable JSON-RPC failure.
#[tokio::test]
async fn test_malformed_envelope_rotates() {
	let mut primary_server = Server::new_async().await;
	let mut fallback_server = Server::new_async().await;

	let primary_mock = primary_server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"id":1,"jsonrpc":"2.0"}"#)
		.expect(1)
		.create_async()
		.await;

	let fallback_mock = fallback_server
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"jsonrpc":"2.0","result":"0x1","id":1}"#)
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		primary_server.url().as_ref(),
		vec![fallback_server.url()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = manager
		.send_raw_request(&transport, "eth_blockNumber", None::<Value>)
		.await
		.unwrap();

	assert_eq!(result["result"], "0x1");
	primary_mock.assert();
	fallback_mock.assert();
	assert_eq!(&*manager.active_url.read().await, &fallback_server.url());
}

/// When every configured endpoint returns a JSON-RPC error envelope, the manager must try
/// each endpoint once and then surface the last error rather than cycling indefinitely.
#[tokio::test]
async fn test_jsonrpc_error_all_endpoints_fail_does_not_loop() {
	let mut server_a = Server::new_async().await;
	let mut server_b = Server::new_async().await;

	// `expect(1)` fails the test if either endpoint is hit more than once — which is what
	// would happen under the prior infinite-rotation behavior.
	let mock_a = server_a
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"id":1,"jsonrpc":"2.0","error":{"message":"boom","code":15}}"#)
		.expect(1)
		.create_async()
		.await;

	let mock_b = server_b
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"id":1,"jsonrpc":"2.0","error":{"message":"boom","code":15}}"#)
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server_a.url().as_ref(),
		vec![server_b.url()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	// Guard against regressions that reintroduce the infinite loop.
	let result = tokio::time::timeout(
		std::time::Duration::from_secs(5),
		manager.send_raw_request(&transport, "eth_blockNumber", None::<Value>),
	)
	.await
	.expect("send_raw_request must terminate when all endpoints fail")
	.expect_err("expected an error after exhausting endpoints");

	match result {
		TransportError::RpcError { code, message, .. } => {
			assert_eq!(code, 15);
			assert_eq!(message, "boom");
		}
		other => panic!("Expected RpcError, got {:?}", other),
	}

	mock_a.assert();
	mock_b.assert();
}

/// When every configured endpoint returns a malformed JSON-RPC envelope (neither `result`
/// nor `error`), the manager must try each endpoint once and surface a malformed-envelope
/// error rather than rotating forever.
#[tokio::test]
async fn test_malformed_envelope_all_endpoints_fail_does_not_loop() {
	let mut server_a = Server::new_async().await;
	let mut server_b = Server::new_async().await;

	let mock_a = server_a
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"id":1,"jsonrpc":"2.0"}"#)
		.expect(1)
		.create_async()
		.await;

	let mock_b = server_b
		.mock("POST", "/")
		.with_status(200)
		.with_header("content-type", "application/json")
		.with_body(r#"{"id":1,"jsonrpc":"2.0"}"#)
		.expect(1)
		.create_async()
		.await;

	let manager = HttpEndpointManager::new(
		get_mock_client_builder(),
		server_a.url().as_ref(),
		vec![server_b.url()],
		TEST_NETWORK_SLUG.to_string(),
		&[],
	);
	let transport = MockTransport::new();

	let result = tokio::time::timeout(
		std::time::Duration::from_secs(5),
		manager.send_raw_request(&transport, "eth_blockNumber", None::<Value>),
	)
	.await
	.expect("send_raw_request must terminate when all endpoints return malformed bodies")
	.expect_err("expected an error after exhausting endpoints");

	match result {
		TransportError::RpcError { code, message, .. } => {
			assert_eq!(code, 0);
			assert_eq!(message, "Malformed JSON-RPC envelope");
		}
		other => panic!("Expected RpcError, got {:?}", other),
	}

	mock_a.assert();
	mock_b.assert();
}
