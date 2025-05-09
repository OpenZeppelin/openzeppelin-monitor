//! Stellar blockchain client implementation.
//!
//! This module provides functionality to interact with the Stellar blockchain,
//! supporting operations like block retrieval, transaction lookup, and event filtering.

use anyhow::Context;
use async_trait::async_trait;
use serde_json::json;
use std::marker::PhantomData;
use stellar_xdr::curr::{Limits, WriteXdr};
use tracing::instrument;

use crate::{
	models::{
		BlockType, ContractSpec, Network, StellarBlock, StellarContractSpec, StellarEvent,
		StellarTransaction, StellarTransactionInfo,
	},
	services::{
		blockchain::{
			client::{BlockChainClient, BlockFilterFactory},
			transports::StellarTransportClient,
			BlockchainTransport,
		},
		filter::{
			stellar_helpers::{
				get_contract_code_ledger_key, get_contract_instance_ledger_key, get_contract_spec,
				get_wasm_code_from_ledger_entry_data, get_wasm_hash_from_ledger_entry_data,
			},
			StellarBlockFilter,
		},
	},
};

/// Client implementation for the Stellar blockchain
///
/// Provides high-level access to Stellar blockchain data and operations through HTTP transport.
#[derive(Clone)]
pub struct StellarClient<T: Send + Sync + Clone> {
	/// The underlying Stellar transport client for RPC communication
	http_client: T,
}

impl<T: Send + Sync + Clone> StellarClient<T> {
	/// Creates a new Stellar client instance with a specific transport client
	pub fn new_with_transport(http_client: T) -> Self {
		Self { http_client }
	}
}

impl StellarClient<StellarTransportClient> {
	/// Creates a new Stellar client instance
	///
	/// # Arguments
	/// * `network` - Network configuration containing RPC endpoints and chain details
	///
	/// # Returns
	/// * `Result<Self, anyhow::Error>` - New client instance or connection error
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let http_client = StellarTransportClient::new(network).await?;
		Ok(Self::new_with_transport(http_client))
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
	/// * `Result<Vec<StellarTransaction>, anyhow::Error>` - Collection of transactions or error
	async fn get_transactions(
		&self,
		start_sequence: u32,
		end_sequence: Option<u32>,
	) -> Result<Vec<StellarTransaction>, anyhow::Error>;

	/// Retrieves events within a sequence range
	///
	/// # Arguments
	/// * `start_sequence` - Starting sequence number
	/// * `end_sequence` - Optional ending sequence number. If None, only fetches start_sequence
	///
	/// # Returns
	/// * `Result<Vec<StellarEvent>, anyhow::Error>` - Collection of events or error
	async fn get_events(
		&self,
		start_sequence: u32,
		end_sequence: Option<u32>,
	) -> Result<Vec<StellarEvent>, anyhow::Error>;
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> StellarClientTrait for StellarClient<T> {
	/// Retrieves transactions within a sequence range with pagination
	///
	/// # Errors
	/// - Returns `anyhow::Error` if start_sequence > end_sequence
	/// - Returns `anyhow::Error` if transaction parsing fails
	#[instrument(skip(self), fields(start_sequence, end_sequence))]
	async fn get_transactions(
		&self,
		start_sequence: u32,
		end_sequence: Option<u32>,
	) -> Result<Vec<StellarTransaction>, anyhow::Error> {
		// Validate input parameters
		if let Some(end_sequence) = end_sequence {
			if start_sequence > end_sequence {
				return Err(anyhow::anyhow!(
					"start_sequence {} cannot be greater than end_sequence {}",
					start_sequence,
					end_sequence
				));
			}
		}

		// max limit for the RPC endpoint is 200
		const PAGE_LIMIT: u32 = 200;
		let mut transactions = Vec::new();
		let target_sequence = end_sequence.unwrap_or(start_sequence);
		let mut cursor: Option<String> = None;
		let mut current_iteration = 0;

		while cursor.is_some() || current_iteration <= 0 {
			let params = if current_iteration == 0 {
				// First iteration, we need to fetch the transactions from the start sequence without a cursor
				json!({
					"startLedger": start_sequence,
					"pagination": {
						"limit": PAGE_LIMIT
					}
				})
			} else {
				// Subsequent iterations, we need to fetch the transactions from the cursor
				json!({
					"pagination": {
						"cursor": cursor,
						"limit": PAGE_LIMIT
					}
				})
			};

			let response = self
				.http_client
				.send_raw_request("getTransactions", Some(params))
				.await
				.with_context(|| {
					format!(
						"Failed to fetch transactions for ledger range {}-{}",
						start_sequence, target_sequence
					)
				})?;

			let ledger_transactions: Vec<StellarTransactionInfo> =
				serde_json::from_value(response["result"]["transactions"].clone())
					.with_context(|| "Failed to parse transaction response")?;

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

			// Increment the number of iterations to ensure we break the loop in case there is no cursor
			current_iteration += 1;
			cursor = response["result"]["cursor"].as_str().map(|s| s.to_string());
			if cursor.is_none() {
				break;
			}
		}
		Ok(transactions)
	}

	/// Retrieves events within a sequence range with pagination
	///
	/// # Errors
	/// - Returns `anyhow::Error` if start_sequence > end_sequence
	/// - Returns `anyhow::Error` if event parsing fails
	#[instrument(skip(self), fields(start_sequence, end_sequence))]
	async fn get_events(
		&self,
		start_sequence: u32,
		end_sequence: Option<u32>,
	) -> Result<Vec<StellarEvent>, anyhow::Error> {
		// Validate input parameters
		if let Some(end_sequence) = end_sequence {
			if start_sequence > end_sequence {
				return Err(anyhow::anyhow!(
					"start_sequence {} cannot be greater than end_sequence {}",
					start_sequence,
					end_sequence
				));
			}
		}

		// max limit for the RPC endpoint is 200
		const PAGE_LIMIT: u32 = 200;
		let mut events = Vec::new();
		let target_sequence = end_sequence.unwrap_or(start_sequence);
		let mut cursor: Option<String> = None;
		let mut current_iteration = 0;

		while cursor.is_some() || current_iteration <= 0 {
			let params = if current_iteration == 0 {
				// First iteration, we need to fetch the events from the start sequence without a cursor
				json!({
					"startLedger": start_sequence,
					"filters": [{
						"type": "contract",
					}],
					"pagination": {
						"limit": PAGE_LIMIT
					}
				})
			} else {
				// Subsequent iterations, we need to fetch the events from the cursor
				json!({
					"filters": [{
						"type": "contract",
					}],
					"pagination": {
						"cursor": cursor,
						"limit": PAGE_LIMIT
					}
				})
			};

			let response = self
				.http_client
				.send_raw_request("getEvents", Some(params))
				.await
				.with_context(|| {
					format!(
						"Failed to fetch events for ledger range {}-{}",
						start_sequence, target_sequence
					)
				})?;

			let ledger_events: Vec<StellarEvent> =
				serde_json::from_value(response["result"]["events"].clone())
					.with_context(|| "Failed to parse event response")?;

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
			// Increment the number of iterations to ensure we break the loop in case there is no cursor
			current_iteration += 1;
			cursor = response["result"]["cursor"].as_str().map(|s| s.to_string());

			if cursor.is_none() {
				break;
			}
		}
		Ok(events)
	}
}

impl<T: Send + Sync + Clone + BlockchainTransport> BlockFilterFactory<Self> for StellarClient<T> {
	type Filter = StellarBlockFilter<Self>;

	fn filter() -> Self::Filter {
		StellarBlockFilter {
			_client: PhantomData {},
		}
	}
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> BlockChainClient for StellarClient<T> {
	/// Retrieves the latest block number with retry functionality
	#[instrument(skip(self))]
	async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
		let response = self
			.http_client
			.send_raw_request::<serde_json::Value>("getLatestLedger", None)
			.await
			.with_context(|| "Failed to get latest ledger")?;

		let sequence = response["result"]["sequence"]
			.as_u64()
			.ok_or_else(|| anyhow::anyhow!("Invalid sequence number"))?;

		Ok(sequence)
	}

	/// Retrieves blocks within the specified range with retry functionality
	///
	/// # Note
	/// If end_block is None, only the start_block will be retrieved
	///
	/// # Errors
	/// - Returns `BlockChainError::RequestError` if start_block > end_block
	/// - Returns `BlockChainError::BlockNotFound` if a block cannot be retrieved
	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, anyhow::Error> {
		// max limit for the RPC endpoint is 200
		const PAGE_LIMIT: u32 = 200;

		// Validate input parameters
		if let Some(end_block) = end_block {
			if start_block > end_block {
				return Err(anyhow::anyhow!(
					"start_block {} cannot be greater than end_block {}",
					start_block,
					end_block
				));
			}
		}

		let mut blocks = Vec::new();
		let target_block = end_block.unwrap_or(start_block);
		let mut cursor: Option<String> = None;
		let mut current_iteration = 0;

		while cursor.is_some() || current_iteration <= 0 {
			let params = if current_iteration == 0 {
				// First iteration, we need to fetch the ledgers from the start block without a cursor
				json!({
					"startLedger": start_block,
					"pagination": {
						"limit": if end_block.is_none() { 1 } else { PAGE_LIMIT }
					}
				})
			} else {
				// Subsequent iterations, we need to fetch the ledgers from the cursor
				json!({
					"pagination": {
						"cursor": cursor,
						"limit": PAGE_LIMIT
					}
				})
			};

			let response = self
				.http_client
				.send_raw_request("getLedgers", Some(params))
				.await
				.with_context(|| {
					format!(
						"Failed to fetch ledgers for range {}-{}",
						start_block, target_block
					)
				})?;

			let ledgers: Vec<StellarBlock> =
				serde_json::from_value(response["result"]["ledgers"].clone())
					.with_context(|| "Failed to parse ledger response")?;

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

			// Increment the number of iterations to ensure we break the loop in case there is no cursor
			current_iteration += 1;
			cursor = response["result"]["cursor"].as_str().map(|s| s.to_string());

			// If the cursor is the same as the start block, we have reached the end of the range
			if cursor == Some(start_block.to_string()) {
				break;
			}

			if cursor.is_none() {
				break;
			}
		}
		Ok(blocks)
	}

	/// Retrieves the contract spec for a given contract ID
	///
	/// # Arguments
	/// * `contract_id` - The ID of the contract to retrieve the spec for
	///
	/// # Returns
	/// * `Result<ContractSpec, anyhow::Error>` - The contract spec or error
	#[instrument(skip(self), fields(contract_id))]
	async fn get_contract_spec(&self, contract_id: &str) -> Result<ContractSpec, anyhow::Error> {
		// Get contract wasm code from contract ID
		let contract_instance_ledger_key = get_contract_instance_ledger_key(contract_id)
			.map_err(|e| anyhow::anyhow!("Failed to get contract instance ledger key: {}", e))?;

		let contract_instance_ledger_key_xdr = contract_instance_ledger_key
			.to_xdr_base64(Limits::none())
			.map_err(|e| {
				anyhow::anyhow!(
					"Failed to convert contract instance ledger key to XDR: {}",
					e
				)
			})?;

		let params = json!({
			"keys": [contract_instance_ledger_key_xdr],
			"xdrFormat": "base64"
		});

		let response = self
			.http_client
			.send_raw_request("getLedgerEntries", Some(params))
			.await
			.with_context(|| format!("Failed to get contract wasm code for {}", contract_id))?;

		let contract_data_xdr_base64 = match response["result"]["entries"][0]["xdr"].as_str() {
			Some(xdr) => xdr,
			None => {
				return Err(anyhow::anyhow!("Failed to get contract data XDR"));
			}
		};

		let wasm_hash = get_wasm_hash_from_ledger_entry_data(contract_data_xdr_base64)
			.map_err(|e| anyhow::anyhow!("Failed to get wasm hash: {}", e))?;

		let contract_code_ledger_key = get_contract_code_ledger_key(wasm_hash.as_str())
			.map_err(|e| anyhow::anyhow!("Failed to get contract code ledger key: {}", e))?;

		let contract_code_ledger_key_xdr = contract_code_ledger_key
			.to_xdr_base64(Limits::none())
			.map_err(|e| {
			anyhow::anyhow!("Failed to convert contract code ledger key to XDR: {}", e)
		})?;

		let params = json!({
			"keys": [contract_code_ledger_key_xdr],
			"xdrFormat": "base64"
		});

		let response = self
			.http_client
			.send_raw_request("getLedgerEntries", Some(params))
			.await
			.with_context(|| format!("Failed to get contract wasm code for {}", contract_id))?;

		let contract_code_xdr_base64 = match response["result"]["entries"][0]["xdr"].as_str() {
			Some(xdr) => xdr,
			None => {
				return Err(anyhow::anyhow!("Failed to get contract code XDR"));
			}
		};

		let wasm_code = get_wasm_code_from_ledger_entry_data(contract_code_xdr_base64)
			.map_err(|e| anyhow::anyhow!("Failed to get wasm code: {}", e))?;

		let contract_spec = get_contract_spec(wasm_code.as_str())
			.map_err(|e| anyhow::anyhow!("Failed to get contract spec: {}", e))?;

		Ok(ContractSpec::Stellar(StellarContractSpec::from(
			contract_spec,
		)))
	}
}
