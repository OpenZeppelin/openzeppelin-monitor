//! Midnight blockchain client implementation.
//!
//! This module provides functionality to interact with Midnight blockchain,
//! supporting operations like block retrieval, transaction receipt lookup,
//! and log filtering.

use std::marker::PhantomData;

use anyhow::Context;
use async_trait::async_trait;
use futures;
use serde_json::json;
use tracing::instrument;

use crate::{
	models::{BlockType, MidnightBlock, MidnightEvent, MidnightTransaction, Network},
	services::{
		blockchain::{
			client::BlockChainClient,
			transports::{BlockchainTransport, MidnightTransportClient},
			BlockFilterFactory,
		},
		filter::MidnightBlockFilter,
	},
};

/// Client implementation for Midnight blockchain
///
/// Provides high-level access to Midnight blockchain data and operations through HTTP transport.
#[derive(Clone)]
pub struct MidnightClient<T: Send + Sync + Clone> {
	/// The underlying Midnight transport client for RPC communication
	http_client: T,
}

impl<T: Send + Sync + Clone> MidnightClient<T> {
	/// Creates a new Midnight client instance with a specific transport client
	pub fn new_with_transport(http_client: T) -> Self {
		Self { http_client }
	}
}

impl MidnightClient<MidnightTransportClient> {
	/// Creates a new Midnight client instance
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints and chain details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let http_client = MidnightTransportClient::new(network).await?;
		Ok(Self::new_with_transport(http_client))
	}
}

impl<T: Send + Sync + Clone + BlockchainTransport> BlockFilterFactory<Self> for MidnightClient<T> {
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
	/// Retrieves transactions within a block range
	///
	/// # Arguments
	/// * `start_block` - Starting block number
	/// * `end_block` - Optional ending block number. If None, only fetches start_block
	///
	/// # Returns
	/// * `Result<Vec<MidnightTransaction>, anyhow::Error>` - Collection of transactions or error
	async fn get_transactions(
		&self,
		start_block: u32,
		end_block: Option<u32>,
	) -> Result<Vec<MidnightTransaction>, anyhow::Error>;

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
		start_block: u32,
		end_block: Option<u32>,
	) -> Result<Vec<MidnightEvent>, anyhow::Error>;
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> MidnightClientTrait for MidnightClient<T> {
	/// Retrieves transactions within a block range
	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_transactions(
		&self,
		start_block: u32,
		end_block: Option<u32>,
	) -> Result<Vec<MidnightTransaction>, anyhow::Error> {
		let params = json!([
			format!("0x{:x}", start_block),
			format!("0x{:x}", end_block.unwrap_or(start_block))
		]);

		let _response = self
			.http_client
			.send_raw_request::<serde_json::Value>("midnight_getTransactions", Some(params))
			.await
			.with_context(|| "Failed to get transactions")?;

		Ok(Vec::<MidnightTransaction>::new())
	}

	/// Retrieves events within a block range
	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_events(
		&self,
		_start_block: u32,
		_end_block: Option<u32>,
	) -> Result<Vec<MidnightEvent>, anyhow::Error> {
		Ok(Vec::<MidnightEvent>::new())
	}
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> BlockChainClient for MidnightClient<T> {
	/// Retrieves the latest block number with retry functionality
	#[instrument(skip(self))]
	async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
		let response = self
			.http_client
			.send_raw_request::<serde_json::Value>("chain_getHeader", None)
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
