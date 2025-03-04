//! EVM-compatible blockchain client implementation.
//!
//! This module provides functionality to interact with Ethereum and other EVM-compatible
//! blockchains, supporting operations like block retrieval, transaction receipt lookup,
//! and log filtering.

use std::{collections::HashMap, marker::PhantomData};

use async_trait::async_trait;
use futures;
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
	utils::{format_target_with_source, ErrorContext, ErrorContextProvider},
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
		let context = HashMap::from([("hash".to_string(), transaction_hash.clone())]);
		let hash = string_to_h256(&transaction_hash).map_err(|_| {
			BlockChainError::internal_error(
				"Invalid transaction hash".to_string(),
				Some(context.clone()),
				Some("get_transaction_receipt"),
			)
		})?;

		let params = json!([format!("0x{:x}", hash)])
			.as_array()
			.ok_or_else(|| {
				BlockChainError::internal_error(
					"Failed to create JSON-RPC params array".to_string(),
					Some(context.clone()),
					Some("get_transaction_receipt"),
				)
			})?
			.to_vec();

		let response = self
			.web3_client
			.send_raw_request(
				"eth_getTransactionReceipt",
				Some(serde_json::Value::Array(params)),
			)
			.await
			.map_err(|e| {
				let err_msg = e.to_string();
				let err_ctx = e.provide_error_context();
				let target = format_target_with_source(Some("get_transaction_receipt"), err_ctx);
				BlockChainError::request_error::<BlockChainError>(
					&err_msg,
					None,
					Some(context.clone()),
					Some(&target),
				)
			})?;

		// Extract the "result" field from the JSON-RPC response
		let receipt_data = response.get("result").ok_or_else(|| {
			BlockChainError::request_error::<ErrorContext<String>>(
				"Missing 'result' field".to_string(),
				None,
				Some(context.clone()),
				Some("get_transaction_receipt"),
			)
		})?;

		// Handle null response case
		if receipt_data.is_null() {
			return Err(BlockChainError::request_error::<ErrorContext<String>>(
				"Transaction receipt not found".to_string(),
				None,
				Some(context.clone()),
				Some("get_transaction_receipt"),
			));
		}

		Ok(serde_json::from_value(receipt_data.clone()).map_err(|e| {
			BlockChainError::request_error::<ErrorContext<String>>(
				format!("Failed to parse transaction receipt: {}", e),
				None,
				Some(context.clone()),
				Some("get_transaction_receipt"),
			)
		})?)
	}

	/// Retrieves logs within the specified block range
	///
	/// Uses Web3's filter builder to construct the log filter query
	async fn get_logs_for_blocks(
		&self,
		from_block: u64,
		to_block: u64,
	) -> Result<Vec<web3::types::Log>, BlockChainError> {
		let context = HashMap::from([
			("from_block".to_string(), from_block.to_string()),
			("to_block".to_string(), to_block.to_string()),
		]);

		// Convert parameters to JSON-RPC format
		let params = json!([{
			"fromBlock": format!("0x{:x}", from_block),
			"toBlock": format!("0x{:x}", to_block)
		}])
		.as_array()
		.ok_or_else(|| {
			BlockChainError::internal_error(
				"Failed to create JSON-RPC params array".to_string(),
				Some(context.clone()),
				Some("get_logs_for_blocks"),
			)
		})?
		.to_vec();

		let response = self
			.web3_client
			.send_raw_request("eth_getLogs", Some(params))
			.await
			.map_err(|e| {
				let err_msg = e.to_string();
				let err_ctx = e.provide_error_context();
				let target = format_target_with_source(Some("get_logs_for_blocks"), err_ctx);
				BlockChainError::request_error::<BlockChainError>(
					&err_msg,
					None,
					Some(context.clone()),
					Some(&target),
				)
			})?;

		// Extract the "result" field from the JSON-RPC response
		let logs_data = response.get("result").ok_or_else(|| {
			BlockChainError::request_error::<ErrorContext<String>>(
				"Missing 'result' field".to_string(),
				None,
				Some(context.clone()),
				Some("get_logs_for_blocks"),
			)
		})?;

		// Parse the response into the expected type
		Ok(serde_json::from_value(logs_data.clone()).map_err(|e| {
			BlockChainError::request_error::<ErrorContext<String>>(
				format!("Failed to parse logs: {}", e),
				None,
				Some(context.clone()),
				Some("get_logs_for_blocks"),
			)
		})?)
	}
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> BlockChainClient for EvmClient<T> {
	/// Retrieves the latest block number with retry functionality
	async fn get_latest_block_number(&self) -> Result<u64, BlockChainError> {
		let response = self
			.web3_client
			.send_raw_request::<serde_json::Value>("eth_blockNumber", None)
			.await
			.map_err(|e| {
				let err_msg = e.to_string();
				let err_ctx = e.provide_error_context();
				let target = format_target_with_source(Some("get_latest_block_number"), err_ctx);
				BlockChainError::request_error::<BlockChainError>(
					&err_msg,
					None,
					None,
					Some(&target),
				)
			})?;

		// Extract the "result" field from the JSON-RPC response
		let hex_str = response
			.get("result")
			.and_then(|v| v.as_str())
			.ok_or_else(|| {
				BlockChainError::request_error::<ErrorContext<String>>(
					"Missing 'result' field".to_string(),
					None,
					None,
					Some("get_latest_block_number"),
				)
			})?;

		// Parse hex string to u64
		u64::from_str_radix(hex_str.trim_start_matches("0x"), 16).map_err(|e| {
			BlockChainError::request_error::<ErrorContext<String>>(
				format!("Failed to parse block number: {}", e),
				None,
				None,
				Some("get_latest_block_number"),
			)
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
		let context = HashMap::from([
			("start_block".to_string(), start_block.to_string()),
			(
				"end_block".to_string(),
				end_block.unwrap_or(start_block).to_string(),
			),
		]);

		let block_futures: Vec<_> = (start_block..=end_block.unwrap_or(start_block))
			.map(|block_number| {
				let params = json!([
					format!("0x{:x}", block_number),
					true // include full transaction objects
				]);
				let client = self.web3_client.clone();
				let mut context = context.clone();

				async move {
					context.insert("block_number".to_string(), block_number.to_string());

					let response = client
						.send_raw_request("eth_getBlockByNumber", Some(params))
						.await
						.map_err(|e| {
							let err_msg = e.to_string();
							let err_ctx = e.provide_error_context();
							let target = format_target_with_source(Some("get_blocks"), err_ctx);
							BlockChainError::request_error::<BlockChainError>(
								&err_msg,
								None,
								Some(context.clone()),
								Some(&target),
							)
						})?;

					let block_data = response.get("result").ok_or_else(|| {
						BlockChainError::request_error::<ErrorContext<String>>(
							"Missing 'result' field".to_string(),
							None,
							Some(context.clone()),
							Some("get_blocks"),
						)
					})?;

					if block_data.is_null() {
						return Err(BlockChainError::block_not_found(
							block_number,
							None,
							Some("get_blocks"),
						));
					}

					let block: web3::types::Block<web3::types::Transaction> =
						serde_json::from_value(block_data.clone()).map_err(|e| {
							BlockChainError::request_error::<ErrorContext<String>>(
								format!("Failed to parse block: {}", e),
								None,
								Some(context.clone()),
								Some("get_blocks"),
							)
						})?;

					Ok(BlockType::EVM(Box::new(EVMBlock::from(block))))
				}
			})
			.collect();

		futures::future::join_all(block_futures)
			.await
			.into_iter()
			.collect::<Result<Vec<_>, _>>()
	}
}
