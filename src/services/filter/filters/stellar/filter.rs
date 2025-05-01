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
use stellar_xdr::curr::{OperationBody, TransactionEnvelope};
use tracing::instrument;

use crate::{
	models::{
		BlockType, EventCondition, FunctionCondition, MatchConditions, Monitor, MonitorMatch,
		Network, StellarEvent, StellarMatchArguments, StellarMatchParamEntry,
		StellarMatchParamsMap, StellarMonitorMatch, StellarTransaction, TransactionCondition,
		TransactionStatus,
	},
	services::{
		blockchain::{BlockChainClient, StellarClientTrait},
		filter::{
			stellar_helpers::{
				are_same_signature, compare_json_values, compare_json_values_vs_string,
				compare_strings, get_kind_from_value, get_nested_value, normalize_address,
				parse_json_safe, parse_xdr_value, process_invoke_host_function,
			},
			BlockFilter, FilterError,
		},
	},
	utils::split_expression,
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
								process_invoke_host_function(invoke_host_function);
							let operation = TxOperation {
								_operation_type: "invoke_host_function".to_string(),
								sender: from.clone(),
								receiver: parsed_operation.contract_address.clone(),
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

								if self.evaluate_expression(expr, &Some(tx_params)) {
									matched_transactions.push(TransactionCondition {
										expression: Some(expr.clone()),
										status: tx_status,
									});
									break;
								}
							}
						} else {
							// Even with no operations, still evaluate base parameters
							if self.evaluate_expression(expr, &Some(base_params)) {
								matched_transactions.push(TransactionCondition {
									expression: Some(expr.clone()),
									status: tx_status,
								});
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

	/// Finds matching functions within a transaction
	///
	/// # Arguments
	/// * `monitored_addresses` - List of addresses being monitored
	/// * `transaction` - The transaction to check
	/// * `monitor` - The monitor containing match conditions
	/// * `matched_functions` - Vector to store matching functions
	/// * `matched_on_args` - Arguments that matched the conditions
	pub fn find_matching_functions_for_transaction(
		&self,
		monitored_addresses: &[String],
		transaction: &StellarTransaction,
		monitor: &Monitor,
		matched_functions: &mut Vec<FunctionCondition>,
		matched_on_args: &mut StellarMatchArguments,
	) {
		if let Some(decoded) = transaction.decoded() {
			if let Some(TransactionEnvelope::Tx(tx)) = &decoded.envelope {
				for operation in tx.tx.operations.iter() {
					if let OperationBody::InvokeHostFunction(invoke_host_function) = &operation.body
					{
						let parsed_operation = process_invoke_host_function(invoke_host_function);

						// Skip if contract address doesn't match
						if !monitored_addresses
							.contains(&normalize_address(&parsed_operation.contract_address))
						{
							continue;
						}

						// Convert parsed operation arguments into param entries
						let param_entries = self
							.convert_arguments_to_match_param_entry(&parsed_operation.arguments);

						if monitor.match_conditions.functions.is_empty() {
							// Match on all functions
							matched_functions.push(FunctionCondition {
								signature: parsed_operation.function_signature.clone(),
								expression: None,
							});
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
										if self
											.evaluate_expression(expr, &Some(param_entries.clone()))
										{
											matched_functions.push(FunctionCondition {
												signature: parsed_operation
													.function_signature
													.clone(),
												expression: Some(expr.clone()),
											});
											if let Some(functions) = &mut matched_on_args.functions
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
									} else {
										// If no expression, match on function name alone
										matched_functions.push(FunctionCondition {
											signature: parsed_operation.function_signature.clone(),
											expression: None,
										});
									}
								}
							}
						}
					}
				}
			}
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
								if self.evaluate_expression(expr, &Some(args.clone())) {
									matched_events.push(EventCondition {
										signature: event.signature.clone(),
										expression: Some(expr.clone()),
									});
									if let Some(events) = &mut matched_on_args.events {
										events.push(event.clone());
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
	pub async fn decode_events(
		&self,
		events: &Vec<StellarEvent>,
		monitored_addresses: &[String],
	) -> Vec<EventMap> {
		let mut decoded_events = Vec::new();
		for event in events {
			// Skip if contract address doesn't match
			if !monitored_addresses.contains(&normalize_address(&event.contract_id)) {
				continue;
			}

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

	/// Compares values based on their type and operator
	///
	/// # Arguments
	/// * `param_type` - The type of parameter being compared
	/// * `param_value` - The actual value to compare
	/// * `operator` - The comparison operator
	/// * `compare_value` - The value to compare against
	///
	/// # Returns
	/// Boolean indicating if the comparison evaluates to true
	fn compare_values(
		&self,
		param_type: &str,
		param_value: &str,
		operator: &str,
		compare_value: &str,
	) -> bool {
		// Remove quotes from the values to normalize them
		let param_value = param_value.trim_matches('"');
		let compare_value = compare_value.trim_matches('"');

		match param_type {
			"Bool" | "bool" => self.compare_bool(param_value, operator, compare_value),
			"U32" | "u32" => self.compare_u32(param_value, operator, compare_value),
			"U64" | "u64" | "Timepoint" | "timepoint" | "Duration" | "duration" => {
				self.compare_u64(param_value, operator, compare_value)
			}
			"I32" | "i32" => self.compare_i32(param_value, operator, compare_value),
			"I64" | "i64" => self.compare_i64(param_value, operator, compare_value),
			"U128" | "u128" => self.compare_u128(param_value, operator, compare_value),
			"I128" | "i128" => self.compare_i128(param_value, operator, compare_value),
			"U256" | "u256" | "I256" | "i256" => {
				self.compare_i256(param_value, operator, compare_value)
			}
			"Vec" | "vec" => self.compare_vec(param_value, operator, compare_value),
			"Map" | "map" => self.compare_map(param_value, operator, compare_value),
			"String" | "string" | "Symbol" | "symbol" | "Address" | "address" | "Bytes"
			| "bytes" => self.compare_string(param_value, operator, compare_value),
			_ => {
				tracing::warn!("Unsupported parameter type: {}", param_type);
				false
			}
		}
	}

	fn compare_bool(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		let Ok(param_value) = param_value.parse::<bool>() else {
			tracing::warn!("Failed to parse bool parameter value: {}", param_value);
			return false;
		};
		let Ok(compare_value) = compare_value.parse::<bool>() else {
			tracing::warn!("Failed to parse bool comparison value: {}", compare_value);
			return false;
		};
		match operator {
			"==" => param_value == compare_value,
			"!=" => param_value != compare_value,
			_ => {
				tracing::warn!("Unsupported operator for bool type: {}", operator);
				false
			}
		}
	}

	fn compare_u64(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		let Ok(param_value) = param_value.parse::<u64>() else {
			tracing::warn!("Failed to parse u64 parameter value: {}", param_value);
			return false;
		};
		let Ok(compare_value) = compare_value.parse::<u64>() else {
			tracing::warn!("Failed to parse u64 comparison value: {}", compare_value);
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
				tracing::warn!("Unsupported operator: {}", operator);
				false
			}
		}
	}

	fn compare_u32(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		let Ok(param_value) = param_value.parse::<u32>() else {
			tracing::warn!("Failed to parse u32 parameter value: {}", param_value);
			return false;
		};
		let Ok(compare_value) = compare_value.parse::<u32>() else {
			tracing::warn!("Failed to parse u32 comparison value: {}", compare_value);
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
				tracing::warn!("Unsupported operator: {}", operator);
				false
			}
		}
	}

	fn compare_i32(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		let Ok(param_value) = param_value.parse::<i32>() else {
			tracing::warn!("Failed to parse i32 parameter value: {}", param_value);
			return false;
		};
		let Ok(compare_value) = compare_value.parse::<i32>() else {
			tracing::warn!("Failed to parse i32 comparison value: {}", compare_value);
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
				tracing::warn!("Unsupported operator: {}", operator);
				false
			}
		}
	}

	fn compare_i64(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		let Ok(param_value) = param_value.parse::<i64>() else {
			tracing::warn!("Failed to parse i64 parameter value: {}", param_value);
			return false;
		};
		let Ok(compare_value) = compare_value.parse::<i64>() else {
			tracing::warn!("Failed to parse i64 comparison value: {}", compare_value);
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
				tracing::warn!("Unsupported operator: {}", operator);
				false
			}
		}
	}

	fn compare_u128(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		let Ok(param_value) = param_value.parse::<u128>() else {
			tracing::warn!("Failed to parse u128 parameter value: {}", param_value);
			return false;
		};
		let Ok(compare_value) = compare_value.parse::<u128>() else {
			tracing::warn!("Failed to parse u128 comparison value: {}", compare_value);
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
				tracing::warn!("Unsupported operator: {}", operator);
				false
			}
		}
	}

	fn compare_i128(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		let Ok(param_value) = param_value.parse::<i128>() else {
			tracing::warn!("Failed to parse i128 parameter value: {}", param_value);
			return false;
		};
		let Ok(compare_value) = compare_value.parse::<i128>() else {
			tracing::warn!("Failed to parse i128 comparison value: {}", compare_value);
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
				tracing::warn!("Unsupported operator: {}", operator);
				false
			}
		}
	}

	fn compare_i256(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		match operator {
			"==" => param_value == compare_value,
			"!=" => param_value != compare_value,
			_ => {
				tracing::warn!(
					"Only == and != operators are supported for i256: {}",
					operator
				);
				false
			}
		}
	}

	fn compare_string(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		let normalized_param = param_value.trim().to_lowercase();
		let normalized_compare = compare_value.trim().to_lowercase();
		match operator {
			"==" => normalized_param == normalized_compare,
			"!=" => normalized_param != normalized_compare,
			_ => {
				tracing::warn!(
					"Only == and != operators are supported for string types: {}",
					operator
				);
				false
			}
		}
	}

	fn compare_vec(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		// Split by comma and trim whitespace
		let values: Vec<&str> = param_value.split(',').map(|s| s.trim()).collect();

		// arguments[0] contains "some_value"
		// arguments[0] == "value1,value2,value3"
		match operator {
			"contains" => values.contains(&compare_value),
			"==" => param_value == compare_value, // For exact array match
			"!=" => param_value != compare_value,
			_ => {
				tracing::warn!(
					"Only contains, == and != operators are supported for vec type: {}",
					operator
				);
				false
			}
		}
	}

	/// Compares two values that might be JSON or plain strings using the specified operator.
	///
	/// # Arguments
	/// * `param_value` - The first value to compare, which could be a JSON string or plain string
	/// * `operator` - The comparison operator ("==", "!=", ">", ">=", "<", "<=")
	/// * `compare_value` - The second value to compare against, which could be a JSON string or
	///   plain string
	///
	/// # Supported Comparison Cases
	/// 1. **JSON vs JSON**: Both values are valid JSON
	///    - Supports equality (==, !=)
	///    - Supports numeric comparisons (>, >=, <, <=) when both values are numbers
	///
	/// 2. **JSON vs String**: First value is JSON, second is plain string
	///    - Supports dot notation to access nested JSON values (e.g., "user.address.city")
	///    - Can check if the string matches a key in a JSON object
	///    - Falls back to direct string comparison if above checks fail
	///
	/// 3. **String vs JSON**: First value is string, second is JSON
	///    - Currently returns false as this is an invalid comparison
	///
	/// 4. **String vs String**: Neither value is valid JSON
	///    - Performs direct string comparison
	///
	/// # Returns
	/// * `bool` - True if the comparison succeeds, false otherwise
	pub fn compare_map(&self, param_value: &str, operator: &str, compare_value: &str) -> bool {
		let param_json = parse_json_safe(param_value);
		let compare_json = parse_json_safe(compare_value);

		match (param_json, compare_json) {
			(Some(ref param_val), Some(ref compare_val)) => {
				compare_json_values(param_val, operator, compare_val)
			}

			(Some(param_val), None) => {
				if compare_value.contains('.') {
					return get_nested_value(&param_val, compare_value)
						.map(|nested_val| {
							compare_json_values_vs_string(nested_val, operator, compare_value)
						})
						.unwrap_or(false);
				}

				if let Some(obj) = param_val.as_object() {
					if let Some(value) = obj.get(compare_value) {
						return compare_json_values_vs_string(value, operator, compare_value);
					}
				}

				compare_strings(param_value, operator, compare_value)
			}

			(None, Some(_)) => {
				tracing::debug!("Invalid comparison: non-JSON value compared against JSON value");
				false
			}

			(None, None) => compare_strings(param_value, operator, compare_value),
		}
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
		args: &Option<Vec<StellarMatchParamEntry>>,
	) -> bool {
		let Some(args) = args else {
			return false;
		};

		// Split by OR to get highest level conditions
		let or_conditions: Vec<&str> = expression.split(" OR ").collect();

		// For OR logic, any condition being true makes the whole expression true
		for or_condition in or_conditions {
			// Split each OR condition by AND
			let and_conditions: Vec<&str> = or_condition.trim().split(" AND ").collect();

			// All AND conditions must be true
			let and_result = and_conditions.iter().all(|condition| {
				// Remove surrounding parentheses and trim
				let clean_condition = condition.trim().trim_matches(|c| c == '(' || c == ')');

				// Split into parts while preserving quoted strings
				let parts = if let Some((left, operator, right)) = split_expression(clean_condition)
				{
					vec![left, operator, right]
				} else {
					tracing::warn!("Invalid expression format: {}", clean_condition);
					return false;
				};

				if parts.len() != 3 {
					tracing::warn!("Invalid expression format: {}", clean_condition);
					return false;
				}

				let [param_expr, operator, value] = [parts[0], parts[1], parts[2]];

				// Find the parameter and its type
				if param_expr.contains('[') {
					// Array indexing: arguments[0][0]
					let indices: Vec<usize> = param_expr
						.split('[')
						.skip(1)
						.filter_map(|s| s.trim_end_matches(']').parse::<usize>().ok())
						.collect();

					if indices.len() != 2 || indices[0] >= args.len() {
						tracing::debug!("Invalid array indices: {:?}", indices);
						return false;
					}

					let param = &args[indices[0]];
					let array_values: Vec<&str> = param.value.split(',').collect();
					if indices[1] >= array_values.len() {
						tracing::debug!("Array index out of bounds: {}", indices[1]);
						return false;
					}

					self.compare_values(
						&param.kind,
						array_values[indices[1]].trim(),
						operator,
						value,
					)
				} else if param_expr.contains('.') {
					// Map access: map.key
					let parts: Vec<&str> = param_expr.split('.').collect();
					if parts.len() != 2 {
						tracing::debug!("Invalid map access format: {}", param_expr);
						return false;
					}

					let [map_name, key] = [parts[0], parts[1]];

					let Some(param) = args.iter().find(|p| p.name == map_name) else {
						tracing::debug!("Map {} not found", map_name);
						return false;
					};

					let Ok(mut map_value) = serde_json::from_str::<serde_json::Value>(&param.value)
					else {
						tracing::debug!("Failed to parse map: {}", param.value);
						return false;
					};

					// Unescape the keys in the map_value
					if let serde_json::Value::Object(ref mut map) = map_value {
						let unescaped_map: serde_json::Map<String, serde_json::Value> = map
							.iter()
							.map(|(k, v)| (k.trim_matches('"').to_string(), v.clone()))
							.collect();
						*map = unescaped_map;
					}

					let Some(key_value) = map_value.get(key) else {
						tracing::debug!("Key {} not found in map", key);
						return false;
					};

					self.compare_values(
						&get_kind_from_value(key_value),
						&key_value.to_string(),
						operator,
						value,
					)
				} else {
					// Regular parameter
					let Some(param) = args.iter().find(|p| p.name == param_expr) else {
						tracing::warn!("Parameter {} not found", param_expr);
						return false;
					};

					self.compare_values(&param.kind, &param.value, operator, value)
				}
			});

			if and_result {
				return true;
			}
		}

		false
	}

	/// Converts Stellar function arguments into match parameter entries
	///
	/// # Arguments
	/// * `arguments` - Vector of argument values to convert
	///
	/// # Returns
	/// Vector of converted parameter entries
	pub fn convert_arguments_to_match_param_entry(
		&self,
		arguments: &[Value],
	) -> Vec<StellarMatchParamEntry> {
		let mut params = Vec::new();
		for (index, arg) in arguments.iter().enumerate() {
			match arg {
				Value::Array(array) => {
					// Handle nested arrays
					params.push(StellarMatchParamEntry {
						name: index.to_string(),
						kind: "Vec".to_string(),
						value: serde_json::to_string(array).unwrap_or_default(),
						indexed: false,
					});
				}
				Value::Object(map) => {
					// Check for the new structure
					if let (Some(Value::String(type_str)), Some(Value::String(value))) =
						(map.get("type"), map.get("value"))
					{
						// Handle the new structure
						params.push(StellarMatchParamEntry {
							name: index.to_string(),
							kind: type_str.clone(),
							value: value.clone(),
							indexed: false,
						});
					} else {
						// Handle generic objects
						params.push(StellarMatchParamEntry {
							name: index.to_string(),
							kind: "Map".to_string(),
							value: serde_json::to_string(map).unwrap_or_default(),
							indexed: false,
						});
					}
				}
				_ => {
					// Handle primitive values
					params.push(StellarMatchParamEntry {
						name: index.to_string(),
						kind: get_kind_from_value(arg),
						value: match arg {
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

		// Process each monitor first
		for monitor in monitors {
			tracing::debug!("Processing monitor: {}", monitor.name);

			let monitored_addresses = monitor
				.addresses
				.iter()
				.map(|addr| normalize_address(&addr.address))
				.collect::<Vec<String>>();

			let decoded_events = self.decode_events(&events, &monitored_addresses).await;

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
			AddressWithABI, MatchConditions, Monitor, StellarDecodedTransaction,
			StellarTransaction, StellarTransactionInfo, TransactionStatus,
		},
		utils::tests::stellar::monitor::MonitorBuilder,
	};
	use serde_json::json;
	use stellar_strkey::ed25519::PublicKey as StrPublicKey;

	use base64::engine::general_purpose::STANDARD as BASE64;
	use stellar_xdr::curr::{
		Asset, Hash, HostFunction, InvokeContractArgs, InvokeHostFunctionOp, MuxedAccount,
		Operation, OperationBody, PaymentOp, ScAddress, ScString, ScSymbol, ScVal, SequenceNumber,
		StringM, Transaction, TransactionEnvelope, TransactionV1Envelope, Uint256, VecM,
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
		addresses: Vec<AddressWithABI>,
	) -> Monitor {
		MonitorBuilder::new()
			.name("test")
			.networks(vec!["stellar_mainnet".to_string()])
			.paused(false)
			.addresses_with_abi(
				addresses
					.iter()
					.map(|a| (a.address.clone(), a.abi.clone()))
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
	fn create_test_transaction(
		status: &str,
		transaction_hash: &str,
		application_order: i32,
		amount: Option<&str>,
		from: Option<&str>,
		to: Option<&str>,
		operation_type: Option<&str>,
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

		// Create the V1 envelope
		let tx_envelope = TransactionV1Envelope {
			tx,
			signatures: Default::default(),
		};

		// Wrap in TransactionEnvelope
		let envelope = TransactionEnvelope::Tx(tx_envelope);

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
		);

		// Create monitor with empty function conditions but using normalized address
		let monitor = create_test_monitor(
			vec![],
			vec![],
			vec![],
			vec![AddressWithABI {
				address: normalized_contract_address.clone(),
				abi: None,
			}],
		);

		// Use normalized address in monitored addresses
		let monitored_addresses = vec![normalized_contract_address];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
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
			vec![AddressWithABI {
				address: normalized_contract_address.clone(),
				abi: None,
			}],
		);

		let monitored_addresses = vec![normalized_contract_address];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
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
		);

		// Create monitor with function signature and expression
		let monitor = create_test_monitor(
			vec![],
			vec![FunctionCondition {
				signature: "mock_function(I32,String)".to_string(),
				expression: Some("0 < 50".to_string()),
			}],
			vec![],
			vec![AddressWithABI {
				address: normalized_contract_address.clone(),
				abi: None,
			}],
		);

		let monitored_addresses = vec![normalized_contract_address];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
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
		);

		// Create monitor with different address
		let monitor = create_test_monitor(
			vec![],
			vec![FunctionCondition {
				signature: "mock_function(i32,string)".to_string(),
				expression: None,
			}],
			vec![],
			vec![AddressWithABI {
				address: normalized_different_address.clone(),
				abi: None,
			}],
		);

		let monitored_addresses = vec![normalized_different_address];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
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
			vec![AddressWithABI {
				address: normalized_contract_address.clone(),
				abi: None,
			}],
		);

		let monitored_addresses = vec![normalized_contract_address];

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_args,
		);

		assert_eq!(matched_functions.len(), 1);
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
			create_test_transaction("SUCCESS", "tx_hash_123", 1, None, None, None, None);

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
			create_test_transaction("SUCCESS", "tx_hash_123", 1, None, None, None, None);

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
			create_test_transaction("SUCCESS", "tx_hash_123", 1, None, None, None, None);

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
			create_test_transaction("SUCCESS", "tx_hash_123", 1, None, None, None, None);

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
			create_test_transaction("SUCCESS", "wrong_tx_hash", 1, None, None, None, None);

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
		let decoded = filter.decode_events(&events, &monitored_addresses).await;

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
		let decoded = filter.decode_events(&events, &monitored_addresses).await;

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
		let decoded = filter.decode_events(&events, &monitored_addresses).await;

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
		let decoded = filter.decode_events(&events, &monitored_addresses).await;

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
	// Test cases for compare_values method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_compare_bool() {
		let filter = create_test_filter();

		// Test true/false equality
		assert!(filter.compare_values("bool", "true", "==", "true"));
		assert!(filter.compare_values("Bool", "false", "==", "false"));
		assert!(!filter.compare_values("bool", "true", "==", "false"));

		// Test inequality
		assert!(filter.compare_values("bool", "true", "!=", "false"));
		assert!(!filter.compare_values("bool", "true", "!=", "true"));

		// Test invalid operator
		assert!(!filter.compare_values("bool", "true", ">", "false"));

		// Test invalid bool values
		assert!(!filter.compare_values("bool", "invalid", "==", "true"));
		assert!(!filter.compare_values("bool", "true", "==", "invalid"));
	}

	#[test]
	fn test_compare_integers() {
		let filter = create_test_filter();

		// Test u32
		assert!(filter.compare_values("u32", "100", ">", "50"));
		assert!(filter.compare_values("U32", "50", "<", "100"));
		assert!(filter.compare_values("u32", "100", "==", "100"));
		assert!(filter.compare_values("u32", "50", "!=", "100"));
		assert!(!filter.compare_values("u32", "invalid", ">", "50"));

		// Test i32
		assert!(filter.compare_values("i32", "-10", "<", "0"));
		assert!(filter.compare_values("I32", "0", ">", "-10"));
		assert!(!filter.compare_values("i32", "invalid", "<", "0"));

		// Test u64
		assert!(filter.compare_values("u64", "1000000", ">", "999999"));
		assert!(filter.compare_values("Timepoint", "100", "<", "200"));
		assert!(filter.compare_values("duration", "50", "==", "50"));

		// Test i64
		assert!(filter.compare_values("i64", "-1000000", "<", "0"));
		assert!(filter.compare_values("I64", "0", ">", "-1000000"));

		// Test u128
		assert!(filter.compare_values(
			"u128",
			"340282366920938463463374607431768211455",
			"==",
			"340282366920938463463374607431768211455"
		));
		assert!(filter.compare_values("U128", "100", "<", "200"));

		// Test i128
		assert!(filter.compare_values(
			"i128",
			"-170141183460469231731687303715884105728",
			"<",
			"0"
		));
		assert!(filter.compare_values("I128", "0", ">", "-100"));
	}

	#[test]
	fn test_compare_strings() {
		let filter = create_test_filter();
		// Test basic string equality
		assert!(filter.compare_values("string", "hello", "==", "hello"));
		assert!(filter.compare_values("String", "HELLO", "==", "hello")); // Case insensitive
		assert!(filter.compare_values("string", "  hello  ", "==", "hello")); // Trim whitespace

		// Test string inequality
		assert!(filter.compare_values("string", "hello", "!=", "world"));
		assert!(!filter.compare_values("String", "hello", "!=", "HELLO")); // Case insensitive

		// Test address comparison
		assert!(filter.compare_values("address", "0x123", "==", "0x123"));
		assert!(filter.compare_values("Address", "0x123", "!=", "0x456"));

		// Test symbol comparison
		assert!(filter.compare_values("symbol", "SYM", "==", "sym"));
		assert!(filter.compare_values("Symbol", "sym1", "!=", "sym2"));

		// Test invalid operators
		assert!(!filter.compare_values("string", "hello", ">", "world"));
		assert!(!filter.compare_values("string", "hello", "<", "world"));
	}

	#[test]
	fn test_compare_vectors() {
		let filter = create_test_filter();

		// Test vector contains
		assert!(filter.compare_values("vec", "value1,value2,value3", "contains", "value2"));
		assert!(!filter.compare_values("Vec", "value1,value2,value3", "contains", "value4"));

		// Test vector equality
		assert!(filter.compare_values("vec", "1,2,3", "==", "1,2,3"));
		assert!(filter.compare_values("Vec", "1,2,3", "!=", "1,2,4"));

		// Test invalid operators
		assert!(!filter.compare_values("vec", "1,2,3", ">", "1,2,3"));
	}

	#[test]
	fn test_unsupported_type() {
		let filter = create_test_filter();

		// Test unsupported type
		assert!(!filter.compare_values("unsupported_type", "value", "==", "value"));
		assert!(!filter.compare_values("float", "1.0", "==", "1.0"));
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for compare_bool method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_compare_bool_valid_equality() {
		let filter = create_test_filter();

		// Test true == true
		assert!(filter.compare_bool("true", "==", "true"));

		// Test false == false
		assert!(filter.compare_bool("false", "==", "false"));

		// Test true != false
		assert!(filter.compare_bool("true", "!=", "false"));

		// Test false != true
		assert!(filter.compare_bool("false", "!=", "true"));

		// Test false == true (should be false)
		assert!(!filter.compare_bool("false", "==", "true"));

		// Test true == false (should be false)
		assert!(!filter.compare_bool("true", "==", "false"));
	}

	#[test]
	fn test_compare_bool_invalid_values() {
		let filter = create_test_filter();

		// Test invalid param_value
		assert!(!filter.compare_bool("not_a_bool", "==", "true"));

		// Test invalid compare_value
		assert!(!filter.compare_bool("true", "==", "not_a_bool"));

		// Test both invalid values
		assert!(!filter.compare_bool("invalid1", "==", "invalid2"));

		// Test empty strings
		assert!(!filter.compare_bool("", "==", "true"));
		assert!(!filter.compare_bool("true", "==", ""));
		assert!(!filter.compare_bool("", "==", ""));
	}

	#[test]
	fn test_compare_bool_unsupported_operators() {
		let filter = create_test_filter();

		// Test greater than operator
		assert!(!filter.compare_bool("true", ">", "false"));

		// Test less than operator
		assert!(!filter.compare_bool("false", "<", "true"));

		// Test greater than or equal operator
		assert!(!filter.compare_bool("true", ">=", "false"));

		// Test less than or equal operator
		assert!(!filter.compare_bool("false", "<=", "true"));

		// Test empty operator
		assert!(!filter.compare_bool("true", "", "false"));

		// Test invalid operator
		assert!(!filter.compare_bool("true", "invalid", "false"));
	}

	#[test]
	fn test_compare_bool_case_sensitivity() {
		let filter = create_test_filter();

		// Test TRUE (uppercase)
		assert!(!filter.compare_bool("TRUE", "==", "true"));

		// Test False (mixed case)
		assert!(!filter.compare_bool("False", "==", "false"));

		// Test TRUE == TRUE (both uppercase)
		assert!(!filter.compare_bool("TRUE", "==", "TRUE"));
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for compare_u64 method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_compare_u64_valid_comparisons() {
		let filter = create_test_filter();

		// Test greater than
		assert!(filter.compare_u64("100", ">", "50"));
		assert!(!filter.compare_u64("50", ">", "100"));
		assert!(!filter.compare_u64("100", ">", "100"));

		// Test greater than or equal
		assert!(filter.compare_u64("100", ">=", "50"));
		assert!(filter.compare_u64("100", ">=", "100"));
		assert!(!filter.compare_u64("50", ">=", "100"));

		// Test less than
		assert!(filter.compare_u64("50", "<", "100"));
		assert!(!filter.compare_u64("100", "<", "50"));
		assert!(!filter.compare_u64("100", "<", "100"));

		// Test less than or equal
		assert!(filter.compare_u64("50", "<=", "100"));
		assert!(filter.compare_u64("100", "<=", "100"));
		assert!(!filter.compare_u64("100", "<=", "50"));

		// Test equality
		assert!(filter.compare_u64("100", "==", "100"));
		assert!(!filter.compare_u64("100", "==", "50"));

		// Test inequality
		assert!(filter.compare_u64("100", "!=", "50"));
		assert!(!filter.compare_u64("100", "!=", "100"));
	}

	#[test]
	fn test_compare_u64_invalid_values() {
		let filter = create_test_filter();

		// Test invalid param_value
		assert!(!filter.compare_u64("not_a_number", ">", "100"));
		assert!(!filter.compare_u64("", ">", "100"));
		assert!(!filter.compare_u64("-100", ">", "100")); // Negative numbers aren't valid u64

		// Test invalid compare_value
		assert!(!filter.compare_u64("100", ">", "not_a_number"));
		assert!(!filter.compare_u64("100", ">", ""));
		assert!(!filter.compare_u64("100", ">", "-100")); // Negative numbers aren't valid u64

		// Test values exceeding u64::MAX
		assert!(!filter.compare_u64("18446744073709551616", ">", "100")); // u64::MAX + 1
		assert!(!filter.compare_u64("100", ">", "18446744073709551616")); // u64::MAX + 1
	}

	#[test]
	fn test_compare_u64_invalid_operators() {
		let filter = create_test_filter();

		// Test unsupported operators
		assert!(!filter.compare_u64("100", "<<", "50")); // Bit shift operator
		assert!(!filter.compare_u64("100", "contains", "50")); // String operator
		assert!(!filter.compare_u64("100", "", "50")); // Empty operator
		assert!(!filter.compare_u64("100", "invalid", "50")); // Invalid operator
	}

	#[test]
	fn test_compare_u64_boundary_values() {
		let filter = create_test_filter();
		let max = u64::MAX.to_string();
		let zero = "0";

		// Test with u64::MAX
		assert!(filter.compare_u64(&max, "==", &max));
		assert!(filter.compare_u64(&max, ">=", zero));
		assert!(filter.compare_u64(zero, "<=", &max));
		assert!(!filter.compare_u64(&max, "<", &max));
		assert!(!filter.compare_u64(&max, ">", &max));

		// Test with zero
		assert!(filter.compare_u64(zero, "==", zero));
		assert!(filter.compare_u64(zero, "<=", zero));
		assert!(filter.compare_u64(zero, ">=", zero));
		assert!(!filter.compare_u64(zero, "<", zero));
		assert!(!filter.compare_u64(zero, ">", zero));
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for compare_i32 method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_compare_i32_valid_comparisons() {
		let filter = create_test_filter();

		// Test greater than
		assert!(filter.compare_i32("100", ">", "50"));
		assert!(!filter.compare_i32("50", ">", "100"));
		assert!(!filter.compare_i32("100", ">", "100"));

		// Test greater than or equal
		assert!(filter.compare_i32("100", ">=", "50"));
		assert!(filter.compare_i32("100", ">=", "100"));
		assert!(!filter.compare_i32("50", ">=", "100"));

		// Test less than
		assert!(filter.compare_i32("50", "<", "100"));
		assert!(!filter.compare_i32("100", "<", "50"));
		assert!(!filter.compare_i32("100", "<", "100"));

		// Test less than or equal
		assert!(filter.compare_i32("50", "<=", "100"));
		assert!(filter.compare_i32("100", "<=", "100"));
		assert!(!filter.compare_i32("100", "<=", "50"));

		// Test equality
		assert!(filter.compare_i32("100", "==", "100"));
		assert!(!filter.compare_i32("100", "==", "50"));

		// Test inequality
		assert!(filter.compare_i32("100", "!=", "50"));
		assert!(!filter.compare_i32("100", "!=", "100"));
	}

	#[test]
	fn test_compare_i32_negative_numbers() {
		let filter = create_test_filter();

		// Test negative numbers
		assert!(filter.compare_i32("-100", ">", "-200"));
		assert!(filter.compare_i32("-200", "<", "-100"));
		assert!(filter.compare_i32("-100", "==", "-100"));
		assert!(filter.compare_i32("0", ">", "-100"));
		assert!(filter.compare_i32("-100", "<", "0"));
	}

	#[test]
	fn test_compare_i32_invalid_values() {
		let filter = create_test_filter();

		// Test invalid param_value
		assert!(!filter.compare_i32("not_a_number", ">", "100"));
		assert!(!filter.compare_i32("", ">", "100"));
		assert!(!filter.compare_i32("2147483648", ">", "100")); // i32::MAX + 1

		// Test invalid compare_value
		assert!(!filter.compare_i32("100", ">", "not_a_number"));
		assert!(!filter.compare_i32("100", ">", ""));
		assert!(!filter.compare_i32("100", ">", "2147483648")); // i32::MAX + 1

		// Test floating point numbers (invalid for i32)
		assert!(!filter.compare_i32("100.5", ">", "100"));
		assert!(!filter.compare_i32("100", ">", "99.9"));
	}

	#[test]
	fn test_compare_i32_boundary_values() {
		let filter = create_test_filter();

		// Test i32::MAX and i32::MIN
		assert!(filter.compare_i32("2147483647", ">", "0")); // i32::MAX
		assert!(filter.compare_i32("-2147483648", "<", "0")); // i32::MIN
		assert!(filter.compare_i32("2147483647", "==", "2147483647")); // i32::MAX == i32::MAX
		assert!(filter.compare_i32("-2147483648", "==", "-2147483648")); // i32::MIN == i32::MIN
		assert!(filter.compare_i32("2147483647", ">", "-2147483648")); // i32::MAX > i32::MIN
	}

	#[test]
	fn test_compare_i32_invalid_operators() {
		let filter = create_test_filter();

		// Test unsupported operators
		assert!(!filter.compare_i32("100", "<<", "50")); // Bit shift operator
		assert!(!filter.compare_i32("100", "contains", "50")); // String operator
		assert!(!filter.compare_i32("100", "", "50")); // Empty operator
		assert!(!filter.compare_i32("100", "&", "50")); // Bitwise operator
		assert!(!filter.compare_i32("100", "===", "50")); // JavaScript-style equality
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for compare_u32 method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_compare_u32_valid_comparisons() {
		let filter = create_test_filter();

		// Test greater than
		assert!(filter.compare_u32("100", ">", "50"));
		assert!(!filter.compare_u32("50", ">", "100"));
		assert!(!filter.compare_u32("100", ">", "100"));

		// Test greater than or equal
		assert!(filter.compare_u32("100", ">=", "50"));
		assert!(filter.compare_u32("100", ">=", "100"));
		assert!(!filter.compare_u32("50", ">=", "100"));

		// Test less than
		assert!(filter.compare_u32("50", "<", "100"));
		assert!(!filter.compare_u32("100", "<", "50"));
		assert!(!filter.compare_u32("100", "<", "100"));

		// Test less than or equal
		assert!(filter.compare_u32("50", "<=", "100"));
		assert!(filter.compare_u32("100", "<=", "100"));
		assert!(!filter.compare_u32("100", "<=", "50"));

		// Test equality
		assert!(filter.compare_u32("100", "==", "100"));
		assert!(!filter.compare_u32("100", "==", "50"));

		// Test inequality
		assert!(filter.compare_u32("100", "!=", "50"));
		assert!(!filter.compare_u32("100", "!=", "100"));
	}

	#[test]
	fn test_compare_u32_boundary_values() {
		let filter = create_test_filter();

		// Test with u32::MAX
		assert!(filter.compare_u32(&u32::MAX.to_string(), ">", "0"));
		assert!(filter.compare_u32("0", "<", &u32::MAX.to_string()));
		assert!(filter.compare_u32(&u32::MAX.to_string(), "==", &u32::MAX.to_string()));

		// Test with u32::MIN (0)
		assert!(filter.compare_u32("0", "==", "0"));
		assert!(filter.compare_u32("1", ">", "0"));
		assert!(filter.compare_u32("0", "<", "1"));
	}

	#[test]
	fn test_compare_u32_invalid_values() {
		let filter = create_test_filter();

		// Test invalid param_value
		assert!(!filter.compare_u32("not_a_number", ">", "100"));
		assert!(!filter.compare_u32("", ">", "100"));
		assert!(!filter.compare_u32("-100", ">", "100")); // Negative numbers aren't valid u32

		// Test invalid compare_value
		assert!(!filter.compare_u32("100", ">", "not_a_number"));
		assert!(!filter.compare_u32("100", ">", ""));
		assert!(!filter.compare_u32("100", ">", "-100")); // Negative numbers aren't valid u32

		// Test values exceeding u32::MAX
		assert!(!filter.compare_u32("4294967296", ">", "100")); // u32::MAX + 1
		assert!(!filter.compare_u32("100", ">", "4294967296")); // u32::MAX + 1

		// Test floating point numbers
		assert!(!filter.compare_u32("100.5", ">", "100"));
		assert!(!filter.compare_u32("100", ">", "99.9"));
	}

	#[test]
	fn test_compare_u32_invalid_operators() {
		let filter = create_test_filter();

		// Test unsupported operators
		assert!(!filter.compare_u32("100", "invalid", "50"));
		assert!(!filter.compare_u32("100", "", "50"));
		assert!(!filter.compare_u32("100", ">>", "50"));
		assert!(!filter.compare_u32("100", "=", "50"));
		assert!(!filter.compare_u32("100", "===", "50"));
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for compare_i64 method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_compare_i64_valid_comparisons() {
		let filter = create_test_filter();

		// Test greater than
		assert!(filter.compare_i64("100", ">", "50"));
		assert!(!filter.compare_i64("50", ">", "100"));
		assert!(!filter.compare_i64("100", ">", "100"));

		// Test greater than or equal
		assert!(filter.compare_i64("100", ">=", "50"));
		assert!(filter.compare_i64("100", ">=", "100"));
		assert!(!filter.compare_i64("50", ">=", "100"));

		// Test less than
		assert!(filter.compare_i64("50", "<", "100"));
		assert!(!filter.compare_i64("100", "<", "50"));
		assert!(!filter.compare_i64("100", "<", "100"));

		// Test less than or equal
		assert!(filter.compare_i64("50", "<=", "100"));
		assert!(filter.compare_i64("100", "<=", "100"));
		assert!(!filter.compare_i64("100", "<=", "50"));

		// Test equality
		assert!(filter.compare_i64("100", "==", "100"));
		assert!(!filter.compare_i64("100", "==", "50"));

		// Test inequality
		assert!(filter.compare_i64("100", "!=", "50"));
		assert!(!filter.compare_i64("100", "!=", "100"));
	}

	#[test]
	fn test_compare_i64_negative_numbers() {
		let filter = create_test_filter();

		// Test negative numbers
		assert!(filter.compare_i64("-100", ">", "-200"));
		assert!(filter.compare_i64("-200", "<", "-100"));
		assert!(filter.compare_i64("-100", "==", "-100"));
		assert!(filter.compare_i64("-100", "!=", "-200"));
		assert!(filter.compare_i64("-100", ">=", "-100"));
		assert!(filter.compare_i64("-100", "<=", "-100"));

		// Test negative vs positive
		assert!(filter.compare_i64("-100", "<", "100"));
		assert!(filter.compare_i64("100", ">", "-100"));
		assert!(!filter.compare_i64("-100", "==", "100"));
	}

	#[test]
	fn test_compare_i64_invalid_values() {
		let filter = create_test_filter();

		// Test invalid param_value
		assert!(!filter.compare_i64("not_a_number", ">", "100"));
		assert!(!filter.compare_i64("", ">", "100"));
		assert!(!filter.compare_i64("9223372036854775808", ">", "100")); // i64::MAX + 1

		// Test invalid compare_value
		assert!(!filter.compare_i64("100", ">", "not_a_number"));
		assert!(!filter.compare_i64("100", ">", ""));
		assert!(!filter.compare_i64("100", ">", "9223372036854775808")); // i64::MAX + 1

		// Test floating point numbers (invalid for i64)
		assert!(!filter.compare_i64("100.5", ">", "100"));
		assert!(!filter.compare_i64("100", ">", "99.9"));
	}

	#[test]
	fn test_compare_i64_boundary_values() {
		let filter = create_test_filter();

		// Test with i64::MAX and i64::MIN
		assert!(filter.compare_i64("9223372036854775807", ">", "0"));
		assert!(filter.compare_i64("-9223372036854775808", "<", "0"));
		assert!(filter.compare_i64("9223372036854775807", "==", "9223372036854775807"));
		assert!(filter.compare_i64("-9223372036854775808", "==", "-9223372036854775808"));
		assert!(filter.compare_i64("9223372036854775807", ">=", "9223372036854775807"));
		assert!(filter.compare_i64("-9223372036854775808", "<=", "-9223372036854775808"));
	}

	#[test]
	fn test_compare_i64_invalid_operators() {
		let filter = create_test_filter();

		// Test unsupported operators
		assert!(!filter.compare_i64("100", "<<", "50"));
		assert!(!filter.compare_i64("100", "contains", "50"));
		assert!(!filter.compare_i64("100", "", "50"));
		assert!(!filter.compare_i64("100", "invalid", "50"));
	}

	#[test]
	fn test_compare_u128_valid_comparisons() {
		let filter = create_test_filter();

		// Test basic comparisons
		assert!(filter.compare_u128("100", ">", "50"));
		assert!(filter.compare_u128("100", ">=", "50"));
		assert!(filter.compare_u128("50", "<", "100"));
		assert!(filter.compare_u128("50", "<=", "100"));
		assert!(filter.compare_u128("100", "==", "100"));
		assert!(filter.compare_u128("100", "!=", "50"));

		// Test equality edge cases
		assert!(filter.compare_u128("0", "==", "0"));
		assert!(filter.compare_u128(
			"340282366920938463463374607431768211455",
			"==",
			"340282366920938463463374607431768211455"
		)); // max u128

		// Test boundary values
		assert!(filter.compare_u128("0", "<=", "0"));
		assert!(filter.compare_u128(
			"340282366920938463463374607431768211455",
			">=",
			"340282366920938463463374607431768211455"
		));

		// Test false conditions
		assert!(!filter.compare_u128("50", ">", "100"));
		assert!(!filter.compare_u128("100", "<", "50"));
		assert!(!filter.compare_u128("100", "==", "50"));
		assert!(!filter.compare_u128("100", "!=", "100"));
	}

	#[test]
	fn test_compare_u128_invalid_inputs() {
		let filter = create_test_filter();

		// Test invalid number formats
		assert!(!filter.compare_u128("not_a_number", ">", "100"));
		assert!(!filter.compare_u128("100", ">", "not_a_number"));
		assert!(!filter.compare_u128("", ">", "100"));
		assert!(!filter.compare_u128("100", ">", ""));

		// Test negative numbers (invalid for u128)
		assert!(!filter.compare_u128("-100", ">", "100"));
		assert!(!filter.compare_u128("100", ">", "-100"));

		// Test invalid operator
		assert!(!filter.compare_u128("100", "invalid_operator", "50"));
		assert!(!filter.compare_u128("100", "", "50"));
	}

	#[test]
	fn test_compare_i128_valid_comparisons() {
		let filter = create_test_filter();

		// Test basic comparisons
		assert!(filter.compare_i128("100", ">", "50"));
		assert!(filter.compare_i128("100", ">=", "50"));
		assert!(filter.compare_i128("50", "<", "100"));
		assert!(filter.compare_i128("50", "<=", "100"));
		assert!(filter.compare_i128("100", "==", "100"));
		assert!(filter.compare_i128("100", "!=", "50"));

		// Test negative numbers
		assert!(filter.compare_i128("-100", "<", "0"));
		assert!(filter.compare_i128("0", ">", "-100"));
		assert!(filter.compare_i128("-100", "==", "-100"));
		assert!(filter.compare_i128("-50", ">", "-100"));

		// Test equality edge cases
		assert!(filter.compare_i128("0", "==", "0"));
		assert!(filter.compare_i128(
			"-170141183460469231731687303715884105728",
			"==",
			"-170141183460469231731687303715884105728"
		)); // min i128
		assert!(filter.compare_i128(
			"170141183460469231731687303715884105727",
			"==",
			"170141183460469231731687303715884105727"
		)); // max i128

		// Test false conditions
		assert!(!filter.compare_i128("50", ">", "100"));
		assert!(!filter.compare_i128("-100", ">", "0"));
		assert!(!filter.compare_i128("100", "==", "-100"));
		assert!(!filter.compare_i128("-100", "!=", "-100"));
	}

	#[test]
	fn test_compare_i128_invalid_inputs() {
		let filter = create_test_filter();
		// Test invalid number formats
		assert!(!filter.compare_i128("not_a_number", ">", "100"));
		assert!(!filter.compare_i128("100", ">", "not_a_number"));
		assert!(!filter.compare_i128("", ">", "100"));
		assert!(!filter.compare_i128("100", ">", ""));

		// Test numbers exceeding i128 bounds
		assert!(!filter.compare_i128("170141183460469231731687303715884105728", ">", "0")); // > max i128
		assert!(!filter.compare_i128("-170141183460469231731687303715884105729", "<", "0")); // < min i128

		// Test invalid operator
		assert!(!filter.compare_i128("100", "invalid_operator", "50"));
		assert!(!filter.compare_i128("100", "", "50"));
	}

	// Tests for compare_i256
	#[test]
	fn test_compare_i256() {
		let filter = create_test_filter();

		// Test equality operator
		assert!(filter.compare_i256("12345", "==", "12345"));
		assert!(!filter.compare_i256("12345", "==", "54321"));

		// Test inequality operator
		assert!(filter.compare_i256("12345", "!=", "54321"));
		assert!(!filter.compare_i256("12345", "!=", "12345"));

		// Test unsupported operators
		assert!(!filter.compare_i256("12345", ">", "54321"));
		assert!(!filter.compare_i256("12345", "<", "54321"));
		assert!(!filter.compare_i256("12345", ">=", "54321"));
		assert!(!filter.compare_i256("12345", "<=", "54321"));

		// Test with large numbers
		assert!(filter.compare_i256(
			"115792089237316195423570985008687907853269984665640564039457584007913129639935",
			"==",
			"115792089237316195423570985008687907853269984665640564039457584007913129639935"
		));
		assert!(filter.compare_i256(
			"115792089237316195423570985008687907853269984665640564039457584007913129639935",
			"!=",
			"0"
		));
	}

	// Tests for compare_string
	#[test]
	fn test_compare_string() {
		let filter = create_test_filter();
		// Test basic equality
		assert!(filter.compare_string("hello", "==", "hello"));
		assert!(!filter.compare_string("hello", "==", "world"));

		// Test case insensitivity
		assert!(filter.compare_string("Hello", "==", "hello"));
		assert!(filter.compare_string("HELLO", "==", "hello"));
		assert!(filter.compare_string("HeLLo", "==", "hEllO"));

		// Test whitespace trimming
		assert!(filter.compare_string("  hello  ", "==", "hello"));
		assert!(filter.compare_string("hello", "==", "  hello  "));
		assert!(filter.compare_string("  hello  ", "==", "  hello  "));

		// Test inequality
		assert!(filter.compare_string("hello", "!=", "world"));
		assert!(!filter.compare_string("hello", "!=", "hello"));
		assert!(!filter.compare_string("Hello", "!=", "hello"));

		// Test empty strings
		assert!(filter.compare_string("", "==", ""));
		assert!(filter.compare_string("  ", "==", ""));
		assert!(filter.compare_string("hello", "!=", ""));

		// Test unsupported operators
		assert!(!filter.compare_string("hello", ">", "world"));
		assert!(!filter.compare_string("hello", "<", "world"));
		assert!(!filter.compare_string("hello", ">=", "world"));
		assert!(!filter.compare_string("hello", "<=", "world"));
	}

	// Tests for compare_vec
	#[test]
	fn test_compare_vec() {
		let filter = create_test_filter();

		// Test contains operator
		assert!(filter.compare_vec("value1,value2,value3", "contains", "value2"));
		assert!(!filter.compare_vec("value1,value2,value3", "contains", "value4"));

		// Test with whitespace
		assert!(filter.compare_vec("value1, value2, value3", "contains", "value2"));
		assert!(filter.compare_vec("value1,  value2  ,value3", "contains", "value2"));

		// Test exact equality
		assert!(filter.compare_vec("value1,value2,value3", "==", "value1,value2,value3"));
		assert!(!filter.compare_vec("value1,value2,value3", "==", "value1,value2"));
		assert!(!filter.compare_vec("value1,value2", "==", "value1,value2,value3"));

		// Test inequality
		assert!(filter.compare_vec("value1,value2,value3", "!=", "value1,value2"));
		assert!(!filter.compare_vec("value1,value2,value3", "!=", "value1,value2,value3"));

		// Test empty vectors
		assert!(filter.compare_vec("", "==", ""));
		assert!(!filter.compare_vec("", "contains", "value1"));
		assert!(filter.compare_vec("value1", "!=", ""));

		// Test single value
		assert!(filter.compare_vec("value1", "contains", "value1"));
		assert!(filter.compare_vec("value1", "==", "value1"));

		// Test unsupported operators
		assert!(!filter.compare_vec("value1,value2,value3", ">", "value1"));
		assert!(!filter.compare_vec("value1,value2,value3", "<", "value1"));
		assert!(!filter.compare_vec("value1,value2,value3", ">=", "value1"));
		assert!(!filter.compare_vec("value1,value2,value3", "<=", "value1"));
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for compare_map method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_compare_map_json_vs_json() {
		let filter = create_test_filter();

		// Test equality comparisons
		assert!(filter.compare_map(r#"{"a": 1}"#, "==", r#"{"a": 1}"#));
		assert!(!filter.compare_map(r#"{"a": 1}"#, "==", r#"{"a": 2}"#));
		assert!(filter.compare_map(r#"{"a": 1}"#, "!=", r#"{"a": 2}"#));

		// Test numeric comparisons
		assert!(filter.compare_map("42", ">", "10"));
		assert!(filter.compare_map("10", "<", "42"));
		assert!(filter.compare_map("42", ">=", "42"));
		assert!(filter.compare_map("42", "<=", "42"));
		assert!(!filter.compare_map("10", ">", "42"));

		// Test invalid numeric comparisons
		assert!(!filter.compare_map(r#"{"a": "string"}"#, ">", r#"{"b": 42}"#));
	}

	#[test]
	fn test_compare_map_string_vs_json() {
		let filter = create_test_filter();

		// This case should always return false
		assert!(!filter.compare_map("plain string", "==", r#"{"any": "json"}"#));
	}

	#[test]
	fn test_compare_map_string_vs_string() {
		let filter = create_test_filter();

		// Test basic string comparisons
		assert!(filter.compare_map("hello", "==", "hello"));
		assert!(filter.compare_map("hello", "!=", "world"));
		assert!(!filter.compare_map("hello", "==", "world"));

		// Test case sensitivity
		assert!(!filter.compare_map("Hello", "==", "hello"));

		// Test with spaces and special characters
		assert!(filter.compare_map("hello world", "==", "hello world"));
		assert!(filter.compare_map("special!@#$", "==", "special!@#$"));
	}

	#[test]
	fn test_compare_map_edge_cases() {
		let filter = create_test_filter();

		// Test empty strings
		assert!(filter.compare_map("", "==", ""));
		assert!(!filter.compare_map("", "==", "non-empty"));

		// Test invalid JSON
		assert!(!filter.compare_map("{invalid json}", "==", "{}"));

		// Test unsupported operators
		assert!(!filter.compare_map(r#"{"a": 1}"#, "invalid_operator", r#"{"a": 1}"#));

		// Test with whitespace
		assert!(filter.compare_map(" hello ", "==", " hello "));

		// Test with null JSON values
		assert!(filter.compare_map(r#"{"a": null}"#, "==", r#"{"a": null}"#));
	}

	//////////////////////////////////////////////////////////////////////////////
	// Test cases for evaluate_expression method:
	//////////////////////////////////////////////////////////////////////////////

	#[test]
	fn test_evaluate_expression_regular_parameters() {
		let filter = create_test_filter();

		// Test setup with simple numeric parameters
		let args = Some(vec![
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
		]);

		// Test simple numeric comparison
		assert!(filter.evaluate_expression("amount > 50", &args));
		assert!(!filter.evaluate_expression("amount < 50", &args));

		// Test boolean comparison
		assert!(filter.evaluate_expression("status == true", &args));
		assert!(!filter.evaluate_expression("status == false", &args));

		// Test non-existent parameter
		assert!(!filter.evaluate_expression("invalid_param == 100", &args));
	}

	#[test]
	fn test_evaluate_expression_array_indexing() {
		let filter = create_test_filter();

		// Test setup with array parameter
		let args = Some(vec![StellarMatchParamEntry {
			name: "0".to_string(),
			value: "10,20,30".to_string(),
			kind: "Vec".to_string(),
			indexed: false,
		}]);

		// Test valid array access
		assert!(filter.evaluate_expression("arguments[0][1] == 20", &args));

		// Test out of bounds index
		assert!(!filter.evaluate_expression("arguments[0][5] == 10", &args));

		// Test invalid array format
		assert!(!filter.evaluate_expression("arguments[0] == 10", &args));
	}

	#[test]
	fn test_evaluate_expression_logical_operators() {
		let filter = create_test_filter();

		// Test setup with multiple parameters
		let args = Some(vec![
			StellarMatchParamEntry {
				name: "value".to_string(),
				value: "100".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "active".to_string(),
				value: "true".to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
		]);

		// Test AND operator
		assert!(filter.evaluate_expression("value > 50 AND active == true", &args));
		assert!(!filter.evaluate_expression("value < 50 AND active == true", &args));

		// Test OR operator
		assert!(filter.evaluate_expression("value < 50 OR active == true", &args));
		assert!(!filter.evaluate_expression("value < 50 OR active == false", &args));

		// Test complex expression
		assert!(filter.evaluate_expression("value > 50 AND active == true OR value == 100", &args));
	}

	#[test]
	fn test_evaluate_expression_edge_cases() {
		let filter = create_test_filter();

		// Test with empty args
		assert!(!filter.evaluate_expression("value == 100", &None));

		// Test with empty vector
		assert!(!filter.evaluate_expression("value == 100", &Some(vec![])));

		// Test invalid expression formats
		let args = Some(vec![StellarMatchParamEntry {
			name: "value".to_string(),
			value: "100".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}]);

		// Invalid number of parts
		assert!(!filter.evaluate_expression("value 100", &args));
		assert!(!filter.evaluate_expression("value == 100 invalid", &args));

		// Invalid operator
		assert!(!filter.evaluate_expression("value invalid 100", &args));

		// Empty expression
		assert!(!filter.evaluate_expression("", &args));
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

		let result = filter.convert_arguments_to_match_param_entry(&arguments);

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

		let result = filter.convert_arguments_to_match_param_entry(&arguments);

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

		let result = filter.convert_arguments_to_match_param_entry(&arguments);

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

		let result = filter.convert_arguments_to_match_param_entry(&arguments);

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

		let result = filter.convert_arguments_to_match_param_entry(&arguments);

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

		let result = filter.convert_arguments_to_match_param_entry(&arguments);

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
}
