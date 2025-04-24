//! Midnight blockchain filter implementation.
//!
//! This module provides filtering capabilities for Midnight blockchain. It handles:
//! - Transaction matching based on conditions
//! - Function call detection

use async_trait::async_trait;
use serde_json::Value;
use std::{collections::VecDeque, marker::PhantomData};
use tracing::instrument;

use crate::{
	models::{
		BlockType, EVMMatchArguments, EVMMatchParamEntry, EVMMatchParamsMap, EVMReceiptLog,
		EVMTransaction, EVMTransactionReceipt, EventCondition, FunctionCondition, Monitor,
		MonitorMatch, Network, TransactionCondition, TransactionStatus,
	},
	services::{
		blockchain::{BlockChainClient, MidnightClientTrait},
		filter::{
			filters::midnight::helpers::{map_chain_type, parse_tx_index_item},
			BlockFilter, FilterError,
		},
	},
};

/// Filter implementation for Midnight blockchain
pub struct MidnightBlockFilter<T> {
	pub _client: PhantomData<T>,
}

impl<T> MidnightBlockFilter<T> {
	/// Finds transactions that match the monitor's conditions.
	///
	/// # Arguments
	/// * `tx_status` - Status of the transaction (success/failure)
	/// * `transaction` - The transaction to check
	/// * `monitor` - Monitor containing match conditions
	/// * `matched_transactions` - Vector to store matching transactions
	pub fn find_matching_transaction(
		&self,
		_tx_status: &TransactionStatus,
		_transaction: &EVMTransaction,
		_monitor: &Monitor,
		_matched_transactions: &mut [TransactionCondition],
	) {
	}

	/// Finds function calls in a transaction that match the monitor's conditions.
	///
	/// Decodes the transaction input data using the contract ABI and matches against
	/// the monitor's function conditions.
	///
	/// # Arguments
	/// * `transaction` - The transaction containing the function call
	/// * `monitor` - Monitor containing function match conditions
	/// * `matched_functions` - Vector to store matching functions
	/// * `matched_on_args` - Arguments from matched function calls
	pub fn find_matching_functions_for_transaction(
		&self,
		_transaction: &EVMTransaction,
		_monitor: &Monitor,
		_matched_functions: &mut [FunctionCondition],
		_matched_on_args: &mut EVMMatchArguments,
	) {
	}

	/// Finds events in a transaction receipt that match the monitor's conditions.
	///
	/// Processes event logs from the transaction receipt and matches them against
	/// the monitor's event conditions.
	///
	/// # Arguments
	/// * `receipt` - Transaction receipt containing event logs
	/// * `monitor` - Monitor containing event match conditions
	/// * `matched_events` - Vector to store matching events
	/// * `matched_on_args` - Arguments from matched events
	/// * `involved_addresses` - Addresses involved in matched events
	pub async fn find_matching_events_for_transaction(
		&self,
		_receipt: &EVMTransactionReceipt,
		_monitor: &Monitor,
		_matched_events: &mut [EventCondition],
		_matched_on_args: &mut EVMMatchArguments,
		_involved_addresses: &mut [String],
	) {
	}

	/// Evaluates a match expression against provided parameters.
	///
	/// # Arguments
	/// * `expression` - The expression to evaluate
	/// * `args` - Optional parameters to use in evaluation
	///
	/// # Returns
	/// `true` if the expression matches, `false` otherwise
	pub fn evaluate_expression(
		&self,
		_expression: &str,
		_args: &Option<Vec<EVMMatchParamEntry>>,
	) -> bool {
		false
	}

	/// Decodes event logs using the provided ABI.
	///
	/// # Arguments
	/// * `abi` - Contract ABI for decoding
	/// * `log` - Event log to decode
	///
	/// # Returns
	/// Option containing EVMMatchParamsMap with decoded event data if successful
	pub async fn decode_events(
		&self,
		_abi: &Value,
		_log: &EVMReceiptLog,
	) -> Option<EVMMatchParamsMap> {
		None
	}
}

#[async_trait]
impl<T: BlockChainClient + MidnightClientTrait> BlockFilter for MidnightBlockFilter<T> {
	type Client = T;
	/// Processes a block and finds matches based on monitor conditions.
	///
	/// # Arguments
	/// * `client` - Blockchain client for additional data fetching
	/// * `network` - Network of the blockchain
	/// * `block` - The block to process
	/// * `monitors` - Active monitors containing match conditions
	///
	/// # Returns
	/// Vector of matches found in the block
	#[instrument(skip_all, fields(network = %_network.slug))]
	async fn filter_block(
		&self,
		client: &T,
		_network: &Network,
		block: &BlockType,
		_monitors: &[Monitor],
	) -> Result<Vec<MonitorMatch>, FilterError> {
		let midnight_block = match block {
			BlockType::Midnight(block) => block,
			_ => {
				return Err(FilterError::block_type_mismatch(
					"Expected Midnight block",
					None,
					None,
				))
			}
		};

		tracing::debug!("Processing block {}", midnight_block.number().unwrap_or(0));

		// 1. Get transactions from the block
		// 2. Decode transactions using Transactions::deserialize from midnight-node
		// 3. Find matching transactions for each monitor (transactions and functions). Excluding events since they are not supported yet.
		// 4. Return matches

		let transactions = midnight_block.transactions_index.clone();
		let mut txs = VecDeque::new();

		// Get chain type and map to network id
		let chain_type = client.get_chain_type().await?;
		let network_id = map_chain_type(&chain_type);

		// TransactionIndex includes only successful midnight transactions
		// See: midnightrpc.rs in node crate
		// We reverse here to create a list of youngest to oldest txs, then push front
		for (hash, body) in transactions.into_iter().rev() {
			let (hash, tx) = match parse_tx_index_item::<()>(&hash, &body, network_id) {
				Ok(res) => res,
				Err(e) => {
					println!("error at {}", midnight_block.number().unwrap_or(0));
					println!("attempted to decode tx {}", &hash);
					println!("{:?}", e);
					return Err(FilterError::internal_error(
						"Failed to parse transaction",
						Some(e.into()),
						None,
					));
				}
			};
			// Push transaction and hash to txs
			txs.push_front((hash, tx));
		}

		println!("transaction: {:?}", txs);

		Ok(vec![])
	}
}
