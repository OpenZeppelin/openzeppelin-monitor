//! Midnight blockchain client implementation.
//!
//! This module provides functionality to interact with the Midnight blockchain network,
//! supporting operations like block retrieval, event handling, and chain information.

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
			client::BlockChainClient, transports::BlockchainTransport, BlockFilterFactory,
			MidnightWsTransportClient,
		},
		filter::MidnightBlockFilter,
	},
};

/// Client implementation for Midnight blockchain
///
/// Provides high-level access to Midnight blockchain data and operations through HTTP and WebSocket transport.
/// The client supports both generic transport implementations and specific Substrate client configurations.
///
/// # Type Parameters
/// * `W` - The WebSocket transport client type, must implement Send, Sync, and Clone
/// * `S` - The Substrate client type, defaults to OnlineClient<subxt::SubstrateConfig>
#[derive(Clone)]
pub struct MidnightClient<
	W: Send + Sync + Clone,
	S: SubstrateClientTrait = OnlineClient<subxt::SubstrateConfig>,
> {
	/// The underlying Midnight transport client for RPC communication
	ws_client: W,
	/// The Substrate client for event handling
	substrate_client: S,
}

impl<W: Send + Sync + Clone, S: SubstrateClientTrait> MidnightClient<W, S> {
	/// Creates a new Midnight client instance with specific transport clients
	///
	/// # Arguments
	/// * `ws_client` - The WebSocket transport client
	/// * `substrate_client` - The Substrate client for event handling
	///
	/// # Returns
	/// A new instance of MidnightClient
	pub fn new_with_transport(ws_client: W, substrate_client: S) -> Self {
		Self {
			ws_client,
			substrate_client,
		}
	}
}

impl MidnightClient<MidnightWsTransportClient, OnlineClient<subxt::SubstrateConfig>> {
	/// Creates a new Midnight client instance with default configuration
	///
	/// This constructor creates both the WebSocket transport client and the Substrate client
	/// using the provided network configuration.
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints and chain details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let ws_client = MidnightWsTransportClient::new(network, None).await?;
		let substrate_client = OnlineClient::<subxt::SubstrateConfig>::from_insecure_url(
			ws_client.get_current_url().await,
		)
		.await
		.map_err(|e| anyhow::anyhow!("Failed to create subxt client: {}", e))?;
		Ok(Self::new_with_transport(ws_client, substrate_client))
	}
}

#[async_trait]
impl<W: Send + Sync + Clone + BlockchainTransport> BlockFilterFactory<Self> for MidnightClient<W> {
	type Filter = MidnightBlockFilter<Self>;
	fn filter() -> Self::Filter {
		MidnightBlockFilter {
			_client: PhantomData,
		}
	}
}

/// Trait for Substrate client implementation
///
/// Provides a method to get events from the Substrate client. This trait is implemented
/// for types that can retrieve events from a Substrate-based blockchain.
#[async_trait]
pub trait SubstrateClientTrait: Send + Sync + Clone {
	/// Get events at a specific block hash
	///
	/// # Arguments
	/// * `block_hash` - The hash of the block to retrieve events from
	///
	/// # Returns
	/// * `Result<subxt::events::Events<subxt::SubstrateConfig>, subxt::Error>` - The events or an error
	async fn get_events_at(
		&self,
		block_hash: subxt::utils::H256,
	) -> Result<subxt::events::Events<subxt::SubstrateConfig>, subxt::Error>;

	async fn get_finalized_block(
		&self,
	) -> Result<
		subxt::blocks::Block<subxt::SubstrateConfig, OnlineClient<subxt::SubstrateConfig>>,
		subxt::Error,
	>;
}

/// Default implementation for Substrate client trait
///
/// Provides a default implementation for the Substrate client trait using the OnlineClient
/// from the subxt crate.
#[async_trait]
impl SubstrateClientTrait for OnlineClient<subxt::SubstrateConfig> {
	async fn get_events_at(
		&self,
		block_hash: subxt::utils::H256,
	) -> Result<subxt::events::Events<subxt::SubstrateConfig>, subxt::Error> {
		self.events().at(block_hash).await
	}

	async fn get_finalized_block(
		&self,
	) -> Result<
		subxt::blocks::Block<subxt::SubstrateConfig, OnlineClient<subxt::SubstrateConfig>>,
		subxt::Error,
	> {
		self.blocks().at_latest().await
	}
}

/// Extended functionality specific to Midnight blockchain
///
/// This trait provides additional methods specific to the Midnight blockchain,
/// such as event retrieval and chain type information.
#[async_trait]
pub trait MidnightClientTrait {
	/// Retrieves events within a block range
	///
	/// Fetches and decodes events from the specified block range. The events are
	/// retrieved in parallel for better performance.
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
	/// Gets the chain type information from the Midnight blockchain.
	/// This is specific for Polkadot-based chains.
	///
	/// # Returns
	/// * `Result<String, anyhow::Error>` - Chain type
	async fn get_chain_type(&self) -> Result<String, anyhow::Error>;
}

#[async_trait]
impl<W: Send + Sync + Clone + BlockchainTransport, S: SubstrateClientTrait> MidnightClientTrait
	for MidnightClient<W, S>
{
	/// Retrieves events within a block range
	/// Compactc does not support events yet
	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_events(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<MidnightEvent>, anyhow::Error> {
		let end_block = end_block.unwrap_or(start_block);
		if start_block > end_block {
			return Err(anyhow::anyhow!(
				"start_block {} cannot be greater than end_block {}",
				start_block,
				end_block
			));
		}
		let block_range = start_block..=end_block;

		// Fetch block hashes in parallel
		let block_hashes = futures::future::join_all(block_range.clone().map(|block_number| {
			let client = self.ws_client.clone();
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
			let client = self.substrate_client.clone();
			async move {
				client
					.get_events_at(block_hash)
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
				let client = self.ws_client.clone();
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
			.ws_client
			.send_raw_request::<serde_json::Value>("system_chain", None)
			.await
			.with_context(|| "Failed to get chain type")?;

		response
			.get("result")
			.and_then(|v| v.as_str())
			.map(|s| s.to_string())
			.ok_or_else(|| anyhow::anyhow!("Missing or invalid 'result' field"))
	}
}

#[async_trait]
impl<W: Send + Sync + Clone + BlockchainTransport, S: SubstrateClientTrait> BlockChainClient
	for MidnightClient<W, S>
{
	/// Retrieves the latest block number with retry functionality
	///
	/// This method ensures we get the correct finalized blocks by first getting the finalized head block hash
	/// and then retrieving its number. This handles potential race conditions where different nodes
	/// might be at different stages of finalization.
	///
	/// # Returns
	/// * `Result<u64, anyhow::Error>` - Latest block number
	#[instrument(skip(self))]
	async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
		// Get latest finalized head hash
		let response = self
			.ws_client
			.send_raw_request::<serde_json::Value>("chain_getFinalisedHead", None)
			.await
			.with_context(|| "Failed to get latest block number")?;

		let finalised_block_hash = response
			.get("result")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

		let params = json!([finalised_block_hash]);

		let response = self
			.ws_client
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
	/// Fetches blocks in parallel for better performance. Each block is retrieved using its hash
	/// and then parsed into a MidnightBlock structure.
	///
	/// # Arguments
	/// * `start_block` - Starting block number
	/// * `end_block` - Optional ending block number. If None, only fetches start_block
	///
	/// # Returns
	/// * `Result<Vec<BlockType>, anyhow::Error>` - Collection of blocks or error
	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, anyhow::Error> {
		let end_block = end_block.unwrap_or(start_block);
		if start_block > end_block {
			return Err(anyhow::anyhow!(
				"start_block {} cannot be greater than end_block {}",
				start_block,
				end_block
			));
		}

		let block_futures: Vec<_> = (start_block..=end_block)
			.map(|block_number| {
				let params = json!([format!("0x{:x}", block_number)]);
				let client = self.ws_client.clone();

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
