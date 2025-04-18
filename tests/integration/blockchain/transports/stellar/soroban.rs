use mockito::Server;
use openzeppelin_monitor::services::blockchain::{
	BlockchainTransport, RotatingTransport, StellarTransportClient,
};
use serde_json::{json, Value};

use crate::integration::mocks::{
	create_stellar_test_network_with_urls, create_stellar_valid_server_mock_network_response,
};

#[tokio::test]
async fn test_client_creation() {
	let mut server = Server::new_async().await;
	let mock = create_stellar_valid_server_mock_network_response(&mut server, "soroban");
	let network = create_stellar_test_network_with_urls(vec![&server.url()], "rpc");

	match StellarTransportClient::new(&network).await {
		Ok(transport) => {
			let active_url = transport.get_current_url().await;
			assert_eq!(active_url, server.url());
			mock.assert();
		}
		Err(e) => panic!("Transport creation failed: {:?}", e),
	}

	let network = create_stellar_test_network_with_urls(vec!["invalid-url"], "rpc");

	match StellarTransportClient::new(&network).await {
		Err(error) => {
			assert!(error.to_string().contains("All RPC URLs failed to connect"));
		}
		_ => panic!("Transport creation should fail"),
	}

	mock.assert();
}

#[tokio::test]
async fn test_client_creation_with_fallback() {
	let mut server = Server::new_async().await;
	let mut server2 = Server::new_async().await;

	let mock = server
		.mock("POST", "/")
		.match_body(r#"{"jsonrpc":"2.0","id":0,"method":"getNetwork"}"#)
		.with_header("content-type", "application/json")
		.with_status(500) // Simulate a failed request
		.create();

	let mock2 = create_stellar_valid_server_mock_network_response(&mut server2, "soroban");

	let network = create_stellar_test_network_with_urls(vec![&server.url(), &server2.url()], "rpc");

	match StellarTransportClient::new(&network).await {
		Ok(transport) => {
			let active_url = transport.get_current_url().await;
			assert_eq!(active_url, server2.url());
			mock.assert();
			mock2.assert();
		}
		Err(e) => panic!("Transport creation failed: {:?}", e),
	}
}

#[tokio::test]
async fn test_client_update_client() {
	let mut server = Server::new_async().await;
	let server2 = Server::new_async().await;

	let mock1 = create_stellar_valid_server_mock_network_response(&mut server, "soroban");

	let network = create_stellar_test_network_with_urls(vec![&server.url()], "rpc");

	let client = StellarTransportClient::new(&network).await.unwrap();

	// Test successful update
	let result = client.update_client(&server2.url()).await;
	assert!(result.is_ok(), "Update to valid URL should succeed");

	assert_eq!(client.get_current_url().await, server2.url());

	// Test invalid URL update
	let result = client.update_client("invalid-url").await;
	assert!(result.is_err(), "Update with invalid URL should fail");
	let e = result.unwrap_err();
	assert!(e.to_string().contains("Failed to create client"));

	// Verify both mock was called the expected number of times
	mock1.assert();
}

#[tokio::test]
async fn test_client_try_connect() {
	let mut server = Server::new_async().await;
	let mut server2 = Server::new_async().await;
	let server3 = Server::new_async().await;

	let mock = create_stellar_valid_server_mock_network_response(&mut server, "soroban");
	let mock2 = create_stellar_valid_server_mock_network_response(&mut server2, "soroban");

	let network = create_stellar_test_network_with_urls(vec![&server.url()], "rpc");
	let client = StellarTransportClient::new(&network).await.unwrap();

	let result = client.try_connect(&server2.url()).await;
	assert!(result.is_ok(), "Try connect should succeed");

	let result = client.try_connect("invalid-url").await;
	assert!(result.is_err(), "Try connect with invalid URL should fail");
	let e = result.unwrap_err();
	assert!(e.to_string().contains("Invalid URL"));

	let result = client.try_connect(&server3.url()).await;
	assert!(result.is_err(), "Try connect with invalid URL should fail");
	let e = result.unwrap_err();
	assert!(e.to_string().contains("Failed to connect"));

	mock.assert();
	mock2.assert();
}

#[tokio::test]
async fn test_send_raw_request() {
	let mut server = Server::new_async().await;

	// First, set up the network verification mock that's called during client creation
	let network_mock = create_stellar_valid_server_mock_network_response(&mut server, "soroban");

	// Then set up the test request mock with the correct field order
	let test_mock = server
		.mock("POST", "/")
		.match_body(r#"{"id":1,"jsonrpc":"2.0","method":"testMethod","params":{"key":"value"}}"#)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":{"data":"success"},"id":1}"#)
		.create();

	let network = create_stellar_test_network_with_urls(vec![&server.url()], "rpc");
	let client = StellarTransportClient::new(&network).await.unwrap();

	// Test with params
	let params = json!({"key": "value"});
	let result = client.send_raw_request("testMethod", Some(params)).await;

	assert!(result.is_ok());
	let response = result.unwrap();
	assert_eq!(response["result"]["data"], "success");

	// Verify both mocks were called
	network_mock.assert();
	test_mock.assert();

	// Test without params
	let no_params_mock = server
		.mock("POST", "/")
		.match_body(r#"{"id":1,"jsonrpc":"2.0","method":"testMethod","params":null}"#)
		.with_header("content-type", "application/json")
		.with_status(200)
		.with_body(r#"{"jsonrpc":"2.0","result":{"data":"success"},"id":1}"#)
		.create();

	let result = client.send_raw_request::<Value>("testMethod", None).await;

	assert!(result.is_ok());
	let response = result.unwrap();
	assert_eq!(response["result"]["data"], "success");
	no_params_mock.assert();
}
