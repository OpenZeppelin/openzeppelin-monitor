//! Midnight blockchain filter implementation.
//!
//! This module provides filtering capabilities for Midnight blockchain. It handles:
//! - Transaction matching based on conditions
//! - Function call detection

#![allow(clippy::result_large_err)]

use async_trait::async_trait;
use midnight_ledger::structure::Proof;
use midnight_node_ledger_helpers::NetworkId;
use std::marker::PhantomData;
use tracing::instrument;

use crate::{
	models::{
		BlockType, ChainConfiguration, ContractSpec, EventCondition, FunctionCondition,
		MatchConditions, MidnightBlock, MidnightEvent, MidnightMatchArguments,
		MidnightMatchParamEntry, MidnightMatchParamsMap, MidnightMonitorMatch,
		MidnightRpcTransactionEnum, MidnightTransaction, Monitor, MonitorMatch, Network,
		TransactionCondition, TransactionStatus,
	},
	services::{
		blockchain::{BlockChainClient, MidnightClientTrait},
		filter::{
			filters::midnight::helpers::{map_chain_type, parse_tx_index_item},
			midnight_helpers::{
				are_same_address, are_same_hash, are_same_signature, normalize_hash,
				remove_parentheses,
			},
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
	/// * `events` - Events from the block
	/// * `transaction` - The transaction to check
	/// * `monitor` - Monitor containing match conditions
	/// * `matched_transactions` - Vector to store matching transactions
	pub fn find_matching_transaction(
		&self,
		events: &[MidnightEvent],
		transaction: &MidnightTransaction,
		monitor: &Monitor,
		matched_transactions: &mut Vec<TransactionCondition>,
	) {
		if monitor.match_conditions.transactions.is_empty() {
			// Match all transactions
			matched_transactions.push(TransactionCondition {
				expression: None,
				status: TransactionStatus::Any,
			});
		} else {
			let tx_status = events
				.iter()
				.find(|event| {
					event.is_success()
						&& are_same_hash(
							event.get_tx_hash().as_deref().unwrap_or_default(),
							transaction.hash(),
						)
				})
				.map(|_| TransactionStatus::Success)
				.unwrap_or(TransactionStatus::Failure);

			// Check each transaction condition
			for condition in &monitor.match_conditions.transactions {
				// First check if status matches (if specified)
				let status_matches = match &condition.status {
					TransactionStatus::Any => true,
					required_status => *required_status == tx_status,
				};

				if status_matches {
					matched_transactions.push(TransactionCondition {
						expression: None,
						status: tx_status,
					});
					break;
				}
			}
		}
	}

	/// Finds function calls in a transaction that match the monitor's conditions.
	///
	/// # Arguments
	/// * `monitored_addresses` - Addresses to monitor
	/// * `transaction` - The transaction containing the function call
	/// * `monitor` - Monitor containing function match conditions
	/// * `matched_functions` - Vector to store matching functions
	/// * `matched_on_args` - Arguments from matched function calls
	pub fn find_matching_functions_for_transaction(
		&self,
		monitored_addresses: &[String],
		transaction: &MidnightTransaction,
		monitor: &Monitor,
		matched_functions: &mut Vec<FunctionCondition>,
		matched_on_args: &mut MidnightMatchArguments,
	) {
		if !monitor.match_conditions.functions.is_empty() {
			let addresses_with_entry_points = transaction.contract_addresses_and_entry_points();

			// Iterate over each function condition in the monitor
			for condition in &monitor.match_conditions.functions {
				// For each function condition, check if there's a matching address and entry point
				if let Some((_, entry_point)) =
					addresses_with_entry_points.iter().find(|(addr, entry)| {
						// Check if the address matches any monitored address
						monitored_addresses.iter().any(|monitored_addr| are_same_address(addr, monitored_addr)) &&
					// Check if the entry point matches the function signature
					are_same_signature(entry, &condition.signature)
					}) {
					let normalized_signature = remove_parentheses(&condition.signature);
					// If we found a match, add it to the matched functions
					matched_functions.push(FunctionCondition {
						signature: normalized_signature.clone(),
						expression: condition.expression.clone(),
					});

					// Add the matched arguments if we have any
					if let Some(functions) = &mut matched_on_args.functions {
						functions.push(MidnightMatchParamsMap {
							signature: normalized_signature.clone(),
							args: None,                               // Arguments are private in Midnight
							hex_signature: Some(entry_point.clone()), // entry_point isalready in hex format
						});
					}
				}
			}
		}
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

	/// Deserializes transactions from a block.
	///
	/// # Arguments
	/// * `block` - The block to deserialize transactions from
	/// * `network_id` - The network ID
	/// * `chain_configurations` - The chain configurations
	///
	/// # Returns
	/// A vector of deserialized transactions
	pub fn deserialize_transactions(
		&self,
		block: &MidnightBlock,
		network_id: NetworkId,
		chain_configurations: &Vec<ChainConfiguration>,
	) -> Result<Vec<MidnightTransaction>, FilterError> {
		let mut txs = Vec::<MidnightTransaction>::new();

		// There are two ways to deserialize transactions from a Midnight block:

		// 1. Using block.body: Contains pre-deserialized transactions as `MidnightRpcTransaction`
		//    - Simpler structure with basic transaction info (hash, operations, identifiers)
		//    - Easier to work with for basic monitoring

		// 2. Using block.transactions_index: Contains raw transaction data that can be deserialized into
		//    ledger-specific transactions `MidnightNodeTransaction` (Standard or ClaimMint)
		//    - Provides access to additional fields like guaranteed_transcript and fallible_transcript
		//    - Enables decryption of ZswapOffers and other private data
		//    - More complex but offers richer transaction data
		//
		// Current implementation uses approach #1 since we don't need the additional data yet.
		// This may change in the future if we need to monitor private transaction details.
		// We are parsing the raw data with `parse_tx_index_item` for each transaction in the block body
		// and then converting it to a MidnightTransaction with `try_from`
		// This will allow us to populate the transaction with the additional data from the transactions_index later.
		for transaction in block.body.iter() {
			if let MidnightRpcTransactionEnum::MidnightTransaction { tx, .. } = transaction {
				let hash = normalize_hash(&tx.tx_hash);
				let raw_tx_data = block
					.transactions_index
					.iter()
					.find(|(h, _)| normalize_hash(h) == hash)
					.map(|(_, raw_tx_data)| raw_tx_data.clone())
					.unwrap_or_default();

				let transaction = MidnightTransaction::from(tx.clone());
				let (_hash, deserialized_ledger_transaction) =
					match parse_tx_index_item::<Proof>(&hash, &raw_tx_data, network_id) {
						Ok(res) => res,
						Err(e) => {
							return Err(FilterError::network_error(
								"Error deserializing transaction",
								Some(e.into()),
								None,
							));
						}
					};
				txs.push(MidnightTransaction::try_from((
					transaction,
					deserialized_ledger_transaction,
					chain_configurations,
				))?);
			}
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

		let events = client
			.get_events(midnight_block.number().unwrap_or(0), None)
			.await?;

		// Get all chain configurations from monitors
		let chain_configurations = monitors
			.iter()
			.flat_map(|m| m.chain_configurations.clone())
			.collect();

		let transactions =
			self.deserialize_transactions(midnight_block, network_id, &chain_configurations)?;

		if transactions.is_empty() {
			tracing::debug!(
				"No transactions found for block {}",
				midnight_block.number().unwrap_or(0)
			);
			return Ok(vec![]);
		}

		tracing::debug!("Processing block {}", midnight_block.number().unwrap_or(0));
		tracing::debug!("Processing {} monitor(s)", monitors.len());

		let mut matching_results = Vec::<MonitorMatch>::new();

		for monitor in monitors {
			tracing::debug!("Processing monitor: {:?}", monitor.name);
			let monitored_addresses: Vec<String> = monitor
				.addresses
				.iter()
				.map(|a| a.address.clone())
				.collect();

			for transaction in transactions.iter() {
				let mut matched_transactions = Vec::<TransactionCondition>::new();
				let mut matched_functions = Vec::<FunctionCondition>::new();
				let matched_events = Vec::<EventCondition>::new();
				let mut matched_on_args = MidnightMatchArguments {
					events: Some(Vec::new()),
					functions: Some(Vec::new()),
				};

				tracing::debug!("Processing transaction: {:?}", transaction.hash());

				self.find_matching_transaction(
					&events,
					transaction,
					monitor,
					&mut matched_transactions,
				);

				self.find_matching_functions_for_transaction(
					&monitored_addresses,
					transaction,
					monitor,
					&mut matched_functions,
					&mut matched_on_args,
				);

				let monitor_conditions = &monitor.match_conditions;
				let has_event_match =
					!monitor_conditions.events.is_empty() && !matched_events.is_empty();
				let has_function_match =
					!monitor_conditions.functions.is_empty() && !matched_functions.is_empty();
				let has_transaction_match =
					!monitor_conditions.transactions.is_empty() && !matched_transactions.is_empty();

				let should_match = match (
					monitor_conditions.events.is_empty(),
					monitor_conditions.functions.is_empty(),
					monitor_conditions.transactions.is_empty(),
				) {
					// Case 1: No conditions defined, match everything
					(true, true, true) => true,

					// Case 2: Only transaction conditions defined
					(true, true, false) => has_transaction_match,

					// Case 3: No transaction conditions, match based on events/functions
					(_, _, true) => has_event_match || has_function_match,

					// Case 4: Transaction conditions exist, they must be satisfied along with
					// events/functions
					_ => (has_event_match || has_function_match) && has_transaction_match,
				};

				if should_match {
					matching_results.push(MonitorMatch::Midnight(Box::new(MidnightMonitorMatch {
						monitor: monitor.clone(),
						transaction: transaction.clone(),
						network_slug: network.slug.clone(),
						matched_on: MatchConditions {
							events: matched_events
								.clone()
								.into_iter()
								.filter(|_| has_event_match)
								.collect(),
							functions: matched_functions
								.clone()
								.into_iter()
								.filter(|_| has_function_match)
								.collect(),
							transactions: matched_transactions
								.clone()
								.into_iter()
								.filter(|_| has_transaction_match)
								.collect(),
						},
						matched_on_args: Some(MidnightMatchArguments {
							events: if has_event_match {
								matched_on_args.events.clone()
							} else {
								None
							},
							functions: if has_function_match {
								matched_on_args.functions.clone()
							} else {
								None
							},
						}),
					})));
				}
			}
		}
		Ok(matching_results)
	}
}
