//! Stellar blockchain client implementation.
//!
//! This module provides functionality to interact with the Stellar blockchain,
//! supporting operations like block retrieval, transaction lookup, and event filtering.
//! It works with both Stellar Core nodes and Horizon API endpoints.

use std::marker::PhantomData;

use async_trait::async_trait;
use serde_json::json;

use crate::{
	models::{
		BlockType, Network, StellarBlock, StellarEvent, StellarTransaction, StellarTransactionInfo,
	},
	services::{
		blockchain::{
			client::{BlockChainClient, BlockFilterFactory},
			transports::StellarTransportClient,
			BlockChainError,
		},
		filter::StellarBlockFilter,
	},
	utils::WithRetry,
};

/// Client implementation for the Stellar blockchain
///
/// Provides high-level access to Stellar blockchain data and operations through
/// both Stellar Core RPC and Horizon API endpoints.
#[derive(Clone)]
pub struct StellarClient {
	/// The underlying Stellar transport client for RPC communication
	stellar_client: StellarTransportClient,
	/// Network configuration for this client instance
	_network: Network,
}

impl StellarClient {
	/// Creates a new Stellar client instance
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints and chain details
	///
	/// # Returns
	/// * `Result<Self, BlockChainError>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, BlockChainError> {
		let stellar_client: StellarTransportClient = StellarTransportClient::new(network).await?;
		Ok(Self {
			stellar_client,
			_network: network.clone(),
		})
	}
}

/// Extended functionality specific to the Stellar blockchain
#[async_trait]
pub trait StellarClientTrait {
	/// Retrieves transactions within a sequence range
	///
	/// # Arguments
	/// * `start_sequence` - Starting sequence number
	/// * `end_sequence` - Optional ending sequence number. If None, only fetches start_sequence
	///
	/// # Returns
	/// * `Result<Vec<StellarTransaction>, BlockChainError>` - Collection of transactions or error
	async fn get_transactions(
		&self,
		start_sequence: u32,
		end_sequence: Option<u32>,
	) -> Result<Vec<StellarTransaction>, BlockChainError>;

	/// Retrieves events within a sequence range
	///
	/// # Arguments
	/// * `start_sequence` - Starting sequence number
	/// * `end_sequence` - Optional ending sequence number. If None, only fetches start_sequence
	///
	/// # Returns
	/// * `Result<Vec<StellarEvent>, BlockChainError>` - Collection of events or error
	async fn get_events(
		&self,
		start_sequence: u32,
		end_sequence: Option<u32>,
	) -> Result<Vec<StellarEvent>, BlockChainError>;
}

#[async_trait]
impl StellarClientTrait for StellarClient {
	/// Retrieves transactions within a sequence range with pagination
	///
	/// # Errors
	/// - Returns `BlockChainError::RequestError` if start_sequence > end_sequence
	/// - Returns `BlockChainError::RequestError` if transaction parsing fails
	async fn get_transactions(
		&self,
		start_sequence: u32,
		end_sequence: Option<u32>,
	) -> Result<Vec<StellarTransaction>, BlockChainError> {
		// Validate input parameters
		if let Some(end_sequence) = end_sequence {
			if start_sequence > end_sequence {
				return Err(BlockChainError::request_error(
					format!(
						"start_sequence {} cannot be greater than end_sequence {}",
						start_sequence, end_sequence
					),
					None,
					Some("get_transactions"),
				));
			}
		}

		let with_retry = WithRetry::with_default_config();
		with_retry
			.attempt(|| async {
				// max limit for the RPC endpoint is 200
				const PAGE_LIMIT: u32 = 200;
				let mut transactions = Vec::new();
				let target_sequence = end_sequence.unwrap_or(start_sequence);
				let mut cursor = None;

				while cursor.unwrap_or(start_sequence) <= target_sequence {
					let params = json!({
						"startLedger": cursor.unwrap_or(start_sequence),
						"pagination": {
							"limit": PAGE_LIMIT
						}
					});

					let response = self
						.stellar_client
						.send_raw_request("getTransactions", params)
						.await?;

					let ledger_transactions: Vec<StellarTransactionInfo> =
						serde_json::from_value(response["result"]["transactions"].clone())
							.map_err(|e| {
								BlockChainError::request_error(
									e.to_string(),
									None,
									Some("get_transactions"),
								)
							})?;

					if ledger_transactions.is_empty() {
						break;
					}

					for transaction in ledger_transactions {
						let sequence = transaction.ledger;
						if sequence > target_sequence {
							return Ok(transactions);
						}
						transactions.push(StellarTransaction::from(transaction));
					}

					cursor = response["result"]["cursor"]
						.as_str()
						.and_then(|s| s.parse::<u32>().ok());

					if cursor.is_none() {
						break;
					}
				}
				Ok(transactions)
			})
			.await
	}

	/// Retrieves events within a sequence range with pagination
	///
	/// # Errors
	/// - Returns `BlockChainError::RequestError` if start_sequence > end_sequence
	/// - Returns `BlockChainError::RequestError` if event parsing fails
	async fn get_events(
		&self,
		start_sequence: u32,
		end_sequence: Option<u32>,
	) -> Result<Vec<StellarEvent>, BlockChainError> {
		// Validate input parameters
		if let Some(end_sequence) = end_sequence {
			if start_sequence > end_sequence {
				return Err(BlockChainError::request_error(
					format!(
						"start_sequence {} cannot be greater than end_sequence {}",
						start_sequence, end_sequence
					),
					None,
					Some("get_events"),
				));
			}
		}

		let with_retry = WithRetry::with_default_config();
		with_retry
			.attempt(|| async {
				// max limit for the RPC endpoint is 200
				const PAGE_LIMIT: u32 = 200;
				let mut events = Vec::new();
				let target_sequence = end_sequence.unwrap_or(start_sequence);
				let mut cursor = None;

				while cursor.unwrap_or(start_sequence) <= target_sequence {
					let params = json!({
						"startLedger": cursor.unwrap_or(start_sequence),
						"filters": [{
							"type": "contract",
						}],
						"pagination": {
							"limit": PAGE_LIMIT
						}
					});

					let response = self
						.stellar_client
						.send_raw_request("getEvents", params)
						.await?;

					let ledger_events: Vec<StellarEvent> = serde_json::from_value(
						response["result"]["events"].clone(),
					)
					.map_err(|e| {
						BlockChainError::request_error(e.to_string(), None, Some("get_events"))
					})?;

					if ledger_events.is_empty() {
						break;
					}

					for event in ledger_events {
						let sequence = event.ledger;
						if sequence > target_sequence {
							return Ok(events);
						}
						events.push(event);
					}

					cursor = response["result"]["cursor"]
						.as_str()
						.and_then(|s| s.parse::<u32>().ok());

					if cursor.is_none() {
						break;
					}
				}
				Ok(events)
			})
			.await
	}
}

impl BlockFilterFactory<StellarClient> for StellarClient {
	type Filter = StellarBlockFilter<Self>;

	fn filter() -> Self::Filter {
		StellarBlockFilter {
			_client: PhantomData {},
		}
	}
}

#[async_trait]
impl BlockChainClient for StellarClient {
	/// Retrieves the latest block number with retry functionality
	async fn get_latest_block_number(&self) -> Result<u64, BlockChainError> {
		let with_retry = WithRetry::with_default_config();
		with_retry
			.attempt(|| async {
				let response = self
					.stellar_client
					.client
					.get_latest_ledger()
					.await
					.map_err(|e| {
						BlockChainError::request_error(
							e.to_string(),
							None,
							Some("get_latest_block_number"),
						)
					})?;

				Ok(response.sequence as u64)
			})
			.await
	}

	/// Retrieves blocks within the specified range with retry functionality
	///
	/// # Note
	/// If end_block is None, only the start_block will be retrieved
	///
	/// # Errors
	/// - Returns `BlockChainError::RequestError` if start_block > end_block
	/// - Returns `BlockChainError::BlockNotFound` if a block cannot be retrieved
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, BlockChainError> {
		// max limit for the RPC endpoint is 200
		const PAGE_LIMIT: u32 = 200;

		// Validate input parameters
		if let Some(end_block) = end_block {
			if start_block > end_block {
				return Err(BlockChainError::request_error(
					format!(
						"start_block {} cannot be greater than end_block {}",
						start_block, end_block
					),
					None,
					Some("get_blocks"),
				));
			}
		}

		let with_retry = WithRetry::with_default_config();
		with_retry
			.attempt(|| async {
				let mut blocks = Vec::new();
				let target_block = end_block.unwrap_or(start_block);
				let mut cursor = None;

				while cursor.unwrap_or(start_block) <= target_block {
					let params = json!({
						"startLedger": cursor.unwrap_or(start_block),
						"pagination": {
							"limit": PAGE_LIMIT
						}
					});

					// TODO: Replace this once the SDK is updated with `get_ledgers`
					let response = self
						.stellar_client
						.send_raw_request("getLedgers", params)
						.await?;

					let ledgers: Vec<StellarBlock> = serde_json::from_value(
						response["result"]["ledgers"].clone(),
					)
					.map_err(|e| {
						BlockChainError::request_error(e.to_string(), None, Some("get_blocks"))
					})?;

					if ledgers.is_empty() {
						break;
					}

					for ledger in ledgers {
						let sequence = ledger.sequence;
						if (sequence as u64) > target_block {
							return Ok(blocks);
						}
						blocks.push(BlockType::Stellar(Box::new(ledger)));
					}

					cursor = response["result"]["cursor"]
						.as_str()
						.and_then(|s| s.parse::<u64>().ok());

					if cursor.is_none() {
						break;
					}
				}
				Ok(blocks)
			})
			.await
	}
}
