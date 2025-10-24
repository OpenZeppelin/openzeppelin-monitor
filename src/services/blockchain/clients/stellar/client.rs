//! Stellar blockchain client implementation.
//!
//! This module provides functionality to interact with the Stellar blockchain,
//! supporting operations like block retrieval, transaction lookup, and event filtering.

use anyhow::Context;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
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

use super::error::StellarClientError;

/// Stellar RPC method constants
const RPC_METHOD_GET_TRANSACTIONS: &str = "getTransactions";
const RPC_METHOD_GET_EVENTS: &str = "getEvents";
const RPC_METHOD_GET_LATEST_LEDGER: &str = "getLatestLedger";
const RPC_METHOD_GET_LEDGERS: &str = "getLedgers";
const RPC_METHOD_GET_LEDGER_ENTRIES: &str = "getLedgerEntries";

const RETENTION_MESSAGES: [&str; 2] = [
	"must be within the ledger range",
	"must be between the oldest ledger",
];

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

	/// Checks a JSON-RPC response for error information and converts it into a `StellarClientError` if present.
	///
	/// This function inspects the given JSON response body for an "error" field.
	/// If a known "outside of retention window" condition is detected (by code/message),
	/// it returns a specific `StellarClientError::outside_retention_window`.
	/// Otherwise, it returns a generic `StellarClientError::rpc_error` for any other error found.
	/// If no error is present, it returns `Ok(())`.
	///
	/// # Arguments
	/// * `response_body` - A reference to the JSON response body to inspect.
	/// * `start_sequence` - The starting ledger sequence number relevant to the request.
	/// * `target_sequence` - The target ledger sequence number relevant to the request.
	/// * `method_name` - The name of the RPC method that was called (for error reporting).
	///
	/// # Returns
	/// * `Ok(())` if no error is present in the response.
	/// * `Err(StellarClientError)` if an error is detected.
	fn check_and_handle_rpc_error(
		&self,
		response_body: &serde_json::Value,
		start_sequence: u32,
		target_sequence: u32,
		method_name: &'static str,
	) -> Result<(), StellarClientError> {
		if let Some(json_rpc_error) = response_body.get("error") {
			let rpc_code = json_rpc_error
				.get("code")
				.and_then(|c| c.as_i64())
				.unwrap_or(0);
			let rpc_message = json_rpc_error
				.get("message")
				.and_then(|m| m.as_str())
				.unwrap_or("Unknown RPC error")
				.to_string();

			if rpc_code == -32600
				&& RETENTION_MESSAGES
					.iter()
					.any(|msg| rpc_message.to_lowercase().contains(msg))
			{
				let ledger_info_str = format!(
					"start_sequence: {}, target_sequence: {}",
					start_sequence, target_sequence
				);

				return Err(StellarClientError::outside_retention_window(
					rpc_code,
					rpc_message,
					ledger_info_str,
					None,
					None,
				));
			} else {
				// Other JSON-RPC error reported by the server
				let message = format!(
					"Stellar RPC request failed for method '{}': {} (code {})",
					method_name, rpc_message, rpc_code
				);

				return Err(StellarClientError::rpc_error(message, None, None));
			}
		}
		Ok(())
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
				let message = format!(
					"start_sequence {} cannot be greater than end_sequence {}",
					start_sequence, end_sequence
				);
				let input_error = StellarClientError::invalid_input(message, None, None);
				return Err(anyhow::anyhow!(input_error))
					.context("Invalid input parameters for Stellar RPC");
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

			let http_response = self
				.http_client
				.send_raw_request(RPC_METHOD_GET_TRANSACTIONS, Some(params))
				.await;

			match http_response {
				Ok(response_body) => {
					// Check for RPC errors in the response
					if let Err(rpc_error) = self.check_and_handle_rpc_error(
						&response_body,
						start_sequence,
						target_sequence,
						RPC_METHOD_GET_TRANSACTIONS,
					) {
						// A terminal JSON-RPC error was found, convert and return
						return Err(anyhow::anyhow!(rpc_error).context(format!(
							"Soroban RPC reported an error during {}",
							RPC_METHOD_GET_TRANSACTIONS,
						)));
					}

					// Extract the transactions from the response
					let raw_transactions = response_body
						.get("result")
						.and_then(|r| r.get("transactions"))
						.ok_or_else(|| {
							let message = format!(
								"Unexpected response structure for method '{}'",
								RPC_METHOD_GET_TRANSACTIONS
							);
							StellarClientError::unexpected_response_structure(message, None, None)
						})
						.map_err(|client_parse_error| {
							anyhow::anyhow!(client_parse_error)
								.context("Failed to parse transaction response")
						})?;

					let ledger_transactions: Vec<StellarTransactionInfo> =
						serde_json::from_value(raw_transactions.clone()).map_err(|e| {
							let message = format!(
								"Failed to parse transactions from response for method '{}': {}",
								RPC_METHOD_GET_TRANSACTIONS, e
							);
							let sce_parse_error = StellarClientError::response_parse_error(
								message,
								Some(e.into()),
								None,
							);
							anyhow::anyhow!(sce_parse_error)
								.context("Failed to parse transaction response")
						})?;

					if ledger_transactions.is_empty() {
						break;
					}

					for transaction in ledger_transactions {
						if transaction.ledger > target_sequence {
							return Ok(transactions);
						}
						transactions.push(StellarTransaction::from(transaction));
					}

					// Increment the number of iterations to ensure we break the loop in case there is no cursor
					current_iteration += 1;
					cursor = response_body["result"]["cursor"]
						.as_str()
						.map(|s| s.to_string());
					if cursor.is_none() {
						break;
					}
				}
				Err(transport_err) => {
					// Ledger info for logging
					let ledger_info = format!(
						"start_sequence: {}, end_sequence: {:?}",
						start_sequence, end_sequence
					);

					return Err(anyhow::anyhow!(transport_err)).context(format!(
						"Failed to {} from Stellar RPC for ledger: {}",
						RPC_METHOD_GET_TRANSACTIONS, ledger_info
					));
				}
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
				let message = format!(
					"start_sequence {} cannot be greater than end_sequence {}",
					start_sequence, end_sequence
				);
				let input_error = StellarClientError::invalid_input(message, None, None);
				return Err(anyhow::anyhow!(input_error))
					.context("Invalid input parameters for Stellar RPC");
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

			let http_response = self
				.http_client
				.send_raw_request(RPC_METHOD_GET_EVENTS, Some(params))
				.await;

			match http_response {
				Ok(response_body) => {
					// Check for RPC errors in the response
					if let Err(rpc_error) = self.check_and_handle_rpc_error(
						&response_body,
						start_sequence,
						target_sequence,
						RPC_METHOD_GET_EVENTS,
					) {
						// A terminal JSON-RPC error was found, convert and return
						return Err(anyhow::anyhow!(rpc_error).context(format!(
							"Soroban RPC reported an error during {}",
							RPC_METHOD_GET_EVENTS
						)));
					}

					// Extract the events from the response
					let raw_events = response_body
						.get("result")
						.and_then(|r| r.get("events"))
						.ok_or_else(|| {
							let message = format!(
								"Unexpected response structure for method '{}'",
								RPC_METHOD_GET_EVENTS
							);
							StellarClientError::unexpected_response_structure(message, None, None)
						})
						.map_err(|client_parse_error| {
							anyhow::anyhow!(client_parse_error)
								.context("Failed to parse event response")
						})?;

					let ledger_events: Vec<StellarEvent> =
						serde_json::from_value(raw_events.clone()).map_err(|e| {
							let message = format!(
								"Failed to parse events from response for method '{}': {}",
								RPC_METHOD_GET_EVENTS, e
							);
							let sce_parse_error = StellarClientError::response_parse_error(
								message,
								Some(e.into()),
								None,
							);
							anyhow::anyhow!(sce_parse_error)
								.context("Failed to parse event response")
						})?;

					for event in ledger_events {
						if event.ledger > target_sequence {
							return Ok(events);
						}
						events.push(event);
					}

					// Increment the number of iterations to ensure we break the loop in case there is no cursor
					current_iteration += 1;
					cursor = response_body["result"]["cursor"]
						.as_str()
						.map(|s| s.to_string());
					if cursor.is_none() {
						break;
					}
				}
				Err(transport_err) => {
					// Ledger info for logging
					let ledger_info = format!(
						"start_sequence: {}, end_sequence: {:?}",
						start_sequence, end_sequence
					);

					return Err(anyhow::anyhow!(transport_err)).context(format!(
						"Failed to {} from Stellar RPC for ledger: {}",
						RPC_METHOD_GET_EVENTS, ledger_info,
					));
				}
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
			.send_raw_request::<serde_json::Value>(RPC_METHOD_GET_LATEST_LEDGER, None)
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
	/// - Returns `anyhow::Error`
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
				let message = format!(
					"start_block {} cannot be greater than end_block {}",
					start_block, end_block
				);
				let input_error = StellarClientError::invalid_input(message, None, None);
				return Err(anyhow::anyhow!(input_error))
					.context("Invalid input parameters for Stellar RPC");
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

			let http_response = self
				.http_client
				.send_raw_request(RPC_METHOD_GET_LEDGERS, Some(params))
				.await;

			match http_response {
				Ok(response_body) => {
					// Check for RPC errors in the response
					if let Err(rpc_error) = self.check_and_handle_rpc_error(
						&response_body,
						start_block as u32,
						target_block as u32,
						RPC_METHOD_GET_LEDGERS,
					) {
						// A terminal JSON-RPC error was found, convert and return
						return Err(anyhow::anyhow!(rpc_error).context(format!(
							"Soroban RPC reported an error during {}",
							RPC_METHOD_GET_LEDGERS,
						)));
					}

					// Extract the ledgers from the response
					let raw_ledgers = response_body
						.get("result")
						.and_then(|r| r.get("ledgers"))
						.ok_or_else(|| {
							let message = format!(
								"Unexpected response structure for method '{}'",
								RPC_METHOD_GET_LEDGERS
							);
							let sce_parse_error = StellarClientError::unexpected_response_structure(
								message, None, None,
							);
							anyhow::anyhow!(sce_parse_error)
								.context("Failed to parse ledger response")
						})?;

					let ledgers: Vec<StellarBlock> = serde_json::from_value(raw_ledgers.clone())
						.map_err(|e| {
							let message = format!(
								"Failed to parse ledgers from response for method '{}': {}",
								RPC_METHOD_GET_LEDGERS, e
							);
							let sce_parse_error = StellarClientError::response_parse_error(
								message,
								Some(e.into()),
								None,
							);
							anyhow::anyhow!(sce_parse_error)
								.context("Failed to parse ledger response")
						})?;

					if ledgers.is_empty() {
						break;
					}

					for ledger in ledgers {
						if (ledger.sequence as u64) > target_block {
							return Ok(blocks);
						}
						blocks.push(BlockType::Stellar(Box::new(ledger)));
					}

					// Increment the number of iterations to ensure we break the loop in case there is no cursor
					current_iteration += 1;
					cursor = response_body["result"]["cursor"]
						.as_str()
						.map(|s| s.to_string());

					// If the cursor is the same as the start block, we have reached the end of the range
					if cursor == Some(start_block.to_string()) {
						break;
					}

					if cursor.is_none() {
						break;
					}
				}
				Err(transport_err) => {
					// Ledger info for logging
					let ledger_info =
						format!("start_block: {}, end_block: {:?}", start_block, end_block);

					return Err(anyhow::anyhow!(transport_err)).context(format!(
						"Failed to {} from Stellar RPC for ledger: {}",
						RPC_METHOD_GET_LEDGERS, ledger_info,
					));
				}
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

		let contract_instance_ledger_key_xdr_bytes = contract_instance_ledger_key
			.to_xdr(Limits::none())
			.map_err(|e| {
				anyhow::anyhow!(
					"Failed to convert contract instance ledger key to XDR: {}",
					e
				)
			})?;
		let contract_instance_ledger_key_xdr =
			BASE64_STANDARD.encode(&contract_instance_ledger_key_xdr_bytes);

		let params = json!({
			"keys": [contract_instance_ledger_key_xdr],
			"xdrFormat": "base64"
		});

		let response = self
			.http_client
			.send_raw_request(RPC_METHOD_GET_LEDGER_ENTRIES, Some(params))
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

		let contract_code_ledger_key_xdr_bytes = contract_code_ledger_key
			.to_xdr(Limits::none())
			.map_err(|e| {
				anyhow::anyhow!("Failed to convert contract code ledger key to XDR: {}", e)
			})?;
		let contract_code_ledger_key_xdr =
			BASE64_STANDARD.encode(&contract_code_ledger_key_xdr_bytes);

		let params = json!({
			"keys": [contract_code_ledger_key_xdr],
			"xdrFormat": "base64"
		});

		let response = self
			.http_client
			.send_raw_request(RPC_METHOD_GET_LEDGER_ENTRIES, Some(params))
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
