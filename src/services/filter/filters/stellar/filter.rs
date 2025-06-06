//! Stellar blockchain filter implementation for processing and matching blockchain events.
//!
//! This module provides functionality to:
//! - Filter and match Stellar blockchain transactions against monitor conditions
//! - Process and decode Stellar events
//! - Compare different types of parameter values
//! - Evaluate complex matching expressions

use std::marker::PhantomData;

use async_trait::async_trait;
use base64::Engine;
use serde_json::Value;
use stellar_xdr::curr::{FeeBumpTransactionInnerTx, OperationBody, TransactionEnvelope};
use tracing::instrument;

use crate::{
	models::{
		BlockType, ContractSpec, EventCondition, FunctionCondition, MatchConditions, Monitor,
		MonitorMatch, Network, StellarContractFunction, StellarEvent, StellarFormattedContractSpec,
		StellarMatchArguments, StellarMatchParamEntry, StellarMatchParamsMap, StellarMonitorMatch,
		StellarTransaction, TransactionCondition, TransactionStatus,
	},
	services::{
		blockchain::{BlockChainClient, StellarClientTrait},
		filter::{
			expression::{self, EvaluationError},
			filters::stellar::evaluator::StellarConditionEvaluator,
			stellar_helpers::{
				are_same_signature, get_kind_from_value, normalize_address, parse_xdr_value,
				process_invoke_host_function,
			},
			BlockFilter, FilterError,
		},
	},
};

/// Represents a mapping between a Stellar event and its transaction hash
#[derive(Debug)]
pub struct EventMap {
	pub event: StellarMatchParamsMap,
	pub tx_hash: String,
}

/// Implementation of the block filter for Stellar blockchain
pub struct StellarBlockFilter<T> {
	pub _client: PhantomData<T>,
}

impl<T> StellarBlockFilter<T> {
	/// Finds matching transactions based on monitor conditions
	///
	/// # Arguments
	/// * `transaction` - The Stellar transaction to check
	/// * `monitor` - The monitor containing match conditions
	/// * `matched_transactions` - Vector to store matching transactions
	pub fn find_matching_transaction(
		&self,
		transaction: &StellarTransaction,
		monitor: &Monitor,
		matched_transactions: &mut Vec<TransactionCondition>,
	) {
		let tx_status: TransactionStatus = match transaction.status.as_str() {
			"SUCCESS" => TransactionStatus::Success,
			"FAILED" => TransactionStatus::Failure,
			"NOT FOUND" => TransactionStatus::Failure,
			_ => TransactionStatus::Any,
		};

		struct TxOperation {
			_operation_type: String,
			sender: String,
			receiver: String,
			value: Option<String>,
		}

		let mut tx_operations: Vec<TxOperation> = vec![];

		if let Some(decoded) = transaction.decoded() {
			if let Some(TransactionEnvelope::Tx(tx)) = &decoded.envelope {
				let from = tx.tx.source_account.to_string();
				for operation in tx.tx.operations.iter() {
					match &operation.body {
						OperationBody::Payment(payment) => {
							let operation = TxOperation {
								_operation_type: "payment".to_string(),
								sender: from.clone(),
								receiver: payment.destination.to_string(),
								value: Some(payment.amount.to_string()),
							};
							tx_operations.push(operation);
						}
						OperationBody::InvokeHostFunction(invoke_host_function) => {
							let parsed_operation =
								process_invoke_host_function(invoke_host_function, None);
							let operation = TxOperation {
								_operation_type: "invoke_host_function".to_string(),
								sender: from.clone(),
								receiver: parsed_operation.0.contract_address.clone(),
								value: None,
							};
							tx_operations.push(operation);
						}
						_ => {}
					}
				}
			}
		}

		// Check transaction match conditions
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
					required_status => *required_status == tx_status,
				};

				if status_matches {
					if let Some(expr) = &condition.expression {
						// Create base transaction parameters outside operation loop
						let base_params = vec![
							StellarMatchParamEntry {
								name: "hash".to_string(),
								value: transaction.hash().clone(),
								kind: "string".to_string(),
								indexed: false,
							},
							StellarMatchParamEntry {
								name: "ledger".to_string(),
								value: transaction.ledger.to_string(),
								kind: "i64".to_string(),
								indexed: false,
							},
							// Default value for value
							StellarMatchParamEntry {
								name: "value".to_string(),
								value: "0".to_string(),
								kind: "i64".to_string(),
								indexed: false,
							},
						];

						// If we have operations, check each one
						if !tx_operations.is_empty() {
							for operation in &tx_operations {
								let mut tx_params = base_params.clone();
								// Remove default value for value
								tx_params.remove(tx_params.len() - 1);
								tx_params.extend(vec![
									StellarMatchParamEntry {
										name: "value".to_string(),
										value: operation.value.clone().unwrap_or("0".to_string()),
										kind: "i64".to_string(),
										indexed: false,
									},
									StellarMatchParamEntry {
										name: "from".to_string(),
										value: operation.sender.clone(),
										kind: "address".to_string(),
										indexed: false,
									},
									StellarMatchParamEntry {
										name: "to".to_string(),
										value: operation.receiver.clone(),
										kind: "address".to_string(),
										indexed: false,
									},
								]);

								// Evaluate the expression with transaction parameters
								match self.evaluate_expression(expr, &tx_params) {
									Ok(true) => {
										matched_transactions.push(TransactionCondition {
											expression: Some(expr.to_string()),
											status: tx_status,
										});
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
						} else {
							// Even with no operations, still evaluate base parameters
							match self.evaluate_expression(expr, &base_params) {
								Ok(true) => {
									matched_transactions.push(TransactionCondition {
										expression: Some(expr.to_string()),
										status: tx_status,
									});
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
					} else {
						// No expression but status matched
						matched_transactions.push(TransactionCondition {
							expression: None,
							status: tx_status,
						});
						break;
					}
				}
			}
		}
	}

	/// Converts Stellar function arguments into match parameter entries
	///
	/// # Arguments
	/// * `arguments` - Vector of argument values to convert
	/// * `function_spec` - Optional function specification for named parameters
	///
	/// # Returns
	/// Vector of converted parameter entries
	pub fn convert_arguments_to_match_param_entry(
		&self,
		arguments: &[Value],
		function_spec: Option<&StellarContractFunction>,
	) -> Vec<StellarMatchParamEntry> {
		let mut params = Vec::new();
		for (index, arg) in arguments.iter().enumerate() {
			// Try to get parameter name and type from function spec if available
			let (param_name, param_type) = if let Some(spec) = function_spec {
				if let Some(input) = spec.inputs.get(index) {
					(input.name.clone(), input.kind.clone())
				} else {
					(index.to_string(), get_kind_from_value(arg))
				}
			} else {
				(index.to_string(), get_kind_from_value(arg))
			};

			match arg {
				Value::Array(array) => {
					params.push(StellarMatchParamEntry {
						name: param_name,
						kind: "Vec".to_string(),
						value: serde_json::to_string(array).unwrap_or_default(),
						indexed: false,
					});
				}
				Value::Object(map) => {
					if let (Some(Value::String(type_str)), Some(Value::String(value))) =
						(map.get("type"), map.get("value"))
					{
						params.push(StellarMatchParamEntry {
							name: param_name,
							kind: type_str.clone(),
							value: value.clone(),
							indexed: false,
						});
					} else {
						params.push(StellarMatchParamEntry {
							name: param_name,
							kind: "Map".to_string(),
							value: serde_json::to_string(map).unwrap_or_default(),
							indexed: false,
						});
					}
				}
				_ => {
					params.push(StellarMatchParamEntry {
						name: param_name,
						kind: param_type,
						value: match arg {
							Value::String(s) => s.clone(),
							Value::Number(n) => n.to_string(),
							Value::Bool(b) => b.to_string(),
							_ => arg.as_str().unwrap_or("").to_string(),
						},
						indexed: false,
					});
				}
			}
		}

		params
	}

	/// Finds matching functions within a transaction
	///
	/// # Arguments
	/// * `monitored_addresses` - List of addresses being monitored
	/// * `contract_specs` - List of contract specifications (SEP-48 ABIs) for monitored addresses
	/// * `transaction` - The transaction to check
	/// * `monitor` - The monitor containing match conditions
	/// * `matched_functions` - Vector to store matching functions
	/// * `matched_on_args` - Arguments that matched the conditions
	pub fn find_matching_functions_for_transaction(
		&self,
		monitored_addresses: &[String],
		contract_specs: &[(String, StellarFormattedContractSpec)],
		transaction: &StellarTransaction,
		monitor: &Monitor,
		matched_functions: &mut Vec<FunctionCondition>,
		matched_on_args: &mut StellarMatchArguments,
	) {
		let mut handle_operations = |tx: &Option<TransactionEnvelope>| {
			let tx_to_process = match tx {
				Some(TransactionEnvelope::Tx(tx)) => tx,
				Some(TransactionEnvelope::TxFeeBump(tx_fee_bump)) => {
					match &tx_fee_bump.tx.inner_tx {
						FeeBumpTransactionInnerTx::Tx(inner_tx) => inner_tx,
					}
				}
				_ => {
					return;
				}
			};

			for operation in tx_to_process.tx.operations.iter() {
				if let OperationBody::InvokeHostFunction(invoke_host_function) = &operation.body {
					let (parsed_operation, contract_spec) =
						process_invoke_host_function(invoke_host_function, Some(contract_specs));

					// Skip if contract address doesn't match
					if !monitored_addresses
						.contains(&normalize_address(&parsed_operation.contract_address))
					{
						continue;
					}

					if let Some(contract_spec) = contract_spec {
						// Get function spec from contract spec
						let function_spec = match contract_spec.functions.iter().find(|f| {
							are_same_signature(&f.signature, &parsed_operation.function_signature)
						}) {
							Some(spec) => spec,
							None => {
								tracing::debug!(
									"No matching function spec found for {} with signature {}",
									parsed_operation.function_name,
									parsed_operation.function_signature
								);
								continue;
							}
						};

						// Convert parsed operation arguments into param entries using function spec
						let param_entries = self.convert_arguments_to_match_param_entry(
							&parsed_operation.arguments,
							Some(function_spec),
						);

						if monitor.match_conditions.functions.is_empty() {
							// Match on all functions
							matched_functions.push(FunctionCondition {
								signature: parsed_operation.function_signature.clone(),
								expression: None,
							});
							if let Some(functions) = &mut matched_on_args.functions {
								functions.push(StellarMatchParamsMap {
									signature: parsed_operation.function_signature.clone(),
									args: Some(param_entries),
								});
							}
						} else {
							// Check function conditions
							for condition in &monitor.match_conditions.functions {
								// Check if function signature matches
								if are_same_signature(
									&condition.signature,
									&parsed_operation.function_signature,
								) {
									// Evaluate expression if it exists
									if let Some(expr) = &condition.expression {
										match self.evaluate_expression(expr, &param_entries) {
											Ok(true) => {
												matched_functions.push(FunctionCondition {
													signature: parsed_operation
														.function_signature
														.clone(),
													expression: Some(expr.clone()),
												});
												if let Some(functions) =
													&mut matched_on_args.functions
												{
													functions.push(StellarMatchParamsMap {
														signature: parsed_operation
															.function_signature
															.clone(),
														args: Some(param_entries.clone()),
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
										// If no expression, match on function name alone
										matched_functions.push(FunctionCondition {
											signature: parsed_operation.function_signature.clone(),
											expression: None,
										});
										if let Some(functions) = &mut matched_on_args.functions {
											functions.push(StellarMatchParamsMap {
												signature: parsed_operation
													.function_signature
													.clone(),
												args: Some(param_entries.clone()),
											});
										}
										break;
									}
								}
							}
						}
					} else {
						tracing::error!(
							"No contract spec found for {}",
							parsed_operation.contract_address
						);
						continue;
					}
				}
			}
		};

		if let Some(decoded) = transaction.decoded() {
			handle_operations(&decoded.envelope);
		}
	}

	/// Finds matching events for a transaction
	///
	/// # Arguments
	/// * `events` - List of decoded events
	/// * `transaction` - The transaction to check
	/// * `monitor` - The monitor containing match conditions
	/// * `matched_events` - Vector to store matching events
	/// * `matched_on_args` - Arguments that matched the conditions
	pub fn find_matching_events_for_transaction(
		&self,
		events: &[EventMap],
		transaction: &StellarTransaction,
		monitor: &Monitor,
		matched_events: &mut Vec<EventCondition>,
		matched_on_args: &mut StellarMatchArguments,
	) {
		let events_for_transaction = events
			.iter()
			.filter(|event| event.tx_hash == *transaction.hash())
			.map(|event| event.event.clone())
			.collect::<Vec<_>>();

		// Check event conditions
		for event in &events_for_transaction {
			if monitor.match_conditions.events.is_empty() {
				// Match all events
				matched_events.push(EventCondition {
					signature: event.signature.clone(),
					expression: None,
				});
				if let Some(events) = &mut matched_on_args.events {
					events.push(event.clone());
				}
			} else {
				// Find all matching conditions for this event
				let matching_conditions =
					monitor.match_conditions.events.iter().filter(|condition| {
						are_same_signature(&condition.signature, &event.signature)
					});

				for condition in matching_conditions {
					match &condition.expression {
						Some(expr) => {
							if let Some(args) = &event.args {
								match self.evaluate_expression(expr, args) {
									Ok(true) => {
										matched_events.push(EventCondition {
											signature: event.signature.clone(),
											expression: Some(expr.clone()),
										});
										if let Some(events) = &mut matched_on_args.events {
											events.push(event.clone());
										}
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
						None => {
							matched_events.push(EventCondition {
								signature: event.signature.clone(),
								expression: None,
							});
						}
					}
				}
			}
		}
	}

	/// Decodes Stellar events into a more processable format
	///
	/// # Arguments
	/// * `events` - Raw Stellar events to decode
	/// * `monitored_addresses` - List of addresses being monitored
	///
	/// # Returns
	/// Vector of decoded events mapped to their transaction hashes
	pub fn decode_events(
		&self,
		events: &Vec<StellarEvent>,
		monitored_addresses: &[String],
		_contract_specs: &[(String, StellarFormattedContractSpec)],
	) -> Vec<EventMap> {
		let mut decoded_events = Vec::new();
		for event in events {
			// Skip if contract address doesn't match
			if !monitored_addresses.contains(&normalize_address(&event.contract_id)) {
				continue;
			}

			// Get contract spec for the event
			// Events are not yet supported in SEP-48
			// let contract_spec = contract_specs
			// 	.iter()
			// 	.find(|(addr, _)| addr == &event.contract_id)
			// 	.map(|(_, spec)| spec)
			// 	.unwrap();

			let topics = match &event.topic_xdr {
				Some(topics) => topics,
				None => {
					tracing::warn!("No topics found in event");
					continue;
				}
			};

			// Decode base64 event name
			let event_name = match base64::engine::general_purpose::STANDARD.decode(&topics[0]) {
				Ok(bytes) => {
					// Skip the first 4 bytes (size) and the next 4 bytes (type)
					if bytes.len() >= 8 {
						match String::from_utf8(bytes[8..].to_vec()) {
							Ok(name) => name.trim_matches(char::from(0)).to_string(),
							Err(e) => {
								tracing::warn!("Failed to decode event name as UTF-8: {}", e);
								continue;
							}
						}
					} else {
						tracing::warn!("Event name bytes too short: {}", bytes.len());
						continue;
					}
				}
				Err(e) => {
					tracing::warn!("Failed to decode base64 event name: {}", e);
					continue;
				}
			};

			// Process indexed parameters from topics
			let mut indexed_args = Vec::new();
			for topic in topics.iter().skip(1) {
				match base64::engine::general_purpose::STANDARD.decode(topic) {
					Ok(bytes) => {
						if let Some(param_entry) = parse_xdr_value(&bytes, true) {
							indexed_args.push(param_entry);
						}
					}
					Err(e) => {
						tracing::warn!("Failed to decode base64 topic: {}", e);
						continue;
					}
				}
			}

			// Process non-indexed parameters from value field
			let mut value_args = Vec::new();
			if let Some(value_xdr) = &event.value_xdr {
				match base64::engine::general_purpose::STANDARD.decode(value_xdr) {
					Ok(bytes) => {
						if let Some(entry) = parse_xdr_value(&bytes, false) {
							value_args.push(entry);
						}
					}
					Err(e) => {
						tracing::warn!("Failed to decode base64 event value: {}", e);
						continue;
					}
				}
			}

			let event_signature = format!(
				"{}({}{})",
				event_name,
				indexed_args
					.iter()
					.map(|arg| arg.kind.clone())
					.collect::<Vec<String>>()
					.join(","),
				if !value_args.is_empty() {
					// Only add a comma if there were indexed args
					if !indexed_args.is_empty() {
						format!(
							",{}",
							value_args
								.iter()
								.map(|arg| arg.kind.clone())
								.collect::<Vec<String>>()
								.join(",")
						)
					} else {
						// No comma needed if there were no indexed args
						value_args
							.iter()
							.map(|arg| arg.kind.clone())
							.collect::<Vec<String>>()
							.join(",")
					}
				} else {
					String::new()
				}
			);

			let decoded_event = StellarMatchParamsMap {
				signature: event_signature,
				args: Some(
					[&indexed_args[..], &value_args[..]]
						.concat()
						.iter()
						.enumerate()
						.map(|(i, arg)| StellarMatchParamEntry {
							kind: arg.kind.clone(),
							value: arg.value.clone(),
							indexed: arg.indexed,
							name: i.to_string(),
						})
						.collect(),
				),
			};

			decoded_events.push(EventMap {
				event: decoded_event,
				tx_hash: event.transaction_hash.clone(),
			});
		}

		decoded_events
	}

	/// Evaluates a complex matching expression against provided arguments
	///
	/// # Arguments
	/// * `expression` - The expression to evaluate (supports AND/OR operations)
	/// * `args` - The arguments to evaluate against
	///
	/// # Returns
	/// Boolean indicating if the expression evaluates to true
	pub fn evaluate_expression(
		&self,
		expression: &str,
		args: &[StellarMatchParamEntry],
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

		let evaluator = StellarConditionEvaluator::new(args);

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
}

#[async_trait]
impl<T: BlockChainClient + StellarClientTrait> BlockFilter for StellarBlockFilter<T> {
	type Client = T;
	/// Filters a Stellar block against provided monitors
	///
	/// # Arguments
	/// * `client` - The blockchain client to use
	/// * `_network` - The network being monitored
	/// * `block` - The block to filter
	/// * `monitors` - List of monitors to check against
	/// * `contract_specs` - List of contract specs to use for decoding events
	///
	/// # Returns
	/// Result containing vector of matching monitors or a filter error
	#[instrument(skip_all, fields(network = %network.slug))]
	async fn filter_block(
		&self,
		client: &Self::Client,
		network: &Network,
		block: &BlockType,
		monitors: &[Monitor],
		contract_specs: Option<&[(String, ContractSpec)]>,
	) -> Result<Vec<MonitorMatch>, FilterError> {
		let stellar_block = match block {
			BlockType::Stellar(block) => block,
			_ => {
				return Err(FilterError::block_type_mismatch(
					"Expected Stellar block".to_string(),
					None,
					None,
				));
			}
		};

		let transactions = match client.get_transactions(stellar_block.sequence, None).await {
			Ok(transactions) => transactions,
			Err(e) => {
				return Err(FilterError::network_error(
					format!(
						"Failed to get transactions for block {}",
						stellar_block.sequence
					),
					Some(e.into()),
					None,
				));
			}
		};

		if transactions.is_empty() {
			tracing::debug!("No transactions found for block {}", stellar_block.sequence);
			return Ok(vec![]);
		}

		tracing::debug!("Processing {} transaction(s)", transactions.len());

		let events = match client.get_events(stellar_block.sequence, None).await {
			Ok(events) => events,
			Err(e) => {
				return Err(FilterError::network_error(
					format!("Failed to get events for block {}", stellar_block.sequence),
					Some(e.into()),
					None,
				));
			}
		};

		tracing::debug!("Processing {} event(s)", events.len());
		tracing::debug!("Processing {} monitor(s)", monitors.len());

		let mut matching_results = Vec::new();

		// Cast contract specs to StellarContractSpec
		let contract_specs = contract_specs
			.unwrap_or(&[])
			.iter()
			.filter_map(|(address, spec)| match spec {
				ContractSpec::Stellar(spec) => Some((
					address.clone(),
					StellarFormattedContractSpec::from(spec.clone()),
				)),
				_ => None,
			})
			.collect::<Vec<(String, StellarFormattedContractSpec)>>();

		// Process each monitor first
		for monitor in monitors {
			tracing::debug!("Processing monitor: {}", monitor.name);

			let monitored_addresses = monitor
				.addresses
				.iter()
				.map(|addr| normalize_address(&addr.address))
				.collect::<Vec<String>>();

			let decoded_events = self.decode_events(&events, &monitored_addresses, &contract_specs);

			// Then process transactions for this monitor
			for transaction in &transactions {
				let mut matched_transactions = Vec::<TransactionCondition>::new();
				let mut matched_functions = Vec::<FunctionCondition>::new();
				let mut matched_events = Vec::<EventCondition>::new();
				let mut matched_on_args = StellarMatchArguments {
					events: Some(Vec::new()),
					functions: Some(Vec::new()),
				};

				tracing::debug!("Processing transaction: {:?}", transaction.hash());

				self.find_matching_transaction(transaction, monitor, &mut matched_transactions);

				// Decoded events already account for monitored addresses, so no need to pass in
				// monitored_addresses
				self.find_matching_events_for_transaction(
					&decoded_events,
					transaction,
					monitor,
					&mut matched_events,
					&mut matched_on_args,
				);

				self.find_matching_functions_for_transaction(
					&monitored_addresses,
					&contract_specs,
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
					matching_results.push(MonitorMatch::Stellar(Box::new(StellarMonitorMatch {
						monitor: monitor.clone(),
						// The conversion to StellarTransaction triggers decoding of the transaction
						#[allow(clippy::useless_conversion)]
						transaction: StellarTransaction::from(transaction.clone()),
						ledger: *stellar_block.clone(),
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
						matched_on_args: Some(StellarMatchArguments {
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		models::{
			AddressWithSpec, MatchConditions, Monitor, StellarContractInput,
			StellarDecodedTransaction, StellarFormattedContractSpec, StellarTransaction,
			StellarTransactionInfo, TransactionStatus,
		},
		utils::tests::stellar::monitor::MonitorBuilder,
	};
	use serde_json::json;
	use stellar_strkey::ed25519::PublicKey as StrPublicKey;

	use base64::engine::general_purpose::STANDARD as BASE64;
	use stellar_xdr::curr::{
		Asset, FeeBumpTransaction, FeeBumpTransactionEnvelope, FeeBumpTransactionExt, Hash,
		HostFunction, InvokeContractArgs, InvokeHostFunctionOp, MuxedAccount, Operation,
		OperationBody, PaymentOp, ScAddress, ScString, ScSymbol, ScVal, SequenceNumber, StringM,
		Transaction, TransactionEnvelope, TransactionV1Envelope, Uint256, VecM,
	};

	fn create_test_filter() -> StellarBlockFilter<()> {
		StellarBlockFilter::<()> {
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
			.networks(vec!["stellar_mainnet".to_string()])
			.paused(false)
			.addresses_with_spec(
				addresses
					.iter()
					.map(|a| (a.address.clone(), a.contract_spec.clone()))
					.collect(),
			)
			.match_conditions(MatchConditions {
				events: event_conditions,
				functions: function_conditions,
				transactions: transaction_conditions,
			})
			.build()
	}

	/// Creates a mock transaction for testing
	#[allow(clippy::too_many_arguments)]
	fn create_test_transaction(
		status: &str,
		transaction_hash: &str,
		application_order: i32,
		amount: Option<&str>,
		from: Option<&str>,
		to: Option<&str>,
		operation_type: Option<&str>,
		is_fee_bump: bool,
	) -> StellarTransaction {
		let sender = if let Some(from_addr) = from {
			StrPublicKey::from_string(from_addr)
				.map(|key| MuxedAccount::Ed25519(Uint256(key.0)))
				.unwrap_or_else(|_| MuxedAccount::Ed25519(Uint256([1; 32])))
		} else {
			MuxedAccount::Ed25519(Uint256([1; 32]))
		};

		let receiver = if let Some(to_addr) = to {
			StrPublicKey::from_string(to_addr)
				.map(|key| MuxedAccount::Ed25519(Uint256(key.0)))
				.unwrap_or_else(|_| MuxedAccount::Ed25519(Uint256([2; 32])))
		} else {
			MuxedAccount::Ed25519(Uint256([2; 32]))
		};

		let payment_amount = amount.and_then(|a| a.parse::<i64>().ok()).unwrap_or(100);

		// Create operation based on type
		let operation_body = match operation_type {
			Some("invoke_host_function") => {
				// Create a mock host function call with proper signature format
				let function_name = ScSymbol("mock_function".try_into().unwrap());
				let args = VecM::try_from(vec![
					ScVal::I32(123),
					ScVal::String(ScString::from(StringM::try_from("test").unwrap())),
				])
				.unwrap();

				// Create contract address from the provided address
				let contract_address = if let Some(_addr) = to {
					// Convert Stellar address to ScAddress
					let bytes = [0u8; 32]; // Initialize with zeros
					ScAddress::Contract(Hash(bytes))
				} else {
					// Default contract address
					let bytes = [0u8; 32];
					ScAddress::Contract(Hash(bytes))
				};

				OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
					host_function: HostFunction::InvokeContract(InvokeContractArgs {
						contract_address,
						function_name,
						args,
					}),
					auth: Default::default(),
				})
			}
			_ => {
				// Default to payment operation
				OperationBody::Payment(PaymentOp {
					destination: receiver.clone(),
					asset: Asset::Native,
					amount: payment_amount,
				})
			}
		};

		let operation = Operation {
			source_account: None,
			body: operation_body,
		};

		// Construct the transaction
		let tx = Transaction {
			source_account: sender.clone(),
			fee: 100,
			seq_num: SequenceNumber::from(4384801150),
			operations: vec![operation].try_into().unwrap(),
			cond: stellar_xdr::curr::Preconditions::None,
			ext: stellar_xdr::curr::TransactionExt::V0,
			memo: stellar_xdr::curr::Memo::None,
		};

		let tx_envelope = TransactionV1Envelope {
			tx,
			signatures: Default::default(),
		};

		let envelope = if is_fee_bump {
			TransactionEnvelope::TxFeeBump(FeeBumpTransactionEnvelope {
				tx: FeeBumpTransaction {
					fee_source: MuxedAccount::Ed25519(Uint256([0; 32])),
					fee: 100,
					inner_tx: FeeBumpTransactionInnerTx::Tx(tx_envelope),
					ext: FeeBumpTransactionExt::V0,
				},
				signatures: Default::default(),
			})
		} else {
			TransactionEnvelope::Tx(tx_envelope)
		};

		// Create the transaction info with appropriate JSON based on operation type
		let envelope_json = match operation_type {
			Some("invoke_host_function") => json!({
				"type": "ENVELOPE_TYPE_TX",
				"tx": {
					"sourceAccount": from.unwrap_or("GCXKG6RN4ONIEPCMNFB732A436Z5PNDSRLGWK7GBLCMQLIFO4S7EYWVU"),
					"fee": 100,
					"seqNum": "4384801150",
					"operations": [{
						"type": "invokeHostFunction",
						"sourceAccount": from.unwrap_or("GCXKG6RN4ONIEPCMNFB732A436Z5PNDSRLGWK7GBLCMQLIFO4S7EYWVU"),
						"function": "mock_function",
						"parameters": [123, "test"]
					}]
				}
			}),
			_ => json!({
				"type": "ENVELOPE_TYPE_TX",
				"tx": {
					"sourceAccount": from.unwrap_or("GCXKG6RN4ONIEPCMNFB732A436Z5PNDSRLGWK7GBLCMQLIFO4S7EYWVU"),
					"fee": 100,
					"seqNum": "4384801150",
					"operations": [{
						"type": "payment",
						"sourceAccount": from.unwrap_or("GCXKG6RN4ONIEPCMNFB732A436Z5PNDSRLGWK7GBLCMQLIFO4S7EYWVU"),
						"destination": to.unwrap_or("GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI"),
						"asset": {
							"type": "native"
						},
						"amount": amount.unwrap_or("100")
					}]
				}
			}),
		};

		// Create the transaction info
		let tx_info = StellarTransactionInfo {
			status: status.to_string(),
			transaction_hash: transaction_hash.to_string(),
			application_order,
			fee_bump: false,
			envelope_xdr: Some(base64::engine::general_purpose::STANDARD.encode("mock_xdr")),
			envelope_json: Some(envelope_json),
			result_xdr: Some(base64::engine::general_purpose::STANDARD.encode("mock_result")),
			result_json: None,
			result_meta_xdr: Some(base64::engine::general_purpose::STANDARD.encode("mock_meta")),
			result_meta_json: None,
			diagnostic_events_xdr: None,
			diagnostic_events_json: None,
			ledger: 1,
			ledger_close_time: 0,
			decoded: Some(StellarDecodedTransaction {
				envelope: Some(envelope),
				result: None,
				meta: None,
			}),
		};

		// Return the wrapped transaction
		StellarTransaction(tx_info)
	}

	/// Creates a test event for testing
	fn create_test_event(
		tx_hash: &str,
		event_signature: &str,
		args: Option<Vec<StellarMatchParamEntry>>,
	) -> EventMap {
		EventMap {
			event: StellarMatchParamsMap {
				signature: event_signature.to_string(),
				args,
			},
			tx_hash: tx_hash.to_string(),
		}
	}

	// Helper function to create a basic StellarEvent
	fn create_test_stellar_event(
		contract_id: &str,
		tx_hash: &str,
		topics: Vec<String>,
		value: Option<String>,
	) -> StellarEvent {
		StellarEvent {
			contract_id: contract_id.to_string(),
			transaction_hash: tx_hash.to_string(),
			topic_xdr: Some(topics),
			value_xdr: value,
			event_type: "contract".to_string(),
			ledger: 0,
			ledger_closed_at: "0".to_string(),
			id: "0".to_string(),
			paging_token: "0".to_string(),
			in_successful_contract_call: true,
			topic_json: None,
			value_json: None,
		}
	}

	// Helper function to create base64 encoded event name
	fn encode_event_name(name: &str) -> String {
		// Create a buffer with 8 bytes prefix (4 for size, 4 for type) + name
		let mut buffer = vec![0u8; 8];
		buffer.extend_from_slice(name.as_bytes());
		BASE64.encode(buffer)
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for find_matching_transaction method:
	//////////////////////////////////////////////////////////////////////////////
	#[test]
	fn test_find_matching_transaction_empty_conditions_matches_all() {
		let filter = create_test_filter();
		let mut matched_transactions = Vec::new();

		let monitor = create_test_monitor(vec![], vec![], vec![], vec![]);
		let transaction = create_test_transaction(
			"SUCCESS",
			"3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889",
			1,
			None,
			None,
			None,
			None,
			false,
		);

		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 1);
		assert_eq!(matched_transactions[0].status, TransactionStatus::Any);
		assert!(matched_transactions[0].expression.is_none());
	}

	#[test]
	fn test_find_matching_transaction_status_match() {
		let filter = create_test_filter();
		let mut matched_transactions = Vec::new();
		let transaction = create_test_transaction(
			"SUCCESS",
			"3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889",
			1,
			None,
			None,
			None,
			None,
			false,
		);

		let monitor = create_test_monitor(
			vec![],
			vec![],
			vec![TransactionCondition {
				status: TransactionStatus::Success,
				expression: None,
			}],
			vec![],
		);

		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 1);
		assert_eq!(matched_transactions[0].status, TransactionStatus::Success);
		assert!(matched_transactions[0].expression.is_none());
	}

	#[test]
	fn test_find_matching_transaction_with_expression() {
		let filter = create_test_filter();
		let mut matched_transactions = Vec::new();
		let transaction = create_test_transaction(
			"SUCCESS",
			"3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889",
			1,
			Some("150"),
			None,
			None,
			None,
			false,
		);

		let monitor = create_test_monitor(
			vec![],
			vec![],
			vec![TransactionCondition {
				status: TransactionStatus::Success,
				expression: Some("value > 100".to_string()),
			}],
			vec![],
		);

		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 1);
		assert_eq!(matched_transactions[0].status, TransactionStatus::Success);
		assert_eq!(
			matched_transactions[0].expression.as_ref().unwrap(),
			"value > 100"
		);
	}

	#[test]
	fn test_find_matching_transaction_no_match() {
		let filter = create_test_filter();
		let mut matched_transactions = Vec::new();
		let transaction = create_test_transaction(
			"SUCCESS",
			"3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889",
			1,
			None,
			None,
			None,
			None,
			false,
		);

		let monitor = create_test_monitor(
			vec![],
			vec![],
			vec![TransactionCondition {
				status: TransactionStatus::Success,
				expression: Some("value > 1000000".to_string()),
			}],
			vec![],
		);

		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 0);
	}

	#[test]
	fn test_find_matching_transaction_status_mismatch() {
		let filter = create_test_filter();
		let mut matched_transactions = Vec::new();
		let transaction = create_test_transaction(
			"FAILED",
			"3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889",
			1,
			None,
			None,
			None,
			None,
			false,
		);

		let monitor = create_test_monitor(
			vec![],
			vec![],
			vec![TransactionCondition {
				status: TransactionStatus::Success,
				expression: None,
			}],
			vec![],
		);

		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 0);
	}

	#[test]
	fn test_find_matching_transaction_complex_expression() {
		let filter = create_test_filter();
		let mut matched_transactions = Vec::new();
		let transaction = create_test_transaction(
			"SUCCESS",
			"3389e9f0f1a65f19736cacf544c2e825313e8447f569233bb8db39aa607c8889",
			1,
			Some("120"),
			Some("GCXKG6RN4ONIEPCMNFB732A436Z5PNDSRLGWK7GBLCMQLIFO4S7EYWVU"),
			None,
			None,
			false,
		);

		let monitor = create_test_monitor(
			vec![],
			vec![],
			vec![TransactionCondition {
				status: TransactionStatus::Success,
				expression: Some(
					"value >= 100 AND from == \
					 GCXKG6RN4ONIEPCMNFB732A436Z5PNDSRLGWK7GBLCMQLIFO4S7EYWVU"
						.to_string(),
				),
			}],
			vec![],
		);

		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 1);
		assert_eq!(matched_transactions[0].status, TransactionStatus::Success);
		assert!(matched_transactions[0].expression.is_some());
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for find_matching_functions_for_transaction method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_find_matching_functions_empty_conditions_matches_all() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		// Use the Stellar format address
		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let normalized_contract_address = normalize_address(contract_address);

		// Create a transaction with an invoke_host_function operation targeting our contract
		let transaction = create_test_transaction(
			"SUCCESS",
			"hash123",
			1,
			None,
			None,
			Some(contract_address),
			Some("invoke_host_function"),
			false,
		);

		// Create monitor with empty function conditions but using normalized address
		let monitor = create_test_monitor(
			vec![],
			vec![],
			vec![],
			vec![AddressWithSpec {
				address: normalized_contract_address.clone(),
				contract_spec: None,
			}],
		);

		// Use normalized address in monitored addresses
		let monitored_addresses = vec![normalized_contract_address];

		// Add contract spec with mock function
		let contract_specs = vec![(
			contract_address.to_string(),
			StellarFormattedContractSpec {
				functions: vec![StellarContractFunction {
					name: "mock_function".to_string(),
					signature: "mock_function(I32,String)".to_string(),
					inputs: vec![
						StellarContractInput {
							name: "param1".to_string(),
							kind: "I32".to_string(),
							index: 0,
						},
						StellarContractInput {
							name: "param2".to_string(),
							kind: "String".to_string(),
							index: 1,
						},
					],
				}],
			},
		)];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
			&contract_specs,
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_args,
		);

		assert_eq!(matched_functions.len(), 1);
		assert!(matched_functions[0].expression.is_none(),);
		assert!(matched_functions[0].signature.contains("mock_function"),);
	}

	#[test]
	fn test_find_matching_functions_with_signature_match() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let normalized_contract_address = normalize_address(contract_address);

		// Create transaction with specific function signature
		let transaction = create_test_transaction(
			"SUCCESS",
			"hash123",
			1,
			None,
			None,
			Some(contract_address),
			Some("invoke_host_function"),
			false,
		);

		// Create monitor with matching function signature condition - match the full signature
		// from the operation
		let monitor = create_test_monitor(
			vec![],
			vec![FunctionCondition {
				signature: "mock_function(I32,String)".to_string(),
				expression: None,
			}],
			vec![],
			vec![AddressWithSpec {
				address: normalized_contract_address.clone(),
				contract_spec: None,
			}],
		);

		let monitored_addresses = vec![normalized_contract_address];
		let contract_specs = vec![(
			contract_address.to_string(),
			StellarFormattedContractSpec {
				functions: vec![StellarContractFunction {
					signature: "mock_function(I32,String)".to_string(),
					name: "mock_function".to_string(),
					inputs: vec![
						StellarContractInput {
							name: "param1".to_string(),
							kind: "I32".to_string(),
							index: 0,
						},
						StellarContractInput {
							name: "param2".to_string(),
							kind: "String".to_string(),
							index: 1,
						},
					],
				}],
			},
		)];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
			&contract_specs,
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_args,
		);

		assert_eq!(matched_functions.len(), 1);
		assert!(matched_functions[0].expression.is_none());
		assert_eq!(matched_functions[0].signature, "mock_function(I32,String)");
	}

	#[test]
	fn test_find_matching_functions_with_expression() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let normalized_contract_address = normalize_address(contract_address);

		let transaction = create_test_transaction(
			"SUCCESS",
			"hash123",
			1,
			None,
			None,
			Some(contract_address),
			Some("invoke_host_function"),
			false,
		);

		// Create monitor with function signature and expression
		let monitor = create_test_monitor(
			vec![],
			vec![FunctionCondition {
				signature: "mock_function(I32,String)".to_string(),
				expression: Some("0 < 50".to_string()),
			}],
			vec![],
			vec![AddressWithSpec {
				address: normalized_contract_address.clone(),
				contract_spec: None,
			}],
		);

		let monitored_addresses = vec![normalized_contract_address];
		let contract_specs = vec![(
			contract_address.to_string(),
			StellarFormattedContractSpec {
				functions: vec![StellarContractFunction {
					signature: "mock_function(I32,String)".to_string(),
					name: "mock_function".to_string(),
					inputs: vec![
						StellarContractInput {
							name: "param1".to_string(),
							kind: "I32".to_string(),
							index: 0,
						},
						StellarContractInput {
							name: "param2".to_string(),
							kind: "String".to_string(),
							index: 1,
						},
					],
				}],
			},
		)];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
			&contract_specs,
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_args,
		);

		// Now this assertion is correct since 123 is not less than 50
		assert_eq!(matched_functions.len(), 0);
	}

	#[test]
	fn test_find_matching_functions_address_mismatch() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let different_address = "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBSC4";
		let normalized_different_address = normalize_address(different_address);

		let transaction = create_test_transaction(
			"SUCCESS",
			"hash123",
			1,
			None,
			None,
			Some(contract_address),
			Some("invoke_host_function"),
			false,
		);

		// Create monitor with different address
		let monitor = create_test_monitor(
			vec![],
			vec![FunctionCondition {
				signature: "mock_function(i32,string)".to_string(),
				expression: None,
			}],
			vec![],
			vec![AddressWithSpec {
				address: normalized_different_address.clone(),
				contract_spec: None,
			}],
		);

		let monitored_addresses = vec![normalized_different_address];
		let contract_specs = vec![(
			contract_address.to_string(),
			StellarFormattedContractSpec {
				functions: vec![StellarContractFunction {
					signature: "mock_function(I32,String)".to_string(),
					name: "mock_function".to_string(),
					inputs: vec![
						StellarContractInput {
							name: "param1".to_string(),
							kind: "I32".to_string(),
							index: 0,
						},
						StellarContractInput {
							name: "param2".to_string(),
							kind: "String".to_string(),
							index: 1,
						},
					],
				}],
			},
		)];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
			&contract_specs,
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_args,
		);

		assert_eq!(matched_functions.len(), 0);
	}

	#[test]
	fn test_find_matching_functions_multiple_conditions() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let normalized_contract_address = normalize_address(contract_address);

		let transaction = create_test_transaction(
			"SUCCESS",
			"hash123",
			1,
			None,
			None,
			Some(contract_address),
			Some("invoke_host_function"),
			false,
		);

		// Create monitor with multiple function conditions
		let monitor = create_test_monitor(
			vec![],
			vec![
				FunctionCondition {
					signature: "wrong_function()".to_string(),
					expression: None,
				},
				FunctionCondition {
					signature: "mock_function(i32,string)".to_string(),
					expression: None,
				},
			],
			vec![],
			vec![AddressWithSpec {
				address: normalized_contract_address.clone(),
				contract_spec: None,
			}],
		);

		let monitored_addresses = vec![normalized_contract_address];
		let contract_specs = vec![(
			contract_address.to_string(),
			StellarFormattedContractSpec {
				functions: vec![StellarContractFunction {
					signature: "mock_function(I32,String)".to_string(),
					name: "mock_function".to_string(),
					inputs: vec![
						StellarContractInput {
							name: "param1".to_string(),
							kind: "I32".to_string(),
							index: 0,
						},
						StellarContractInput {
							name: "param2".to_string(),
							kind: "String".to_string(),
							index: 1,
						},
					],
				}],
			},
		)];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
			&contract_specs,
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_args,
		);

		assert_eq!(matched_functions.len(), 1);
		assert_eq!(matched_functions[0].signature, "mock_function(I32,String)");
	}

	#[test]
	fn test_find_matching_functions_with_fee_bump() {
		let filter = create_test_filter();
		let mut matched_functions = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let normalized_contract_address = normalize_address(contract_address);

		// Create transaction with specific function signature
		let transaction = create_test_transaction(
			"SUCCESS",
			"hash123",
			1,
			None,
			None,
			Some(contract_address),
			Some("invoke_host_function"),
			true,
		);

		// Create monitor with matching function signature condition - match the full signature
		// from the operation
		let monitor = create_test_monitor(
			vec![],
			vec![FunctionCondition {
				signature: "mock_function(I32,String)".to_string(),
				expression: None,
			}],
			vec![],
			vec![AddressWithSpec {
				address: normalized_contract_address.clone(),
				contract_spec: None,
			}],
		);

		let monitored_addresses = vec![normalized_contract_address];
		let contract_specs = vec![(
			contract_address.to_string(),
			StellarFormattedContractSpec {
				functions: vec![StellarContractFunction {
					signature: "mock_function(I32,String)".to_string(),
					name: "mock_function".to_string(),
					inputs: vec![
						StellarContractInput {
							name: "param1".to_string(),
							kind: "I32".to_string(),
							index: 0,
						},
						StellarContractInput {
							name: "param2".to_string(),
							kind: "String".to_string(),
							index: 1,
						},
					],
				}],
			},
		)];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
			&contract_specs,
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_args,
		);

		assert_eq!(matched_functions.len(), 1);
		assert!(matched_functions[0].expression.is_none());
		assert_eq!(matched_functions[0].signature, "mock_function(I32,String)");
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for find_matching_events_for_transaction method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_find_matching_events_empty_conditions_matches_all() {
		let filter = create_test_filter();
		let mut matched_events = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		// Create test transaction and event
		let transaction =
			create_test_transaction("SUCCESS", "tx_hash_123", 1, None, None, None, None, false);

		let test_event = create_test_event(
			"tx_hash_123",
			"Transfer(address,uint256)",
			Some(vec![
				StellarMatchParamEntry {
					name: "0".to_string(),
					value: "address1".to_string(),
					kind: "address".to_string(),
					indexed: true,
				},
				StellarMatchParamEntry {
					name: "1".to_string(),
					value: "100".to_string(),
					kind: "u256".to_string(),
					indexed: false,
				},
			]),
		);

		let events = vec![test_event];
		let monitor = create_test_monitor(vec![], vec![], vec![], vec![]);

		filter.find_matching_events_for_transaction(
			&events,
			&transaction,
			&monitor,
			&mut matched_events,
			&mut matched_args,
		);

		assert_eq!(matched_events.len(), 1);
		assert!(matched_events[0].expression.is_none());
		assert_eq!(matched_events[0].signature, "Transfer(address,uint256)");
		assert_eq!(matched_args.events.as_ref().unwrap().len(), 1);
	}

	#[test]
	fn test_find_matching_events_with_signature_match() {
		let filter = create_test_filter();
		let mut matched_events = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		let transaction =
			create_test_transaction("SUCCESS", "tx_hash_123", 1, None, None, None, None, false);

		let test_event = create_test_event(
			"tx_hash_123",
			"Transfer(address,uint256)",
			Some(vec![StellarMatchParamEntry {
				name: "0".to_string(),
				value: "address1".to_string(),
				kind: "address".to_string(),
				indexed: true,
			}]),
		);

		let events = vec![test_event];
		let monitor = create_test_monitor(
			vec![EventCondition {
				signature: "Transfer(address,uint256)".to_string(),
				expression: None,
			}],
			vec![],
			vec![],
			vec![],
		);

		filter.find_matching_events_for_transaction(
			&events,
			&transaction,
			&monitor,
			&mut matched_events,
			&mut matched_args,
		);

		assert_eq!(matched_events.len(), 1);
		assert!(matched_events[0].expression.is_none());
		assert_eq!(matched_events[0].signature, "Transfer(address,uint256)");
	}

	#[test]
	fn test_find_matching_events_with_expression() {
		let filter = create_test_filter();
		let mut matched_events = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		let transaction =
			create_test_transaction("SUCCESS", "tx_hash_123", 1, None, None, None, None, false);

		let test_event = create_test_event(
			"tx_hash_123",
			"Transfer(address,uint256)",
			Some(vec![StellarMatchParamEntry {
				name: "0".to_string(),
				value: "100".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			}]),
		);

		let events = vec![test_event];
		let monitor = create_test_monitor(
			vec![EventCondition {
				signature: "Transfer(address,uint256)".to_string(),
				expression: Some("0 > 50".to_string()),
			}],
			vec![],
			vec![],
			vec![],
		);

		filter.find_matching_events_for_transaction(
			&events,
			&transaction,
			&monitor,
			&mut matched_events,
			&mut matched_args,
		);

		assert_eq!(matched_events.len(), 1);
		assert_eq!(matched_events[0].expression.as_ref().unwrap(), "0 > 50");
		assert_eq!(matched_args.events.as_ref().unwrap().len(), 1);
	}

	#[test]
	fn test_find_matching_events_no_match() {
		let filter = create_test_filter();
		let mut matched_events = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		let transaction =
			create_test_transaction("SUCCESS", "tx_hash_123", 1, None, None, None, None, false);
		let test_event = create_test_event(
			"tx_hash_123",
			"Transfer(address,uint256)",
			Some(vec![StellarMatchParamEntry {
				name: "0".to_string(),
				value: "10".to_string(),
				kind: "u256".to_string(),
				indexed: true,
			}]),
		);

		let events = vec![test_event];
		let monitor = create_test_monitor(
			vec![EventCondition {
				signature: "Transfer(address,uint256)".to_string(),
				expression: Some("0 > 100".to_string()), // This won't match
			}],
			vec![],
			vec![],
			vec![],
		);

		filter.find_matching_events_for_transaction(
			&events,
			&transaction,
			&monitor,
			&mut matched_events,
			&mut matched_args,
		);

		assert_eq!(matched_events.len(), 0);
		assert_eq!(matched_args.events.as_ref().unwrap().len(), 0);
	}

	#[test]
	fn test_find_matching_events_wrong_transaction() {
		let filter = create_test_filter();
		let mut matched_events = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		let transaction =
			create_test_transaction("SUCCESS", "wrong_tx_hash", 1, None, None, None, None, false);

		let test_event = create_test_event(
			"tx_hash_123",
			"Transfer(address,uint256)",
			Some(vec![StellarMatchParamEntry {
				name: "0".to_string(),
				value: "100".to_string(),
				kind: "u256".to_string(),
				indexed: true,
			}]),
		);

		let events = vec![test_event];
		let monitor = create_test_monitor(
			vec![EventCondition {
				signature: "Transfer(address,uint256)".to_string(),
				expression: None,
			}],
			vec![],
			vec![],
			vec![],
		);

		filter.find_matching_events_for_transaction(
			&events,
			&transaction,
			&monitor,
			&mut matched_events,
			&mut matched_args,
		);

		assert_eq!(matched_events.len(), 0);
		assert_eq!(matched_args.events.as_ref().unwrap().len(), 0);
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for decode_event method:
	//////////////////////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_decode_events_basic_success() {
		let filter = create_test_filter();
		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let monitored_addresses = vec![normalize_address(contract_address)];

		// Create a test event with a simple Transfer event name and one parameter
		let event_name = encode_event_name("Transfer");
		// Encode a simple u32 value (100) in base64
		let value = BASE64.encode([0u8; 4]); // Simplified value encoding

		let event = create_test_stellar_event(
			contract_address,
			"tx_hash_123",
			vec![event_name],
			Some(value),
		);

		let events = vec![event];
		let contract_specs = vec![];
		let decoded = filter.decode_events(&events, &monitored_addresses, &contract_specs);

		assert_eq!(decoded.len(), 1);
		assert_eq!(decoded[0].tx_hash, "tx_hash_123");
		assert!(decoded[0].event.signature.starts_with("Transfer"));
	}

	#[tokio::test]
	async fn test_decode_events_address_mismatch() {
		let filter = create_test_filter();
		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let different_address = "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBSC4";
		let monitored_addresses = vec![normalize_address(different_address)];

		let event_name = encode_event_name("Transfer");
		let event =
			create_test_stellar_event(contract_address, "tx_hash_123", vec![event_name], None);

		let events = vec![event];
		let contract_specs = vec![];
		let decoded = filter.decode_events(&events, &monitored_addresses, &contract_specs);

		assert_eq!(decoded.len(), 0);
	}

	#[tokio::test]
	async fn test_decode_events_invalid_event_name() {
		let filter = create_test_filter();
		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let monitored_addresses = vec![normalize_address(contract_address)];

		// Create invalid base64 for event name
		let event = create_test_stellar_event(
			contract_address,
			"tx_hash_123",
			vec!["invalid_base64!!!".to_string()],
			None,
		);

		let events = vec![event];
		let contract_specs = vec![];
		let decoded = filter.decode_events(&events, &monitored_addresses, &contract_specs);

		assert_eq!(decoded.len(), 0);
	}

	#[tokio::test]
	async fn test_decode_events_with_indexed_and_value_args() {
		let filter = create_test_filter();
		let contract_address = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let monitored_addresses = vec![normalize_address(contract_address)];

		let event_name = encode_event_name("Transfer");

		// Create a proper XDR-encoded ScVal::Symbol for the first topic
		let mut symbol_bytes = vec![0, 0, 0, 10]; // discriminant for ScVal::Symbol
		symbol_bytes.extend_from_slice(b"address1"); // symbol value
		let symbol_topic = BASE64.encode(&symbol_bytes);

		// Create a proper XDR-encoded value for int64
		let mut value_bytes = vec![0, 0, 0, 6]; // discriminant for ScVal::I64
		value_bytes.extend_from_slice(&42i64.to_be_bytes()); // 8 bytes for int64
		let value = BASE64.encode(&value_bytes);

		let event = create_test_stellar_event(
			contract_address,
			"tx_hash_123",
			vec![event_name, symbol_topic],
			Some(value),
		);

		let events = vec![event];
		let contract_specs = vec![];
		let decoded = filter.decode_events(&events, &monitored_addresses, &contract_specs);

		assert_eq!(decoded.len(), 1);

		let decoded_event = &decoded[0].event;

		assert!(decoded_event.signature.starts_with("Transfer"));
		assert!(decoded_event.args.is_some());

		let args = decoded_event.args.as_ref().unwrap();

		assert_eq!(args.len(), 1);

		assert!(args[0].kind.contains("64"));
		assert_eq!(args[0].value, "42");
		assert!(!args[0].indexed);
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for evaluate_expression method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_evaluate_expression_regular_parameters() {
		let filter = create_test_filter();

		// Test setup with simple numeric parameters
		let args = vec![
			StellarMatchParamEntry {
				name: "amount".to_string(),
				value: "100".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "status".to_string(),
				value: "true".to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
		];

		// Test simple numeric comparison
		assert!(filter.evaluate_expression("amount > 50", &args).unwrap());
		assert!(!filter.evaluate_expression("amount < 50", &args).unwrap());
		assert!(filter.evaluate_expression("amount == 100", &args).unwrap());
		assert!(!filter.evaluate_expression("amount != 100", &args).unwrap());
		assert!(filter.evaluate_expression("amount >= 100", &args).unwrap());
		assert!(!filter.evaluate_expression("amount <= 50", &args).unwrap());
		assert!(filter.evaluate_expression("amount == 100", &args).unwrap());
		assert!(!filter.evaluate_expression("amount == 50", &args).unwrap());
		assert!(filter.evaluate_expression("amount != 50", &args).unwrap());
		assert!(!filter.evaluate_expression("amount != 100", &args).unwrap());

		// Test boolean comparison
		assert!(filter.evaluate_expression("status == true", &args).unwrap());
		assert!(!filter
			.evaluate_expression("status == false", &args)
			.unwrap());

		// Test non-existent parameter
		assert!(filter
			.evaluate_expression("invalid_param == 100", &args)
			.is_err());
	}

	#[test]
	fn test_evaluate_expression_string_comparisons() {
		let filter = create_test_filter();
		let args = vec![StellarMatchParamEntry {
			name: "name".to_string(),
			value: "Alice".to_string(),
			kind: "string".to_string(),
			indexed: false,
		}];

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
		let args = vec![StellarMatchParamEntry {
			name: "object".to_string(),
			value: r#"{"key1": "value", "key2": "100"}"#.to_string(),
			kind: "object".to_string(),
			indexed: false,
		}];

		// Test true conditions
		assert!(filter
			.evaluate_expression("object.key1 == 'value'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("object.key2 != '200'", &args)
			.unwrap());

		// Test false conditions
		assert!(!filter
			.evaluate_expression("object.key1 == 'wrong'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("object.key2 != '100'", &args)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_nested_field_access() {
		let filter = create_test_filter();
		let args = vec![StellarMatchParamEntry {
			name: "nested_object".to_string(),
			value: r#"{"outer": {"inner": "value"}}"#.to_string(),
			kind: "object".to_string(),
			indexed: false,
		}];

		// Test true conditions
		assert!(filter
			.evaluate_expression("nested_object.outer.inner == 'value'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("nested_object.outer.inner != 'wrong'", &args)
			.unwrap());

		// Test false conditions
		assert!(!filter
			.evaluate_expression("nested_object.outer.inner == 'wrong'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("nested_object.outer.inner != 'value'", &args)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_array_indexing() {
		let filter = create_test_filter();
		let args = vec![
			StellarMatchParamEntry {
				name: "array_str".to_string(),
				value: r#"["100", "200", "300", "test"]"#.to_string(),
				kind: "array".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "array_num".to_string(),
				value: r#"[100, 200, 300]"#.to_string(),
				kind: "array".to_string(),
				indexed: false,
			},
		];

		// Test true conditions
		assert!(filter
			.evaluate_expression("array_str[0] == '100'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("array_str[1] != '100'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("array_num[0] == 100", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("array_num[1] != 100", &args)
			.unwrap());

		// Test false conditions
		assert!(!filter
			.evaluate_expression("array_str[3] == '100'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("array_str[0] != '100'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("array_num[0] != 100", &args)
			.unwrap());

		// Test out-of-bounds access
		assert!(filter
			.evaluate_expression("array_num[3] == 100", &args)
			.is_err());
	}

	#[test]
	fn test_evaluate_expression_object_in_array() {
		let filter = create_test_filter();
		let args = vec![StellarMatchParamEntry {
			name: "objects".to_string(),
			value: r#"[{"name": "Alice"}, {"name": "Bob"}]"#.to_string(),
			kind: "array".to_string(),
			indexed: false,
		}];

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
	fn test_evaluate_expression_vec_csv_contains() {
		let filter = create_test_filter();
		let args = vec![StellarMatchParamEntry {
			name: "csv_list".to_string(),
			value: "apple,banana,cherry".to_string(),
			kind: "vec".to_string(),
			indexed: false,
		}];

		assert!(filter
			.evaluate_expression("csv_list contains 'banana'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("csv_list contains 'grape'", &args)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_vec_json_array_contains() {
		let filter = create_test_filter();
		let args = vec![StellarMatchParamEntry {
			name: "json_array_param".to_string(),
			value: r#"["alice", "bob"]"#.to_string(), // JSON array string
			kind: "vec".to_string(),
			indexed: false,
		}];

		assert!(filter
			.evaluate_expression("json_array_param contains 'alice'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("json_array_param contains 'charlie'", &args)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_vec_json_array_object_contains_field_value() {
		let filter = create_test_filter();
		let args = vec![StellarMatchParamEntry {
			name: "obj_array".to_string(),
			value: r#"[{"id": 1, "name": "alice"}, {"id": 2, "name": "bob"}]"#.to_string(),
			kind: "vec".to_string(),
			indexed: false,
		}];

		assert!(filter
			.evaluate_expression("obj_array contains 'alice'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("obj_array contains '2'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("obj_array contains 'charlie'", &args)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_vec_json_array_object_contains_nested_value_key() {
		let filter = create_test_filter();
		let args = vec![StellarMatchParamEntry {
			name: "nested_obj_array".to_string(),
			value: r#"[{"item": {"type": "name", "value": "alice"}}, {"item": {"type": "name", "value": "bob"}}]"#.to_string(),
			kind: "vec".to_string(),
			indexed: false,
		}];

		assert!(filter
			.evaluate_expression("nested_obj_array contains 'alice'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("nested_obj_array contains 'charlie'", &args)
			.unwrap());
		// "name" is a key or a value of "type", not directly a "value" field's content as per the logic
		assert!(!filter
			.evaluate_expression("nested_obj_array contains 'name'", &args)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_vec_eq_ne() {
		let filter = create_test_filter();
		let args_csv = vec![StellarMatchParamEntry {
			name: "csv_list".to_string(),
			value: "alice,bob".to_string(),
			kind: "vec".to_string(),
			indexed: false,
		}];
		let args_json_array = vec![StellarMatchParamEntry {
			name: "json_list".to_string(),
			value: r#"["alice", "bob"]"#.to_string(),
			kind: "vec".to_string(),
			indexed: false,
		}];

		// Eq/Ne on "vec" compares the raw string value
		assert!(filter
			.evaluate_expression("csv_list == 'alice,bob'", &args_csv)
			.unwrap());
		assert!(!filter
			.evaluate_expression("csv_list == 'alice,charlie'", &args_csv)
			.unwrap());
		assert!(filter
			.evaluate_expression("json_list == '[\"alice\", \"bob\"]'", &args_json_array)
			.unwrap());
		assert!(filter
			.evaluate_expression("json_list != '[\"alice\", \"charlie\"]'", &args_json_array)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_with_map_path_access() {
		let filter = create_test_filter();
		let args = vec![StellarMatchParamEntry {
			name: "map".to_string(),
			value: r#"{"prop1": "val1", "nested": {"prop2": 123}}"#.to_string(),
			kind: "Map".to_string(),
			indexed: false,
		}];

		assert!(filter
			.evaluate_expression("map.prop1 == 'val1'", &args)
			.unwrap());
		assert!(!filter
			.evaluate_expression("map.prop1 == 'wrong'", &args)
			.unwrap());
		assert!(filter
			.evaluate_expression("map.nested.prop2 == 123", &args)
			.unwrap());

		// Non-existent nested property
		assert!(filter
			.evaluate_expression("map.non_existent_key == 'anything'", &args)
			.is_err());
	}

	#[test]
	fn test_evaluate_expression_map_eq_ne_raw_json() {
		let filter = create_test_filter();
		let args_json_map = vec![StellarMatchParamEntry {
			name: "my_json_map".to_string(),
			value: r#"{"key1": "value1", "key2": "value2"}"#.to_string(),
			kind: "map".to_string(),
			indexed: false,
		}];

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
	fn test_evaluate_expression_logical_and_operator() {
		let filter = create_test_filter();
		let args_true_true = vec![
			StellarMatchParamEntry {
				name: "value".to_string(),
				value: "150".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "name".to_string(),
				value: "Alice".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
		];
		let args_true_false = vec![
			StellarMatchParamEntry {
				name: "value".to_string(),
				value: "150".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "name".to_string(),
				value: "Bob".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
		];
		let args_false_true = vec![
			StellarMatchParamEntry {
				name: "value".to_string(),
				value: "50".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "name".to_string(),
				value: "Alice".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
		];
		let args_false_false = vec![
			StellarMatchParamEntry {
				name: "value".to_string(),
				value: "50".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "name".to_string(),
				value: "Bob".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
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
			StellarMatchParamEntry {
				name: "value".to_string(),
				value: "150".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "name".to_string(),
				value: "Alice".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
		];
		let args_true_false = vec![
			StellarMatchParamEntry {
				name: "value".to_string(),
				value: "150".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "name".to_string(),
				value: "Bob".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
		];
		let args_false_true = vec![
			StellarMatchParamEntry {
				name: "value".to_string(),
				value: "50".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "name".to_string(),
				value: "Alice".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
		];
		let args_false_false = vec![
			StellarMatchParamEntry {
				name: "value".to_string(),
				value: "50".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "name".to_string(),
				value: "Bob".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
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
			StellarMatchParamEntry {
				name: "val1".to_string(),
				value: "10".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "str1".to_string(),
				value: "hello".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "bool1".to_string(),
				value: "true".to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
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
			StellarMatchParamEntry {
				name: "val1".to_string(),
				value: "10".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "str1".to_string(),
				value: "world".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "bool1".to_string(),
				value: "true".to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
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
			StellarMatchParamEntry {
				name: "val1".to_string(),
				value: "1".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "str1".to_string(),
				value: "world".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "bool1".to_string(),
				value: "false".to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
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
			StellarMatchParamEntry {
				name: "a".to_string(),
				value: "10".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "b".to_string(),
				value: "foo".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "c".to_string(),
				value: "false".to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
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
			StellarMatchParamEntry {
				name: "a".to_string(),
				value: "-5".to_string(),
				kind: "i64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "b".to_string(),
				value: "bar".to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "c".to_string(),
				value: "true".to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
		];
		assert!(filter
			.evaluate_expression("a > 0 AND b == 'bar' OR c == true", &args_f_t_t)
			.unwrap());

		// Case 10: (F OR T) AND T -> T AND T -> T
		assert!(filter
			.evaluate_expression("(a > 0 OR b == 'bar') AND c == true", &args_f_t_t)
			.unwrap());
	}

	#[test]
	fn test_evaluate_expression_event() {
		let filter = create_test_filter();

		let args = vec![
			StellarMatchParamEntry {
				name: "0".to_string(),
				value: "100".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "1".to_string(),
				value: "true".to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
		];

		assert!(filter.evaluate_expression("0 == 100", &args).unwrap());
		assert!(!filter.evaluate_expression("0 == 200", &args).unwrap());
		assert!(filter.evaluate_expression("1 == true", &args).unwrap());
		assert!(!filter.evaluate_expression("1 == false", &args).unwrap());
	}

	#[test]
	fn test_evaluate_expression_event_vector() {
		let filter = create_test_filter();

		let args = vec![StellarMatchParamEntry {
			name: "0".to_string(),
			value: r#"["100", "200", "300", "test"]"#.to_string(),
			kind: "array".to_string(),
			indexed: false,
		}];

		assert!(filter.evaluate_expression("0[0] == '100'", &args).unwrap());
		assert!(!filter.evaluate_expression("0[0] == '200'", &args).unwrap());
	}

	#[test]
	fn test_evaluate_expression_event_key_access() {
		let filter = create_test_filter();

		let args = vec![StellarMatchParamEntry {
			name: "0".to_string(),
			value: r#"{"0": "value", "1": "100"}"#.to_string(),
			kind: "Map".to_string(),
			indexed: false,
		}];

		assert!(filter.evaluate_expression("0.0 == 'value'", &args).unwrap());
		assert!(!filter.evaluate_expression("0.1 == '200'", &args).unwrap());
	}

	#[test]
	fn test_evaluate_expression_edge_cases() {
		let filter = create_test_filter();

		// Test with empty args
		assert!(filter.evaluate_expression("amount > 1000", &[]).is_err());

		// Test with invalid parameter name
		let args = vec![StellarMatchParamEntry {
			name: "amount".to_string(),
			value: "1000".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];
		assert!(filter
			.evaluate_expression("invalid_param > 1000", &args)
			.is_err());

		// Test with invalid operator
		assert!(filter
			.evaluate_expression("amount >>> 1000", &args)
			.is_err());

		// Test with invalid value format
		let args = vec![StellarMatchParamEntry {
			name: "amount".to_string(),
			value: "not_a_number".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];
		assert!(filter.evaluate_expression("amount > 1000", &args).is_err());

		// Test with unsupported parameter type
		let args = vec![StellarMatchParamEntry {
			name: "param".to_string(),
			value: "value".to_string(),
			kind: "unsupported_type".to_string(),
			indexed: false,
		}];
		assert!(filter.evaluate_expression("param == value", &args).is_err());
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for convert_arguments_to_match_param_entry method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_convert_primitive_values() {
		let filter = create_test_filter();

		let arguments = vec![
			// Use explicit type/value pairs with string values
			json!({
				"type": "U64",
				"value": "42"
			}),
			json!({
				"type": "I64",
				"value": "-42"
			}),
			// For bool and string, use type/value format consistently
			json!({
				"type": "Bool",
				"value": "true"
			}),
			json!({
				"type": "String",
				"value": "hello"
			}),
		];

		let function_spec = StellarContractFunction::default();
		let result =
			filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		assert_eq!(result.len(), 4);

		// Check U64
		assert_eq!(result[0].name, "0");
		assert_eq!(result[0].kind, "U64");
		assert_eq!(result[0].value, "42");
		assert!(!result[0].indexed);

		// Check I64
		assert_eq!(result[1].name, "1");
		assert_eq!(result[1].kind, "I64");
		assert_eq!(result[1].value, "-42");
		assert!(!result[1].indexed);

		// Check Bool
		assert_eq!(result[2].name, "2");
		assert_eq!(result[2].kind, "Bool");
		assert_eq!(result[2].value, "true");
		assert!(!result[2].indexed);

		// Check String
		assert_eq!(result[3].name, "3");
		assert_eq!(result[3].kind, "String");
		assert_eq!(result[3].value, "hello");
		assert!(!result[3].indexed);
	}

	#[test]
	fn test_convert_array_values() {
		let filter = create_test_filter();

		let arguments = vec![json!([1, 2, 3]), json!(["a", "b", "c"])];
		let function_spec = StellarContractFunction::default();
		let result =
			filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		assert_eq!(result.len(), 2);

		// Check first array
		assert_eq!(result[0].name, "0");
		assert_eq!(result[0].kind, "Vec");
		assert_eq!(result[0].value, "[1,2,3]");
		assert!(!result[0].indexed);

		// Check second array
		assert_eq!(result[1].name, "1");
		assert_eq!(result[1].kind, "Vec");
		assert_eq!(result[1].value, "[\"a\",\"b\",\"c\"]");
		assert!(!result[1].indexed);
	}

	#[test]
	fn test_convert_object_with_type_value() {
		let filter = create_test_filter();

		let arguments = vec![
			json!({
				"type": "Address",
				"value": "0x123"
			}),
			json!({
				"type": "U256",
				"value": "1000000"
			}),
		];
		let function_spec = StellarContractFunction::default();
		let result =
			filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		assert_eq!(result.len(), 2);

		// Check Address object
		assert_eq!(result[0].name, "0");
		assert_eq!(result[0].kind, "Address");
		assert_eq!(result[0].value, "0x123");
		assert!(!result[0].indexed);

		// Check U256 object
		assert_eq!(result[1].name, "1");
		assert_eq!(result[1].kind, "U256");
		assert_eq!(result[1].value, "1000000");
		assert!(!result[1].indexed);
	}

	#[test]
	fn test_convert_generic_objects() {
		let filter = create_test_filter();

		let arguments = vec![
			json!({
				"key1": "value1",
				"key2": 42
			}),
			json!({
				"nested": {
					"key": "value"
				}
			}),
		];
		let function_spec = StellarContractFunction::default();
		let result =
			filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		assert_eq!(result.len(), 2);

		// Check first object
		assert_eq!(result[0].name, "0");
		assert_eq!(result[0].kind, "Map");
		assert_eq!(result[0].value, "{\"key1\":\"value1\",\"key2\":42}");
		assert!(!result[0].indexed);

		// Check nested object
		assert_eq!(result[1].name, "1");
		assert_eq!(result[1].kind, "Map");
		assert_eq!(result[1].value, "{\"nested\":{\"key\":\"value\"}}");
		assert!(!result[1].indexed);
	}

	#[test]
	fn test_convert_empty_array() {
		let filter = create_test_filter();
		let arguments = vec![];
		let function_spec = StellarContractFunction::default();
		let result =
			filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		assert_eq!(result.len(), 0);
	}

	#[test]
	fn test_convert_mixed_values() {
		let filter = create_test_filter();

		let arguments = vec![
			json!({
				"type": "U64",
				"value": "42"
			}),
			json!({
				"type": "Vec",
				"value": "1,2"
			}),
			json!({
				"type": "Address",
				"value": "0x123"
			}),
			json!({
				"type": "Map",
				"value": "{\"key\":\"value\"}"
			}),
		];
		let function_spec = StellarContractFunction::default();
		let result =
			filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		assert_eq!(result.len(), 4);

		// Check primitive
		assert_eq!(result[0].name, "0");
		assert_eq!(result[0].kind, "U64");
		assert_eq!(result[0].value, "42");
		assert!(!result[0].indexed);

		// Check array
		assert_eq!(result[1].name, "1");
		assert_eq!(result[1].kind, "Vec");
		assert_eq!(result[1].value, "1,2");
		assert!(!result[1].indexed);

		// Check typed object
		assert_eq!(result[2].name, "2");
		assert_eq!(result[2].kind, "Address");
		assert_eq!(result[2].value, "0x123");
		assert!(!result[2].indexed);

		// Check generic object
		assert_eq!(result[3].name, "3");
		assert_eq!(result[3].kind, "Map");
		assert_eq!(result[3].value, "{\"key\":\"value\"}");
		assert!(!result[3].indexed);
	}

	#[test]
	fn test_convert_arguments_to_match_param_entry() {
		let filter = create_test_filter();
		let arguments = vec![
			json!({
				"type": "Address",
				"value": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
			}),
			json!({
				"type": "U128",
				"value": "1000000000"
			}),
			json!({
				"type": "U128",
				"value": "1000000000"
			}),
		];

		let function_spec = StellarFormattedContractSpec {
			functions: vec![StellarContractFunction {
				signature: "swap(Address,U128,U128)".to_string(),
				name: "swap".to_string(),
				inputs: vec![
					StellarContractInput {
						name: "token_a".to_string(),
						kind: "Address".to_string(),
						index: 0,
					},
					StellarContractInput {
						name: "amount_a".to_string(),
						kind: "U128".to_string(),
						index: 1,
					},
					StellarContractInput {
						name: "min_b".to_string(),
						kind: "U128".to_string(),
						index: 2,
					},
				],
			}],
		};

		let params = filter
			.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec.functions[0]));

		assert_eq!(params.len(), 3);

		// Check first parameter (token_a)
		assert_eq!(params[0].name, "token_a");
		assert_eq!(
			params[0].value,
			"CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
		);
		assert_eq!(params[0].kind, "Address");
		assert!(!params[0].indexed);

		// Check second parameter (amount_a)
		assert_eq!(params[1].name, "amount_a");
		assert_eq!(params[1].value, "1000000000");
		assert_eq!(params[1].kind, "U128");
		assert!(!params[1].indexed);

		// Check third parameter (min_b)
		assert_eq!(params[2].name, "min_b");
		assert_eq!(params[2].value, "1000000000");
		assert_eq!(params[2].kind, "U128");
		assert!(!params[2].indexed);
	}

	#[test]
	fn test_convert_arguments_to_match_param_entry_without_spec() {
		let filter = create_test_filter();
		let arguments = vec![
			json!({
				"type": "Address",
				"value": "CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
			}),
			json!({
				"type": "U128",
				"value": "1000000000"
			}),
			json!({
				"type": "U128",
				"value": "1000000000"
			}),
		];

		let params = filter.convert_arguments_to_match_param_entry(&arguments, None);

		assert_eq!(params.len(), 3);

		// Without spec, parameters should be numbered
		assert_eq!(params[0].name, "0");
		assert_eq!(
			params[0].value,
			"CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
		);
		assert_eq!(params[0].kind, "Address");
		assert!(!params[0].indexed);

		assert_eq!(params[1].name, "1");
		assert_eq!(params[1].value, "1000000000");
		assert_eq!(params[1].kind, "U128");
		assert!(!params[1].indexed);

		assert_eq!(params[2].name, "2");
		assert_eq!(params[2].value, "1000000000");
		assert_eq!(params[2].kind, "U128");
		assert!(!params[2].indexed);
	}
}
