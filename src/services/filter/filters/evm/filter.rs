//! EVM blockchain filter implementation.
//!
//! This module provides filtering capabilities for Ethereum Virtual Machine (EVM) compatible
//! blockchains. It handles:
//! - Transaction matching based on conditions
//! - Function call detection and parameter matching
//! - Event log processing and filtering
//! - ABI-based decoding of function calls and events

use alloy::primitives::U64;
use anyhow::Context;
use async_trait::async_trait;
use ethabi::Contract;
use std::marker::PhantomData;
use tracing::instrument;

use crate::{
	models::{
		AddressWithSpec, BlockType, ContractSpec, EVMContractSpec, EVMMatchArguments,
		EVMMatchParamEntry, EVMMatchParamsMap, EVMMonitorMatch, EVMReceiptLog, EVMTransaction,
		EVMTransactionReceipt, EventCondition, FunctionCondition, MatchConditions, Monitor,
		MonitorMatch, Network, TransactionCondition, TransactionStatus,
	},
	services::{
		blockchain::{BlockChainClient, EvmClientTrait},
		filter::{
			evm_helpers::{
				are_same_address, are_same_signature, b256_to_string, format_token_value,
				h160_to_string, h256_to_string, normalize_address,
			},
			expression::{self, EvaluationError},
			filters::evm::evaluator::EVMConditionEvaluator,
			BlockFilter, FilterError,
		},
	},
};

/// Filter implementation for EVM-compatible blockchains
pub struct EVMBlockFilter<T> {
	pub _client: PhantomData<T>,
}

impl<T> EVMBlockFilter<T> {
	/// Finds transactions that match the monitor's conditions.
	///
	/// # Arguments
	/// * `tx_status` - Status of the transaction (success/failure)
	/// * `transaction` - The transaction to check
	/// * `tx_receipt` - Transaction receipt
	/// * `monitor` - Monitor containing match conditions
	/// * `matched_transactions` - Vector to store matching transactions
	pub fn find_matching_transaction(
		&self,
		tx_status: &TransactionStatus,
		transaction: &EVMTransaction,
		tx_receipt: &Option<EVMTransactionReceipt>,
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
			// Check each transaction condition
			for condition in &monitor.match_conditions.transactions {
				// First check if status matches (if specified)
				let status_matches = match &condition.status {
					TransactionStatus::Any => true,
					required_status => *required_status == *tx_status,
				};

				if status_matches {
					if let Some(expr) = &condition.expression {
						let tx_params = vec![
							EVMMatchParamEntry {
								name: "value".to_string(),
								value: transaction.value.to_string(),
								kind: "uint256".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "from".to_string(),
								value: transaction.from.map_or("".to_string(), h160_to_string),
								kind: "address".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "to".to_string(),
								value: transaction.to.map_or("".to_string(), h160_to_string),
								kind: "address".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "hash".to_string(),
								value: b256_to_string(transaction.hash),
								kind: "string".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "gas_price".to_string(),
								value: transaction.gas_price.unwrap_or_default().to_string(),
								kind: "uint256".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "max_fee_per_gas".to_string(),
								value: transaction.max_fee_per_gas.unwrap_or_default().to_string(),
								kind: "uint256".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "max_priority_fee_per_gas".to_string(),
								value: transaction
									.max_priority_fee_per_gas
									.unwrap_or_default()
									.to_string(),
								kind: "uint256".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "gas_limit".to_string(),
								value: transaction.gas.to_string(),
								kind: "uint256".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "nonce".to_string(),
								value: transaction.nonce.to_string(),
								kind: "uint256".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "input".to_string(),
								value: format!("0x{}", hex::encode(&transaction.input)),
								kind: "string".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "gas_used".to_string(),
								value: tx_receipt
									.as_ref()
									.map(|r| r.gas_used.unwrap_or_default().to_string())
									.unwrap_or_default(),
								kind: "uint256".to_string(),
								indexed: false,
							},
							EVMMatchParamEntry {
								name: "transaction_index".to_string(),
								value: transaction
									.transaction_index
									.map_or("0".to_string(), |idx| idx.0.to_string()),
								kind: "uint64".to_string(),
								indexed: false,
							},
						];

						// Evaluate the expression with transaction parameters
						match self.evaluate_expression(expr, &tx_params) {
							Ok(true) => {
								matched_transactions.push(TransactionCondition {
									expression: Some(expr.to_string()),
									status: *tx_status,
								});
								break;
							}
							Ok(false) => continue,
							Err(e) => {
								tracing::error!("Failed to evaluate expression '{}': {}", expr, e);
								continue;
							}
						}
					} else {
						// No expression but status matched
						matched_transactions.push(TransactionCondition {
							expression: None,
							status: *tx_status,
						});
						break;
					}
				}
			}
		}
	}

	/// Finds function calls in a transaction that match the monitor's conditions.
	///
	/// Decodes the transaction input data using the contract ABI and matches against
	/// the monitor's function conditions.
	///
	/// # Arguments
	/// * `contract_specs` - List of contract specifications
	/// * `transaction` - The transaction containing the function call
	/// * `monitor` - Monitor containing function match conditions
	/// * `matched_functions` - Vector to store matching functions
	/// * `matched_on_args` - Arguments from matched function calls
	pub fn find_matching_functions_for_transaction(
		&self,
		contract_specs: &[(String, EVMContractSpec)],
		transaction: &EVMTransaction,
		monitor: &Monitor,
		matched_functions: &mut Vec<FunctionCondition>,
		matched_on_args: &mut EVMMatchArguments,
	) {
		if !monitor.match_conditions.functions.is_empty() {
			// Try to decode the function call if there's input data
			let input_data = &transaction.input;
			// Find the matching monitored address for the transaction
			if let Some(monitored_addr) = monitor.addresses.iter().find(|addr| {
				transaction
					.to
					.is_some_and(|to| are_same_address(&addr.address, &h160_to_string(to)))
			}) {
				// Process the matching address's ABI
				if let Some((_, abi)) = contract_specs
					.iter()
					.find(|(address, _)| are_same_address(address, &monitored_addr.address))
				{
					// Create contract object from ABI
					let contract = match Contract::load(abi.to_string().as_bytes()) {
						Ok(c) => c,
						Err(e) => {
							FilterError::internal_error(
								format!("Failed to parse ABI: {}", e),
								None,
								None,
							);
							return;
						}
					};

					// Get the function selector (first 4 bytes of input data)
					if input_data.0.len() >= 4 {
						let selector = &input_data.0[..4];

						// Try to find matching function in ABI
						if let Some(function) = contract
							.functions()
							.find(|f| f.short_signature().as_slice() == selector)
						{
							let function_signature_with_params = format!(
								"{}({})",
								function.name,
								function
									.inputs
									.iter()
									.map(|p| p.kind.to_string())
									.collect::<Vec<String>>()
									.join(",")
							);

							// Check each function condition
							for condition in &monitor.match_conditions.functions {
								if are_same_signature(
									&condition.signature,
									&function_signature_with_params,
								) {
									let decoded = function
										.decode_input(&input_data.0[4..])
										.unwrap_or_else(|e| {
											FilterError::internal_error(
												format!("Failed to decode function input: {}", e),
												None,
												None,
											);
											vec![]
										});

									let params: Vec<EVMMatchParamEntry> = function
										.inputs
										.iter()
										.zip(decoded.iter())
										.map(|(input, value)| EVMMatchParamEntry {
											name: input.name.clone(),
											value: format_token_value(value),
											kind: input.kind.to_string(),
											indexed: false,
										})
										.collect();
									if let Some(expr) = &condition.expression {
										// Evaluate the expression condition
										match self.evaluate_expression(expr, &params) {
											Ok(true) => {
												matched_functions.push(FunctionCondition {
													signature: function_signature_with_params
														.clone(),
													expression: Some(expr.to_string()),
												});
												if let Some(functions) =
													&mut matched_on_args.functions
												{
													functions.push(EVMMatchParamsMap {
														signature: function_signature_with_params
															.clone(),
														args: Some(params.clone()),
														hex_signature: Some(format!(
															"0x{}",
															hex::encode(function.short_signature())
														)),
													});
												}
												break;
											}
											Ok(false) => continue,
											Err(e) => {
												tracing::error!(
													"Failed to evaluate expression '{}': {}",
													expr,
													e
												);
												continue;
											}
										}
									} else {
										// No expression, just match on function name
										matched_functions.push(FunctionCondition {
											signature: function_signature_with_params.clone(),
											expression: None,
										});
										if let Some(functions) = &mut matched_on_args.functions {
											functions.push(EVMMatchParamsMap {
												signature: function_signature_with_params.clone(),
												args: Some(params.clone()),
												hex_signature: Some(hex::encode(
													function.short_signature(),
												)),
											});
										}
										break;
									}
								}
							}
						}
					}
				}
			}
		}
	}

	/// Finds events in a transaction receipt that match the monitor's conditions.
	///
	/// Processes event logs from the transaction receipt and matches them against
	/// the monitor's event conditions.
	///
	/// # Arguments
	/// * `logs` - Transaction receipt containing event logs
	/// * `monitor` - Monitor containing event match conditions
	/// * `matched_events` - Vector to store matching events
	/// * `matched_on_args` - Arguments from matched events
	/// * `involved_addresses` - Addresses involved in matched events
	pub fn find_matching_events_for_transaction(
		&self,
		logs: &[EVMReceiptLog],
		monitor: &Monitor,
		matched_events: &mut Vec<EventCondition>,
		matched_on_args: &mut EVMMatchArguments,
		involved_addresses: &mut Vec<String>,
	) {
		for log in logs {
			// Find the specific monitored address that matches the log address
			let matching_monitored_addr = monitor
				.addresses
				.iter()
				.find(|addr| are_same_address(&addr.address, &h160_to_string(log.address)));

			// Only process logs from monitored addresses
			let Some(monitored_addr) = matching_monitored_addr else {
				continue;
			};

			// Add the contract address that emitted the event
			involved_addresses.push(h160_to_string(log.address));

			// Process the matching address's ABI
			if let Some(abi) = &monitored_addr.contract_spec {
				let decoded_log = self.decode_events(abi, log);

				if let Some(event_condition) = decoded_log {
					if monitor.match_conditions.events.is_empty() {
						// Match all events
						matched_events.push(EventCondition {
							signature: event_condition.signature.clone(),
							expression: None,
						});
						if let Some(events) = &mut matched_on_args.events {
							events.push(event_condition);
						}
					} else {
						// Check if this event matches any of the conditions
						for condition in &monitor.match_conditions.events {
							// Remove any whitespaces to ensure accurate matching
							// For example: Transfer(address, address, uint256) ==
							// Transfer(address,address,uint256)
							if are_same_signature(&condition.signature, &event_condition.signature)
							{
								if condition.expression.is_none() {
									matched_events.push(EventCondition {
										signature: event_condition.signature.clone(),
										expression: None,
									});
									if let Some(events) = &mut matched_on_args.events {
										events.push(event_condition);
									}
									break;
								} else {
									// Evaluate the expression condition
									if let Some(expr) = &condition.expression {
										match self.evaluate_expression(
											expr,
											event_condition.args.as_deref().unwrap_or_default(),
										) {
											Ok(true) => {
												matched_events.push(EventCondition {
													signature: event_condition.signature.clone(),
													expression: Some(expr.to_string()),
												});
												if let Some(events) = &mut matched_on_args.events {
													events.push(event_condition);
												}
												break;
											}
											Ok(false) => continue,
											Err(e) => {
												tracing::error!(
													"Failed to evaluate expression '{}': {}",
													expr,
													e
												);
												continue;
											}
										}
									}
								}
							}
						}
					}
				}
			}
		}
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
		expression: &str,
		args: &[EVMMatchParamEntry],
	) -> Result<bool, EvaluationError> {
		// Check if the expression is empty
		if expression.trim().is_empty() {
			tracing::error!("Empty expression provided for evaluation");
			return Err(EvaluationError::parse_error(
				"Expression cannot be empty".to_string(),
				None,
				None,
			));
		}

		let evaluator = EVMConditionEvaluator::new(args);

		// Parse the expression
		let parsed_ast = expression::parse(expression).map_err(|e| {
			tracing::error!("Failed to parse expression '{}': {}", expression, e);
			let msg = format!("Failed to parse expression '{}': {}", expression, e);
			EvaluationError::parse_error(msg, None, None)
		})?;
		tracing::debug!("Parsed AST for '{}': {:?}", expression, parsed_ast);

		// Evaluate the expression
		expression::evaluate(&parsed_ast, &evaluator)
	}

	/// Decodes event logs using the provided ABI.
	///
	/// # Arguments
	/// * `abi` - Contract ABI for decoding
	/// * `log` - Event log to decode
	///
	/// # Returns
	/// Option containing EVMMatchParamsMap with decoded event data if successful
	pub fn decode_events(
		&self,
		abi: &ContractSpec,
		log: &EVMReceiptLog,
	) -> Option<EVMMatchParamsMap> {
		// Create contract object from ABI
		let contract = match abi {
			ContractSpec::EVM(evm_spec) => Contract::load(evm_spec.to_string().as_bytes())
				.with_context(|| "Failed to parse ABI")
				.ok()?,
			_ => return None,
		};

		let decoded_log = contract
			.events()
			.find(|event| h256_to_string(event.signature()) == b256_to_string(log.topics[0]))
			.and_then(|event| {
				event
					.parse_log(ethabi::RawLog {
						topics: log
							.topics
							.iter()
							.map(|t| ethabi::Hash::from_slice(t.as_slice()))
							.collect(),
						data: log.data.0.to_vec(),
					})
					.ok()
					.map(|parsed| {
						let event_params_map = EVMMatchParamsMap {
							signature: format!(
								"{}({})",
								event.name,
								event
									.inputs
									.iter()
									.map(|p| p.kind.to_string())
									.collect::<Vec<String>>()
									.join(",")
							),
							args: Some(
								event
									.inputs
									.iter()
									.filter_map(|input| {
										parsed
											.params
											.iter()
											.find(|param| param.name == input.name)
											.map(|param| EVMMatchParamEntry {
												name: input.name.clone(),
												value: format_token_value(&param.value),
												kind: input.kind.to_string(),
												indexed: input.indexed,
											})
									})
									.collect(),
							),
							hex_signature: Some(h256_to_string(event.signature())),
						};
						event_params_map
					})
			});

		decoded_log
	}

	/// Checks if a monitor has any transaction conditions that require a receipt
	fn needs_receipt(&self, monitor: &Monitor, logs: &[EVMReceiptLog]) -> bool {
		monitor
			.match_conditions
			.transactions
			.iter()
			.any(|condition| {
				// If the status is not Any, and there are no logs, we need a receipt to validate the transaction most likely failed
				let status_needs_receipt =
					condition.status != TransactionStatus::Any && logs.is_empty();
				// If the expression contains gas_used, we need a receipt to get the gas used
				let gas_used_in_expr = condition
					.clone()
					.expression
					.is_some_and(|expr| expr.contains("gas_used"));
				status_needs_receipt || gas_used_in_expr
			})
	}
}

#[async_trait]
impl<T: BlockChainClient + EvmClientTrait> BlockFilter for EVMBlockFilter<T> {
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
	#[instrument(skip_all, fields(network = %network.slug))]
	async fn filter_block(
		&self,
		client: &T,
		network: &Network,
		block: &BlockType,
		monitors: &[Monitor],
		contract_specs: Option<&[(String, ContractSpec)]>,
	) -> Result<Vec<MonitorMatch>, FilterError> {
		let evm_block = match block {
			BlockType::EVM(block) => block,
			_ => {
				return Err(FilterError::block_type_mismatch(
					"Expected EVM block",
					None,
					None,
				));
			}
		};

		tracing::debug!(
			"Processing block {}",
			evm_block.number.unwrap_or(U64::from(0))
		);

		let current_block_number = evm_block.number.unwrap_or(U64::from(0)).to::<u64>();

		// Get logs for the block
		// We use this to get all the logs for a single block.
		// We could further optimize by getting logs for a range of blocks and calling this in the parent function
		// However, due to limitations by certain RPC providers (e.g. Quicknode only allows a block range of 5),
		// it's safer to just fetch the logs for a single block at a time as it's more reliable.
		let all_block_logs = client
			.get_logs_for_blocks(current_block_number, current_block_number, None)
			.await?;

		tracing::debug!(
			"Found {} logs for block {}",
			all_block_logs.len(),
			current_block_number
		);

		let mut matching_results = Vec::new();

		// Cast contract specs to EVMContractSpec
		let contract_specs = contract_specs
			.unwrap_or(&[])
			.iter()
			.filter_map(|(address, spec)| match spec {
				ContractSpec::EVM(spec) => Some((address.clone(), spec.clone())),
				_ => None,
			})
			.collect::<Vec<(String, EVMContractSpec)>>();

		// Group logs by transaction hash
		let mut logs_by_tx: std::collections::HashMap<String, Vec<EVMReceiptLog>> =
			std::collections::HashMap::new();
		for log in all_block_logs.clone() {
			let tx_hash = b256_to_string(log.transaction_hash.unwrap_or_default());
			logs_by_tx.entry(tx_hash).or_default().push(log);
		}

		tracing::debug!("Processing {} transactions with logs", logs_by_tx.len());

		for monitor in monitors {
			tracing::debug!("Processing monitor: {:?}", monitor.name);
			let monitored_addresses: Vec<String> = monitor
				.addresses
				.iter()
				.map(|a| a.address.clone())
				.collect();

			// Check if this monitor needs a receipt
			let should_fetch_receipt = self.needs_receipt(monitor, &all_block_logs);

			// Process all transactions in the block
			for transaction in &evm_block.transactions {
				let tx_hash = b256_to_string(transaction.hash);
				let empty_logs = Vec::new();
				let logs = logs_by_tx.get(&tx_hash).unwrap_or(&empty_logs);
				let tx_hash_str = tx_hash.clone();

				let receipt = if should_fetch_receipt {
					Some(client.get_transaction_receipt(tx_hash_str).await?)
				} else {
					None
				};

				// Reset matched_on_args for each transaction
				let mut matched_on_args = EVMMatchArguments {
					events: Some(Vec::new()),
					functions: Some(Vec::new()),
				};

				// Get transaction status from receipt
				let tx_status = if let Some(receipt) = receipt.clone() {
					if receipt.status.map(|s| s.to::<u64>() == 1).unwrap_or(false) {
						TransactionStatus::Success
					} else {
						TransactionStatus::Failure
					}
				} else {
					// Transaction receipt is only fetched when:
					// 1. The monitor has conditions requiring receipt data (e.g., gas_used)
					// 2. We need to verify transaction status and have no logs
					// Otherwise, we can assume success since failed transactions don't emit logs
					TransactionStatus::Success
				};

				// Collect all involved addresses from receipt logs, transaction.to, and transaction.from
				let mut involved_addresses = Vec::new();
				// Add transaction addresses
				if let Some(from) = transaction.from {
					involved_addresses.push(h160_to_string(from));
				}
				if let Some(to) = transaction.to {
					involved_addresses.push(h160_to_string(to));
				}

				let mut matched_events = Vec::<EventCondition>::new();
				let mut matched_transactions = Vec::<TransactionCondition>::new();
				let mut matched_functions = Vec::<FunctionCondition>::new();

				// Check transaction match conditions
				self.find_matching_transaction(
					&tx_status,
					transaction,
					&receipt.clone(),
					monitor,
					&mut matched_transactions,
				);

				// Check for event match conditions
				self.find_matching_events_for_transaction(
					logs,
					monitor,
					&mut matched_events,
					&mut matched_on_args,
					&mut involved_addresses,
				);

				// Check function match conditions
				self.find_matching_functions_for_transaction(
					&contract_specs,
					transaction,
					monitor,
					&mut matched_functions,
					&mut matched_on_args,
				);

				// Remove duplicates
				involved_addresses.sort_unstable();
				involved_addresses.dedup();

				let has_address_match = monitored_addresses.iter().any(|addr| {
					involved_addresses
						.iter()
						.map(|a| normalize_address(a))
						.collect::<Vec<String>>()
						.contains(&normalize_address(addr))
				});

				// Only proceed if we have a matching address
				if has_address_match {
					let monitor_conditions = &monitor.match_conditions;
					let has_event_match =
						!monitor_conditions.events.is_empty() && !matched_events.is_empty();
					let has_function_match =
						!monitor_conditions.functions.is_empty() && !matched_functions.is_empty();
					let has_transaction_match = !monitor_conditions.transactions.is_empty()
						&& !matched_transactions.is_empty();

					let should_match: bool = match (
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

						// Case 4: Transaction conditions exist, they must be satisfied along
						// with events/functions
						_ => (has_event_match || has_function_match) && has_transaction_match,
					};

					if should_match {
						matching_results.push(MonitorMatch::EVM(Box::new(EVMMonitorMatch {
							monitor: Monitor {
								// Omit ABI from monitor since we do not need it here
								addresses: monitor
									.addresses
									.iter()
									.map(|addr| AddressWithSpec {
										contract_spec: None,
										..addr.clone()
									})
									.collect(),
								..monitor.clone()
							},
							transaction: transaction.clone(),
							receipt,
							logs: Some(logs.clone()),
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
							matched_on_args: Some(EVMMatchArguments {
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
		}

		Ok(matching_results)
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		models::{ContractSpec, EVMContractSpec},
		utils::tests::evm::{
			monitor::MonitorBuilder, receipt::ReceiptBuilder, transaction::TransactionBuilder,
		},
	};

	use super::*;
	use alloy::primitives::{Address, Bytes, B256, U256};
	use ethabi::{Function, Param, ParamType};
	use serde_json::json;
	use std::str::FromStr;

	fn create_test_filter() -> EVMBlockFilter<()> {
		EVMBlockFilter::<()> {
			_client: PhantomData,
		}
	}

	/// Creates a test monitor with customizable parameters
	fn create_test_monitor(
		event_conditions: Vec<EventCondition>,
		function_conditions: Vec<FunctionCondition>,
		transaction_conditions: Vec<TransactionCondition>,
		addresses: Vec<AddressWithSpec>,
	) -> Monitor {
		MonitorBuilder::new()
			.name("test")
			.networks(vec!["evm_mainnet".to_string()])
			.match_conditions(MatchConditions {
				events: event_conditions,
				functions: function_conditions,
				transactions: transaction_conditions,
			})
			.addresses_with_spec(
				addresses
					.into_iter()
					.map(|a| (a.address, a.contract_spec))
					.collect(),
			)
			.build()
	}

	fn create_test_abi(abi_type: &str) -> ContractSpec {
		let spec = match abi_type {
			"function" => json!([{
				"type": "function",
				"name": "transfer",
				"inputs": [
					{
						"name": "recipient",
						"type": "address",
						"internalType": "address"
					},
					{
						"name": "amount",
						"type": "uint256",
						"internalType": "uint256"
					}
				],
				"outputs": [
					{
						"name": "",
						"type": "bool",
						"internalType": "bool"
					}
				],
				"stateMutability": "nonpayable"
			}]),
			"event" => json!([{
				"type": "event",
				"name": "Transfer",
				"inputs": [
					{
						"name": "from",
						"type": "address",
						"indexed": true
					},
					{
						"name": "to",
						"type": "address",
						"indexed": true
					},
					{
						"name": "value",
						"type": "uint256",
						"indexed": false
					}
				],
				"anonymous": false,
			}]),
			_ => json!([]),
		};
		ContractSpec::EVM(EVMContractSpec::from(spec))
	}

	/// Creates a test address with ABI
	fn create_test_address(address: &str, spec: Option<ContractSpec>) -> AddressWithSpec {
		AddressWithSpec {
			address: address.to_string(),
			contract_spec: spec,
		}
	}

	fn create_test_log(
		contract_address: Address,
		event_signature: &str,
		from_address: Address,
		to_address: Address,
		value_hex: &str,
	) -> EVMReceiptLog {
		EVMReceiptLog {
			address: contract_address,
			topics: vec![
				B256::from_str(event_signature).unwrap(),
				B256::from_slice(&[&[0u8; 12], from_address.as_slice()].concat()),
				B256::from_slice(&[&[0u8; 12], to_address.as_slice()].concat()),
			],
			data: Bytes(hex::decode(value_hex).unwrap().into()),
			block_hash: None,
			block_number: None,
			transaction_hash: None,
			transaction_index: None,
			log_index: Some(U256::from(0)),
			transaction_log_index: Some(U256::from(0)),
			log_type: None,
			removed: Some(false),
		}
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for find_matching_transaction method:
	//////////////////////////////////////////////////////////////////////////////
	#[test]
	fn test_empty_conditions_matches_all() {
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(vec![], vec![], vec![], vec![]);
		let receipt = ReceiptBuilder::new().build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&TransactionBuilder::new().build(),
			&Some(receipt),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].status, TransactionStatus::Any);
		assert!(matched[0].expression.is_none());
	}

	#[test]
	fn test_status_matching() {
		let filter = create_test_filter();
		let mut matched = Vec::new();

		let monitor = create_test_monitor(
			vec![], // events
			vec![], // functions
			vec![TransactionCondition {
				status: TransactionStatus::Success,
				expression: None,
			}], // transactions
			vec![], // addresses
		);

		let receipt_success = ReceiptBuilder::new().build();

		// Test successful transaction
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&TransactionBuilder::new().build(),
			&Some(receipt_success),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].status, TransactionStatus::Success);

		// Test failed transaction
		let receipt_failure = ReceiptBuilder::new().status(false).build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Failure,
			&TransactionBuilder::new().build(),
			&Some(receipt_failure),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_expression_matching() {
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(
			vec![], // events
			vec![], // functions
			vec![TransactionCondition {
				status: TransactionStatus::Any,
				expression: Some("value > 100".to_string()),
			}], // transactions
			vec![], // addresses
		);

		let tx_1 = TransactionBuilder::new().value(U256::from(150)).build();
		let tx_receipt_1 = ReceiptBuilder::new()
			.status(true)
			.transaction_hash(tx_1.hash)
			.build();

		// Test transaction with value > 100
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_1,
			&Some(tx_receipt_1),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].expression, Some("value > 100".to_string()));

		// Test transaction with value < 100
		let tx_2 = TransactionBuilder::new().value(U256::from(50)).build();
		let tx_receipt_2 = ReceiptBuilder::new()
			.status(true)
			.transaction_hash(tx_2.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_2,
			&Some(tx_receipt_2),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_address_expression_matching() {
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let test_address = Address::from_str("0x0000000000000000000000000000000000001234").unwrap();

		let monitor = create_test_monitor(
			vec![], // events
			vec![], // functions
			vec![TransactionCondition {
				status: TransactionStatus::Any,
				expression: Some(format!("to == {}", h160_to_string(test_address))),
			}], // transactions
			vec![], // addresses
		);

		// Test matching 'to' address
		let tx_matching = TransactionBuilder::new().to(test_address).build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.to(test_address)
			.transaction_hash(tx_matching.hash)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 1);

		// Test non-matching 'to' address
		let address_non_matching =
			Address::from_str("0x0000000000000000000000000000000000004321").unwrap();
		let tx_non_matching = TransactionBuilder::new().to(address_non_matching).build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.to(address_non_matching)
			.transaction_hash(tx_non_matching.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_from_address_expression_matching() {
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let test_address = Address::from_str("0x0000000000000000000000000000000000001234").unwrap();

		let monitor = create_test_monitor(
			vec![], // events
			vec![], // functions
			vec![TransactionCondition {
				status: TransactionStatus::Any,
				expression: Some(format!("from == {}", h160_to_string(test_address))),
			}], // transactions
			vec![], // addresses
		);

		// Test matching 'from' address
		let tx_matching = TransactionBuilder::new().from(test_address).build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.from(test_address)
			.transaction_hash(tx_matching.hash)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 1);

		// Test non-matching 'from' address
		let address_non_matching =
			Address::from_str("0x0000000000000000000000000000000000004321").unwrap();
		let tx_non_matching = TransactionBuilder::new().from(address_non_matching).build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.from(address_non_matching)
			.transaction_hash(tx_non_matching.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_gas_price_matching() {
		let expression = "gas_price > 1000000000".to_string(); // more than 1 Gwei
		let condition = TransactionCondition {
			status: TransactionStatus::Any,
			expression: Some(expression.clone()),
		};
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(vec![], vec![], vec![condition], vec![]);

		// Test transaction with gas price > 1 Gwei
		let gas_price_matching = U256::from(1500000000); // 1.5 Gwei
		let tx_matching = TransactionBuilder::new()
			.gas_price(gas_price_matching)
			.build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.transaction_hash(tx_matching.hash)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].expression, Some(expression));

		// Test transaction with gas price < 1 Gwei
		let gas_price_non_matching = U256::from(500000000); // 0.5 Gwei
		let tx_non_matching = TransactionBuilder::new()
			.gas_price(gas_price_non_matching)
			.build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.transaction_hash(tx_non_matching.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);

		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_max_fee_per_gas_matching() {
		let expression = "max_fee_per_gas > 1000000000".to_string(); // more than 1 Gwei
		let condition = TransactionCondition {
			status: TransactionStatus::Any,
			expression: Some(expression.clone()),
		};
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(vec![], vec![], vec![condition], vec![]);

		// Test transaction with max_fee_per_gas > 1 Gwei
		let max_fee_per_gas_matching = U256::from(1500000000); // 1.5 Gwei
		let tx_matching = TransactionBuilder::new()
			.max_fee_per_gas(max_fee_per_gas_matching)
			.build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.transaction_hash(tx_matching.hash)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].expression, Some(expression));

		// Test transaction with max_fee_per_gas < 1 Gwei
		let max_fee_per_gas_non_matching = U256::from(500000000); // 0.5 Gwei
		let tx_non_matching = TransactionBuilder::new()
			.max_fee_per_gas(max_fee_per_gas_non_matching)
			.build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.transaction_hash(tx_non_matching.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_max_priority_fee_per_gas_matching() {
		let expression = "max_priority_fee_per_gas > 1000000000".to_string(); // more than 1 Gwei
		let condition = TransactionCondition {
			status: TransactionStatus::Any,
			expression: Some(expression.clone()),
		};
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(vec![], vec![], vec![condition], vec![]);

		// Test transaction with max_priority_fee_per_gas > 1 Gwei
		let max_priority_fee_per_gas_matching = U256::from(1500000000); // 1.5 Gwei
		let tx_matching = TransactionBuilder::new()
			.max_priority_fee_per_gas(max_priority_fee_per_gas_matching)
			.build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.transaction_hash(tx_matching.hash)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].expression, Some(expression));

		// Test transaction with max_priority_fee_per_gas < 1 Gwei
		let max_priority_fee_per_gas_non_matching = U256::from(500000000); // 0.5 Gwei
		let tx_non_matching = TransactionBuilder::new()
			.max_priority_fee_per_gas(max_priority_fee_per_gas_non_matching)
			.build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.transaction_hash(tx_non_matching.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_gas_limit_matching() {
		let expression = "gas_limit > 20000".to_string(); // more than 20k
		let condition = TransactionCondition {
			status: TransactionStatus::Any,
			expression: Some(expression.clone()),
		};
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(vec![], vec![], vec![condition], vec![]);

		// Test transaction with gas_limit > 20k
		let gas_limit_matching = U256::from(30000); // 30k
		let tx_matching = TransactionBuilder::new()
			.gas_limit(gas_limit_matching)
			.build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.transaction_hash(tx_matching.hash)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].expression, Some(expression));

		// Test transaction with gas_limit < 20k
		let gas_limit_non_matching = U256::from(10000); // 10k
		let tx_non_matching = TransactionBuilder::new()
			.gas_limit(gas_limit_non_matching)
			.build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.transaction_hash(tx_non_matching.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_nonce_matching() {
		let expression = "nonce == 5".to_string();
		let condition = TransactionCondition {
			status: TransactionStatus::Any,
			expression: Some(expression.clone()),
		};
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(vec![], vec![], vec![condition], vec![]);

		// Test transaction with gas_limit > 20k
		let nonce_matching = U256::from(5);
		let tx_matching = TransactionBuilder::new().nonce(nonce_matching).build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.transaction_hash(tx_matching.hash)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].expression, Some(expression));

		// Test transaction with gas_limit < 20k
		let nonce_not_matching = U256::from(55);
		let tx_non_matching = TransactionBuilder::new().nonce(nonce_not_matching).build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.transaction_hash(tx_non_matching.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_input_matching() {
		let expression = "input == 0x1234".to_string();
		let condition = TransactionCondition {
			status: TransactionStatus::Any,
			expression: Some(expression.clone()),
		};
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(vec![], vec![], vec![condition], vec![]);

		// Test transaction with matching input
		let input_matching = Bytes(hex::decode("1234").unwrap().into());
		let tx_matching = TransactionBuilder::new().input(input_matching).build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.transaction_hash(tx_matching.hash)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].expression, Some(expression));

		// Test transaction with non-matching input
		let input_not_matching = Bytes(hex::decode("5678").unwrap().into());
		let tx_non_matching = TransactionBuilder::new().input(input_not_matching).build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.transaction_hash(tx_non_matching.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_gas_used_matching() {
		let expression = "gas_used > 20000".to_string(); // more than 20k
		let condition = TransactionCondition {
			status: TransactionStatus::Any,
			expression: Some(expression.clone()),
		};
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(vec![], vec![], vec![condition], vec![]);

		// Test transaction with gas_used > 20k
		let gas_used_matching = U256::from(30000); // 30k
		let tx_matching = TransactionBuilder::new().build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.transaction_hash(tx_matching.hash)
			.gas_used(gas_used_matching)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].expression, Some(expression));

		// Test transaction with gas_used < 20k
		let gas_used_non_matching = U256::from(10000); // 10k
		let tx_non_matching = TransactionBuilder::new().build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.transaction_hash(tx_non_matching.hash)
			.gas_used(gas_used_non_matching)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 0);
	}

	#[test]
	fn test_transaction_index_matching() {
		let expression = "transaction_index == 15".to_string();
		let condition = TransactionCondition {
			status: TransactionStatus::Any,
			expression: Some(expression.clone()),
		};
		let filter = create_test_filter();
		let mut matched = Vec::new();
		let monitor = create_test_monitor(vec![], vec![], vec![condition], vec![]);

		// Test transaction with matching transaction index
		let tx_matching = TransactionBuilder::new().transaction_index(15).build();
		let tx_receipt_matching = ReceiptBuilder::new()
			.transaction_hash(tx_matching.hash)
			.build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_matching,
			&Some(tx_receipt_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].expression, Some(expression));

		// Test transaction with non-matching transaction index
		let tx_non_matching = TransactionBuilder::new().transaction_index(1).build();
		let tx_receipt_non_matching = ReceiptBuilder::new()
			.transaction_hash(tx_non_matching.hash)
			.build();

		matched.clear();
		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx_non_matching,
			&Some(tx_receipt_non_matching),
			&monitor,
			&mut matched,
		);
		assert_eq!(matched.len(), 0);
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for find_matching_functions_for_transaction method:
	//////////////////////////////////////////////////////////////////////////////
	#[test]
	fn test_find_matching_functions_basic_match() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_on_args = EVMMatchArguments {
			events: None,
			functions: Some(Vec::new()),
		};

		let contract_with_spec = (
			"0x0000000000000000000000000000000000004321".to_string(),
			EVMContractSpec::from(create_test_abi("function")),
		);

		// Create a monitor with a simple function match condition
		let monitor = create_test_monitor(
			vec![], // events
			vec![FunctionCondition {
				signature: "transfer(address,uint256)".to_string(),
				expression: None,
			}], // functions
			vec![], // transactions
			vec![create_test_address(
				&contract_with_spec.0,
				Some(ContractSpec::EVM(contract_with_spec.1.clone())),
			)], // addresses
		);

		// Create a transaction with transfer function call
		#[allow(deprecated)]
		let function = Function {
			name: "transfer".to_string(),
			inputs: vec![
				Param {
					name: "recipient".to_string(),
					kind: ParamType::Address,
					internal_type: None,
				},
				Param {
					name: "amount".to_string(),
					kind: ParamType::Uint(256),
					internal_type: None,
				},
			],
			outputs: vec![Param {
				name: "".to_string(),
				kind: ParamType::Bool,
				internal_type: None,
			}],
			constant: None,
			state_mutability: ethabi::StateMutability::NonPayable,
		};

		let params = vec![
			ethabi::Token::Address(
				ethabi::Address::from_str("0x0000000000000000000000000000000000004321").unwrap(),
			),
			ethabi::Token::Uint(ethabi::Uint::from(1000)),
		];

		let encoded = function.encode_input(&params).unwrap();
		let transaction = TransactionBuilder::new()
			.from(Address::from_str("0x0000000000000000000000000000000000001234").unwrap())
			.to(Address::from_str("0x0000000000000000000000000000000000004321").unwrap())
			.input(Bytes(encoded.into()))
			.build();

		filter.find_matching_functions_for_transaction(
			&[contract_with_spec],
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_on_args,
		);

		assert_eq!(matched_functions.len(), 1);
		assert_eq!(matched_functions[0].signature, "transfer(address,uint256)");
		assert!(matched_functions[0].expression.is_none());

		let functions = matched_on_args.functions.unwrap();

		assert_eq!(functions.len(), 1);
	}

	#[test]
	fn test_find_matching_functions_with_expression() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_on_args = EVMMatchArguments {
			events: None,
			functions: Some(Vec::new()),
		};

		let contract_with_spec = (
			"0x0000000000000000000000000000000000004321".to_string(),
			EVMContractSpec::from(create_test_abi("function")),
		);

		// Create a monitor with a function match condition including an expression
		let monitor = create_test_monitor(
			vec![], // events
			vec![FunctionCondition {
				signature: "transfer(address,uint256)".to_string(),
				expression: Some("amount > 500".to_string()),
			}], // functions
			vec![], // transactions
			vec![create_test_address(
				&contract_with_spec.0,
				Some(ContractSpec::EVM(contract_with_spec.1.clone())),
			)], // addresses
		);

		#[allow(deprecated)]
		let function = Function {
			name: "transfer".to_string(),
			inputs: vec![
				Param {
					name: "recipient".to_string(),
					kind: ParamType::Address,
					internal_type: None,
				},
				Param {
					name: "amount".to_string(),
					kind: ParamType::Uint(256),
					internal_type: None,
				},
			],
			outputs: vec![Param {
				name: "".to_string(),
				kind: ParamType::Bool,
				internal_type: None,
			}],
			constant: None,
			state_mutability: ethabi::StateMutability::NonPayable,
		};

		// Test with amount > 500 (should match)
		let params = vec![
			ethabi::Token::Address(
				ethabi::Address::from_str("0x0000000000000000000000000000000000004321").unwrap(),
			),
			ethabi::Token::Uint(ethabi::Uint::from(1000)),
		];

		let encoded = function.encode_input(&params).unwrap();
		let transaction = TransactionBuilder::new()
			.to(Address::from_str("0x0000000000000000000000000000000000004321").unwrap())
			.input(Bytes(encoded.into()))
			.build();

		filter.find_matching_functions_for_transaction(
			&[contract_with_spec.clone()],
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_on_args,
		);

		assert_eq!(matched_functions.len(), 1);
		assert_eq!(
			matched_functions[0].expression,
			Some("amount > 500".to_string())
		);

		// Test with amount <= 500 (should not match)
		matched_functions.clear();
		if let Some(functions) = &mut matched_on_args.functions {
			functions.clear();
		}

		let params = vec![
			ethabi::Token::Address(
				ethabi::Address::from_str("0x0000000000000000000000000000000000004321").unwrap(),
			),
			ethabi::Token::Uint(ethabi::Uint::from(500)),
		];

		let encoded = function.encode_input(&params).unwrap();
		let transaction = TransactionBuilder::new()
			.to(Address::from_str("0x0000000000000000000000000000000000004321").unwrap())
			.input(Bytes(encoded.into()))
			.build();

		filter.find_matching_functions_for_transaction(
			&[contract_with_spec],
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_on_args,
		);

		assert_eq!(matched_functions.len(), 0);
	}

	#[test]
	fn test_find_matching_functions_non_matching_address() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_on_args = EVMMatchArguments {
			events: None,
			functions: Some(Vec::new()),
		};

		let contract_with_spec = (
			"0x0000000000000000000000000000000000004321".to_string(),
			EVMContractSpec::from(create_test_abi("function")),
		);

		let monitor = create_test_monitor(
			vec![],
			vec![FunctionCondition {
				signature: "transfer(address,uint256)".to_string(),
				expression: None,
			}],
			vec![],
			vec![create_test_address(
				&contract_with_spec.0,
				Some(ContractSpec::EVM(contract_with_spec.1.clone())),
			)],
		);

		// Create transaction with non-matching 'to' address
		#[allow(deprecated)]
		let function = Function {
			name: "transfer".to_string(),
			inputs: vec![
				Param {
					name: "recipient".to_string(),
					kind: ParamType::Address,
					internal_type: None,
				},
				Param {
					name: "amount".to_string(),
					kind: ParamType::Uint(256),
					internal_type: None,
				},
			],
			outputs: vec![Param {
				name: "".to_string(),
				kind: ParamType::Bool,
				internal_type: None,
			}],
			constant: None,
			state_mutability: ethabi::StateMutability::NonPayable,
		};

		let params = vec![
			ethabi::Token::Address(
				ethabi::Address::from_str("0x0000000000000000000000000000000000004321").unwrap(),
			),
			ethabi::Token::Uint(ethabi::Uint::from(1000)),
		];

		let encoded = function.encode_input(&params).unwrap();
		let transaction = TransactionBuilder::new()
			.to(Address::from_str("0x0000000000000000000000000000000000001234").unwrap())
			.input(Bytes(encoded.into()))
			.build();

		filter.find_matching_functions_for_transaction(
			&[contract_with_spec],
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_on_args,
		);

		assert_eq!(matched_functions.len(), 0);
	}

	#[test]
	fn test_find_matching_functions_invalid_input_data() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_on_args = EVMMatchArguments {
			events: None,
			functions: Some(Vec::new()),
		};

		let contract_with_spec = (
			"0x0000000000000000000000000000000000004321".to_string(),
			EVMContractSpec::from(create_test_abi("function")),
		);

		let monitor = MonitorBuilder::new()
			.match_conditions(MatchConditions {
				functions: vec![FunctionCondition {
					signature: "transfer(address,uint256)".to_string(),
					expression: None,
				}],
				events: vec![],
				transactions: vec![],
			})
			.addresses_with_spec(vec![(
				contract_with_spec.0.clone(),
				Some(ContractSpec::EVM(contract_with_spec.1.clone())),
			)])
			.name("test")
			.networks(vec!["evm_mainnet".to_string()])
			.paused(false)
			.build();

		// Test with invalid input data (less than 4 bytes)
		let transaction = TransactionBuilder::new()
			.to(Address::from_str("0x0000000000000000000000000000000000004321").unwrap())
			.input(Bytes(vec![0x12, 0x34].into()))
			.build();

		filter.find_matching_functions_for_transaction(
			&[contract_with_spec],
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_on_args,
		);

		assert_eq!(matched_functions.len(), 0);
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for find_matching_events_for_transaction method:
	//////////////////////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_find_matching_events_basic_match() {
		let filter = create_test_filter();
		let mut matched_events = Vec::new();
		let mut matched_on_args = EVMMatchArguments {
			events: Some(Vec::new()),
			functions: None,
		};
		let mut involved_addresses = Vec::new();

		// Create a monitor with a simple event match condition
		let monitor = create_test_monitor(
			vec![EventCondition {
				signature: "Transfer(address,address,uint256)".to_string(),
				expression: None,
			}], // events
			vec![], // functions
			vec![], // transactions
			vec![create_test_address(
				"0x0000000000000000000000000000000000004321",
				Some(create_test_abi("event")), // Changed to event ABI
			)], // addresses
		);

		// Create a transaction receipt with a Transfer event
		let contract_address =
			Address::from_str("0x0000000000000000000000000000000000004321").unwrap();
		let receipt = ReceiptBuilder::new()
			.contract_address(contract_address)
			.from(Address::from_str("0x0000000000000000000000000000000000001234").unwrap())
			.to(Address::from_str("0x0000000000000000000000000000000000005678").unwrap())
			.value(U256::from(100))
			.build();

		filter.find_matching_events_for_transaction(
			&receipt.logs,
			&monitor,
			&mut matched_events,
			&mut matched_on_args,
			&mut involved_addresses,
		);

		assert_eq!(matched_events.len(), 1);
		assert_eq!(
			matched_events[0].signature,
			"Transfer(address,address,uint256)"
		);
		assert!(matched_events[0].expression.is_none());
		assert_eq!(involved_addresses.len(), 1);
		assert_eq!(
			involved_addresses[0],
			"0x0000000000000000000000000000000000004321"
		);
	}

	#[tokio::test]
	async fn test_find_matching_events_with_expression() {
		let filter = create_test_filter();
		let mut matched_events = Vec::new();
		let mut matched_on_args = EVMMatchArguments {
			events: Some(Vec::new()),
			functions: None,
		};
		let mut involved_addresses = Vec::new();

		// Create a monitor with an event match condition including an expression
		let monitor = create_test_monitor(
			vec![EventCondition {
				signature: "Transfer(address,address,uint256)".to_string(),
				expression: Some("value > 500".to_string()),
			}], // events
			vec![], // functions
			vec![], // transactions
			vec![create_test_address(
				"0x0000000000000000000000000000000000004321",
				Some(create_test_abi("event")), // Changed to event ABI
			)], // addresses
		);

		// Create a receipt with value > 500 (should match)
		let contract_address =
			Address::from_str("0x0000000000000000000000000000000000004321").unwrap();
		let receipt = ReceiptBuilder::new()
			.contract_address(contract_address)
			.from(Address::from_str("0x0000000000000000000000000000000000001234").unwrap())
			.to(Address::from_str("0x0000000000000000000000000000000000005678").unwrap())
			.value(U256::from(1000))
			.build();

		filter.find_matching_events_for_transaction(
			&receipt.logs,
			&monitor,
			&mut matched_events,
			&mut matched_on_args,
			&mut involved_addresses,
		);

		assert_eq!(matched_events.len(), 1);
		assert_eq!(
			matched_events[0].expression,
			Some("value > 500".to_string())
		);

		// Test with value <= 500 (should not match)
		matched_events.clear();
		if let Some(events) = &mut matched_on_args.events {
			events.clear();
		}
		involved_addresses.clear();

		let receipt_no_match = ReceiptBuilder::new()
			.contract_address(contract_address)
			.from(Address::from_str("0x0000000000000000000000000000000000001234").unwrap())
			.to(Address::from_str("0x0000000000000000000000000000000000005678").unwrap())
			.value(U256::from(50))
			.build();

		filter.find_matching_events_for_transaction(
			&receipt_no_match.logs,
			&monitor,
			&mut matched_events,
			&mut matched_on_args,
			&mut involved_addresses,
		);

		assert_eq!(matched_events.len(), 0);
	}

	#[tokio::test]
	async fn test_find_matching_events_non_matching_address() {
		let filter = create_test_filter();
		let mut matched_events = Vec::new();
		let mut matched_on_args = EVMMatchArguments {
			events: Some(Vec::new()),
			functions: None,
		};
		let mut involved_addresses = Vec::new();

		let monitor = create_test_monitor(
			vec![], // events
			vec![FunctionCondition {
				signature: "transfer(address,uint256)".to_string(),
				expression: None,
			}], // functions
			vec![], // transactions
			vec![create_test_address(
				"0x0000000000000000000000000000000000004321",
				Some(create_test_abi("function")),
			)], // addresses
		);

		// Create a receipt with non-matching contract address
		let different_address =
			Address::from_str("0x0000000000000000000000000000000000001234").unwrap();
		let receipt = ReceiptBuilder::new()
			.contract_address(different_address)
			.from(Address::from_str("0x0000000000000000000000000000000000001234").unwrap())
			.to(Address::from_str("0x0000000000000000000000000000000000005678").unwrap())
			.value(U256::from(100))
			.build();

		filter.find_matching_events_for_transaction(
			&receipt.logs,
			&monitor,
			&mut matched_events,
			&mut matched_on_args,
			&mut involved_addresses,
		);

		assert_eq!(matched_events.len(), 0);
		assert_eq!(involved_addresses.len(), 0);
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for evaluate_expression method:
	//////////////////////////////////////////////////////////////////////////////
	fn create_test_param(name: &str, value: &str, kind: &str) -> EVMMatchParamEntry {
		EVMMatchParamEntry {
			name: name.to_string(),
			value: value.to_string(),
			kind: kind.to_string(),
			indexed: false,
		}
	}

	#[test]
	fn test_evaluate_expression_simple_uint_comparisons() {
		let filter = create_test_filter();
		let args = vec![create_test_param("amount", "1000", "uint256")];

		// Test all operators
		assert!(filter.evaluate_expression("amount > 500", &args).unwrap());
		assert!(filter.evaluate_expression("amount >= 1000", &args).unwrap());
		assert!(filter.evaluate_expression("amount < 2000", &args).unwrap());
		assert!(filter.evaluate_expression("amount <= 1000", &args).unwrap());
		assert!(filter.evaluate_expression("amount == 1000", &args).unwrap());
		assert!(filter.evaluate_expression("amount != 999", &args).unwrap());

		// Test false conditions
		assert!(!filter.evaluate_expression("amount > 1000", &args).unwrap());
		assert!(!filter.evaluate_expression("amount < 1000", &args).unwrap());
		assert!(!filter.evaluate_expression("amount == 999", &args).unwrap());
		assert!(!filter.evaluate_expression("amount != 1000", &args).unwrap());
		assert!(!filter.evaluate_expression("amount <= 500", &args).unwrap());
		assert!(!filter.evaluate_expression("amount >= 2000", &args).unwrap());
	}

	#[test]
	fn test_evaluate_expression_signed_int_comparisons() {
		let filter = create_test_filter();
		let args = vec![create_test_param("balance", "-100", "int256")];

		// Test all operators
		assert!(filter
			.evaluate_expression("balance == -100", &args)
			.unwrap());
		assert!(filter.evaluate_expression("balance != 0", &args).unwrap());
		assert!(filter.evaluate_expression("balance < 0", &args).unwrap());
		assert!(filter.evaluate_expression("balance <= 0", &args).unwrap());
		assert!(filter.evaluate_expression("balance > -200", &args).unwrap());
		assert!(filter
			.evaluate_expression("balance >= -100", &args)
			.unwrap());

		// Test false conditions
		assert!(!filter.evaluate_expression("balance < -100", &args).unwrap());
		assert!(!filter
			.evaluate_expression("balance <= -200", &args)
			.unwrap());
		assert!(!filter.evaluate_expression("balance > -100", &args).unwrap());
		assert!(!filter.evaluate_expression("balance >= 0", &args).unwrap());
		assert!(!filter
			.evaluate_expression("balance != -100", &args)
			.unwrap());
		assert!(!filter.evaluate_expression("balance == 0", &args).unwrap());
	}

	#[test]
	fn test_evaluate_expression_fixed_point_comparisons() {
		let filter = create_test_filter();
		let args = vec![create_test_param("price", "123.45", "fixed")];

		// Test all operators
		assert!(filter
			.evaluate_expression("price == 123.45", &args)
			.unwrap());
		assert!(filter.evaluate_expression("price != 0.0", &args).unwrap());
		assert!(filter.evaluate_expression("price < 200.0", &args).unwrap());
		assert!(filter
			.evaluate_expression("price <= 123.45", &args)
			.unwrap());
		assert!(filter.evaluate_expression("price > 100.0", &args).unwrap());
		assert!(filter
			.evaluate_expression("price >= 123.45", &args)
			.unwrap());

		// Test false conditions
		assert!(!filter.evaluate_expression("price < 123.45", &args).unwrap());
		assert!(!filter.evaluate_expression("price <= 100.0", &args).unwrap());
		assert!(!filter.evaluate_expression("price > 123.45", &args).unwrap());
		assert!(!filter.evaluate_expression("price >= 200.0", &args).unwrap());
		assert!(!filter
			.evaluate_expression("price != 123.45", &args)
			.unwrap());
		assert!(!filter.evaluate_expression("price == 0.0", &args).unwrap());
	}

	#[test]
	fn test_evaluate_expression_address_comparisons() {
		let filter = create_test_filter();
		let args = vec![create_test_param(
			"recipient",
			"0x1234567890123456789012345678901234567890",
			"address",
		)];

		// Test equality
		assert!(filter
			.evaluate_expression(
				"recipient == 0x1234567890123456789012345678901234567890",
				&args
			)
			.unwrap());
		assert!(filter
			.evaluate_expression(
				"recipient != 0x0000000000000000000000000000000000000000",
				&args
			)
			.unwrap());

		// Test case-insensitive comparison
		assert!(filter
			.evaluate_expression(
				"recipient == 0x1234567890123456789012345678901234567890",
				&args
			)
			.unwrap());

		// Test false conditions
		assert!(!filter
			.evaluate_expression(
				"recipient == 0x0000000000000000000000000000000000000000",
				&args
			)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_boolean_comparisons() {
		let filter = create_test_filter();
		let args = vec![create_test_param("is_active", "true", "bool")];

		// Test equality
		assert!(filter
			.evaluate_expression("is_active == true", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("is_active != false", &args)
			.unwrap());

		// Test false conditions
		assert!(!filter
			.evaluate_expression("is_active == false", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("is_active != true", &args)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_string_comparisons() {
		let filter = create_test_filter();
		let args = vec![create_test_param("name", "Alice", "string")];

		// Test true conditions
		assert!(filter
			.evaluate_expression("name == 'Alice'", &args)
			.unwrap());
		assert!(filter.evaluate_expression("name != 'Bob'", &args).unwrap());
		assert!(filter
			.evaluate_expression("name contains 'ice'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("name starts_with 'ali'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("name ends_with 'ice'", &args)
			.unwrap());

		// Test false conditions
		assert!(!filter.evaluate_expression("name == 'Bob'", &args).unwrap());
		assert!(!filter
			.evaluate_expression("name != 'Alice'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("name contains 'Bob'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("name starts_with 'Bob'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("name ends_with 'Bob'", &args)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_basic_field_access() {
		let filter = create_test_filter();
		let args = vec![create_test_param(
			"transaction",
			r#"{"to": "0x1234567890123456789012345678901234567890"}"#,
			"object",
		)];

		// Test field access
		assert!(filter
			.evaluate_expression(
				"transaction.to == 0x1234567890123456789012345678901234567890",
				&args
			)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_nested_field_access() {
		let filter = create_test_filter();

		let args = vec![create_test_param(
			"transaction",
			r#"{"from": {"address": "0x1234567890123456789012345678901234567890"}}"#,
			"object",
		)];

		assert!(filter
			.evaluate_expression(
				"transaction.from.address == 0x1234567890123456789012345678901234567890",
				&args
			)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_array_indexing() {
		let filter = create_test_filter();
		let args = vec![create_test_param("array", "[1, 2, 3]", "array")];

		// Test array indexing
		assert!(filter.evaluate_expression("array[0] == 1", &args).unwrap());
		assert!(filter.evaluate_expression("array[1] == 2", &args).unwrap());
		assert!(filter.evaluate_expression("array[2] == 3", &args).unwrap());

		// Test out-of-bounds access
		assert!(filter.evaluate_expression("array[3] == 4", &args).is_err());
	}

	#[test]
	fn test_evaluate_expression_object_in_array() {
		let filter = create_test_filter();
		let args = vec![create_test_param(
			"objects",
			r#"[{"name": "Alice"}, {"name": "Bob"}]"#,
			"array",
		)];

		// Test object in array
		assert!(filter
			.evaluate_expression("objects[0].name == 'Alice'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("objects[1].name == 'Bob'", &args)
			.unwrap());

		// Test out-of-bounds access
		assert!(filter
			.evaluate_expression("objects[2].name == 'Charlie'", &args)
			.is_err());
	}

	#[test]
	fn test_evaluate_expression_array_json_contains_simple_values() {
		let filter = create_test_filter();
		let args = vec![create_test_param(
			"my_array",
			r#"["alice", "0x1234567890123456789012345678901234567890", 123, true]"#,
			"array",
		)];

		// String contains
		assert!(filter
			.evaluate_expression("my_array contains 'alice'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression(
				"my_array contains '0x1234567890123456789012345678901234567890'",
				&args
			)
			.unwrap());
		// Number contains (target is number literal, EVMConditionEvaluator::compare_array gets it as string)
		assert!(filter
			.evaluate_expression("my_array contains '123'", &args)
			.unwrap()); // RHS target_str will be "123"
			   // Boolean contains
		assert!(filter
			.evaluate_expression("my_array contains 'true'", &args)
			.unwrap());

		// Not contains
		assert!(!filter
			.evaluate_expression("my_array contains 'dave'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression(
				"my_array contains '0xNonExistentAddress00000000000000000000'",
				&args
			)
			.unwrap());
		assert!(!filter
			.evaluate_expression("my_array contains '456'", &args)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_array_json_contains_in_object_field() {
		let filter = create_test_filter();
		let args = vec![create_test_param(
			"obj_array",
			r#"[{"id": 1, "name": "alice", "details": {"color": "red"}}, {"id": 2, "name": "bob"}]"#,
			"array",
		)];

		// Checks if "bob" is a value of any field in any object within the array
		assert!(filter
			.evaluate_expression("obj_array contains 'bob'", &args)
			.unwrap());
		// Checks if "2" (as a string, from number 2) is a value
		assert!(filter
			.evaluate_expression("obj_array contains '2'", &args)
			.unwrap());
		// Checks a value within a nested object field
		assert!(filter
			.evaluate_expression("obj_array contains 'red'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("obj_array contains 'charlie'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("obj_array contains 'green'", &args)
			.unwrap()); // color not green
	}

	#[test]
	fn test_evaluate_expression_array_eq_ne_raw_json() {
		let filter = create_test_filter();
		let args_json_array = vec![create_test_param(
			"my_json_list",
			r#"["alice", "bob"]"#,
			"array",
		)];

		// Eq/Ne on "array" kind compares the raw JSON string value
		assert!(filter
			.evaluate_expression("my_json_list == '[\"alice\", \"bob\"]'", &args_json_array)
			.unwrap());
		assert!(!filter
			.evaluate_expression(
				"my_json_list == '[\"alice\", \"charlie\"]'",
				&args_json_array
			)
			.unwrap());
		assert!(filter
			.evaluate_expression(
				"my_json_list != '[\"alice\", \"charlie\"]'",
				&args_json_array
			)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_map_eq_ne_raw_json() {
		let filter = create_test_filter();
		let args_json_map = vec![create_test_param(
			"my_json_map",
			r#"{"key1": "value1", "key2": "value2"}"#,
			"map",
		)];

		// Eq/Ne on "object" kind compares the raw JSON string value
		assert!(filter
			.evaluate_expression(
				"my_json_map == '{\"key1\": \"value1\", \"key2\": \"value2\"}'",
				&args_json_map
			)
			.unwrap());
		assert!(!filter
			.evaluate_expression(
				"my_json_map == '{\"key1\": \"value1\", \"key2\": \"value3\"}'",
				&args_json_map
			)
			.unwrap());
		assert!(filter
			.evaluate_expression(
				"my_json_map != '{\"key1\": \"value1\", \"key2\": \"value3\"}'",
				&args_json_map
			)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_error_cases() {
		let filter = create_test_filter();

		// Test with empty args
		assert!(filter.evaluate_expression("amount > 1000", &[]).is_err());

		// Test with invalid parameter name
		let args = vec![create_test_param("amount", "1000", "uint256")];
		assert!(filter
			.evaluate_expression("invalid_param > 1000", &args)
			.is_err());

		// Test with invalid operator
		assert!(filter
			.evaluate_expression("amount >>> 1000", &args)
			.is_err());

		// Test with invalid value format
		let args = vec![create_test_param("amount", "not_a_number", "uint256")];
		assert!(filter.evaluate_expression("amount > 1000", &args).is_err());

		// Test with unsupported parameter type
		let args = vec![create_test_param("param", "value", "unsupported_type")];
		assert!(filter.evaluate_expression("param == value", &args).is_err());
	}

	#[test]
	fn test_evaluate_expression_logical_and_operator() {
		let filter = create_test_filter();
		let args_true_true = vec![
			create_test_param("value", "150", "uint256"),
			create_test_param("name", "Alice", "string"),
		];
		let args_true_false = vec![
			create_test_param("value", "150", "uint256"),
			create_test_param("name", "Bob", "string"), // This will make 'name == "Alice"' false
		];
		let args_false_true = vec![
			create_test_param("value", "50", "uint256"), // This will make 'value > 100' false
			create_test_param("name", "Alice", "string"),
		];
		let args_false_false = vec![
			create_test_param("value", "50", "uint256"), // 'value > 100' is false
			create_test_param("name", "Bob", "string"),  // 'name == "Alice"' is false
		];

		// True AND True
		assert!(filter
			.evaluate_expression("value > 100 AND name == 'Alice'", &args_true_true)
			.unwrap());
		// True AND False
		assert!(!filter
			.evaluate_expression("value > 100 AND name == 'Alice'", &args_true_false)
			.unwrap());
		// False AND True
		assert!(!filter
			.evaluate_expression("value > 100 AND name == 'Alice'", &args_false_true)
			.unwrap());
		// False AND False
		assert!(!filter
			.evaluate_expression("value > 100 AND name == 'Alice'", &args_false_false)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_logical_or_operator() {
		let filter = create_test_filter();
		let args_true_true = vec![
			create_test_param("value", "150", "uint256"),
			create_test_param("name", "Alice", "string"),
		];
		let args_true_false = vec![
			create_test_param("value", "150", "uint256"),
			create_test_param("name", "Bob", "string"),
		];
		let args_false_true = vec![
			create_test_param("value", "50", "uint256"),
			create_test_param("name", "Alice", "string"),
		];
		let args_false_false = vec![
			create_test_param("value", "50", "uint256"),
			create_test_param("name", "Bob", "string"),
		];

		// True OR True
		assert!(filter
			.evaluate_expression("value > 100 OR name == 'Alice'", &args_true_true)
			.unwrap());
		// True OR False
		assert!(filter
			.evaluate_expression("value > 100 OR name == 'Alice'", &args_true_false)
			.unwrap());
		// False OR True
		assert!(filter
			.evaluate_expression("value > 100 OR name == 'Alice'", &args_false_true)
			.unwrap());
		// False OR False
		assert!(!filter
			.evaluate_expression("value > 100 OR name == 'Alice'", &args_false_false)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_logical_combinations_and_precedence() {
		let filter = create_test_filter();

		// Case 1: (T AND T) OR F  => T (due to AND precedence over OR)
		let args1 = vec![
			create_test_param("val1", "10", "uint256"), // T for val1 > 5
			create_test_param("str1", "hello", "string"), // T for str1 == 'hello'
			create_test_param("bool1", "false", "bool"), // F for bool1 == true
		];
		assert!(filter
			.evaluate_expression("val1 > 5 AND str1 == 'hello' OR bool1 == true", &args1)
			.unwrap());

		// Case 2: T AND (T OR F) => T (parentheses first)
		assert!(filter
			.evaluate_expression("val1 > 5 AND (str1 == 'hello' OR bool1 == true)", &args1)
			.unwrap());

		// Case 3: (T AND F) OR T => T
		let args2 = vec![
			create_test_param("val1", "10", "uint256"),   // T
			create_test_param("str1", "world", "string"), // F
			create_test_param("bool1", "true", "bool"),   // T
		];
		assert!(filter
			.evaluate_expression("val1 > 5 AND str1 == 'hello' OR bool1 == true", &args2)
			.unwrap());

		// Case 4: (T OR F) AND T => T
		assert!(filter
			.evaluate_expression("(val1 > 5 OR str1 == 'hello') AND bool1 == true", &args2)
			.unwrap());

		// Case 5: (F AND F) OR F => F
		let args3 = vec![
			create_test_param("val1", "1", "uint256"),    // F
			create_test_param("str1", "world", "string"), // F
			create_test_param("bool1", "false", "bool"),  // F
		];
		assert!(!filter
			.evaluate_expression("val1 > 5 AND str1 == 'hello' OR bool1 == true", &args3)
			.unwrap());

		// Case 6: (F OR F) AND F => F
		assert!(!filter
			.evaluate_expression("(val1 > 5 OR str1 == 'hello') AND bool1 == true", &args3)
			.unwrap());

		// Case 7: T AND F OR F -> (T AND F) OR F -> F OR F -> F
		let args_t_f_f = vec![
			create_test_param("a", "10", "uint256"), // a > 0 (T)
			create_test_param("b", "foo", "string"), // b == 'bar' (F)
			create_test_param("c", "false", "bool"), // c == true (F)
		];
		assert!(!filter
			.evaluate_expression("a > 0 AND b == 'bar' OR c == true", &args_t_f_f)
			.unwrap());

		// Case 8: (T OR F) AND F -> T AND F -> F
		assert!(!filter
			.evaluate_expression("(a > 0 OR b == 'bar') AND c == true", &args_t_f_f)
			.unwrap());

		// Case 9: F AND T OR T -> (F AND T) OR T -> F OR T -> T
		let args_f_t_t = vec![
			create_test_param("a", "-5", "int256"),  // a > 0 (F)
			create_test_param("b", "bar", "string"), // b == 'bar' (T)
			create_test_param("c", "true", "bool"),  // c == true (T)
		];
		assert!(filter
			.evaluate_expression("a > 0 AND b == 'bar' OR c == true", &args_f_t_t)
			.unwrap());

		// Case 10: (F OR T) AND T -> T AND T -> T
		assert!(filter
			.evaluate_expression("(a > 0 OR b == 'bar') AND c == true", &args_f_t_t)
			.unwrap());
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for decode_events method:
	//////////////////////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_decode_events_successful_decode() {
		let filter = create_test_filter();

		// Create contract address and log
		let contract_address =
			Address::from_str("0x0000000000000000000000000000000000004321").unwrap();
		let log = create_test_log(
			contract_address,
			// Transfer event signature
			"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
			// from address
			Address::from_str("0x0000000000000000000000000000000000001234").unwrap(),
			// to address
			Address::from_str("0x0000000000000000000000000000000000005678").unwrap(),
			// value (100 in hex)
			"0000000000000000000000000000000000000000000000000000000000000064",
		);

		// Use the event ABI
		let abi = create_test_abi("event");

		let result = filter.decode_events(&abi, &log);

		assert!(result.is_some());
		let decoded = result.unwrap();

		// Verify decoded event signature
		assert_eq!(decoded.signature, "Transfer(address,address,uint256)");

		// Verify decoded arguments
		let args = decoded.args.unwrap();
		assert_eq!(args.len(), 3); // Transfer event has 3 parameters

		// Check each parameter
		let from_param = args.iter().find(|p| p.name == "from").unwrap();
		assert_eq!(from_param.kind, "address");
		assert!(from_param.indexed);

		let to_param = args.iter().find(|p| p.name == "to").unwrap();
		assert_eq!(to_param.kind, "address");
		assert!(to_param.indexed);

		let value_param = args.iter().find(|p| p.name == "value").unwrap();
		assert_eq!(value_param.kind, "uint256");
		assert!(!value_param.indexed);
		assert_eq!(value_param.value, "100"); // 0x64 in decimal
	}

	#[tokio::test]
	async fn test_decode_events_invalid_abi() {
		let filter = create_test_filter();
		let contract_address =
			Address::from_str("0x0000000000000000000000000000000000003039").unwrap();
		let log = create_test_log(
			contract_address,
			"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
			Address::from_str("0x0000000000000000000000000000000000001234").unwrap(),
			Address::from_str("0x0000000000000000000000000000000000005678").unwrap(),
			"0000000000000000000000000000000000000000000000000000000000000064",
		);

		// Use invalid ABI
		let invalid_abi = json!([{
			"type": "event",
			"name": "InvalidEvent",
			"inputs": [], // Empty inputs won't match our log
			"anonymous": false,
		}]);

		let result =
			filter.decode_events(&ContractSpec::EVM(EVMContractSpec::from(invalid_abi)), &log);
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_decode_events_mismatched_signature() {
		let filter = create_test_filter();
		let contract_address =
			Address::from_str("0x0000000000000000000000000000000000004321").unwrap();

		// Create log with different event signature
		let log = create_test_log(
			contract_address,
			// Different event signature
			"0x0000000000000000000000000000000000000000000000000000000000000000",
			Address::from_str("0x0000000000000000000000000000000000001234").unwrap(),
			Address::from_str("0x0000000000000000000000000000000000005678").unwrap(),
			"0000000000000000000000000000000000000000000000000000000000000064",
		);

		let abi = create_test_abi("event");
		let result = filter.decode_events(&abi, &log);

		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_decode_events_malformed_log_data() {
		let filter = create_test_filter();
		let contract_address =
			Address::from_str("0x0000000000000000000000000000000000004321").unwrap();

		let log = create_test_log(
			contract_address,
			// Transfer event signature
			"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
			// from address
			Address::from_str("0x0000000000000000000000000000000000001234").unwrap(),
			// to address
			Address::from_str("0x0000000000000000000000000000000000005678").unwrap(),
			// value (100 in hex)
			"0000000000000000000000000000000000000000000000000000000000000064",
		);

		// Create log with invalid data length
		let log = EVMReceiptLog {
			data: Bytes(vec![0x00].into()), // Invalid data length
			..log
		};

		let abi = create_test_abi("event");
		let result = filter.decode_events(&abi, &log);

		assert!(result.is_none());
	}
}
