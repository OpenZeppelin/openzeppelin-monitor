use futures_util::{SinkExt, StreamExt};
use mockall::mock;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

use openzeppelin_monitor::services::blockchain::{
	BlockchainTransport, RotatingTransport, TransientErrorRetryStrategy,
};

// Mock implementation of a EVM transport client.
// Used for testing Ethereum compatible blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub EVMTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Vec<Value>>) -> Result<Value, anyhow::Error>;
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
	) -> Result<Value, anyhow::Error>
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
		pub async fn send_raw_request(&self, method: &str, params: Option<Value>) -> Result<Value, anyhow::Error>;
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
	) -> Result<Value, anyhow::Error>
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

// Mock implementation of a Midnight transport client.
// Used for testing Midnight compatible blockchain interactions.
// Provides functionality to simulate raw JSON-RPC request handling.
mock! {
	pub MidnightTransportClient {
		pub async fn send_raw_request(&self, method: &str, params: Option<Vec<Value>>) -> Result<Value, anyhow::Error>;
		pub async fn get_current_url(&self) -> String;
		pub async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error>;
		pub async fn update_client(&self, url: &str) -> Result<(), anyhow::Error>;
	}

	impl Clone for MidnightTransportClient {
		fn clone(&self) -> Self;
	}
}

#[async_trait::async_trait]
impl BlockchainTransport for MockMidnightTransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		method: &str,
		params: Option<P>,
	) -> Result<Value, anyhow::Error>
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
impl RotatingTransport for MockMidnightTransportClient {
	async fn try_connect(&self, url: &str) -> Result<(), anyhow::Error> {
		self.try_connect(url).await
	}

	async fn update_client(&self, url: &str) -> Result<(), anyhow::Error> {
		self.update_client(url).await
	}
}

// Mock implementation of a WebSocket transport client.
// Used for testing WebSocket connections.
mock! {
	pub WsTransportClient {
		pub async fn get_current_url(&self) -> String;
	}

	impl Clone for WsTransportClient {
		fn clone(&self) -> Self;
	}
}

/// Wrapper for WsTransportClient that manages the WebSocket server lifecycle
pub struct WsTransportClientWrapper {
	pub client: MockWsTransportClient,
	_shutdown_tx: oneshot::Sender<()>,
}

impl WsTransportClientWrapper {
	pub async fn new() -> Result<Self, anyhow::Error> {
		let (url, shutdown_tx) = start_test_websocket_server().await;
		let mut client = MockWsTransportClient::new();
		client
			.expect_get_current_url()
			.returning(move || url.clone());
		Ok(Self {
			client,
			_shutdown_tx: shutdown_tx,
		})
	}

	pub async fn _new_with_url(url: String) -> Result<Self, anyhow::Error> {
		let mut client = MockWsTransportClient::new();
		client
			.expect_get_current_url()
			.returning(move || url.clone());
		Ok(Self {
			client,
			_shutdown_tx: oneshot::channel().0,
		})
	}
}

// impl Drop for WsTransportClientWrapper {
// 	fn drop(&mut self) {
// 		// Send shutdown signal when the client is dropped
// 		let _ = std::mem::replace(&mut self.shutdown_tx, oneshot::channel().0).send(());
// 	}
// }

#[async_trait::async_trait]
impl BlockchainTransport for MockWsTransportClient {
	async fn get_current_url(&self) -> String {
		self.get_current_url().await
	}

	async fn send_raw_request<P>(
		&self,
		_method: &str,
		_params: Option<P>,
	) -> Result<Value, anyhow::Error>
	where
		P: Into<Value> + Send + Clone,
	{
		Err(anyhow::anyhow!("`send_raw_request` not implemented"))
	}

	fn set_retry_policy(
		&mut self,
		_: ExponentialBackoff,
		_: Option<TransientErrorRetryStrategy>,
	) -> Result<(), anyhow::Error> {
		Err(anyhow::anyhow!("`set_retry_policy` not implemented"))
	}

	fn update_endpoint_manager_client(
		&mut self,
		_: ClientWithMiddleware,
	) -> Result<(), anyhow::Error> {
		Err(anyhow::anyhow!(
			"`update_endpoint_manager_client` not implemented"
		))
	}
}

/// Start a test WebSocket server that simulates a Substrate client.
/// Returns a URL for the server and a channel for shutting down the server.
///
/// # Returns
///
/// A tuple containing:
/// - The URL of the server
/// - A channel for shutting down the server
pub async fn start_test_websocket_server() -> (String, oneshot::Sender<()>) {
	let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
	let addr = listener.local_addr().unwrap();
	let url = format!("ws://{}", addr);
	let (shutdown_tx, shutdown_rx) = oneshot::channel();

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
														// This will depend on the config of the WebSocket client
														// But for testing purposes we set this at a low number (1s)
														println!("Sleeping for 10 seconds");
														tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
														return;
													},
													"system_chain" => {
														// Send chain response
														let response = json!({
															"jsonrpc": "2.0",
															"id": id,
															"result": "Development"
														});
														let _ = write.send(Message::Text(response.to_string().into())).await;
													}
													"system_chainType" => {
														// Send chain type response
														let response = json!({
															"jsonrpc": "2.0",
															"id": id,
															"result": "Development"
														});
														let _ = write.send(Message::Text(response.to_string().into())).await;
													}
													"chain_subscribeNewHeads" => {
														// Send subscription confirmation
														let response = json!({
															"jsonrpc": "2.0",
															"id": id,
															"result": "0x1"
														});
														let _ = write.send(Message::Text(response.to_string().into())).await;
													}
													"chain_getBlockHash" => {
														// Send block hash response
														let response = json!({
															"jsonrpc": "2.0",
															"id": id,
															"result": "0x0000000000000000000000000000000000000000000000000000000000000000"
														});
														let _ = write.send(Message::Text(response.to_string().into())).await;
													}
													"chain_getFinalizedHead" => {
														// Send finalized head response
														let response = json!({
															"jsonrpc": "2.0",
															"id": id,
															"result": "0x0000000000000000000000000000000000000000000000000000000000000000"
														});
														let _ = write.send(Message::Text(response.to_string().into())).await;
													}
													"state_getRuntimeVersion" => {
														// Send runtime version response
														let response = json!({
															"jsonrpc": "2.0",
															"id": id,
															"result": {
																"specName": "midnight",
																"implName": "midnight-node",
																"authoringVersion": 1,
																"specVersion": 1,
																"implVersion": 1,
																"apis": [],
																"transactionVersion": 1
															}
														});
														let _ = write.send(Message::Text(response.to_string().into())).await;
													}
													"state_call" => {
														let data = std::fs::read_to_string("tests/integration/fixtures/midnight/state_call.json").unwrap();
														let json_response: Value = serde_json::from_str(&data).unwrap();
														let response = json!({
															"jsonrpc": "2.0",
															"id": id,
															"result": json_response["result"]
														});
														let _ = write.send(Message::Text(response.to_string().into())).await;
													}
													"state_getStorage" => {
														// Send storage response
														let response = json!({
															"jsonrpc": "2.0",
															"id": id,
															"result": "0x0000000000000000000000000000000000000000000000000000000000000000"
														});
														let _ = write.send(Message::Text(response.to_string().into())).await;
													}
													_ => {
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
