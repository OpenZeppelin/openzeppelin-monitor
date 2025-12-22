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
	#[allow(dead_code)]
	pub const GET_SLOT: &str = "getSlot";
	pub const GET_BLOCK: &str = "getBlock";
	pub const GET_BLOCKS: &str = "getBlocks";
	#[allow(dead_code)]
	pub const GET_TRANSACTION: &str = "getTransaction";
	pub const GET_ACCOUNT_INFO: &str = "getAccountInfo";
	pub const GET_PROGRAM_ACCOUNTS: &str = "getProgramAccounts";
	pub const GET_SIGNATURES_FOR_ADDRESS: &str = "getSignaturesForAddress";
}

/// Client implementation for the Solana blockchain
///
/// Provides high-level access to Solana blockchain data and operations through HTTP transport.
#[derive(Clone)]
pub struct SolanaClient<T: Send + Sync + Clone> {
	/// The underlying Solana transport client for RPC communication
	http_client: T,
}

impl<T: Send + Sync + Clone> SolanaClient<T> {
	/// Creates a new Solana client instance with a specific transport client
	pub fn new_with_transport(http_client: T) -> Self {
		Self { http_client }
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

	/// Retrieves signatures for an address
	async fn get_signatures_for_address(
		&self,
		address: &str,
		limit: Option<usize>,
	) -> Result<Vec<String>, anyhow::Error>;

	/// Retrieves account info for a given public key
	async fn get_account_info(&self, pubkey: &str) -> Result<serde_json::Value, anyhow::Error>;

	/// Retrieves program accounts for a given program ID
	async fn get_program_accounts(
		&self,
		program_id: &str,
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

	#[instrument(skip(self), fields(address, limit))]
	async fn get_signatures_for_address(
		&self,
		address: &str,
		limit: Option<usize>,
	) -> Result<Vec<String>, anyhow::Error> {
		let config = json!({
			"commitment": "finalized",
			"limit": limit.unwrap_or(100)
		});
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

		let signatures: Vec<String> = result
			.iter()
			.filter_map(|item| {
				item.get("signature")
					.and_then(|s| s.as_str())
					.map(|s| s.to_string())
			})
			.collect();

		Ok(signatures)
	}

	#[instrument(skip(self), fields(pubkey))]
	async fn get_account_info(&self, pubkey: &str) -> Result<serde_json::Value, anyhow::Error> {
		let config = json!({
			"encoding": "jsonParsed",
			"commitment": "finalized"
		});
		let params = json!([pubkey, config]);

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
		program_id: &str,
	) -> Result<Vec<serde_json::Value>, anyhow::Error> {
		let config = json!({
			"encoding": "jsonParsed",
			"commitment": "finalized"
		});
		let params = json!([program_id, config]);

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
