//! EVM-compatible blockchain client implementation.
//!
//! This module provides functionality to interact with Ethereum and other EVM-compatible
//! blockchains, supporting operations like block retrieval, transaction receipt lookup,
//! and log filtering.

use std::marker::PhantomData;

use async_trait::async_trait;
use serde_json::json;

use crate::{
	models::{BlockType, EVMBlock, Network},
	services::{
		blockchain::{
			client::BlockChainClient,
			transports::{BlockchainTransport, Web3TransportClient},
			BlockChainError, BlockFilterFactory,
		},
		filter::{evm_helpers::string_to_h256, EVMBlockFilter},
	},
};

/// Client implementation for Ethereum Virtual Machine (EVM) compatible blockchains
///
/// Provides high-level access to EVM blockchain data and operations through Web3
/// transport layer.
#[derive(Clone)]
pub struct EvmClient<T: Send + Sync + Clone> {
	/// The underlying Web3 transport client for RPC communication
	web3_client: T,
}

impl<T: Send + Sync + Clone> EvmClient<T> {
	/// Creates a new EVM client instance with a specific transport client
	pub fn new_with_transport(web3_client: T) -> Self {
		Self { web3_client }
	}
}

impl EvmClient<Web3TransportClient> {
	/// Creates a new EVM client instance
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints and chain details
	///
	/// # Returns
	/// * `Result<Self, BlockChainError>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
		let web3_client = Web3TransportClient::new(network).await?;
		Ok(Self::new_with_transport(web3_client))
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
	/// * `Result<TransactionReceipt, BlockChainError>` - Transaction receipt or error
	async fn get_transaction_receipt(
		&self,
		transaction_hash: String,
	) -> Result<web3::types::TransactionReceipt, BlockChainError>;

	/// Retrieves logs for a range of blocks
	///
	/// # Arguments
	/// * `from_block` - Starting block number
	/// * `to_block` - Ending block number
	///
	/// # Returns
	/// * `Result<Vec<Log>, BlockChainError>` - Collection of matching logs or error
	async fn get_logs_for_blocks(
		&self,
		from_block: u64,
		to_block: u64,
	) -> Result<Vec<web3::types::Log>, BlockChainError>;
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> EvmClientTrait for EvmClient<T> {
	/// Retrieves a transaction receipt by hash with proper error handling
	///
	/// # Errors
	/// - Returns `BlockChainError::InternalError` if the hash format is invalid
	/// - Returns `BlockChainError::RequestError` if the receipt is not found
	async fn get_transaction_receipt(
		&self,
		transaction_hash: String,
	) -> Result<web3::types::TransactionReceipt, BlockChainError> {
		let hash = string_to_h256(&transaction_hash).map_err(|e| {
			BlockChainError::internal_error(format!(
				"Invalid transaction hash ({}): {}",
				transaction_hash, e
			))
		})?;

		let params = json!([format!("0x{:x}", hash)])
			.as_array()
			.ok_or_else(|| {
				BlockChainError::internal_error(
					"Failed to create JSON-RPC params array".to_string(),
				)
			})?
			.to_vec();

		let response = self
			.web3_client
			.send_raw_request(
				"eth_getTransactionReceipt",
				Some(serde_json::Value::Array(params)),
			)
			.await?;

		// Extract the "result" field from the JSON-RPC response
		let receipt_data = response
			.get("result")
			.ok_or_else(|| BlockChainError::request_error("Missing 'result' field".to_string()))?;

		// Handle null response case
		if receipt_data.is_null() {
			return Err(BlockChainError::request_error(
				"Transaction receipt not found".to_string(),
			));
		}

		serde_json::from_value(receipt_data.clone())
			.map_err(|e| BlockChainError::request_error(format!("Failed to parse receipt: {}", e)))
	}

	/// Retrieves logs within the specified block range
	///
	/// Uses Web3's filter builder to construct the log filter query
	async fn get_logs_for_blocks(
		&self,
		from_block: u64,
		to_block: u64,
	) -> Result<Vec<web3::types::Log>, BlockChainError> {
		// Convert parameters to JSON-RPC format
		let params = json!([{
			"fromBlock": format!("0x{:x}", from_block),
			"toBlock": format!("0x{:x}", to_block)
		}])
		.as_array()
		.ok_or_else(|| {
			BlockChainError::internal_error("Failed to create JSON-RPC params array".to_string())
		})?
		.to_vec();

		let response = self
			.web3_client
			.send_raw_request("eth_getLogs", Some(params))
			.await?;

		// Extract the "result" field from the JSON-RPC response
		let logs_data = response
			.get("result")
			.ok_or_else(|| BlockChainError::request_error("Missing 'result' field".to_string()))?;

		// Parse the response into the expected type
		serde_json::from_value(logs_data.clone())
			.map_err(|e| BlockChainError::request_error(format!("Failed to parse logs: {}", e)))
	}
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> BlockChainClient for EvmClient<T> {
	/// Retrieves the latest block number with retry functionality
	async fn get_latest_block_number(&self) -> Result<u64, BlockChainError> {
		let response = self
			.web3_client
			.send_raw_request::<serde_json::Value>("eth_blockNumber", None)
			.await?;

		// Extract the "result" field from the JSON-RPC response
		let hex_str = response
			.get("result")
			.and_then(|v| v.as_str())
			.ok_or_else(|| BlockChainError::request_error("Missing 'result' field".to_string()))?;

		// Parse hex string to u64
		u64::from_str_radix(hex_str.trim_start_matches("0x"), 16).map_err(|e| {
			BlockChainError::request_error(format!("Failed to parse block number: {}", e))
		})
	}

	/// Retrieves blocks within the specified range with retry functionality
	///
	/// # Note
	/// If end_block is None, only the start_block will be retrieved
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, BlockChainError> {
		let mut blocks = Vec::new();
		for block_number in start_block..=end_block.unwrap_or(start_block) {
			// Create the params array directly
			let params = json!([
				format!("0x{:x}", block_number),
				true // include full transaction objects
			]);

			let response = self
				.web3_client
				.send_raw_request("eth_getBlockByNumber", Some(params))
				.await?;

			// Extract the "result" field from the JSON-RPC response
			let block_data = response.get("result").ok_or_else(|| {
				BlockChainError::request_error("Missing 'result' field".to_string())
			})?;

			if block_data.is_null() {
				return Err(BlockChainError::block_not_found(block_number));
			}

			let block: web3::types::Block<web3::types::Transaction> =
				serde_json::from_value(block_data.clone()).map_err(|e| {
					BlockChainError::request_error(format!("Failed to parse block: {}", e))
				})?;

			blocks.push(BlockType::EVM(Box::new(EVMBlock::from(block))));
		}
		Ok(blocks)
	}
}
