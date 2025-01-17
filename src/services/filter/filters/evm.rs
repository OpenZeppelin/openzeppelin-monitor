//! EVM blockchain filter implementation.
//!
//! This module provides filtering capabilities for Ethereum Virtual Machine (EVM) compatible
//! blockchains. It handles:
//! - Transaction matching based on conditions
//! - Function call detection and parameter matching
//! - Event log processing and filtering
//! - ABI-based decoding of function calls and events

use async_trait::async_trait;
use ethabi::Contract;
use log::{info, warn};
use serde_json::Value;
use std::str::FromStr;
use web3::types::{Log, Transaction, TransactionReceipt};

use crate::{
	models::{
		AddressWithABI, BlockType, EVMMatchArguments, EVMMatchParamEntry, EVMMatchParamsMap,
		EVMMonitorMatch, EVMTransaction, EventCondition, FunctionCondition, MatchConditions,
		Monitor, MonitorMatch, Network, TransactionCondition, TransactionStatus,
	},
	services::{
		blockchain::BlockChainClientEnum,
		filter::{
			helpers::evm::{
				are_same_address, are_same_signature, format_token_value, h160_to_string,
				h256_to_string, normalize_address,
			},
			BlockFilter, FilterError,
		},
	},
};

/// Filter implementation for EVM-compatible blockchains
pub struct EVMBlockFilter {}

impl EVMBlockFilter {
	/// Finds transactions that match the monitor's conditions.
	///
	/// # Arguments
	/// * `tx_status` - Status of the transaction (success/failure)
	/// * `transaction` - The transaction to check
	/// * `monitor` - Monitor containing match conditions
	/// * `matched_transactions` - Vector to store matching transactions
	pub fn find_matching_transaction(
		&self,
		tx_status: &TransactionStatus,
		transaction: &Transaction,
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
								value: h256_to_string(transaction.hash),
								kind: "string".to_string(),
								indexed: false,
							},
						];

						if self.evaluate_expression(expr, &Some(tx_params)) {
							matched_transactions.push(TransactionCondition {
								expression: Some(expr.to_string()),
								status: *tx_status,
							});
							break;
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
	/// * `transaction` - The transaction containing the function call
	/// * `monitor` - Monitor containing function match conditions
	/// * `matched_functions` - Vector to store matching functions
	/// * `matched_on_args` - Arguments from matched function calls
	pub fn find_matching_functions_for_transaction(
		&self,
		transaction: &Transaction,
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
				if let Some(abi) = &monitored_addr.abi {
					// Create contract object from ABI
					if let Ok(contract) = Contract::load(abi.to_string().as_bytes()) {
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
												FilterError::internal_error(format!(
													"Failed to decode function input: {}",
													e
												));
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
											if self.evaluate_expression(expr, &Some(params.clone()))
											{
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
														hex_signature: Some(hex::encode(
															function.short_signature(),
														)),
													});
												}
												break;
											}
										} else {
											// No expression, just match on function name
											matched_functions.push(FunctionCondition {
												signature: function_signature_with_params.clone(),
												expression: None,
											});
											if let Some(functions) = &mut matched_on_args.functions
											{
												functions.push(EVMMatchParamsMap {
													signature: function_signature_with_params
														.clone(),
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
		receipt: &TransactionReceipt,
		monitor: &Monitor,
		matched_events: &mut Vec<EventCondition>,
		matched_on_args: &mut EVMMatchArguments,
		involved_addresses: &mut Vec<String>,
	) {
		for log in &receipt.logs {
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
			if let Some(abi) = &monitored_addr.abi {
				let decoded_log = self.decode_events(abi, log).await;

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
										if self.evaluate_expression(expr, &event_condition.args) {
											matched_events.push(EventCondition {
												signature: event_condition.signature.clone(),
												expression: Some(expr.to_string()),
											});
											if let Some(events) = &mut matched_on_args.events {
												events.push(event_condition);
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
		args: &Option<Vec<EVMMatchParamEntry>>,
	) -> bool {
		let Some(args) = args else {
			return false;
		};

		// First split by OR to get the highest level conditions
		let or_conditions: Vec<&str> = expression.split(" OR ").collect();

		// For OR logic, any condition being true makes the whole expression true
		for or_condition in or_conditions {
			// Split each OR condition by AND
			let and_conditions: Vec<&str> = or_condition.trim().split(" AND ").collect();

			// All AND conditions must be true
			let and_result = and_conditions.iter().all(|condition| {
				// Remove any surrounding parentheses and trim
				let clean_condition = condition.trim().trim_matches(|c| c == '(' || c == ')');

				// Split condition into parts (e.g., "amount > 1000")
				let parts: Vec<&str> = clean_condition.split_whitespace().collect();
				if parts.len() != 3 {
					warn!("Invalid expression format: {}", clean_condition);
					return false;
				}

				let [param_name, operator, value] = [parts[0], parts[1], parts[2]];

				// Find the parameter in args
				let Some(param) = args.iter().find(|p| p.name == param_name) else {
					warn!("Parameter {} not found in event args", param_name);
					return false;
				};

				// Evaluate single condition
				match param.kind.as_str() {
					"uint256" | "uint" => {
						let Ok(param_value) = u128::from_str(&param.value) else {
							warn!("Failed to parse parameter value: {}", param.value);
							return false;
						};
						let Ok(compare_value) = u128::from_str(value) else {
							warn!("Failed to parse comparison value: {}", value);
							return false;
						};

						match operator {
							">" => param_value > compare_value,
							">=" => param_value >= compare_value,
							"<" => param_value < compare_value,
							"<=" => param_value <= compare_value,
							"==" => param_value == compare_value,
							"!=" => param_value != compare_value,
							_ => {
								warn!("Unsupported operator: {}", operator);
								false
							}
						}
					}
					"address" => match operator {
						"==" => are_same_address(&param.value, value),
						"!=" => !are_same_address(&param.value, value),
						_ => {
							warn!("Unsupported operator for address type: {}", operator);
							false
						}
					},
					_ => {
						warn!("Unsupported parameter type: {}", param.kind);
						false
					}
				}
			});

			// If any OR condition is true, return true
			if and_result {
				return true;
			}
		}

		// No conditions were true
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
	pub async fn decode_events(&self, abi: &Value, log: &Log) -> Option<EVMMatchParamsMap> {
		// Create contract object from ABI
		let contract = Contract::load(abi.to_string().as_bytes())
			.map_err(|e| FilterError::internal_error(format!("Failed to parse ABI: {}", e)))
			.unwrap();

		let decoded_log = contract
			.events()
			.find(|event| event.signature() == log.topics[0])
			.and_then(|event| {
				event
					.parse_log((log.topics.clone(), log.data.0.clone()).into())
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
}

#[async_trait]
impl BlockFilter for EVMBlockFilter {
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
	async fn filter_block(
		&self,
		client: &BlockChainClientEnum,
		_network: &Network,
		block: &BlockType,
		monitors: &[Monitor],
	) -> Result<Vec<MonitorMatch>, FilterError> {
		let evm_block = match block {
			BlockType::EVM(block) => block,
			_ => {
				return Err(FilterError::block_type_mismatch(
					"Expected EVM block".to_string(),
				))
			}
		};

		info!("Processing block {}", evm_block.number.unwrap());

		let evm_client = match client {
			BlockChainClientEnum::EVM(client) => client,
			_ => {
				return Err(FilterError::internal_error(
					"Expected EVM client".to_string(),
				));
			}
		};

		// Process all transaction receipts in parallel
		let receipt_futures: Vec<_> = evm_block
			.transactions
			.iter()
			.map(|transaction| {
				let tx_hash = h256_to_string(transaction.hash);
				evm_client.get_transaction_receipt(tx_hash)
			})
			.collect();

		let receipts: Vec<_> = futures::future::join_all(receipt_futures)
			.await
			.into_iter()
			.filter_map(|result| match result {
				Ok(receipt) => Some(receipt),
				Err(e) => {
					warn!("Failed to get transaction receipt: {}", e);
					None
				}
			})
			.collect();

		if receipts.is_empty() {
			info!("No transactions in block");
			return Ok(vec![]);
		}

		let mut matching_results = Vec::new();

		info!("Processing {} monitor(s)", monitors.len());

		for monitor in monitors {
			info!("Processing monitor: {:?}", monitor.name);
			let monitored_addresses: Vec<String> = monitor
				.addresses
				.iter()
				.map(|a| a.address.clone())
				.collect();
			// Check each receipt and transaction for matches
			info!("Processing {} receipt(s)", receipts.len());
			for receipt in &receipts {
				let matching_transaction = evm_block
					.transactions
					.iter()
					.find(|tx| tx.hash == receipt.transaction_hash);

				if let Some(transaction) = matching_transaction {
					// Reset matched_on_args for each transaction
					let mut matched_on_args = EVMMatchArguments {
						events: Some(Vec::new()),
						functions: Some(Vec::new()),
					};

					// Get transaction status from receipt
					let tx_status = if receipt.status.map(|s| s.as_u64() == 1).unwrap_or(false) {
						TransactionStatus::Success
					} else {
						TransactionStatus::Failure
					};

					// Collect all involved addresses from receipt logs, transaction.to, and
					// transaction.from
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
						monitor,
						&mut matched_transactions,
					);

					// Check for event match conditions
					self.find_matching_events_for_transaction(
						receipt,
						monitor,
						&mut matched_events,
						&mut matched_on_args,
						&mut involved_addresses,
					)
					.await;

					// Check function match conditions
					self.find_matching_functions_for_transaction(
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
						let has_function_match = !monitor_conditions.functions.is_empty()
							&& !matched_functions.is_empty();
						let has_transaction_match = !monitor_conditions.transactions.is_empty()
							&& !matched_transactions.is_empty();

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
										.map(|addr| AddressWithABI {
											abi: None,
											..addr.clone()
										})
										.collect(),
									..monitor.clone()
								},
								transaction: EVMTransaction::from(transaction.clone()),
								receipt: receipt.clone(),
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
		}

		Ok(matching_results)
	}
}
