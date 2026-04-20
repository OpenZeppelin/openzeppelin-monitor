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
			client::{BlockChainClient, BlockFetchResult, BlockFilterFactory, FetchStreamKind},
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

			let inner_instructions: Vec<crate::models::SolanaInnerInstruction> = m
				.get("innerInstructions")
				.and_then(|ii| ii.as_array())
				.map(|arr| {
					arr.iter()
						.filter_map(|inner| {
							let index = u8::try_from(inner.get("index")?.as_u64()?).ok()?;
							let instructions: Vec<SolanaInstruction> = inner
								.get("instructions")
								.and_then(|instrs| instrs.as_array())
								.map(|instrs| {
									instrs
										.iter()
										.filter_map(Self::parse_single_instruction)
										.collect()
								})
								.unwrap_or_default();
							Some(crate::models::SolanaInnerInstruction {
								index,
								instructions,
							})
						})
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
				inner_instructions,
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

		Ok(raw_instructions
			.iter()
			.filter_map(Self::parse_single_instruction)
			.collect())
	}

	/// Parses a single instruction JSON value into a `SolanaInstruction`.
	///
	/// Shared by top-level and inner-instruction parsing so field extraction
	/// stays consistent across both code paths.
	///
	/// Returns `None` when `programIdIndex` is present but exceeds `u8::MAX`,
	/// since silently truncating would redirect the instruction to a
	/// different account. A missing `programIdIndex` (as in the `jsonParsed`
	/// encoding, where `programId` is the real identifier) defaults to 0.
	/// Individual out-of-range `accounts` entries are dropped, consistent
	/// with the existing handling of non-numeric entries.
	fn parse_single_instruction(raw_instr: &serde_json::Value) -> Option<SolanaInstruction> {
		let program_id_index = match raw_instr.get("programIdIndex").and_then(|idx| idx.as_u64()) {
			Some(n) => u8::try_from(n).ok()?,
			None => 0,
		};

		let accounts: Vec<u8> = raw_instr
			.get("accounts")
			.and_then(|accs| accs.as_array())
			.map(|accs| {
				accs.iter()
					.filter_map(|idx| idx.as_u64().and_then(|i| u8::try_from(i).ok()))
					.collect()
			})
			.unwrap_or_default();

		let data = raw_instr
			.get("data")
			.and_then(|d| d.as_str())
			.unwrap_or_default()
			.to_string();

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

		Some(SolanaInstruction {
			program_id_index,
			accounts,
			data,
			parsed,
			program,
			program_id,
		})
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
	///
	/// Returns a tuple of (successful transactions, failed slot numbers).
	/// Uses all-or-nothing semantics per slot: if any transaction fetch fails for a slot,
	/// all transactions for that slot are discarded and the slot is added to failed_slots.
	async fn get_transactions_for_addresses(
		&self,
		addresses: &[String],
		start_slot: u64,
		end_slot: Option<u64>,
	) -> Result<(Vec<SolanaTransaction>, Vec<u64>), anyhow::Error>;

	/// Retrieves blocks containing only transactions relevant to the specified addresses
	/// This is the main optimization: instead of fetching all blocks, we fetch only
	/// transactions that involve the monitored addresses and group them into virtual blocks
	///
	/// Returns a tuple of (blocks, failed slot numbers).
	/// Returns BlockType::Solana blocks, compatible with the existing filter infrastructure
	async fn get_blocks_for_addresses(
		&self,
		addresses: &[String],
		start_slot: u64,
		end_slot: Option<u64>,
	) -> Result<(Vec<BlockType>, Vec<u64>), anyhow::Error>;

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
	) -> Result<(Vec<SolanaTransaction>, Vec<u64>), anyhow::Error> {
		use futures::stream::{self, StreamExt};
		use std::collections::HashMap;

		let end_slot = end_slot.unwrap_or(start_slot);

		if addresses.is_empty() {
			return Ok((Vec::new(), Vec::new()));
		}

		tracing::debug!(
			addresses = ?addresses,
			start_slot = start_slot,
			end_slot = end_slot,
			"Fetching transactions for addresses using signatures approach"
		);

		// Collect all unique signatures with their slot info from all addresses
		let mut signature_slots: HashMap<String, u64> = HashMap::new();

		for address in addresses {
			let signatures = self
				.get_all_signatures_for_address(address.clone(), start_slot, end_slot)
				.await?;

			tracing::debug!(
				address = %address,
				signatures_count = signatures.len(),
				"Got signatures for address"
			);

			for sig_info in signatures {
				signature_slots
					.entry(sig_info.signature)
					.or_insert(sig_info.slot);
			}
		}

		tracing::debug!(
			unique_signatures = signature_slots.len(),
			"Fetching transactions for unique signatures in slot range"
		);

		// Group signatures by slot for all-or-nothing semantics
		let mut slot_signatures: HashMap<u64, Vec<String>> = HashMap::new();
		for (signature, slot) in signature_slots {
			slot_signatures.entry(slot).or_default().push(signature);
		}

		// Process each slot concurrently with all-or-nothing semantics:
		// If ANY tx fetch fails for a slot, discard ALL txs for that slot.
		let slot_results: Vec<Result<Vec<SolanaTransaction>, u64>> =
			stream::iter(slot_signatures.into_iter())
				.map(|(slot, signatures)| async move {
					// Fetch all transactions for this slot concurrently
					let tx_results: Vec<
						Result<Option<SolanaTransaction>, (String, anyhow::Error)>,
					> = stream::iter(signatures.into_iter())
						.map(|signature| async move {
							let sig = signature.clone();
							match self.get_transaction(signature).await {
								Ok(tx) => Ok(tx),
								Err(e) => Err((sig, e)),
							}
						})
						.buffer_unordered(5)
						.collect()
						.await;

					// Check if any fetch failed — if so, fail the entire slot
					let mut txs = Vec::new();
					for result in tx_results {
						match result {
							Ok(Some(tx)) => txs.push(tx),
							Ok(None) => {
								// Transaction not found is not a failure worth retrying
							}
							Err((sig, e)) => {
								tracing::warn!(
									signature = %sig,
									slot = slot,
									error = %e,
									"Failed to fetch transaction, entire slot will be marked for recovery"
								);
								return Err(slot);
							}
						}
					}
					Ok(txs)
				})
				.buffer_unordered(10)
				.collect()
				.await;

		// Separate successful transactions from failed slots
		let mut transactions = Vec::new();
		let mut failed_slots = Vec::new();
		for result in slot_results {
			match result {
				Ok(txs) => transactions.extend(txs),
				Err(slot) => failed_slots.push(slot),
			}
		}

		if !failed_slots.is_empty() {
			failed_slots.sort();
			failed_slots.dedup();
			tracing::warn!(
				failed_slots = ?failed_slots,
				"Failed to fetch transactions for {} slots",
				failed_slots.len()
			);
		}

		tracing::debug!(
			fetched_transactions = transactions.len(),
			"Successfully fetched transactions"
		);

		Ok((transactions, failed_slots))
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
	) -> Result<(Vec<BlockType>, Vec<u64>), anyhow::Error> {
		use std::collections::BTreeMap;

		// Fetch transactions using the optimized signatures approach
		let (transactions, failed_slots) = self
			.get_transactions_for_addresses(addresses, start_slot, end_slot)
			.await?;

		if transactions.is_empty() {
			return Ok((Vec::new(), failed_slots));
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

		Ok((blocks, failed_slots))
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

	async fn get_blocks_with_meta(
		&self,
		start_block: u64,
		end_block: Option<u64>,
	) -> Result<BlockFetchResult, anyhow::Error> {
		if !self.monitored_addresses.is_empty() {
			tracing::debug!(
				addresses = ?self.monitored_addresses,
				start_block = start_block,
				end_block = ?end_block,
				"Using optimized getSignaturesForAddress approach"
			);
			let (blocks, failed_blocks) = SolanaClientTrait::get_blocks_for_addresses(
				self,
				&self.monitored_addresses,
				start_block,
				end_block,
			)
			.await?;
			Ok(BlockFetchResult {
				blocks,
				failed_blocks,
				stream_kind: FetchStreamKind::Sparse,
			})
		} else {
			let blocks = self.get_blocks(start_block, end_block).await?;
			Ok(BlockFetchResult {
				blocks,
				failed_blocks: Vec::new(),
				stream_kind: FetchStreamKind::Dense,
			})
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::services::blockchain::transports::TransportError;
	use reqwest_middleware::ClientWithMiddleware;
	use serde::Serialize;
	use std::sync::atomic::{AtomicUsize, Ordering};
	use std::sync::Arc;

	/// Configurable mock transport for unit tests.
	/// Returns preset responses based on the RPC method name.
	#[derive(Clone)]
	struct MockTransport {
		responses: std::collections::HashMap<String, Result<serde_json::Value, String>>,
	}

	impl MockTransport {
		fn new() -> Self {
			Self {
				responses: std::collections::HashMap::new(),
			}
		}

		fn with_response(mut self, method: &str, response: serde_json::Value) -> Self {
			self.responses.insert(method.to_string(), Ok(response));
			self
		}

		fn with_error(mut self, method: &str, error: &str) -> Self {
			self.responses
				.insert(method.to_string(), Err(error.to_string()));
			self
		}
	}

	#[async_trait]
	impl crate::services::blockchain::BlockchainTransport for MockTransport {
		async fn get_current_url(&self) -> String {
			"http://mock".to_string()
		}

		async fn send_raw_request<P>(
			&self,
			method: &str,
			_params: Option<P>,
		) -> Result<serde_json::Value, TransportError>
		where
			P: Into<serde_json::Value> + Send + Clone + Serialize,
		{
			match self.responses.get(method) {
				Some(Ok(response)) => Ok(response.clone()),
				Some(Err(msg)) => Err(TransportError::network(msg, None, None)),
				None => Err(TransportError::network(
					format!("no mock response for method: {}", method),
					None,
					None,
				)),
			}
		}

		fn update_endpoint_manager_client(
			&mut self,
			_client: ClientWithMiddleware,
		) -> Result<(), anyhow::Error> {
			Ok(())
		}
	}

	/// Mock transport that returns different responses based on request parameters.
	/// Uses a callback function that receives the method and params, allowing
	/// fine-grained control over responses per-signature, per-address, etc.
	type MockHandler =
		dyn Fn(&str, &serde_json::Value) -> Result<serde_json::Value, String> + Send + Sync;

	#[derive(Clone)]
	struct CallbackMockTransport {
		handler: Arc<MockHandler>,
	}

	impl CallbackMockTransport {
		fn new<F>(handler: F) -> Self
		where
			F: Fn(&str, &serde_json::Value) -> Result<serde_json::Value, String>
				+ Send
				+ Sync
				+ 'static,
		{
			Self {
				handler: Arc::new(handler),
			}
		}
	}

	#[async_trait]
	impl crate::services::blockchain::BlockchainTransport for CallbackMockTransport {
		async fn get_current_url(&self) -> String {
			"http://mock-callback".to_string()
		}

		async fn send_raw_request<P>(
			&self,
			method: &str,
			params: Option<P>,
		) -> Result<serde_json::Value, TransportError>
		where
			P: Into<serde_json::Value> + Send + Clone + Serialize,
		{
			let params_value = params.map(|p| p.into()).unwrap_or(serde_json::Value::Null);
			match (self.handler)(method, &params_value) {
				Ok(val) => Ok(val),
				Err(msg) => Err(TransportError::network(msg, None, None)),
			}
		}

		fn update_endpoint_manager_client(
			&mut self,
			_client: ClientWithMiddleware,
		) -> Result<(), anyhow::Error> {
			Ok(())
		}
	}

	fn mock_client() -> SolanaClient<MockTransport> {
		SolanaClient::new_with_transport(MockTransport::new())
	}

	fn mock_client_with_transport(transport: MockTransport) -> SolanaClient<MockTransport> {
		SolanaClient::new_with_transport(transport)
	}

	fn mock_tx_response(slot: u64, sig: &str) -> serde_json::Value {
		json!({
			"result": {
				"slot": slot,
				"blockTime": 1234567890,
				"transaction": {
					"signatures": [sig],
					"message": {
						"accountKeys": ["Account1"],
						"instructions": [],
						"recentBlockhash": "hash1"
					}
				},
				"meta": {
					"err": null,
					"fee": 5000,
					"preBalances": [100],
					"postBalances": [95],
					"logMessages": []
				}
			}
		})
	}

	#[test]
	fn test_solana_client_implements_traits() {
		fn assert_send_sync<T: Send + Sync>() {}
		fn assert_clone<T: Clone>() {}

		assert_send_sync::<SolanaClient<SolanaTransportClient>>();
		assert_clone::<SolanaClient<SolanaTransportClient>>();
	}

	#[test]
	fn test_new_client_has_empty_monitored_addresses() {
		let client = mock_client();
		assert!(client.monitored_addresses.is_empty());
	}

	#[tokio::test]
	async fn test_get_transactions_for_addresses_empty_addresses() {
		let client = mock_client();

		let result =
			SolanaClientTrait::get_transactions_for_addresses(&client, &[], 100, Some(200)).await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert!(txs.is_empty());
		assert!(failed.is_empty());
	}

	#[tokio::test]
	async fn test_get_transactions_for_addresses_records_failed_slots() {
		// Set up transport: getSignaturesForAddress returns one signature,
		// but getTransaction fails
		let transport = MockTransport::new()
			.with_response(
				rpc_methods::GET_SIGNATURES_FOR_ADDRESS,
				json!({
					"result": [
						{
							"signature": "sig1",
							"slot": 150,
							"err": null,
							"blockTime": 1234567890
						}
					]
				}),
			)
			.with_error(rpc_methods::GET_TRANSACTION, "RPC node unavailable");

		let client = mock_client_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["ProgramId1".to_string()],
			100,
			Some(200),
		)
		.await;

		// Should succeed but return no transactions, with slot 150 as failed
		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert!(txs.is_empty());
		assert_eq!(failed, vec![150]);
	}

	#[tokio::test]
	async fn test_get_transactions_for_addresses_success_no_failed_slots() {
		// Set up transport: getSignaturesForAddress returns one signature,
		// getTransaction returns a valid transaction
		let transport = MockTransport::new()
			.with_response(
				rpc_methods::GET_SIGNATURES_FOR_ADDRESS,
				json!({
					"result": [
						{
							"signature": "sig1",
							"slot": 150,
							"err": null,
							"blockTime": 1234567890
						}
					]
				}),
			)
			.with_response(
				rpc_methods::GET_TRANSACTION,
				json!({
					"result": {
						"slot": 150,
						"blockTime": 1234567890,
						"transaction": {
							"signatures": ["sig1"],
							"message": {
								"accountKeys": ["Account1"],
								"instructions": [],
								"recentBlockhash": "hash1"
							}
						},
						"meta": {
							"err": null,
							"fee": 5000,
							"preBalances": [100],
							"postBalances": [95],
							"logMessages": []
						}
					}
				}),
			);

		let client = mock_client_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["ProgramId1".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert_eq!(txs.len(), 1);
		assert!(failed.is_empty());
	}

	#[tokio::test]
	async fn test_all_or_nothing_partial_failure() {
		// Slot 150 has 2 signatures: sig1 succeeds, sig2 fails
		// All-or-nothing: entire slot should fail, zero txs returned for it
		let transport = MockTransport::new()
			.with_response(
				rpc_methods::GET_SIGNATURES_FOR_ADDRESS,
				json!({
					"result": [
						{
							"signature": "sig1",
							"slot": 150,
							"err": null,
							"blockTime": 1234567890
						},
						{
							"signature": "sig2",
							"slot": 150,
							"err": null,
							"blockTime": 1234567890
						}
					]
				}),
			)
			// MockTransport returns the same response for all calls to a method,
			// so we use an error to simulate one failure (all sigs for slot fail)
			.with_error(rpc_methods::GET_TRANSACTION, "RPC node unavailable");

		let client = mock_client_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["ProgramId1".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		// All-or-nothing: no txs should be returned for the failed slot
		assert!(txs.is_empty(), "No txs should be returned for failed slot");
		assert_eq!(failed, vec![150], "Slot 150 should be in failed_blocks");
	}

	#[tokio::test]
	async fn test_all_or_nothing_mixed_slots() {
		// We need a transport that can return different results for different signatures.
		// Since MockTransport returns the same response per method, we'll use a custom approach.
		// Slot 150 succeeds (sig1), slot 160 fails (sig2 fails)
		// We'll test with a transport that succeeds for getTransaction — both slots succeed.
		// Then verify with separate test that failure correctly isolates to one slot.

		// For this test: both slots succeed
		let transport = MockTransport::new()
			.with_response(
				rpc_methods::GET_SIGNATURES_FOR_ADDRESS,
				json!({
					"result": [
						{
							"signature": "sig1",
							"slot": 150,
							"err": null,
							"blockTime": 1234567890
						},
						{
							"signature": "sig2",
							"slot": 160,
							"err": null,
							"blockTime": 1234567891
						}
					]
				}),
			)
			.with_response(
				rpc_methods::GET_TRANSACTION,
				json!({
					"result": {
						"slot": 150,
						"blockTime": 1234567890,
						"transaction": {
							"signatures": ["sig1"],
							"message": {
								"accountKeys": ["Account1"],
								"instructions": [],
								"recentBlockhash": "hash1"
							}
						},
						"meta": {
							"err": null,
							"fee": 5000,
							"preBalances": [100],
							"postBalances": [95],
							"logMessages": []
						}
					}
				}),
			);

		let client = mock_client_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["ProgramId1".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		// Both slots succeed — 2 transactions returned
		assert_eq!(txs.len(), 2);
		assert!(failed.is_empty());
	}

	#[tokio::test]
	async fn test_get_blocks_with_meta_sparse_path() {
		// With monitored addresses set, should return Sparse stream kind
		let transport = MockTransport::new()
			.with_response(
				rpc_methods::GET_SIGNATURES_FOR_ADDRESS,
				json!({
					"result": [
						{
							"signature": "sig1",
							"slot": 150,
							"err": null,
							"blockTime": 1234567890
						}
					]
				}),
			)
			.with_error(rpc_methods::GET_TRANSACTION, "RPC node unavailable");

		let client = mock_client_with_transport(transport)
			.with_monitored_addresses(vec!["ProgramId1".to_string()]);

		let result = BlockChainClient::get_blocks_with_meta(&client, 100, Some(200)).await;
		assert!(result.is_ok());
		let fetch_result = result.unwrap();
		assert_eq!(fetch_result.stream_kind, FetchStreamKind::Sparse);
		assert!(fetch_result.blocks.is_empty());
		assert_eq!(fetch_result.failed_blocks, vec![150]);
	}

	#[tokio::test]
	async fn test_get_blocks_with_meta_dense_path() {
		// Without monitored addresses, should return Dense stream kind
		let transport = MockTransport::new()
			.with_response(rpc_methods::GET_BLOCKS, json!({ "result": [100] }))
			.with_response(
				rpc_methods::GET_BLOCK,
				json!({
					"result": {
						"blockhash": "hash1",
						"previousBlockhash": "hash0",
						"parentSlot": 99,
						"blockTime": 1234567890,
						"blockHeight": 100,
						"transactions": []
					}
				}),
			);

		let client = mock_client_with_transport(transport);
		// No monitored addresses — Dense path

		let result = BlockChainClient::get_blocks_with_meta(&client, 100, Some(100)).await;
		assert!(result.is_ok());
		let fetch_result = result.unwrap();
		assert_eq!(fetch_result.stream_kind, FetchStreamKind::Dense);
		assert!(fetch_result.failed_blocks.is_empty());
		assert_eq!(fetch_result.blocks.len(), 1);
	}

	#[tokio::test]
	async fn test_get_blocks_with_meta_dense_error_propagation() {
		// Dense path should propagate errors from get_blocks
		let transport = MockTransport::new().with_error(rpc_methods::GET_BLOCK, "RPC node crashed");

		let client = mock_client_with_transport(transport);

		let result = BlockChainClient::get_blocks_with_meta(&client, 100, None).await;
		assert!(
			result.is_err(),
			"Error from get_blocks should propagate through get_blocks_with_meta"
		);
	}

	#[tokio::test]
	async fn test_all_or_nothing_mixed_slots_one_fails() {
		// Slot 150 (sig1) succeeds, slot 160 (sig2) fails.
		// Uses param-aware mock to return different results per signature.
		let transport = CallbackMockTransport::new(|method, params| match method {
			"getSignaturesForAddress" => Ok(json!({
				"result": [
					{ "signature": "sig1", "slot": 150, "err": null, "blockTime": 100 },
					{ "signature": "sig2", "slot": 160, "err": null, "blockTime": 101 }
				]
			})),
			"getTransaction" => {
				let sig = params
					.as_array()
					.and_then(|a| a.first())
					.and_then(|v| v.as_str())
					.unwrap_or("");
				if sig == "sig1" {
					Ok(mock_tx_response(150, "sig1"))
				} else {
					Err("RPC unavailable".to_string())
				}
			}
			_ => Err(format!("unexpected method: {}", method)),
		});

		let client = SolanaClient::new_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["ProgramId1".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert_eq!(txs.len(), 1, "Only slot 150's tx should be returned");
		assert_eq!(txs[0].slot(), 150);
		assert_eq!(failed, vec![160], "Slot 160 should be in failed_blocks");
	}

	#[tokio::test]
	async fn test_all_or_nothing_all_slots_fail() {
		// Both slots fail — zero txs, both in failed_blocks
		let transport = MockTransport::new()
			.with_response(
				rpc_methods::GET_SIGNATURES_FOR_ADDRESS,
				json!({
					"result": [
						{
							"signature": "sig1",
							"slot": 150,
							"err": null,
							"blockTime": 1234567890
						},
						{
							"signature": "sig2",
							"slot": 160,
							"err": null,
							"blockTime": 1234567891
						}
					]
				}),
			)
			.with_error(rpc_methods::GET_TRANSACTION, "RPC node down");

		let client = mock_client_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["ProgramId1".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert!(
			txs.is_empty(),
			"No txs should be returned when all slots fail"
		);
		assert_eq!(failed.len(), 2);
		assert!(failed.contains(&150));
		assert!(failed.contains(&160));
	}

	#[tokio::test]
	async fn test_all_or_nothing_tx_not_found_is_not_failure() {
		// getTransaction returns Ok(None) (null result) — not a failure,
		// the slot should NOT appear in failed_blocks
		let transport = MockTransport::new()
			.with_response(
				rpc_methods::GET_SIGNATURES_FOR_ADDRESS,
				json!({
					"result": [
						{
							"signature": "sig1",
							"slot": 150,
							"err": null,
							"blockTime": 1234567890
						}
					]
				}),
			)
			.with_response(rpc_methods::GET_TRANSACTION, json!({ "result": null }));

		let client = mock_client_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["ProgramId1".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert!(
			txs.is_empty(),
			"Not-found tx should not produce a transaction"
		);
		assert!(
			failed.is_empty(),
			"Not-found tx should not mark slot as failed"
		);
	}

	#[tokio::test]
	async fn test_overlapping_signatures_from_multiple_addresses_deduped() {
		// Two addresses return the same signature — should be deduplicated.
		// Uses a call counter to verify getTransaction is called only once.
		let call_count = Arc::new(AtomicUsize::new(0));
		let call_count_clone = call_count.clone();
		let transport = CallbackMockTransport::new(move |method, _params| {
			match method {
				"getSignaturesForAddress" => {
					// Both addresses return the same signature
					Ok(json!({
						"result": [
							{
								"signature": "shared_sig",
								"slot": 150,
								"err": null,
								"blockTime": 1234567890
							}
						]
					}))
				}
				"getTransaction" => {
					call_count_clone.fetch_add(1, Ordering::SeqCst);
					Ok(mock_tx_response(150, "shared_sig"))
				}
				_ => Err(format!("unexpected method: {}", method)),
			}
		});

		let client = SolanaClient::new_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["Addr1".to_string(), "Addr2".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert_eq!(txs.len(), 1, "Duplicate signatures should be deduplicated");
		assert!(failed.is_empty());
		assert_eq!(
			call_count.load(Ordering::SeqCst),
			1,
			"getTransaction should only be called once for deduplicated signature"
		);
	}

	#[tokio::test]
	async fn test_get_blocks_for_addresses_returns_blocks_and_failed_slots() {
		// Test that get_blocks_for_addresses returns both blocks AND failed slots
		// sig1 at slot 150 succeeds, sig2 at slot 160 fails
		let transport = CallbackMockTransport::new(|method, params| match method {
			"getSignaturesForAddress" => Ok(json!({
				"result": [
					{ "signature": "sig1", "slot": 150, "err": null, "blockTime": 100 },
					{ "signature": "sig2", "slot": 160, "err": null, "blockTime": 101 }
				]
			})),
			"getTransaction" => {
				let sig = params
					.as_array()
					.and_then(|a| a.first())
					.and_then(|v| v.as_str())
					.unwrap_or("");
				if sig == "sig1" {
					Ok(mock_tx_response(150, "sig1"))
				} else {
					Err("RPC unavailable".to_string())
				}
			}
			_ => Err(format!("unexpected method: {}", method)),
		});

		let client = SolanaClient::new_with_transport(transport);

		let result = SolanaClientTrait::get_blocks_for_addresses(
			&client,
			&["ProgramId1".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (blocks, failed) = result.unwrap();
		assert_eq!(blocks.len(), 1, "Only slot 150 should produce a block");
		match &blocks[0] {
			BlockType::Solana(block) => assert_eq!(block.slot, 150),
			_ => panic!("Expected Solana block"),
		}
		assert_eq!(failed, vec![160], "Slot 160 should be in failed_blocks");
	}

	#[tokio::test]
	async fn test_get_blocks_with_meta_sparse_returns_blocks_and_failed() {
		// get_blocks_with_meta in sparse mode should propagate both blocks and failures
		let transport = CallbackMockTransport::new(|method, params| match method {
			"getSignaturesForAddress" => Ok(json!({
				"result": [
					{ "signature": "sig1", "slot": 150, "err": null, "blockTime": 100 },
					{ "signature": "sig2", "slot": 160, "err": null, "blockTime": 101 }
				]
			})),
			"getTransaction" => {
				let sig = params
					.as_array()
					.and_then(|a| a.first())
					.and_then(|v| v.as_str())
					.unwrap_or("");
				if sig == "sig1" {
					Ok(mock_tx_response(150, "sig1"))
				} else {
					Err("RPC unavailable".to_string())
				}
			}
			_ => Err(format!("unexpected method: {}", method)),
		});

		let client = SolanaClient::new_with_transport(transport)
			.with_monitored_addresses(vec!["ProgramId1".to_string()]);

		let result = BlockChainClient::get_blocks_with_meta(&client, 100, Some(200)).await;
		assert!(result.is_ok());
		let fetch_result = result.unwrap();
		assert_eq!(fetch_result.stream_kind, FetchStreamKind::Sparse);
		assert_eq!(fetch_result.blocks.len(), 1);
		assert_eq!(fetch_result.failed_blocks, vec![160]);
	}

	#[tokio::test]
	async fn test_get_blocks_standard_mode_empty_slot_list() {
		// When getBlocks returns an empty list for a range, get_blocks should return empty
		let transport =
			MockTransport::new().with_response(rpc_methods::GET_BLOCKS, json!({ "result": [] }));

		let client = mock_client_with_transport(transport);

		let result = client.get_blocks(100, Some(200)).await;
		assert!(result.is_ok());
		assert!(result.unwrap().is_empty());
	}

	#[tokio::test]
	async fn test_get_blocks_standard_mode_still_works_with_monitored_addresses() {
		// After the redesign, get_blocks always uses the standard path
		// even when monitored_addresses are set (get_blocks_with_meta handles routing)
		let transport = MockTransport::new()
			.with_response(rpc_methods::GET_BLOCKS, json!({ "result": [100] }))
			.with_response(
				rpc_methods::GET_BLOCK,
				json!({
					"result": {
						"blockhash": "hash1",
						"previousBlockhash": "hash0",
						"parentSlot": 99,
						"blockTime": 1234567890,
						"blockHeight": 100,
						"transactions": []
					}
				}),
			);

		let client = mock_client_with_transport(transport)
			.with_monitored_addresses(vec!["SomeProgram".to_string()]);

		// get_blocks should still use standard path regardless of monitored_addresses
		let result = client.get_blocks(100, Some(100)).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap().len(), 1);
	}

	#[tokio::test]
	async fn test_single_sig_per_slot_failure_isolation() {
		// Each slot has exactly 1 signature. Slot 100 (sigA) succeeds, slot 200 (sigB) fails.
		// Uses param-aware mock to deterministically control per-signature outcomes.
		let transport = CallbackMockTransport::new(|method, params| match method {
			"getSignaturesForAddress" => Ok(json!({
				"result": [
					{ "signature": "sigA", "slot": 100, "err": null, "blockTime": 100 },
					{ "signature": "sigB", "slot": 200, "err": null, "blockTime": 101 }
				]
			})),
			"getTransaction" => {
				let sig = params
					.as_array()
					.and_then(|a| a.first())
					.and_then(|v| v.as_str())
					.unwrap_or("");
				if sig == "sigA" {
					Ok(mock_tx_response(100, "sigA"))
				} else {
					Err("timeout".to_string())
				}
			}
			_ => Err(format!("unexpected method: {}", method)),
		});

		let client = SolanaClient::new_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["Addr1".to_string()],
			50,
			Some(250),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert_eq!(txs.len(), 1);
		assert_eq!(txs[0].slot(), 100);
		assert_eq!(failed, vec![200]);
	}

	#[tokio::test]
	async fn test_multiple_sigs_same_slot_all_succeed() {
		// A slot with 3 signatures, all succeed — all txs should be returned
		let transport = CallbackMockTransport::new(|method, _params| {
			match method {
				"getSignaturesForAddress" => Ok(json!({
					"result": [
						{ "signature": "s1", "slot": 150, "err": null, "blockTime": 100 },
						{ "signature": "s2", "slot": 150, "err": null, "blockTime": 100 },
						{ "signature": "s3", "slot": 150, "err": null, "blockTime": 100 }
					]
				})),
				"getTransaction" => {
					// All succeed
					Ok(mock_tx_response(150, "tx"))
				}
				_ => Err(format!("unexpected method: {}", method)),
			}
		});

		let client = SolanaClient::new_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["Addr1".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert_eq!(
			txs.len(),
			3,
			"All 3 txs from the same slot should be returned"
		);
		assert!(failed.is_empty());
	}

	#[tokio::test]
	async fn test_multiple_sigs_same_slot_one_fails_discards_all() {
		// A slot with 3 signatures: s1 and s2 succeed, s3 fails
		// ALL txs for that slot should be discarded (all-or-nothing)
		let transport = CallbackMockTransport::new(|method, params| match method {
			"getSignaturesForAddress" => Ok(json!({
				"result": [
					{ "signature": "s1", "slot": 150, "err": null, "blockTime": 100 },
					{ "signature": "s2", "slot": 150, "err": null, "blockTime": 100 },
					{ "signature": "s3", "slot": 150, "err": null, "blockTime": 100 }
				]
			})),
			"getTransaction" => {
				let sig = params
					.as_array()
					.and_then(|a| a.first())
					.and_then(|v| v.as_str())
					.unwrap_or("");
				if sig == "s3" {
					Err("connection reset".to_string())
				} else {
					Ok(mock_tx_response(150, sig))
				}
			}
			_ => Err(format!("unexpected method: {}", method)),
		});

		let client = SolanaClient::new_with_transport(transport);

		let result = SolanaClientTrait::get_transactions_for_addresses(
			&client,
			&["Addr1".to_string()],
			100,
			Some(200),
		)
		.await;

		assert!(result.is_ok());
		let (txs, failed) = result.unwrap();
		assert!(
			txs.is_empty(),
			"All txs for the slot should be discarded when one fails"
		);
		assert_eq!(
			failed,
			vec![150],
			"The entire slot should be marked as failed"
		);
	}

	#[test]
	fn test_parse_single_transaction_with_inner_instructions() {
		let client = mock_client();
		let raw_tx = json!({
			"transaction": {
				"signatures": ["sig_with_inner"],
				"message": {
					"accountKeys": [
						"FeePayer111111111111111111111111111111111111",
						"SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf",
						"BPFLoaderUpgradeab1e11111111111111111111111",
						"KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD"
					],
					"instructions": [{
						"programIdIndex": 1,
						"accounts": [0, 2, 3],
						"data": "3Bxs4h24hBtQy9rw"
					}],
					"recentBlockhash": "hash1"
				}
			},
			"meta": {
				"err": null,
				"fee": 5000,
				"preBalances": [1000000, 500000],
				"postBalances": [995000, 500000],
				"logMessages": [
					"Program SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf invoke [1]",
					"Program BPFLoaderUpgradeab1e11111111111111111111111 invoke [2]",
					"Upgraded program KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD",
					"Program BPFLoaderUpgradeab1e11111111111111111111111 success",
					"Program SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf success"
				],
				"innerInstructions": [{
					"index": 0,
					"instructions": [{
						"programIdIndex": 2,
						"accounts": [3],
						"data": ""
					}]
				}]
			}
		});

		let result = client.parse_single_transaction(100, &raw_tx);
		assert!(result.is_ok());
		let tx = result.unwrap().expect("should parse transaction");

		// Verify inner instructions were parsed
		let meta = tx.meta.as_ref().unwrap();
		assert_eq!(meta.inner_instructions.len(), 1);
		assert_eq!(meta.inner_instructions[0].index, 0);
		assert_eq!(meta.inner_instructions[0].instructions.len(), 1);
		assert_eq!(
			meta.inner_instructions[0].instructions[0].program_id_index,
			2
		);

		// Verify program_ids() includes both top-level and inner instruction programs
		let program_ids = tx.program_ids();
		assert!(program_ids.contains(&"BPFLoaderUpgradeab1e11111111111111111111111".to_string()));
	}

	#[test]
	fn test_parse_single_transaction_with_inner_instructions_parsed_format() {
		let client = mock_client();
		let raw_tx = json!({
			"transaction": {
				"signatures": ["sig_parsed_inner"],
				"message": {
					"accountKeys": [
						"FeePayer111111111111111111111111111111111111",
						"SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf"
					],
					"instructions": [{
						"programIdIndex": 1,
						"accounts": [0],
						"data": "abc"
					}],
					"recentBlockhash": "hash1"
				}
			},
			"meta": {
				"err": null,
				"fee": 5000,
				"preBalances": [100],
				"postBalances": [95],
				"logMessages": [],
				"innerInstructions": [{
					"index": 0,
					"instructions": [{
						"programIdIndex": 0,
						"accounts": [],
						"data": "xyz",
						"program": "bpf-upgradeable-loader",
						"programId": "BPFLoaderUpgradeab1e11111111111111111111111",
						"parsed": {
							"type": "upgrade",
							"info": {
								"programId": "KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD"
							}
						}
					}]
				}]
			}
		});

		let result = client.parse_single_transaction(100, &raw_tx);
		assert!(result.is_ok());
		let tx = result.unwrap().expect("should parse transaction");

		let inner_ix = &tx.meta.as_ref().unwrap().inner_instructions[0].instructions[0];
		assert_eq!(
			inner_ix.program_id,
			Some("BPFLoaderUpgradeab1e11111111111111111111111".to_string())
		);
		assert_eq!(inner_ix.program, Some("bpf-upgradeable-loader".to_string()));
		assert!(inner_ix.parsed.is_some());
		assert_eq!(
			inner_ix.parsed.as_ref().unwrap().instruction_type,
			"upgrade"
		);
	}

	#[test]
	fn test_parse_single_transaction_without_inner_instructions() {
		let client = mock_client();
		let raw_tx = json!({
			"transaction": {
				"signatures": ["sig_no_inner"],
				"message": {
					"accountKeys": ["Account1"],
					"instructions": [],
					"recentBlockhash": "hash1"
				}
			},
			"meta": {
				"err": null,
				"fee": 5000,
				"preBalances": [100],
				"postBalances": [95],
				"logMessages": []
			}
		});

		let result = client.parse_single_transaction(100, &raw_tx);
		assert!(result.is_ok());
		let tx = result.unwrap().expect("should parse transaction");

		// No innerInstructions field -> empty vec
		assert!(tx.meta.as_ref().unwrap().inner_instructions.is_empty());
	}

	#[test]
	fn test_parse_single_transaction_with_empty_inner_instructions() {
		let client = mock_client();
		let raw_tx = json!({
			"transaction": {
				"signatures": ["sig_empty_inner"],
				"message": {
					"accountKeys": ["Account1"],
					"instructions": [],
					"recentBlockhash": "hash1"
				}
			},
			"meta": {
				"err": null,
				"fee": 5000,
				"preBalances": [100],
				"postBalances": [95],
				"logMessages": [],
				"innerInstructions": []
			}
		});

		let result = client.parse_single_transaction(100, &raw_tx);
		assert!(result.is_ok());
		let tx = result.unwrap().expect("should parse transaction");

		assert!(tx.meta.as_ref().unwrap().inner_instructions.is_empty());
	}

	#[test]
	fn test_parse_single_transaction_drops_instruction_with_oob_program_id_index() {
		// A conforming RPC cannot return programIdIndex > 255 (Solana's wire
		// format packs it as u8), but if one does, silently truncating with
		// `as u8` would redirect the instruction to account 0 and could
		// produce false monitor matches. The parser must drop it instead.
		let client = mock_client();
		let raw_tx = json!({
			"transaction": {
				"signatures": ["sig_oob_pid"],
				"message": {
					"accountKeys": ["Account1", "Account2"],
					"instructions": [
						{ "programIdIndex": 1, "accounts": [0], "data": "ok" },
						{ "programIdIndex": 300, "accounts": [0], "data": "bad" },
						{ "programIdIndex": 0, "accounts": [1], "data": "ok2" }
					],
					"recentBlockhash": "hash1"
				}
			},
			"meta": {
				"err": null,
				"fee": 0,
				"preBalances": [0, 0],
				"postBalances": [0, 0],
				"logMessages": []
			}
		});

		let tx = client
			.parse_single_transaction(1, &raw_tx)
			.unwrap()
			.expect("should parse transaction");

		// Out-of-range instruction dropped; the two valid ones remain in order.
		assert_eq!(tx.transaction.instructions.len(), 2);
		assert_eq!(tx.transaction.instructions[0].data, "ok");
		assert_eq!(tx.transaction.instructions[1].data, "ok2");
	}

	#[test]
	fn test_parse_single_transaction_drops_oob_account_indices() {
		// Individual out-of-range account indices should be dropped, matching
		// the existing filter_map behavior for non-numeric entries. Other
		// valid indices in the same instruction are preserved.
		let client = mock_client();
		let raw_tx = json!({
			"transaction": {
				"signatures": ["sig_oob_acct"],
				"message": {
					"accountKeys": ["Account1"],
					"instructions": [{
						"programIdIndex": 0,
						"accounts": [1, 500, 2, 9999, 3],
						"data": "d"
					}],
					"recentBlockhash": "hash1"
				}
			},
			"meta": {
				"err": null,
				"fee": 0,
				"preBalances": [0],
				"postBalances": [0],
				"logMessages": []
			}
		});

		let tx = client
			.parse_single_transaction(1, &raw_tx)
			.unwrap()
			.expect("should parse transaction");

		assert_eq!(tx.transaction.instructions.len(), 1);
		assert_eq!(tx.transaction.instructions[0].accounts, vec![1u8, 2, 3]);
	}

	#[test]
	fn test_parse_single_transaction_drops_inner_instruction_with_oob_program_id_index() {
		let client = mock_client();
		let raw_tx = json!({
			"transaction": {
				"signatures": ["sig_inner_oob_pid"],
				"message": {
					"accountKeys": ["Account1", "Account2"],
					"instructions": [{
						"programIdIndex": 1, "accounts": [0], "data": "x"
					}],
					"recentBlockhash": "hash1"
				}
			},
			"meta": {
				"err": null,
				"fee": 0,
				"preBalances": [0, 0],
				"postBalances": [0, 0],
				"logMessages": [],
				"innerInstructions": [{
					"index": 0,
					"instructions": [
						{ "programIdIndex": 1, "accounts": [], "data": "a" },
						{ "programIdIndex": 999, "accounts": [], "data": "b" },
						{ "programIdIndex": 0, "accounts": [], "data": "c" }
					]
				}]
			}
		});

		let tx = client
			.parse_single_transaction(1, &raw_tx)
			.unwrap()
			.expect("should parse transaction");

		let inner = &tx.meta.as_ref().unwrap().inner_instructions;
		assert_eq!(inner.len(), 1);
		assert_eq!(inner[0].instructions.len(), 2);
		assert_eq!(inner[0].instructions[0].data, "a");
		assert_eq!(inner[0].instructions[1].data, "c");
	}

	#[test]
	fn test_parse_single_transaction_drops_inner_group_with_oob_index() {
		// The `index` on an innerInstructions group must fit in u8 (it
		// references an outer instruction, which is u8-indexed on the wire).
		// An out-of-range group index drops the whole group.
		let client = mock_client();
		let raw_tx = json!({
			"transaction": {
				"signatures": ["sig_inner_oob_group"],
				"message": {
					"accountKeys": ["Account1"],
					"instructions": [{
						"programIdIndex": 0, "accounts": [], "data": "x"
					}],
					"recentBlockhash": "hash1"
				}
			},
			"meta": {
				"err": null,
				"fee": 0,
				"preBalances": [0],
				"postBalances": [0],
				"logMessages": [],
				"innerInstructions": [
					{
						"index": 0,
						"instructions": [{
							"programIdIndex": 0, "accounts": [], "data": "kept"
						}]
					},
					{
						"index": 300,
						"instructions": [{
							"programIdIndex": 0, "accounts": [], "data": "dropped"
						}]
					}
				]
			}
		});

		let tx = client
			.parse_single_transaction(1, &raw_tx)
			.unwrap()
			.expect("should parse transaction");

		let inner = &tx.meta.as_ref().unwrap().inner_instructions;
		assert_eq!(inner.len(), 1);
		assert_eq!(inner[0].index, 0);
		assert_eq!(inner[0].instructions[0].data, "kept");
	}

	#[test]
	fn test_parse_single_transaction_missing_program_id_index_defaults_to_zero() {
		// The `jsonParsed` encoding omits `programIdIndex` — the real
		// identifier lives in the `programId` string. Missing must still
		// default to 0 (not be treated as an out-of-range drop).
		let client = mock_client();
		let raw_tx = json!({
			"transaction": {
				"signatures": ["sig_parsed_no_pid"],
				"message": {
					"accountKeys": ["Account1"],
					"instructions": [{
						"accounts": [0],
						"data": "d",
						"program": "system",
						"programId": "11111111111111111111111111111111"
					}],
					"recentBlockhash": "hash1"
				}
			},
			"meta": {
				"err": null,
				"fee": 0,
				"preBalances": [0],
				"postBalances": [0],
				"logMessages": []
			}
		});

		let tx = client
			.parse_single_transaction(1, &raw_tx)
			.unwrap()
			.expect("should parse transaction");

		assert_eq!(tx.transaction.instructions.len(), 1);
		let ix = &tx.transaction.instructions[0];
		assert_eq!(ix.program_id_index, 0);
		assert_eq!(
			ix.program_id,
			Some("11111111111111111111111111111111".to_string())
		);
	}

	#[tokio::test]
	async fn test_fetch_stream_kind_derives() {
		// Verify derived traits work correctly
		let dense = FetchStreamKind::Dense;
		let sparse = FetchStreamKind::Sparse;

		// PartialEq / Eq
		assert_eq!(dense, FetchStreamKind::Dense);
		assert_ne!(dense, sparse);

		// Clone
		let dense_clone = dense.clone();
		assert_eq!(dense, dense_clone);

		// Debug
		let debug_str = format!("{:?}", sparse);
		assert_eq!(debug_str, "Sparse");
	}

	#[tokio::test]
	async fn test_block_fetch_result_clone() {
		let result = BlockFetchResult {
			blocks: vec![],
			failed_blocks: vec![1, 2, 3],
			stream_kind: FetchStreamKind::Sparse,
		};
		let cloned = result.clone();
		assert_eq!(cloned.failed_blocks, vec![1, 2, 3]);
		assert_eq!(cloned.stream_kind, FetchStreamKind::Sparse);
		assert!(cloned.blocks.is_empty());
	}
}
