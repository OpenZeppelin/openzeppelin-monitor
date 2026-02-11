//! Solana blockchain client implementation.
//!
//! This module provides functionality to interact with the Solana blockchain,
//! supporting operations like block retrieval, transaction lookup, and program account queries.

use anyhow::Context;
use async_trait::async_trait;
use serde_json::json;
use std::marker::PhantomData;
use tracing::instrument;

use crate::{
	models::{
		BlockType, ContractSpec, Network, SolanaBlock, SolanaConfirmedBlock, SolanaContractSpec,
		SolanaInstruction, SolanaTransaction, SolanaTransactionInfo, SolanaTransactionMessage,
		SolanaTransactionMeta,
	},
	services::{
		blockchain::{
			client::{BlockChainClient, BlockFilterFactory},
			transports::{SolanaGetBlockConfig, SolanaTransportClient},
			BlockchainTransport,
		},
		filter::SolanaBlockFilter,
	},
};

use super::error::{error_codes, is_slot_unavailable_error, SolanaClientError};

/// Solana RPC method constants
mod rpc_methods {
	pub const GET_SLOT: &str = "getSlot";
	pub const GET_BLOCK: &str = "getBlock";
	pub const GET_BLOCKS: &str = "getBlocks";
	pub const GET_TRANSACTION: &str = "getTransaction";
	pub const GET_ACCOUNT_INFO: &str = "getAccountInfo";
	pub const GET_PROGRAM_ACCOUNTS: &str = "getProgramAccounts";
	pub const GET_SIGNATURES_FOR_ADDRESS: &str = "getSignaturesForAddress";
}

/// Information about a transaction signature from getSignaturesForAddress
#[derive(Debug, Clone)]
pub struct SignatureInfo {
	/// The transaction signature
	pub signature: String,
	/// The slot the transaction was processed in
	pub slot: u64,
	/// Whether the transaction had an error (None = success)
	pub err: Option<serde_json::Value>,
	/// Block time if available
	pub block_time: Option<i64>,
}

/// Client implementation for the Solana blockchain
///
/// Provides high-level access to Solana blockchain data and operations through HTTP transport.
/// Supports optimized block fetching when monitored addresses are configured.
#[derive(Clone)]
pub struct SolanaClient<T: Send + Sync + Clone> {
	/// The underlying Solana transport client for RPC communication
	http_client: T,
	/// Addresses to monitor for optimized block fetching (e.g., program IDs)
	/// When set, get_blocks uses getSignaturesForAddress instead of getBlock
	monitored_addresses: Vec<String>,
}

impl<T: Send + Sync + Clone> SolanaClient<T> {
	/// Creates a new Solana client instance with a specific transport client
	pub fn new_with_transport(http_client: T) -> Self {
		Self {
			http_client,
			monitored_addresses: Vec::new(),
		}
	}

	/// Configures the client with addresses to monitor
	///
	/// When addresses are set, `get_blocks` will use the optimized
	/// `getSignaturesForAddress` approach instead of fetching all blocks.
	///
	/// # Arguments
	/// * `addresses` - Program IDs or addresses to monitor
	pub fn with_monitored_addresses(mut self, addresses: Vec<String>) -> Self {
		self.monitored_addresses = addresses;
		self
	}

	/// Sets the monitored addresses (mutable version)
	pub fn set_monitored_addresses(&mut self, addresses: Vec<String>) {
		self.monitored_addresses = addresses;
	}

	/// Returns the currently monitored addresses
	pub fn monitored_addresses(&self) -> &[String] {
		&self.monitored_addresses
	}

	/// Checks a JSON-RPC response for error information and converts it into a `SolanaClientError` if present.
	fn check_and_handle_rpc_error(
		&self,
		response_body: &serde_json::Value,
		slot: u64,
		method_name: &'static str,
	) -> Result<(), SolanaClientError> {
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

			// Check for slot unavailable errors
			if is_slot_unavailable_error(rpc_code) {
				return Err(SolanaClientError::slot_not_available(
					slot,
					rpc_message,
					None,
					None,
				));
			}

			// Check for block not available
			if rpc_code == error_codes::BLOCK_NOT_AVAILABLE {
				return Err(SolanaClientError::block_not_available(
					slot,
					rpc_message,
					None,
					None,
				));
			}

			// Other JSON-RPC error
			let message = format!(
				"Solana RPC request failed for method '{}': {} (code {})",
				method_name, rpc_message, rpc_code
			);

			return Err(SolanaClientError::rpc_error(message, None, None));
		}
		Ok(())
	}

	/// Parses a raw block response into a SolanaBlock
	fn parse_block_response(
		&self,
		slot: u64,
		response_body: &serde_json::Value,
	) -> Result<SolanaBlock, SolanaClientError> {
		let result = response_body.get("result").ok_or_else(|| {
			SolanaClientError::unexpected_response_structure(
				"Missing 'result' field in block response",
				None,
				None,
			)
		})?;

		// Handle null result (slot was skipped or block not available)
		if result.is_null() {
			return Err(SolanaClientError::block_not_available(
				slot,
				"Block data is null (slot may have been skipped)",
				None,
				None,
			));
		}

		let blockhash = result
			.get("blockhash")
			.and_then(|v| v.as_str())
			.unwrap_or_default()
			.to_string();

		let previous_blockhash = result
			.get("previousBlockhash")
			.and_then(|v| v.as_str())
			.unwrap_or_default()
			.to_string();

		let parent_slot = result
			.get("parentSlot")
			.and_then(|v| v.as_u64())
			.unwrap_or(0);

		let block_time = result.get("blockTime").and_then(|v| v.as_i64());

		let block_height = result.get("blockHeight").and_then(|v| v.as_u64());

		// Parse transactions
		let transactions = self.parse_transactions_from_block(slot, result)?;

		let confirmed_block = SolanaConfirmedBlock {
			slot,
			blockhash,
			previous_blockhash,
			parent_slot,
			block_time,
			block_height,
			transactions,
		};

		Ok(SolanaBlock::from(confirmed_block))
	}

	/// Parses transactions from a block response
	fn parse_transactions_from_block(
		&self,
		slot: u64,
		block_result: &serde_json::Value,
	) -> Result<Vec<SolanaTransaction>, SolanaClientError> {
		let raw_transactions = match block_result.get("transactions") {
			Some(txs) if txs.is_array() => txs.as_array().unwrap(),
			_ => return Ok(Vec::new()),
		};

		let mut transactions = Vec::with_capacity(raw_transactions.len());

		for raw_tx in raw_transactions {
			if let Some(tx) = self.parse_single_transaction(slot, raw_tx)? {
				transactions.push(tx);
			}
		}

		Ok(transactions)
	}

	/// Parses a single transaction from the block response
	fn parse_single_transaction(
		&self,
		slot: u64,
		raw_tx: &serde_json::Value,
	) -> Result<Option<SolanaTransaction>, SolanaClientError> {
		// Get transaction data
		let transaction = match raw_tx.get("transaction") {
			Some(tx) => tx,
			None => return Ok(None),
		};

		// Get meta data
		let meta = raw_tx.get("meta");

		// Parse signature
		let signature = transaction
			.get("signatures")
			.and_then(|sigs| sigs.get(0))
			.and_then(|sig| sig.as_str())
			.unwrap_or_default()
			.to_string();

		// Parse message
		let message = transaction.get("message");

		// Parse account keys
		let account_keys: Vec<String> = message
			.and_then(|m| m.get("accountKeys"))
			.and_then(|keys| keys.as_array())
			.map(|keys| {
				keys.iter()
					.filter_map(|k| {
						// Handle both string and object formats
						if let Some(s) = k.as_str() {
							Some(s.to_string())
						} else {
							k.get("pubkey")
								.and_then(|p| p.as_str())
								.map(|s| s.to_string())
						}
					})
					.collect()
			})
			.unwrap_or_default();

		// Parse recent blockhash
		let recent_blockhash = message
			.and_then(|m| m.get("recentBlockhash"))
			.and_then(|h| h.as_str())
			.unwrap_or_default()
			.to_string();

		// Parse instructions
		let instructions = self.parse_instructions(message, &account_keys)?;

		// Create transaction message
		let tx_message = SolanaTransactionMessage {
			account_keys,
			recent_blockhash,
			instructions,
			address_table_lookups: Vec::new(),
		};

		// Parse meta
		let tx_meta = meta.map(|m| {
			// err is null for successful transactions, so we need to handle that
			let err = m.get("err").and_then(|e| {
				if e.is_null() {
					None // Success - no error
				} else {
					Some(e.clone()) // Failure - has error
				}
			});
			let fee = m.get("fee").and_then(|f| f.as_u64()).unwrap_or(0);
			let pre_balances: Vec<u64> = m
				.get("preBalances")
				.and_then(|b| b.as_array())
				.map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
				.unwrap_or_default();
			let post_balances: Vec<u64> = m
				.get("postBalances")
				.and_then(|b| b.as_array())
				.map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
				.unwrap_or_default();
			let log_messages: Vec<String> = m
				.get("logMessages")
				.and_then(|logs| logs.as_array())
				.map(|logs| {
					logs.iter()
						.filter_map(|l| l.as_str().map(|s| s.to_string()))
						.collect()
				})
				.unwrap_or_default();

			SolanaTransactionMeta {
				err,
				fee,
				pre_balances,
				post_balances,
				pre_token_balances: Vec::new(),
				post_token_balances: Vec::new(),
				inner_instructions: Vec::new(),
				log_messages,
				compute_units_consumed: m.get("computeUnitsConsumed").and_then(|c| c.as_u64()),
				loaded_addresses: None,
			}
		});

		let tx_info = SolanaTransactionInfo {
			signature,
			slot,
			block_time: None,
			transaction: tx_message,
			meta: tx_meta,
		};

		Ok(Some(SolanaTransaction::from(tx_info)))
	}

	/// Parses instructions from transaction message
	fn parse_instructions(
		&self,
		message: Option<&serde_json::Value>,
		_account_keys: &[String],
	) -> Result<Vec<SolanaInstruction>, SolanaClientError> {
		let raw_instructions = match message.and_then(|m| m.get("instructions")) {
			Some(instrs) if instrs.is_array() => instrs.as_array().unwrap(),
			_ => return Ok(Vec::new()),
		};

		let mut instructions = Vec::with_capacity(raw_instructions.len());

		for raw_instr in raw_instructions {
			// Get program ID index
			let program_id_index = raw_instr
				.get("programIdIndex")
				.and_then(|idx| idx.as_u64())
				.unwrap_or(0) as u8;

			// Get account indices
			let accounts: Vec<u8> = raw_instr
				.get("accounts")
				.and_then(|accs| accs.as_array())
				.map(|accs| {
					accs.iter()
						.filter_map(|idx| idx.as_u64().map(|i| i as u8))
						.collect()
				})
				.unwrap_or_default();

			// Get data (base58 encoded)
			let data = raw_instr
				.get("data")
				.and_then(|d| d.as_str())
				.unwrap_or_default()
				.to_string();

			// Check for parsed instruction
			let parsed = raw_instr.get("parsed").map(|p| {
				let instruction_type = p.get("type").and_then(|t| t.as_str()).unwrap_or_default();
				let info = p.get("info").cloned().unwrap_or(serde_json::Value::Null);
				crate::models::SolanaParsedInstruction {
					instruction_type: instruction_type.to_string(),
					info,
				}
			});

			let program = raw_instr
				.get("program")
				.and_then(|p| p.as_str())
				.map(|s| s.to_string());

			let program_id = raw_instr
				.get("programId")
				.and_then(|p| p.as_str())
				.map(|s| s.to_string());

			instructions.push(SolanaInstruction {
				program_id_index,
				accounts,
				data,
				parsed,
				program,
				program_id,
			});
		}

		Ok(instructions)
	}
}

impl SolanaClient<SolanaTransportClient> {
	/// Creates a new Solana client instance
	pub async fn new(network: &Network) -> Result<Self, anyhow::Error> {
		let http_client = SolanaTransportClient::new(network).await?;
		Ok(Self::new_with_transport(http_client))
	}
}

/// Extended functionality specific to the Solana blockchain
#[async_trait]
pub trait SolanaClientTrait {
	/// Retrieves transactions for a specific slot
	async fn get_transactions(&self, slot: u64) -> Result<Vec<SolanaTransaction>, anyhow::Error>;

	/// Retrieves a single transaction by signature
	async fn get_transaction(
		&self,
		signature: String,
	) -> Result<Option<SolanaTransaction>, anyhow::Error>;

	/// Retrieves signatures with full info (slot, err, block_time) for an address
	/// Optionally filter by slot range
	async fn get_signatures_for_address_with_info(
		&self,
		address: String,
		limit: Option<usize>,
		min_slot: Option<u64>,
		until_signature: Option<String>,
	) -> Result<Vec<SignatureInfo>, anyhow::Error>;

	/// Retrieves all signatures for an address within a slot range with automatic pagination
	/// This method handles pagination internally and returns all signatures up to a safety limit
	async fn get_all_signatures_for_address(
		&self,
		address: String,
		start_slot: u64,
		end_slot: u64,
	) -> Result<Vec<SignatureInfo>, anyhow::Error>;

	/// Retrieves transactions for multiple addresses within a slot range
	/// This is the optimized method that uses getSignaturesForAddress instead of getBlock
	async fn get_transactions_for_addresses(
		&self,
		addresses: &[String],
		start_slot: u64,
		end_slot: Option<u64>,
	) -> Result<Vec<SolanaTransaction>, anyhow::Error>;

	/// Retrieves blocks containing only transactions relevant to the specified addresses
	/// This is the main optimization: instead of fetching all blocks, we fetch only
	/// transactions that involve the monitored addresses and group them into virtual blocks
	///
	/// Returns BlockType::Solana blocks, compatible with the existing filter infrastructure
	async fn get_blocks_for_addresses(
		&self,
		addresses: &[String],
		start_slot: u64,
		end_slot: Option<u64>,
	) -> Result<Vec<BlockType>, anyhow::Error>;

	/// Retrieves account info for a given public key
	async fn get_account_info(&self, pubkey: String) -> Result<serde_json::Value, anyhow::Error>;

	/// Retrieves program accounts for a given program ID
	async fn get_program_accounts(
		&self,
		program_id: String,
	) -> Result<Vec<serde_json::Value>, anyhow::Error>;
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> SolanaClientTrait for SolanaClient<T> {
	#[instrument(skip(self), fields(slot))]
	async fn get_transactions(&self, slot: u64) -> Result<Vec<SolanaTransaction>, anyhow::Error> {
		let config = SolanaGetBlockConfig::full();
		let params = json!([slot, config]);

		let response = self
			.http_client
			.send_raw_request(rpc_methods::GET_BLOCK, Some(params))
			.await
			.with_context(|| format!("Failed to get block for slot {}", slot))?;

		if let Err(rpc_error) =
			self.check_and_handle_rpc_error(&response, slot, rpc_methods::GET_BLOCK)
		{
			return Err(anyhow::anyhow!(rpc_error)
				.context(format!("Solana RPC error while fetching slot {}", slot)));
		}

		let block = self.parse_block_response(slot, &response).map_err(|e| {
			anyhow::anyhow!(e).context(format!("Failed to parse block response for slot {}", slot))
		})?;

		Ok(block.transactions.clone())
	}

	#[instrument(skip(self), fields(signature))]
	async fn get_transaction(
		&self,
		signature: String,
	) -> Result<Option<SolanaTransaction>, anyhow::Error> {
		let config = json!({
			"encoding": "json",
			"commitment": "finalized",
			"maxSupportedTransactionVersion": 0
		});
		let params = json!([signature, config]);

		let response = self
			.http_client
			.send_raw_request(rpc_methods::GET_TRANSACTION, Some(params))
			.await
			.with_context(|| format!("Failed to get transaction {}", signature))?;

		// Check for null result (transaction not found)
		let result = response.get("result");
		if result.is_none() || result.unwrap().is_null() {
			return Ok(None);
		}

		let result = result.unwrap();

		// Extract slot from response
		let slot = result.get("slot").and_then(|s| s.as_u64()).unwrap_or(0);

		// Parse the transaction using existing parsing logic
		// We need to wrap it in the format expected by parse_single_transaction
		let wrapped_tx = json!({
			"transaction": result.get("transaction"),
			"meta": result.get("meta")
		});

		match self.parse_single_transaction(slot, &wrapped_tx) {
			Ok(Some(mut tx)) => {
				// Update block_time if available
				if let Some(block_time) = result.get("blockTime").and_then(|t| t.as_i64()) {
					tx.0.block_time = Some(block_time);
				}
				Ok(Some(tx))
			}
			Ok(None) => Ok(None),
			Err(e) => Err(anyhow::anyhow!(e).context("Failed to parse transaction")),
		}
	}

	#[instrument(skip(self), fields(address, limit, min_slot))]
	async fn get_signatures_for_address_with_info(
		&self,
		address: String,
		limit: Option<usize>,
		min_slot: Option<u64>,
		until_signature: Option<String>,
	) -> Result<Vec<SignatureInfo>, anyhow::Error> {
		let address = &address;
		let until_signature = until_signature.as_deref();
		let mut config = json!({
			"commitment": "finalized",
			"limit": limit.unwrap_or(1000)
		});

		// Add minContextSlot if specified (helps filter old transactions)
		if let Some(min) = min_slot {
			config["minContextSlot"] = json!(min);
		}

		// Add until signature to paginate
		if let Some(until) = until_signature {
			config["until"] = json!(until);
		}

		let params = json!([address, config]);

		let response = self
			.http_client
			.send_raw_request(rpc_methods::GET_SIGNATURES_FOR_ADDRESS, Some(params))
			.await
			.with_context(|| format!("Failed to get signatures for address {}", address))?;

		let result = response
			.get("result")
			.and_then(|r| r.as_array())
			.ok_or_else(|| anyhow::anyhow!("Invalid response structure"))?;

		let signatures: Vec<SignatureInfo> = result
			.iter()
			.filter_map(|item| {
				let signature = item.get("signature")?.as_str()?.to_string();
				let slot = item.get("slot")?.as_u64()?;
				let err =
					item.get("err")
						.and_then(|e| if e.is_null() { None } else { Some(e.clone()) });
				let block_time = item.get("blockTime").and_then(|t| t.as_i64());

				Some(SignatureInfo {
					signature,
					slot,
					err,
					block_time,
				})
			})
			.collect();

		Ok(signatures)
	}

	#[instrument(skip(self), fields(address, start_slot, end_slot))]
	async fn get_all_signatures_for_address(
		&self,
		address: String,
		start_slot: u64,
		end_slot: u64,
	) -> Result<Vec<SignatureInfo>, anyhow::Error> {
		const PAGE_LIMIT: usize = 1000;
		const MAX_SIGNATURES: usize = 100_000; // Safety limit

		let mut all_signatures = Vec::new();
		let mut until_signature: Option<String> = None;
		let mut iteration = 0;

		loop {
			let batch = self
				.get_signatures_for_address_with_info(
					address.clone(),
					Some(PAGE_LIMIT),
					Some(start_slot),
					until_signature.clone(),
				)
				.await?;

			if batch.is_empty() {
				break;
			}

			// Filter by slot range and collect
			let filtered: Vec<SignatureInfo> = batch
				.into_iter()
				.filter(|sig| sig.slot >= start_slot && sig.slot <= end_slot)
				.collect();

			let batch_len = filtered.len();
			until_signature = filtered.last().map(|s| s.signature.clone());
			all_signatures.extend(filtered);

			// Break conditions
			if batch_len < PAGE_LIMIT {
				break; // Last page
			}

			if all_signatures.len() >= MAX_SIGNATURES {
				tracing::warn!(
					address = %address,
					count = all_signatures.len(),
					"Reached maximum signature limit, stopping pagination"
				);
				break;
			}

			iteration += 1;
		}

		tracing::debug!(
			address = %address,
			signatures = all_signatures.len(),
			iterations = iteration + 1,
			"Completed signature pagination"
		);

		Ok(all_signatures)
	}

	#[instrument(skip(self), fields(addresses_count = addresses.len(), start_slot, end_slot))]
	async fn get_transactions_for_addresses(
		&self,
		addresses: &[String],
		start_slot: u64,
		end_slot: Option<u64>,
	) -> Result<Vec<SolanaTransaction>, anyhow::Error> {
		use futures::stream::{self, StreamExt};
		use std::collections::HashSet;

		let end_slot = end_slot.unwrap_or(start_slot);

		if addresses.is_empty() {
			return Ok(Vec::new());
		}

		tracing::debug!(
			addresses = ?addresses,
			start_slot = start_slot,
			end_slot = end_slot,
			"Fetching transactions for addresses using signatures approach"
		);

		// Collect all unique signatures from all addresses within the slot range
		let mut all_signatures: HashSet<String> = HashSet::new();

		for address in addresses {
			// Use paginated method to fetch all signatures
			let signatures = self
				.get_all_signatures_for_address(address.clone(), start_slot, end_slot)
				.await?;

			tracing::debug!(
				address = %address,
				signatures_count = signatures.len(),
				"Got signatures for address"
			);

			for sig_info in signatures {
				all_signatures.insert(sig_info.signature);
			}
		}

		tracing::debug!(
			unique_signatures = all_signatures.len(),
			"Fetching transactions for unique signatures in slot range"
		);

		// Fetch transactions in parallel with controlled concurrency
		let transactions: Vec<SolanaTransaction> = stream::iter(all_signatures)
			.map(|signature| async move {
				let sig = signature.clone();
				match self.get_transaction(signature).await {
					Ok(Some(tx)) => Some(tx),
					Ok(None) => {
						tracing::debug!(signature = %sig, "Transaction not found");
						None
					}
					Err(e) => {
						tracing::warn!(signature = %sig, error = %e, "Failed to fetch transaction");
						None
					}
				}
			})
			.buffer_unordered(20) // 20 concurrent requests
			.filter_map(|result| async move { result })
			.collect()
			.await;

		tracing::debug!(
			fetched_transactions = transactions.len(),
			"Successfully fetched transactions"
		);

		Ok(transactions)
	}

	#[instrument(skip(self), fields(pubkey))]
	async fn get_account_info(&self, pubkey: String) -> Result<serde_json::Value, anyhow::Error> {
		let config = json!({
			"encoding": "jsonParsed",
			"commitment": "finalized"
		});
		let params = json!([&pubkey, config]);

		let response = self
			.http_client
			.send_raw_request(rpc_methods::GET_ACCOUNT_INFO, Some(params))
			.await
			.with_context(|| format!("Failed to get account info for {}", pubkey))?;

		let result = response
			.get("result")
			.cloned()
			.ok_or_else(|| anyhow::anyhow!("Invalid response structure"))?;

		Ok(result)
	}

	#[instrument(skip(self), fields(program_id))]
	async fn get_program_accounts(
		&self,
		program_id: String,
	) -> Result<Vec<serde_json::Value>, anyhow::Error> {
		let config = json!({
			"encoding": "jsonParsed",
			"commitment": "finalized"
		});
		let params = json!([&program_id, config]);

		let response = self
			.http_client
			.send_raw_request(rpc_methods::GET_PROGRAM_ACCOUNTS, Some(params))
			.await
			.with_context(|| format!("Failed to get program accounts for {}", program_id))?;

		let result = response
			.get("result")
			.and_then(|r| r.as_array())
			.cloned()
			.ok_or_else(|| anyhow::anyhow!("Invalid response structure"))?;

		Ok(result)
	}

	#[instrument(skip(self), fields(addresses_count = addresses.len(), start_slot, end_slot))]
	async fn get_blocks_for_addresses(
		&self,
		addresses: &[String],
		start_slot: u64,
		end_slot: Option<u64>,
	) -> Result<Vec<BlockType>, anyhow::Error> {
		use std::collections::BTreeMap;

		// Fetch transactions using the optimized signatures approach
		let transactions = self
			.get_transactions_for_addresses(addresses, start_slot, end_slot)
			.await?;

		if transactions.is_empty() {
			return Ok(Vec::new());
		}

		// Group transactions by slot
		let mut slot_transactions: BTreeMap<u64, Vec<SolanaTransaction>> = BTreeMap::new();
		for tx in transactions {
			let slot = tx.slot();
			slot_transactions.entry(slot).or_default().push(tx);
		}

		// Create virtual blocks for each slot
		let blocks: Vec<BlockType> = slot_transactions
			.into_iter()
			.map(|(slot, txs)| {
				let confirmed_block = SolanaConfirmedBlock {
					slot,
					blockhash: String::new(), // Not available from getTransaction
					previous_blockhash: String::new(),
					parent_slot: slot.saturating_sub(1),
					block_time: txs.first().and_then(|tx| tx.0.block_time),
					block_height: None,
					transactions: txs,
				};
				BlockType::Solana(Box::new(SolanaBlock::from(confirmed_block)))
			})
			.collect();

		tracing::debug!(
			blocks_count = blocks.len(),
			"Created virtual blocks from address-filtered transactions"
		);

		Ok(blocks)
	}
}

impl<T: Send + Sync + Clone + BlockchainTransport> BlockFilterFactory<Self> for SolanaClient<T> {
	type Filter = SolanaBlockFilter<Self>;

	fn filter() -> Self::Filter {
		SolanaBlockFilter {
			_client: PhantomData {},
		}
	}
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockchainTransport> BlockChainClient for SolanaClient<T> {
	#[instrument(skip(self))]
	async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error> {
		let config = json!({ "commitment": "finalized" });
		let params = json!([config]);

		let response = self
			.http_client
			.send_raw_request(rpc_methods::GET_SLOT, Some(params))
			.await
			.with_context(|| "Failed to get latest slot")?;

		let slot = response["result"]
			.as_u64()
			.ok_or_else(|| anyhow::anyhow!("Invalid slot number in response"))?;

		Ok(slot)
	}

	#[instrument(skip(self), fields(start_block, end_block))]
	async fn get_blocks(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<Vec<BlockType>, anyhow::Error> {
		// If monitored addresses are configured, use the optimized approach
		if !self.monitored_addresses.is_empty() {
			tracing::debug!(
				addresses = ?self.monitored_addresses,
				start_block = start_block,
				end_block = ?end_block,
				"Using optimized getSignaturesForAddress approach"
			);
			return SolanaClientTrait::get_blocks_for_addresses(
				self,
				&self.monitored_addresses,
				start_block,
				end_block,
			)
			.await;
		}

		// Standard approach: fetch all blocks
		// Validate input parameters
		if let Some(end_block) = end_block {
			if start_block > end_block {
				let message = format!(
					"start_block {} cannot be greater than end_block {}",
					start_block, end_block
				);
				let input_error = SolanaClientError::invalid_input(message, None, None);
				return Err(anyhow::anyhow!(input_error))
					.context("Invalid input parameters for Solana RPC");
			}
		}

		let target_block = end_block.unwrap_or(start_block);

		// First, get the list of available slots in the range
		let slots = if start_block == target_block {
			vec![start_block]
		} else {
			let params = json!([start_block, target_block, { "commitment": "finalized" }]);
			let response = self
				.http_client
				.send_raw_request(rpc_methods::GET_BLOCKS, Some(params))
				.await
				.with_context(|| {
					format!(
						"Failed to get blocks list from {} to {}",
						start_block, target_block
					)
				})?;

			let slots: Vec<u64> = response["result"]
				.as_array()
				.ok_or_else(|| anyhow::anyhow!("Invalid blocks list response"))?
				.iter()
				.filter_map(|v| v.as_u64())
				.collect();

			if slots.is_empty() {
				return Ok(Vec::new());
			}

			slots
		};

		// Fetch each block
		let mut blocks = Vec::with_capacity(slots.len());
		let config = SolanaGetBlockConfig::full();

		for slot in slots {
			let params = json!([slot, config]);

			let response = self
				.http_client
				.send_raw_request(rpc_methods::GET_BLOCK, Some(params))
				.await;

			match response {
				Ok(response_body) => {
					if let Err(rpc_error) = self.check_and_handle_rpc_error(
						&response_body,
						slot,
						rpc_methods::GET_BLOCK,
					) {
						if rpc_error.is_slot_not_available() || rpc_error.is_block_not_available() {
							tracing::debug!("Skipping unavailable slot {}: {}", slot, rpc_error);
							continue;
						}
						return Err(anyhow::anyhow!(rpc_error)
							.context(format!("Solana RPC error while fetching slot {}", slot)));
					}

					match self.parse_block_response(slot, &response_body) {
						Ok(block) => {
							blocks.push(BlockType::Solana(Box::new(block)));
						}
						Err(parse_error) => {
							if parse_error.is_block_not_available() {
								tracing::debug!(
									"Skipping slot {} due to parse error: {}",
									slot,
									parse_error
								);
								continue;
							}
							return Err(anyhow::anyhow!(parse_error)
								.context(format!("Failed to parse block for slot {}", slot)));
						}
					}
				}
				Err(transport_err) => {
					return Err(anyhow::anyhow!(transport_err)).context(format!(
						"Failed to fetch block from Solana RPC for slot: {}",
						slot
					));
				}
			}
		}

		Ok(blocks)
	}

	#[instrument(skip(self), fields(contract_id))]
	async fn get_contract_spec(&self, contract_id: &str) -> Result<ContractSpec, anyhow::Error> {
		tracing::warn!(
			"Automatic IDL fetching not yet implemented for program {}. \
             Please provide the IDL manually in the monitor configuration.",
			contract_id
		);

		Ok(ContractSpec::Solana(SolanaContractSpec::default()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_solana_client_implements_traits() {
		fn assert_send_sync<T: Send + Sync>() {}
		fn assert_clone<T: Clone>() {}

		assert_send_sync::<SolanaClient<SolanaTransportClient>>();
		assert_clone::<SolanaClient<SolanaTransportClient>>();
	}
}
