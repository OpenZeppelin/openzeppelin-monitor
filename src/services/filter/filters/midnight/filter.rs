//! Midnight blockchain filter implementation.
//!
//! This module provides filtering capabilities for Midnight blockchain. It handles:
//! - Transaction matching based on conditions
//! - Function call detection

#![allow(clippy::result_large_err)]

use async_trait::async_trait;
use midnight_ledger::structure::Proof;
use midnight_node_ledger_helpers::NetworkId;
use std::{collections::VecDeque, marker::PhantomData};
use tracing::instrument;

use crate::{
	models::{
		BlockType, ContractSpec, EventCondition, FunctionCondition, MidnightBlock,
		MidnightMatchArguments, MidnightMatchParamEntry, MidnightRpcTransactionEnum,
		MidnightTransaction, Monitor, MonitorMatch, Network, TransactionCondition,
		TransactionStatus,
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
		_transaction: &MidnightTransaction,
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
		_monitored_addresses: &[String],
		_transaction: &MidnightTransaction,
		_monitor: &Monitor,
		_matched_functions: &mut [FunctionCondition],
		_matched_on_args: &mut MidnightMatchArguments,
	) {
	}

	/// Finds events in a transaction that match the monitor's conditions.
	///
	/// Processes event logs from the transaction and matches them against
	/// the monitor's event conditions.
	///
	/// # Arguments
	/// * `monitor` - Monitor containing event match conditions
	/// * `matched_events` - Vector to store matching events
	/// * `matched_on_args` - Arguments from matched events
	/// * `involved_addresses` - Addresses involved in matched events
	pub async fn find_matching_events_for_transaction(
		&self,
		_monitor: &Monitor,
		_matched_events: &mut [EventCondition],
		_matched_on_args: &mut MidnightMatchArguments,
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
		_args: &Option<Vec<MidnightMatchParamEntry>>,
	) -> bool {
		false
	}

	pub fn deserialize_transactions(
		&self,
		block: &MidnightBlock,
		network_id: NetworkId,
	) -> Result<Vec<MidnightTransaction>, FilterError> {
		let mut txs = Vec::<MidnightTransaction>::new();
		let tx_index = block.transactions_index.iter().rev();
		for (hash, body) in tx_index {
			let (_hash, tx) = match parse_tx_index_item::<Proof>(hash, body, network_id) {
				Ok(res) => res,
				Err(e) => {
					return Err(FilterError::network_error(
						"Error deserializing transaction",
						Some(e.into()),
						None,
					));
				}
			};
			txs.push(MidnightTransaction::from(tx));
		}
		Ok(txs)
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
	/// * `contract_specs` - Optional contract specs for decoding events
	///
	/// # Returns
	/// Vector of matches found in the block
	#[instrument(skip_all, fields(network = %network.slug))]
	async fn filter_block(
		&self,
		client: &T,
		network: &Network,
		block: &BlockType,
		monitors: &[Monitor],
		_contract_specs: Option<&[(String, ContractSpec)]>,
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

		let chain_type = client.get_chain_type().await?;
		let network_id = map_chain_type(&chain_type);
		let _decoded_transactions = self.deserialize_transactions(midnight_block, network_id)?;

		tracing::debug!("Processing block {}", midnight_block.number().unwrap_or(0));

		// 1. Get transactions from the block
		// 2. Decode transactions using Transactions::deserialize from midnight-node
		// 3. Find matching transactions for each monitor (transactions and functions). Excluding events since they are not supported yet.
		// 4. Return matches

		let transactions: VecDeque<_> = midnight_block
			.body
			.iter()
			.filter_map(|entry| match entry {
				MidnightRpcTransactionEnum::MidnightTransaction { tx, .. } => {
					Some(MidnightTransaction::from(tx.clone()))
				}
				_ => None,
			})
			.collect();

		if transactions.is_empty() {
			tracing::debug!(
				"No transactions found for block {}",
				midnight_block.number().unwrap_or(0)
			);
			return Ok(vec![]);
		}

		let _matching_results = Vec::<MonitorMatch>::new();
		tracing::debug!("Processing {} monitor(s)", monitors.len());

		for monitor in monitors {
			tracing::debug!("Processing monitor: {:?}", monitor.name);
			let _monitored_addresses: Vec<String> = monitor
				.addresses
				.iter()
				.map(|a| a.address.clone())
				.collect();

			for transaction in transactions.iter() {
				let _matched_transactions = Vec::<TransactionCondition>::new();
				let _matched_functions = Vec::<FunctionCondition>::new();
				let _matched_events = Vec::<EventCondition>::new();
				let _matched_on_args = MidnightMatchArguments {
					events: Some(Vec::new()),
					functions: Some(Vec::new()),
				};

				tracing::debug!("Processing transaction: {:?}", transaction.hash());

				// self.find_matching_transaction(transaction, monitor, &mut matched_transactions);

				// self.find_matching_functions_for_transaction(
				// 	&monitored_addresses,
				// 	transaction,
				// 	monitor,
				// 	&mut matched_functions,
				// 	&mut matched_on_args,
				// );
			}
		}

		Ok(vec![])
	}
}
