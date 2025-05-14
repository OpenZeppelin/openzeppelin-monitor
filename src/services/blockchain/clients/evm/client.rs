//! EVM-compatible blockchain client implementation.
//!
//! This module provides functionality to interact with Ethereum and other EVM-compatible
//! blockchains, supporting operations like block retrieval, transaction receipt lookup,
//! and log filtering.

use std::marker::PhantomData;

use anyhow::Context;
use async_trait::async_trait;
use futures;
use serde_json::json;
use tracing::instrument;

use crate::{
	models::{BlockType, EVMBlock, EVMReceiptLog, EVMTransactionReceipt, Network},
	services::{
		blockchain::{
			client::BlockChainClient,
			transports::{BlockchainTransport, EVMTransportClient},
			BlockFilterFactory,
		},
		filter::{evm_helpers::string_to_h256, EVMBlockFilter},
	},
};

/// Client implementation for Ethereum Virtual Machine (EVM) compatible blockchains
///
/// Provides high-level access to EVM blockchain data and operations through HTTP transport.
#[derive(Clone)]
pub struct EvmClient<T: Send + Sync + Clone> {
	/// The underlying HTTP transport client for RPC communication
	http_client: T,
}

impl<T: Send + Sync + Clone> EvmClient<T> {
	/// Creates a new EVM client instance with a specific transport client
	pub fn new_with_transport(http_client: T) -> Self {
		Self { http_client }
	}
}

impl EvmClient<EVMTransportClient> {
	/// Creates a new EVM client instance
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints and chain details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let client = EVMTransportClient::new(network).await?;
		Ok(Self::new_with_transport(client))
	}
}

impl<T: Send + Sync + Clone + BlockchainTransport> BlockFilterFactory<Self> for EvmClient<T> {
	type Filter = EVMBlockFilter<Self>;
	fn filter() -> Self::Filter {
		EVMBlockFilter {
			_client: PhantomData,
		}
	}
}

/// Extended functionality specific to EVM-compatible blockchains
#[async_trait]
pub trait EvmClientTrait {
	/// Retrieves a transaction receipt by its hash
	///
	/// # Arguments
	/// * `transaction_hash` - The hash of the transaction to look up
	///
	/// # Returns
	/// * `Result<TransactionReceipt, anyhow::Error>` - Transaction receipt or error
	async fn get_transaction_receipt(
		&self,
		transaction_hash: String,
	) -> Result<EVMTransactionReceipt, anyhow::Error>;

	/// Retrieves logs for a range of blocks
	///
	/// # Arguments
	/// * `from_block` - Starting block number
	/// * `to_block` - Ending block number
	/// * `addresses` - Optional list of addresses to filter logs by
	/// # Returns
	/// * `Result<Vec<Log>, anyhow::Error>` - Collection of matching logs or error
	async fn get_logs_for_blocks(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Option<Vec<String>>,
	) -> Result<Vec<EVMReceiptLog>, anyhow::Error>;
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> EvmClientTrait for EvmClient<T> {
	/// Retrieves a transaction receipt by hash with proper error handling
	#[instrument(skip(self), fields(transaction_hash))]
	async fn get_transaction_receipt(
		&self,
		transaction_hash: String,
	) -> Result<EVMTransactionReceipt, anyhow::Error> {
		let hash = string_to_h256(&transaction_hash)
			.map_err(|e| anyhow::anyhow!("Invalid transaction hash: {}", e))?;

		let params = json!([format!("0x{:x}", hash)])
			.as_array()
			.with_context(|| "Failed to create JSON-RPC params array")?
			.to_vec();

		let response = self
			.http_client
			.send_raw_request(
				"eth_getTransactionReceipt",
				Some(serde_json::Value::Array(params)),
			)
			.await
			.with_context(|| format!("Failed to get transaction receipt: {}", transaction_hash))?;

		// Extract the "result" field from the JSON-RPC response
		let receipt_data = response
			.get("result")
			.with_context(|| "Missing 'result' field")?;

		// Handle null response case
		if receipt_data.is_null() {
			return Err(anyhow::anyhow!("Transaction receipt not found"));
		}

		Ok(serde_json::from_value(receipt_data.clone())
			.with_context(|| "Failed to parse transaction receipt")?)
	}

	/// Retrieves logs within the specified block range
	///
	/// # Arguments
	/// * `from_block` - Starting block number
	/// * `to_block` - Ending block number
	/// * `addresses` - Optional list of addresses to filter logs by
	/// # Returns
	/// * `Result<Vec<EVMReceiptLog>, anyhow::Error>` - Collection of matching logs or error
	#[instrument(skip(self), fields(from_block, to_block))]
	async fn get_logs_for_blocks(
		&self,
		from_block: u64,
		to_block: u64,
		addresses: Option<Vec<String>>,
	) -> Result<Vec<EVMReceiptLog>, anyhow::Error> {
		// Convert parameters to JSON-RPC format
		let params = json!([{
			"fromBlock": format!("0x{:x}", from_block),
			"toBlock": format!("0x{:x}", to_block),
			"address": addresses
		}])
		.as_array()
		.with_context(|| "Failed to create JSON-RPC params array")?
		.to_vec();

		let response = self
			.http_client
			.send_raw_request("eth_getLogs", Some(params))
			.await
			.with_context(|| {
				format!(
					"Failed to get logs for blocks: {} - {}",
					from_block, to_block
				)
			})?;

		// Extract the "result" field from the JSON-RPC response
		let logs_data = response
			.get("result")
			.with_context(|| "Missing 'result' field")?;

		// Parse the response into the expected type
		Ok(serde_json::from_value(logs_data.clone()).with_context(|| "Failed to parse logs")?)
	}
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> BlockChainClient for EvmClient<T> {
	/// Retrieves the latest block number with retry functionality
	#[instrument(skip(self))]
	async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
		let response = self
			.http_client
			.send_raw_request::<serde_json::Value>("eth_blockNumber", None)
			.await
			.with_context(|| "Failed to get latest block number")?;

		// Extract the "result" field from the JSON-RPC response
		let hex_str = response
			.get("result")
			.and_then(|v| v.as_str())
			.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

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
				let params = json!([
					format!("0x{:x}", block_number),
					true // include full transaction objects
				]);
				let client = self.http_client.clone();

				async move {
					let response = client
						.send_raw_request("eth_getBlockByNumber", Some(params))
						.await
						.with_context(|| format!("Failed to get block: {}", block_number))?;

					let block_data = response
						.get("result")
						.ok_or_else(|| anyhow::anyhow!("Missing 'result' field"))?;

					if block_data.is_null() {
						return Err(anyhow::anyhow!("Block not found"));
					}

					let block: EVMBlock = serde_json::from_value(block_data.clone())
						.map_err(|e| anyhow::anyhow!("Failed to parse block: {}", e))?;

					Ok(BlockType::EVM(Box::new(block)))
				}
			})
			.collect();

		futures::future::join_all(block_futures)
			.await
			.into_iter()
			.collect::<Result<Vec<_>, _>>()
	}
}
