//! EVM-compatible blockchain client implementation.
//!
//! This module provides functionality to interact with Ethereum and other EVM-compatible
//! blockchains, supporting operations like block retrieval, transaction receipt lookup,
//! and log filtering.

use std::{collections::HashMap, marker::PhantomData};

use async_trait::async_trait;
use web3::types::{BlockId, BlockNumber};

use crate::{
	models::{BlockType, EVMBlock, Network},
	services::{
		blockchain::{
			client::BlockChainClient, transports::Web3TransportClient, BlockChainError,
			BlockFilterFactory,
		},
		filter::{evm_helpers::string_to_h256, EVMBlockFilter},
	},
	utils::WithRetry,
};

/// Client implementation for Ethereum Virtual Machine (EVM) compatible blockchains
///
/// Provides high-level access to EVM blockchain data and operations through Web3
/// transport layer.
#[derive(Clone)]
pub struct EvmClient {
	/// The underlying Web3 transport client for RPC communication
	web3_client: Web3TransportClient,
	/// Network configuration for this client instance
	_network: Network,
}

impl EvmClient {
	/// Creates a new EVM client instance
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints and chain details
	///
	/// # Returns
	/// * `Result<Self, BlockChainError>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
		let web3_client = Web3TransportClient::new(network).await?;
		Ok(Self {
			web3_client,
			_network: network.clone(),
		})
	}
}

impl BlockFilterFactory<Self> for EvmClient {
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
impl EvmClientTrait for EvmClient {
	/// Retrieves a transaction receipt by hash with proper error handling
	///
	/// # Errors
	/// - Returns `BlockChainError::InternalError` if the hash format is invalid
	/// - Returns `BlockChainError::RequestError` if the receipt is not found
	async fn get_transaction_receipt(
		&self,
		transaction_hash: String,
	) -> Result<web3::types::TransactionReceipt, BlockChainError> {
		let context = HashMap::from([
			("network".to_string(), self._network.name.clone()),
			("hash".to_string(), transaction_hash.clone()),
		]);
		let hash = string_to_h256(&transaction_hash).map_err(|_| {
			BlockChainError::internal_error(
				format!("Invalid transaction hash ({})", transaction_hash),
				Some(context.clone()),
			)
		})?;

		let with_retry = WithRetry::with_default_config();
		with_retry
			.attempt(|| async {
				let receipt = self
					.web3_client
					.client
					.eth()
					.transaction_receipt(hash)
					.await
					.map_err(|e| {
						BlockChainError::request_error_with_source(
							"Failed to get transaction receipt",
							e,
							Some(context.clone()),
						)
					})?;

				receipt.ok_or_else(|| {
					BlockChainError::request_error(
						"Transaction receipt not found".to_string(),
						Some(context.clone()),
					)
				})
			})
			.await
	}

	/// Retrieves logs within the specified block range
	///
	/// Uses Web3's filter builder to construct the log filter query
	async fn get_logs_for_blocks(
		&self,
		from_block: u64,
		to_block: u64,
	) -> Result<Vec<web3::types::Log>, BlockChainError> {
		let with_retry = WithRetry::with_default_config();
		with_retry
			.attempt(|| async {
				self.web3_client
					.client
					.eth()
					.logs(
						web3::types::FilterBuilder::default()
							.from_block(BlockNumber::Number(from_block.into()))
							.to_block(BlockNumber::Number(to_block.into()))
							.build(),
					)
					.await
					.map_err(|e| {
						BlockChainError::request_error_with_source(
							"Failed to get logs for blocks",
							e,
							Some(HashMap::from([
								("network".to_string(), self._network.name.clone()),
								("from_block".to_string(), from_block.to_string()),
								("to_block".to_string(), to_block.to_string()),
							])),
						)
					})
			})
			.await
	}
}

#[async_trait]
impl BlockChainClient for EvmClient {
	/// Retrieves the latest block number with retry functionality
	async fn get_latest_block_number(&self) -> Result<u64, BlockChainError> {
		let with_retry = WithRetry::with_default_config();
		with_retry
			.attempt(|| async {
				self.web3_client
					.client
					.eth()
					.block_number()
					.await
					.map(|n| n.as_u64())
					.map_err(|e| {
						BlockChainError::request_error_with_source(
							"Failed to get latest block number",
							e,
							Some(HashMap::from([(
								"network".to_string(),
								self._network.name.clone(),
							)])),
						)
					})
			})
			.await
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
		let with_retry = WithRetry::with_default_config();
		with_retry
			.attempt(|| async {
				let mut blocks = Vec::new();
				for block_number in start_block..=end_block.unwrap_or(start_block) {
					let block = self
						.web3_client
						.client
						.eth()
						.block_with_txs(BlockId::Number(BlockNumber::Number(block_number.into())))
						.await?
						.ok_or_else(|| {
							BlockChainError::block_not_found(
								block_number,
								Some(HashMap::from([
									("network".to_string(), self._network.name.clone()),
									("block_number".to_string(), block_number.to_string()),
								])),
							)
						})?;

					blocks.push(BlockType::EVM(Box::new(EVMBlock::from(block))));
				}
				Ok(blocks)
			})
			.await
	}
}
