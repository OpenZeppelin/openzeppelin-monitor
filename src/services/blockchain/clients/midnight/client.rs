//! Midnight blockchain client implementation.
//!
//! This module provides functionality to interact with Midnight blockchain,
//! supporting operations like block retrieval.

use anyhow::Context;
use async_trait::async_trait;
use futures;
use serde_json::json;
use std::marker::PhantomData;
use std::str::FromStr;
use subxt::client::OnlineClient;
use tracing::instrument;

use crate::{
	models::{BlockType, MidnightBlock, MidnightEvent, Network},
	services::{
		blockchain::{
			client::BlockChainClient,
			transports::{BlockchainTransport, MidnightTransportClient},
			BlockFilterFactory, WsTransportClient,
		},
		filter::MidnightBlockFilter,
	},
};

/// Client implementation for Midnight blockchain
///
/// Provides high-level access to Midnight blockchain data and operations through HTTP and WebSocket transport.
#[derive(Clone)]
pub struct MidnightClient<H: Send + Sync + Clone, W: Send + Sync + Clone> {
	/// The underlying Midnight transport client for RPC communication
	http_client: H,
	ws_client: Option<W>,
}

impl<H: Send + Sync + Clone, W: Send + Sync + Clone> MidnightClient<H, W> {
	/// Creates a new Midnight client instance with specific transport clients
	pub fn new_with_transport(http_client: H, ws_client: Option<W>) -> Self {
		Self {
			http_client,
			ws_client,
		}
	}
}

impl MidnightClient<MidnightTransportClient, WsTransportClient> {
	/// Creates a new Midnight client instance
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints and chain details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let http_client = MidnightTransportClient::new(network).await?;
		let ws_client = WsTransportClient::new(network).await.map_or_else(
			|e| {
				// We fail to create a WebSocket client if there are no working URLs
				// This limits the functionality of the service, by not allowing monitoring of transaction status or event-related data
				// but it is not a critical issue
				tracing::warn!("Failed to create WebSocket client: {}", e);
				None
			},
			Some,
		);
		Ok(Self::new_with_transport(http_client, ws_client))
	}
}

#[async_trait]
impl<
		H: Send + Sync + Clone + BlockchainTransport,
		W: Send + Sync + Clone + BlockchainTransport,
	> BlockFilterFactory<Self> for MidnightClient<H, W>
{
	type Filter = MidnightBlockFilter<Self>;
	fn filter() -> Self::Filter {
		MidnightBlockFilter {
			_client: PhantomData,
		}
	}
}

/// Extended functionality specific to Midnight blockchain
#[async_trait]
pub trait MidnightClientTrait {
	/// Retrieves events within a block range
	///
	/// # Arguments
	/// * `start_block` - Starting block number
	/// * `end_block` - Optional ending block number. If None, only fetches start_block
	///
	/// # Returns
	/// * `Result<Vec<MidnightEvent>, anyhow::Error>` - Collection of events or error
	async fn get_events(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<MidnightEvent>, anyhow::Error>;

	/// Retrieves the chain type
	///
	/// This is specific for Polkadot-based chains
	///
	/// # Returns
	/// * `Result<String, anyhow::Error>` - Chain type
	async fn get_chain_type(&self) -> Result<String, anyhow::Error>;
}

#[async_trait]
impl<
		H: Send + Sync + Clone + BlockchainTransport,
		W: Send + Sync + Clone + BlockchainTransport,
	> MidnightClientTrait for MidnightClient<H, W>
{
	/// Retrieves events within a block range
	/// Compactc does not support events yet
	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_events(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<MidnightEvent>, anyhow::Error> {
		let websocket_client = self
			.ws_client
			.as_ref()
			.ok_or_else(|| anyhow::anyhow!("WebSocket client not initialized"))?;

		let substrate_client = OnlineClient::<subxt::SubstrateConfig>::from_insecure_url(
			websocket_client.get_current_url().await,
		)
		.await
		.map_err(|e| anyhow::anyhow!("Failed to create subxt client: {}", e))?;

		let end_block = end_block.unwrap_or(start_block);
		let block_range = start_block..=end_block;

		// Fetch block hashes in parallel
		let block_hashes = futures::future::join_all(block_range.clone().map(|block_number| {
			let client = self.http_client.clone();
			async move {
				let params = json!([format!("0x{:x}", block_number)]);
				let response = client
					.send_raw_request("chain_getBlockHash", Some(params))
					.await
					.with_context(|| format!("Failed to get block hash for: {}", block_number))?;

				let hash_str = response
					.get("result")
					.and_then(|v| v.as_str())
					.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

				subxt::utils::H256::from_str(hash_str)
					.map_err(|e| anyhow::anyhow!("Failed to parse block hash: {}", e))
			}
		}))
		.await
		.into_iter()
		.collect::<Result<Vec<_>, _>>()?;

		// Fetch events for each block in parallel
		let raw_events = futures::future::join_all(block_hashes.into_iter().map(|block_hash| {
			let client = substrate_client.clone();
			async move {
				client
					.events()
					.at(block_hash)
					.await
					.map_err(|e| anyhow::anyhow!("Failed to get events: {}", e))
			}
		}))
		.await
		.into_iter()
		.collect::<Result<Vec<_>, _>>()?;

		// Decode events in parallel
		let decoded_events =
			futures::future::join_all(raw_events.into_iter().map(|block_events| {
				let client = self.http_client.clone();
				async move {
					let event_bytes = block_events.bytes();
					let params = json!([hex::encode(event_bytes)]);
					let response = client
						.send_raw_request("midnight_decodeEvents", Some(params))
						.await?;

					let response_result = response
						.get("result")
						.and_then(|v| v.as_array())
						.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

					Ok::<Vec<MidnightEvent>, anyhow::Error>(
						response_result
							.iter()
							.map(|v| MidnightEvent::from(v.clone()))
							.collect(),
					)
				}
			}))
			.await
			.into_iter()
			.collect::<Result<Vec<_>, _>>()?
			.into_iter()
			.flatten()
			.collect();

		Ok(decoded_events)
	}

	/// Retrieves the chain type
	#[instrument(skip(self))]
	async fn get_chain_type(&self) -> Result<String, anyhow::Error> {
		let response = self
			.http_client
			.send_raw_request::<serde_json::Value>("system_chain", None)
			.await
			.with_context(|| "Failed to get chain type")?;

		Ok(response
			.get("result")
			.and_then(|v| v.as_str())
			.unwrap_or_default()
			.to_string())
	}
}

#[async_trait]
impl<
		H: Send + Sync + Clone + BlockchainTransport,
		W: Send + Sync + Clone + BlockchainTransport,
	> BlockChainClient for MidnightClient<H, W>
{
	/// Retrieves the latest block number with retry functionality
	///
	/// Blocks may only be finalised on a particular node, and not on others due to load-balancing.
	/// This means it's possible for there to be multiple blocks with the same number (height).
	/// To handle this race condition, we first get the finalized head block hash and number,
	/// This ensures we get the correct finalized blocks even if different nodes are at different
	/// stages of finalization.
	///
	/// # Returns
	/// * `Result<u64, anyhow::Error>` - Latest block number
	#[instrument(skip(self))]
	async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
		// Get latest finalized head hash
		let response = self
			.http_client
			.send_raw_request::<serde_json::Value>("chain_getFinalisedHead", None)
			.await
			.with_context(|| "Failed to get latest block number")?;

		let finalised_block_hash = response
			.get("result")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

		let params = json!([finalised_block_hash]);

		let response = self
			.http_client
			.send_raw_request::<serde_json::Value>("chain_getHeader", Some(params))
			.await
			.with_context(|| "Failed to get latest block number")?;

		// Extract the "result" field and then the "number" field from the JSON-RPC response
		let hex_str = response
			.get("result")
			.and_then(|v| v.get("number"))
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing block number in response"))?;

		// Parse hex string to u64
		u64::from_str_radix(hex_str.trim_start_matches("0x"), 16)
			.map_err(|e| anyhow::anyhow!("Failed to parse block number: {}", e))
	}

	/// Retrieves blocks within the specified range with retry functionality
	///
	/// # Note
	/// If end_block is None, only the start_block will be retrieved
	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, anyhow::Error> {
		let block_futures: Vec<_> = (start_block..=end_block.unwrap_or(start_block))
			.map(|block_number| {
				let params = json!([format!("0x{:x}", block_number)]);
				let client = self.http_client.clone();

				async move {
					let response = client
						.send_raw_request("chain_getBlockHash", Some(params))
						.await
						.with_context(|| {
							format!("Failed to get block hash for: {}", block_number)
						})?;

					let block_hash = response
						.get("result")
						.and_then(|v| v.as_str())
						.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

					let params = json!([block_hash]);

					let response = client
						.send_raw_request("midnight_jsonBlock", Some(params))
						.await
						.with_context(|| format!("Failed to get block: {}", block_number))?;

					let block_data = response
						.get("result")
						.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

					// Parse the JSON string into a Value
					let block_value: serde_json::Value = serde_json::from_str(
						block_data
							.as_str()
							.ok_or_else(|| anyhow::anyhow!("Result is not a string"))?,
					)
					.with_context(|| "Failed to parse block JSON string")?;

					if block_value.is_null() {
						return Err(anyhow::anyhow!("Block not found"));
					}

					let block: MidnightBlock = serde_json::from_value(block_value.clone())
						.map_err(|e| anyhow::anyhow!("Failed to parse block: {}", e))?;

					Ok(BlockType::Midnight(Box::new(block)))
				}
			})
			.collect();

		futures::future::join_all(block_futures)
			.await
			.into_iter()
			.collect::<Result<Vec<_>, _>>()
	}
}
