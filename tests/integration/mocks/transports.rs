use futures_util::{SinkExt, StreamExt};
use mockall::mock;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

use openzeppelin_monitor::services::blockchain::{
	BlockchainTransport, RotatingTransport, TransientErrorRetryStrategy, TransportError,
};

// Mock implementation of a EVM transport client.
// Used for testing Ethereum compatible blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub EVMTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Vec<Value>>) -> Result<Value, TransportError>;
		pub async fn get_current_url(&self) -> String;
	}

	impl Clone for EVMTransportClient {
		fn clone(&self) -> Self;
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockEVMTransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone,
	{
		let params_value = params.map(|p| p.into());
		self.send_raw_request(method, params_value.and_then(|v| v.as_array().cloned()))
			.await
	}

	fn set_retry_policy(
		&mut self,
		_: ExponentialBackoff,
		_: Option<TransientErrorRetryStrategy>,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}

	fn update_endpoint_manager_client(
		&mut self,
		_: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockEVMTransportClient {
	async fn try_connect(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}

	async fn update_client(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

// Mock implementation of a Stellar transport client.
// Used for testing Stellar blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub StellarTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Value>) -> Result<Value, TransportError>;
		pub async fn get_current_url(&self) -> String;
	}

	impl Clone for StellarTransportClient {
		fn clone(&self) -> Self;
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockStellarTransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone,
	{
		self.send_raw_request(method, params.map(|p| p.into()))
			.await
	}

	fn set_retry_policy(
		&mut self,
		_: ExponentialBackoff,
		_: Option<TransientErrorRetryStrategy>,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}

	fn update_endpoint_manager_client(
		&mut self,
		_: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockStellarTransportClient {
	async fn try_connect(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}

	async fn update_client(&self, _url: &str) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

// Mock implementation of a WebSocket transport client.
// Used for testing WebSocket connections.
mock! {
	pub MidnightWsTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Vec<Value>>) -> Result<Value, TransportError>;
		pub async fn get_current_url(&self) -> String;
		pub async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error>;
		pub async fn update_client(&self, url: &str) -> Result<(), anyhow::Error>;
	}

	impl Clone for MidnightWsTransportClient {
		fn clone(&self) -> Self;
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockMidnightWsTransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, TransportError>
	where
		P: Into<Value> + Send + Clone,
	{
		let params_value = params.map(|p| p.into());
		self.send_raw_request(method, params_value.and_then(|v| v.as_array().cloned()))
			.await
	}

	fn set_retry_policy(
		&mut self,
		_: ExponentialBackoff,
		_: Option<TransientErrorRetryStrategy>,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}

	fn update_endpoint_manager_client(
		&mut self,
		_: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		Ok(())
	}
}

#[async_trait::async_trait]
impl RotatingTransport for MockMidnightWsTransportClient {
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		self.try_connect(url).await
	}

	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		self.update_client(url).await
	}
}

/// Type alias for method responses
pub type MethodResponse = Box<dyn Fn(&Value) -> Value + Send + Sync>;

/// Start a test WebSocket server that simulates a Substrate client.
/// Returns a URL for the server and a channel for shutting down the server.
///
/// # Arguments
///
/// * `method_responses` - A map of method names to their response functions.
///   Each function takes the request and returns a response value.
///
/// # Returns
///
/// A tuple containing:
/// - The URL of the server
/// - A channel for shutting down the server
pub async fn start_test_websocket_server(
	method_responses: Option<std::collections::HashMap<String, MethodResponse>>,
) -> (String, oneshot::Sender<()>) {
	let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
	let addr = listener.local_addr().unwrap();
	let url = format!("ws://{}", addr);
	let (shutdown_tx, shutdown_rx) = oneshot::channel();
	let method_responses = Arc::new(method_responses.unwrap_or_default());

	tokio::spawn(async move {
		let mut shutdown_rx = shutdown_rx;
		let mut handles = Vec::new();
		let listener = Arc::new(listener);

		loop {
			let listener = listener.clone();
			tokio::select! {
				accept_result = listener.accept() => {
					if let Ok((stream, _addr)) = accept_result {
						// First, read the HTTP upgrade request
						let mut buf = [0u8; 1024];
						let n = match stream.peek(&mut buf).await {
							Ok(n) => n,
							Err(_) => continue,
						};

						// Check if this is a WebSocket upgrade request
						let request = String::from_utf8_lossy(&buf[..n]);

						if !request.contains("Upgrade: websocket") {
							continue;
						}

						// Now accept the WebSocket connection
						let ws_stream = match tokio_tungstenite::accept_async(stream).await {
							Ok(ws_stream) => ws_stream,
							Err(_) => continue,
						};

						let (write, read) = ws_stream.split();
						let method_responses = method_responses.clone();

						// Spawn a new task to handle this connection
						let handle = tokio::spawn(async move {
							let mut write = write;
							let mut read = read;

							while let Some(msg) = read.next().await {
								match msg {
									Ok(Message::Text(text)) => {
										// Parse the incoming message
										if let Ok(request) = serde_json::from_str::<Value>(&text) {
											// Get the request ID
											let id = request.get("id").cloned();

											// Create a mock response for all methods called by substrate client upon connection
											if let Some(method) = request.get("method").and_then(|m| m.as_str()) {
												match method {
													"timeout_test" => {
														// Sleep for 10 seconds to cause a timeout
														tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
														return;
													},
													_ => {
														if let Some(response_fn) = method_responses.get(method) {
															let response = response_fn(&request);
															let _ = write.send(Message::Text(response.to_string().into())).await;
														} else {
															// Send error for unknown methods
															let response = json!({
																"jsonrpc": "2.0",
																"id": id,
																"error": {
																	"code": -32601,
																	"message": format!("Method not found: {}", method)
																}
															});
															let _ = write.send(Message::Text(response.to_string().into())).await;
														}
													}
												}
											}
										}
									}
									Ok(Message::Close(_)) => {
										break;
									}
									Ok(Message::Ping(data)) => {
										let _ = write.send(Message::Pong(data)).await;
									}
									Ok(Message::Pong(_)) => {
										continue;
									}
									Err(_) => {
										break;
									}
									_ => {
										continue;
									}
								}
							}
						});

						handles.push(handle);
					}
				}
				_ = &mut shutdown_rx => {
					// Abort all connection tasks
					for handle in handles {
						handle.abort();
					}
					// Drop the listener to stop accepting new connections
					drop(listener);
					break;
				}
			}
		}
	});

	(url, shutdown_tx)
}

pub fn create_method_response(
	responses: &mut HashMap<String, MethodResponse>,
	method: &str,
	result: &Value,
) {
	let result = result.clone();
	responses.insert(
		method.to_string(),
		Box::new(move |request: &Value| {
			let request_id = request.get("id").cloned();
			json!({
				"jsonrpc": "2.0",
				"id": request_id,
				"result": result
			})
		}) as MethodResponse,
	);
}

/// Helper function to create default method responses for common Substrate methods
pub fn create_default_method_responses() -> HashMap<String, MethodResponse> {
	let mut responses = HashMap::new();

	// Add default responses for common methods
	create_method_response(&mut responses, "system_chain", &json!("testnet-02-1"));
	create_method_response(&mut responses, "system_chainType", &json!("Development"));
	create_method_response(&mut responses, "chain_subscribeNewHeads", &json!("0x1"));
	create_method_response(
		&mut responses,
		"chain_getBlockHash",
		&json!("0x0000000000000000000000000000000000000000000000000000000000000000"),
	);
	create_method_response(
		&mut responses,
		"chain_getFinalizedHead",
		&json!("0x0000000000000000000000000000000000000000000000000000000000000000"),
	);
	create_method_response(
		&mut responses,
		"chain_getFinalisedHead",
		&json!("0x0000000000000000000000000000000000000000000000000000000000000000"),
	);
	create_method_response(
		&mut responses,
		"state_getRuntimeVersion",
		&json!({
			"specName": "midnight",
			"implName": "midnight-node",
			"authoringVersion": 1,
			"specVersion": 1,
			"implVersion": 1,
			"apis": [],
			"transactionVersion": 1
		}),
	);
	create_method_response(
		&mut responses,
		"state_getStorage",
		&json!("0x0000000000000000000000000000000000000000000000000000000000000000"),
	);

	// Special case for state_call as it needs to read from a file
	let data =
		std::fs::read_to_string("tests/integration/fixtures/midnight/state_call.json").unwrap();
	let json_response: Value = serde_json::from_str(&data).unwrap();
	create_method_response(&mut responses, "state_call", &json_response["result"]);

	responses
}
