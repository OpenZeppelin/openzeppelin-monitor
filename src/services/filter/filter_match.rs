//! Match handling and processing logic.
//!
//! This module implements the processing of matched transactions and events:
//! - Converts blockchain data to trigger-friendly format
//! - Prepares notification payloads by converting blockchain-specific data into a generic format
//! - Handles match execution through configured triggers
//! - Manages the transformation of complex blockchain data into template variables

use std::collections::HashMap;

use alloy::primitives::Address;
use hex;
use serde_json::{json, Value as JsonValue};

use crate::{
	models::{MonitorMatch, ScriptLanguage},
	services::{
		filter::{
			evm_helpers::{b256_to_string, h160_to_string},
			FilterError,
		},
		trigger::TriggerExecutionServiceTrait,
	},
};

/// Process a monitor match by executing associated triggers.
///
/// Takes a matched monitor event and processes it through the appropriate trigger service.
/// Converts blockchain-specific data into a standardized format that can be used in trigger
/// templates.
///
/// # Arguments
/// * `matching_monitor` - The matched monitor event containing transaction and trigger information
/// * `trigger_service` - Service responsible for executing triggers
/// * `trigger_scripts` - Scripts to be executed for each trigger
///
/// # Returns
/// Result indicating success or failure of trigger execution
///
/// # Example (EVM)
/// The function converts blockchain data into template variables like:
/// ```text
/// "monitor.name": "Transfer USDT Token"
/// "network.slug": "ethereum_mainnet"
/// "transaction.hash": "0x99139c8f64b9b939678e261e1553660b502d9fd01c2ab1516e699ee6c8cc5791"
/// "transaction.from": "0xf401346fd255e034a2e43151efe1d68c1e0f8ca5"
/// "transaction.to": "0x0000000000001ff3684f28c67538d4d072c22734"
/// "transaction.value": "24504000000000000"
/// "transaction.nonce": "42"
/// "transaction.gas": "21000"
/// "transaction.gasPrice": "20000000000"
/// "transaction.blockNumber": "12345678"
/// "transaction.blockHash": "0x..."
/// "transaction.input": "0xa9059cbb..."
/// "receipt.blockNumber": "12345678"
/// "receipt.gasUsed": "21000"
/// "receipt.status": "1"
/// "events.0.signature": "Transfer(address,address,uint256)"
/// "events.0.args.to": "0x70bf6634ee8cb27d04478f184b9b8bb13e5f4710"
/// "events.0.args.from": "0x2e8135be71230c6b1b4045696d41c09db0414226"
/// "events.0.args.value": "88248701"
/// ```
pub async fn handle_match<T: TriggerExecutionServiceTrait>(
	matching_monitor: MonitorMatch,
	trigger_service: &T,
	trigger_scripts: &HashMap<String, (ScriptLanguage, String)>,
) -> Result<(), FilterError> {
	match &matching_monitor {
		MonitorMatch::EVM(evm_monitor_match) => {
			let transaction = evm_monitor_match.transaction.clone();
			// If sender does not exist, we replace with 0x0000000000000000000000000000000000000000
			let sender = transaction.sender().unwrap_or(&Address::ZERO);

			// Create structured JSON data
			let mut data_json = json!({
				"monitor": {
					"name": evm_monitor_match.monitor.name.clone(),
				},
				"network": {
					"slug": evm_monitor_match.network_slug.clone()
				},
				"transaction": {
					"hash": b256_to_string(*transaction.hash()),
					"from": h160_to_string(*sender),
					"value": transaction.value().to_string(),
					"nonce": transaction.nonce().to_string(),
					"gas": transaction.gas().to_string(),
					"input": format!("0x{}", hex::encode(transaction.0.input.as_ref())),
				},
				"receipt": {},
				"functions": [],
				"events": []
			});

			if let Some(to) = transaction.to() {
				data_json["transaction"]["to"] = json!(h160_to_string(*to));
			}
			if let Some(gas_price) = transaction.gas_price() {
				data_json["transaction"]["gasPrice"] = json!(gas_price.to_string());
			}
			if let Some(block_number) = transaction.0.block_number {
				data_json["transaction"]["blockNumber"] = json!(block_number.to_string());
			}
			if let Some(block_hash) = transaction.0.block_hash {
				data_json["transaction"]["blockHash"] = json!(b256_to_string(block_hash));
			}
			if let Some(transaction_index) = &transaction.0.transaction_index {
				data_json["transaction"]["transactionIndex"] =
					json!(transaction_index.0.to_string());
			}
			if let Some(tx_type) = transaction.0.transaction_type {
				data_json["transaction"]["type"] = json!(tx_type.to_string());
			}
			if let Some(max_fee_per_gas) = transaction.0.max_fee_per_gas {
				data_json["transaction"]["maxFeePerGas"] = json!(max_fee_per_gas.to_string());
			}
			if let Some(max_priority_fee_per_gas) = transaction.0.max_priority_fee_per_gas {
				data_json["transaction"]["maxPriorityFeePerGas"] =
					json!(max_priority_fee_per_gas.to_string());
			}

			if let Some(receipt) = &evm_monitor_match.receipt {
				if let Some(block_number) = receipt.0.block_number {
					data_json["receipt"]["blockNumber"] = json!(block_number.to_string());
				}
				if let Some(block_hash) = receipt.0.block_hash {
					data_json["receipt"]["blockHash"] = json!(b256_to_string(block_hash));
				}
				if let Some(gas_used) = receipt.0.gas_used {
					data_json["receipt"]["gasUsed"] = json!(gas_used.to_string());
				}
				if let Some(status) = receipt.0.status {
					data_json["receipt"]["status"] = json!(status.to_string());
				}
				if let Some(contract_address) = receipt.0.contract_address {
					data_json["receipt"]["contractAddress"] =
						json!(h160_to_string(contract_address));
				}
				if let Some(effective_gas_price) = receipt.0.effective_gas_price {
					data_json["receipt"]["effectiveGasPrice"] =
						json!(effective_gas_price.to_string());
				}
				data_json["receipt"]["cumulativeGasUsed"] =
					json!(receipt.0.cumulative_gas_used.to_string());
				data_json["receipt"]["transactionIndex"] =
					json!(receipt.0.transaction_index.0.to_string());
			}

			// Process matched functions
			let functions = data_json["functions"].as_array_mut().unwrap();
			for func in evm_monitor_match.matched_on.functions.iter() {
				let mut function_data = json!({
					"signature": func.signature.clone(),
					"args": {}
				});

				if let Some(args) = &evm_monitor_match.matched_on_args {
					if let Some(func_args) = &args.functions {
						for func_arg in func_args {
							if func_arg.signature == func.signature {
								if let Some(arg_entries) = &func_arg.args {
									let args_obj = function_data["args"].as_object_mut().unwrap();
									for arg in arg_entries {
										args_obj.insert(arg.name.clone(), json!(arg.value.clone()));
									}
								}
							}
						}
					}
				}

				functions.push(function_data);
			}

			// Process matched events
			let events = data_json["events"].as_array_mut().unwrap();
			for event in evm_monitor_match.matched_on.events.iter() {
				let mut event_data = json!({
					"signature": event.signature.clone(),
					"args": {}
				});

				if let Some(args) = &evm_monitor_match.matched_on_args {
					if let Some(event_args) = &args.events {
						for event_arg in event_args {
							if event_arg.signature == event.signature {
								if let Some(arg_entries) = &event_arg.args {
									let args_obj = event_data["args"].as_object_mut().unwrap();
									for arg in arg_entries {
										args_obj.insert(arg.name.clone(), json!(arg.value.clone()));
									}
								}
							}
						}
					}
				}

				events.push(event_data);
			}

			// Swallow any errors since it's logged in the trigger service and we want to continue
			// processing other matches
			let _ = trigger_service
				.execute(
					&evm_monitor_match
						.monitor
						.triggers
						.iter()
						.map(|s| s.to_string())
						.collect::<Vec<_>>(),
					json_to_hashmap(&data_json),
					&matching_monitor,
					trigger_scripts,
				)
				.await;
		}
		MonitorMatch::Stellar(stellar_monitor_match) => {
			let transaction = stellar_monitor_match.transaction.clone();
			let ledger = &stellar_monitor_match.ledger;

			// Create structured JSON data
			let mut data_json = json!({
				"monitor": {
					"name": stellar_monitor_match.monitor.name.clone(),
				},
				"network": {
					"slug": stellar_monitor_match.network_slug.clone()
				},
				"transaction": {
					"hash": transaction.hash().to_string(),
					"ledger": ledger.sequence.to_string(),
					"ledgerCloseTime": ledger.ledger_close_time.clone(),
					"status": transaction.0.status.clone(),
					"applicationOrder": transaction.0.application_order.to_string(),
				},
				"ledger": {
					"sequence": ledger.sequence.to_string(),
					"hash": ledger.hash.clone(),
					"closeTime": ledger.ledger_close_time.clone(),
				},
				"functions": [],
				"events": []
			});

			// Process matched functions
			let functions = data_json["functions"].as_array_mut().unwrap();
			for func in stellar_monitor_match.matched_on.functions.iter() {
				let mut function_data = json!({
					"signature": func.signature.clone(),
					"args": {}
				});

				if let Some(args) = &stellar_monitor_match.matched_on_args {
					if let Some(func_args) = &args.functions {
						for func_arg in func_args {
							if func_arg.signature == func.signature {
								if let Some(arg_entries) = &func_arg.args {
									let args_obj = function_data["args"].as_object_mut().unwrap();
									for arg in arg_entries {
										args_obj.insert(arg.name.clone(), json!(arg.value.clone()));
									}
								}
							}
						}
					}
				}

				functions.push(function_data);
			}

			// Process matched events
			let events = data_json["events"].as_array_mut().unwrap();
			for event in stellar_monitor_match.matched_on.events.iter() {
				let mut event_data = json!({
					"signature": event.signature.clone(),
					"args": {}
				});

				if let Some(args) = &stellar_monitor_match.matched_on_args {
					if let Some(event_args) = &args.events {
						for event_arg in event_args {
							if event_arg.signature == event.signature {
								if let Some(arg_entries) = &event_arg.args {
									let args_obj = event_data["args"].as_object_mut().unwrap();
									for arg in arg_entries {
										args_obj.insert(arg.name.clone(), json!(arg.value.clone()));
									}
								}
							}
						}
					}
				}

				events.push(event_data);
			}

			// Swallow any errors since it's logged in the trigger service and we want to continue
			// processing other matches
			let _ = trigger_service
				.execute(
					&stellar_monitor_match
						.monitor
						.triggers
						.iter()
						.map(|s| s.to_string())
						.collect::<Vec<_>>(),
					json_to_hashmap(&data_json),
					&matching_monitor,
					trigger_scripts,
				)
				.await;
		}
		MonitorMatch::Midnight(midnight_monitor_match) => {
			let transaction = midnight_monitor_match.transaction.clone();

			// Create structured JSON data
			let mut data_json = json!({
				"monitor": {
					"name": midnight_monitor_match.monitor.name.clone(),
				},
				"network": {
					"slug": midnight_monitor_match.network_slug.clone()
				},
				"transaction": {
					"hash": transaction.hash().to_string(),
				},
				"functions": [],
				"events": []
			});

			// Process matched functions
			let functions = data_json["functions"].as_array_mut().unwrap();
			for func in midnight_monitor_match.matched_on.functions.iter() {
				let mut function_data = json!({
					"signature": func.signature.clone(),
					"args": {}
				});

				if let Some(args) = &midnight_monitor_match.matched_on_args {
					if let Some(func_args) = &args.functions {
						for func_arg in func_args {
							if func_arg.signature == func.signature {
								if let Some(arg_entries) = &func_arg.args {
									let args_obj = function_data["args"].as_object_mut().unwrap();
									for arg in arg_entries {
										args_obj.insert(arg.name.clone(), json!(arg.value.clone()));
									}
								}
							}
						}
					}
				}

				functions.push(function_data);
			}

			// Process matched events
			let events = data_json["events"].as_array_mut().unwrap();
			for event in midnight_monitor_match.matched_on.events.iter() {
				let mut event_data = json!({
					"signature": event.signature.clone(),
					"args": {}
				});

				if let Some(args) = &midnight_monitor_match.matched_on_args {
					if let Some(event_args) = &args.events {
						for event_arg in event_args {
							if event_arg.signature == event.signature {
								if let Some(arg_entries) = &event_arg.args {
									let args_obj = event_data["args"].as_object_mut().unwrap();
									for arg in arg_entries {
										args_obj.insert(arg.name.clone(), json!(arg.value.clone()));
									}
								}
							}
						}
					}
				}

				events.push(event_data);
			}

			// Swallow any errors since it's logged in the trigger service and we want to continue
			// processing other matches
			let _ = trigger_service
				.execute(
					&midnight_monitor_match
						.monitor
						.triggers
						.iter()
						.map(|s| s.to_string())
						.collect::<Vec<_>>(),
					json_to_hashmap(&data_json),
					&matching_monitor,
					trigger_scripts,
				)
				.await;
		}
		MonitorMatch::Solana(solana_monitor_match) => {
			let transaction = solana_monitor_match.transaction.clone();
			let block = &solana_monitor_match.block;

			// Create structured JSON data
			let mut data_json = json!({
				"monitor": {
					"name": solana_monitor_match.monitor.name.clone(),
				},
				"network": {
					"slug": solana_monitor_match.network_slug.clone()
				},
				"transaction": {
					"signature": transaction.signature().to_string(),
					"slot": transaction.0.slot.to_string(),
				},
				"block": {
					"slot": block.slot.to_string(),
					"blockhash": block.blockhash.clone(),
					"parentSlot": block.parent_slot.to_string(),
				},
				"functions": [],
				"events": []
			});

			if let Some(block_time) = transaction.0.block_time {
				data_json["transaction"]["blockTime"] = json!(block_time.to_string());
			}
			if let Some(block_time) = block.block_time {
				data_json["block"]["blockTime"] = json!(block_time.to_string());
			}
			if let Some(block_height) = block.block_height {
				data_json["block"]["blockHeight"] = json!(block_height.to_string());
			}

			// Process matched functions (instructions)
			let functions = data_json["functions"].as_array_mut().unwrap();
			for func in solana_monitor_match.matched_on.functions.iter() {
				let mut function_data = json!({
					"signature": func.signature.clone(),
					"args": {}
				});

				if let Some(args) = &solana_monitor_match.matched_on_args {
					if let Some(func_args) = &args.functions {
						for func_arg in func_args {
							if func_arg.signature == func.signature {
								if let Some(arg_entries) = &func_arg.args {
									let args_obj = function_data["args"].as_object_mut().unwrap();
									for arg in arg_entries {
										args_obj.insert(arg.name.clone(), json!(arg.value.clone()));
									}
								}
							}
						}
					}
				}

				functions.push(function_data);
			}

			// Process matched events (logs)
			let events = data_json["events"].as_array_mut().unwrap();
			for event in solana_monitor_match.matched_on.events.iter() {
				let mut event_data = json!({
					"signature": event.signature.clone(),
					"args": {}
				});

				if let Some(args) = &solana_monitor_match.matched_on_args {
					if let Some(event_args) = &args.events {
						for event_arg in event_args {
							if event_arg.signature == event.signature {
								if let Some(arg_entries) = &event_arg.args {
									let args_obj = event_data["args"].as_object_mut().unwrap();
									for arg in arg_entries {
										args_obj.insert(arg.name.clone(), json!(arg.value.clone()));
									}
								}
							}
						}
					}
				}

				events.push(event_data);
			}

			// Swallow any errors since it's logged in the trigger service and we want to continue
			// processing other matches
			let _ = trigger_service
				.execute(
					&solana_monitor_match
						.monitor
						.triggers
						.iter()
						.map(|s| s.to_string())
						.collect::<Vec<_>>(),
					json_to_hashmap(&data_json),
					&matching_monitor,
					trigger_scripts,
				)
				.await;
		}
	}
	Ok(())
}

/// Converts a JsonValue to a flattened HashMap with dotted path notation
fn json_to_hashmap(json: &JsonValue) -> HashMap<String, String> {
	let mut result = HashMap::new();
	flatten_json_path(json, "", &mut result);
	result
}

/// Flattens a JsonValue into a HashMap with dotted path notation
fn flatten_json_path(value: &JsonValue, prefix: &str, result: &mut HashMap<String, String>) {
	match value {
		JsonValue::Object(obj) => {
			for (key, val) in obj {
				let new_prefix = if prefix.is_empty() {
					key.clone()
				} else {
					format!("{}.{}", prefix, key)
				};
				flatten_json_path(val, &new_prefix, result);
			}
		}
		JsonValue::Array(arr) => {
			for (idx, val) in arr.iter().enumerate() {
				let new_prefix = format!("{}.{}", prefix, idx);
				flatten_json_path(val, &new_prefix, result);
			}
		}
		JsonValue::String(s) => insert_primitive(prefix, result, s),
		JsonValue::Number(n) => insert_primitive(prefix, result, n.to_string()),
		JsonValue::Bool(b) => insert_primitive(prefix, result, b.to_string()),
		JsonValue::Null => insert_primitive(prefix, result, "null".to_string()),
	}
}

/// Helper function to insert primitive values with consistent key handling
fn insert_primitive<T: ToString>(prefix: &str, result: &mut HashMap<String, String>, value: T) {
	let key = if prefix.is_empty() {
		"value".to_string()
	} else {
		prefix.to_string()
	};
	result.insert(key, value.to_string());
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn test_json_to_hashmap() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
			"transaction": {
				"hash": "0x1234567890abcdef",
			},
		});

		let hashmap = json_to_hashmap(&json);
		assert_eq!(hashmap["monitor.name"], "Test Monitor");
		assert_eq!(hashmap["transaction.hash"], "0x1234567890abcdef");
	}

	#[test]
	fn test_json_to_hashmap_with_functions() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
			"functions": [
				{
					"signature": "function1(uint256)",
					"args": {
						"arg1": "100",
					},
				},
			],
		});

		let hashmap = json_to_hashmap(&json);
		assert_eq!(hashmap["monitor.name"], "Test Monitor");
		assert_eq!(hashmap["functions.0.signature"], "function1(uint256)");
		assert_eq!(hashmap["functions.0.args.arg1"], "100");
	}

	#[test]
	fn test_json_to_hashmap_with_events() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
			"events": [
				{
					"signature": "event1(uint256)",
					"args": {
						"arg1": "100",
					},
				},
			],
		});

		let hashmap = json_to_hashmap(&json);
		assert_eq!(hashmap["monitor.name"], "Test Monitor");
		assert_eq!(hashmap["events.0.signature"], "event1(uint256)");
		assert_eq!(hashmap["events.0.args.arg1"], "100");
	}

	// Add tests for flatten_json_path
	#[test]
	fn test_flatten_json_path_object() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
		});

		let mut result = HashMap::new();
		flatten_json_path(&json, "", &mut result);
		assert_eq!(result["monitor.name"], "Test Monitor");
	}

	#[test]
	fn test_flatten_json_path_array() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
		});

		let mut result = HashMap::new();
		flatten_json_path(&json, "", &mut result);
		assert_eq!(result["monitor.name"], "Test Monitor");
	}

	#[test]
	fn test_flatten_json_path_string() {
		let json = json!("Test String");
		let mut result = HashMap::new();
		flatten_json_path(&json, "test_prefix", &mut result);
		assert_eq!(result["test_prefix"], "Test String");

		let mut result2 = HashMap::new();
		flatten_json_path(&json, "", &mut result2);
		assert_eq!(result2["value"], "Test String");
	}

	#[test]
	fn test_flatten_json_path_number() {
		let json = json!(123);
		let mut result = HashMap::new();
		flatten_json_path(&json, "test_prefix", &mut result);
		assert_eq!(result["test_prefix"], "123");

		let mut result2 = HashMap::new();
		flatten_json_path(&json, "", &mut result2);
		assert_eq!(result2["value"], "123");
	}

	#[test]
	fn test_flatten_json_path_boolean() {
		let json = json!(true);
		let mut result = HashMap::new();
		flatten_json_path(&json, "test_prefix", &mut result);
		assert_eq!(result["test_prefix"], "true");

		// Test with empty prefix
		let mut result2 = HashMap::new();
		flatten_json_path(&json, "", &mut result2);
		assert_eq!(result2["value"], "true");
	}

	#[test]
	fn test_flatten_json_path_null() {
		let json = json!(null);
		let mut result = HashMap::new();
		flatten_json_path(&json, "test_prefix", &mut result);
		assert_eq!(result["test_prefix"], "null");

		let mut result2 = HashMap::new();
		flatten_json_path(&json, "", &mut result2);
		assert_eq!(result2["value"], "null");
	}

	#[test]
	fn test_flatten_json_path_nested_object() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
				"nested": {
					"key": "value",
				},
			},
		});

		let mut result = HashMap::new();
		flatten_json_path(&json, "", &mut result);
		assert_eq!(result["monitor.nested.key"], "value");
	}

	#[test]
	fn test_flatten_json_path_nested_array() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
				"nested": [
					{
						"key": "value1",
					},
					{
						"key": "value2",
					},
				],
			},
		});

		let mut result = HashMap::new();
		flatten_json_path(&json, "", &mut result);
		assert_eq!(result["monitor.nested.0.key"], "value1");
		assert_eq!(result["monitor.nested.1.key"], "value2");
	}

	#[test]
	fn test_flatten_json_path_with_prefix() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
		});

		let mut result = HashMap::new();
		flatten_json_path(&json, "prefix", &mut result);
		assert_eq!(result["prefix.monitor.name"], "Test Monitor");
	}

	#[test]
	fn test_json_to_hashmap_with_primitive_values() {
		// String
		let json_string = json!("Test String");
		let hashmap_string = json_to_hashmap(&json_string);
		assert_eq!(hashmap_string["value"], "Test String");

		// Number
		let json_number = json!(123);
		let hashmap_number = json_to_hashmap(&json_number);
		assert_eq!(hashmap_number["value"], "123");

		// Boolean
		let json_bool = json!(true);
		let hashmap_bool = json_to_hashmap(&json_bool);
		assert_eq!(hashmap_bool["value"], "true");

		// Null
		let json_null = json!(null);
		let hashmap_null = json_to_hashmap(&json_null);
		assert_eq!(hashmap_null["value"], "null");
	}

	#[test]
	fn test_insert_primitive() {
		let mut result = HashMap::new();
		insert_primitive("prefix", &mut result, "Test String");
		assert_eq!(result["prefix"], "Test String");

		let mut result2 = HashMap::new();
		insert_primitive("", &mut result2, "Test String");
		assert_eq!(result2["value"], "Test String");

		let mut result3 = HashMap::new();
		insert_primitive("prefix", &mut result3, 123);
		assert_eq!(result3["prefix"], "123");

		let mut result4 = HashMap::new();
		insert_primitive("", &mut result4, 123);
		assert_eq!(result4["value"], "123");

		let mut result5 = HashMap::new();
		insert_primitive("prefix", &mut result5, true);
		assert_eq!(result5["prefix"], "true");

		let mut result6 = HashMap::new();
		insert_primitive("", &mut result6, true);
		assert_eq!(result6["value"], "true");

		let mut result7 = HashMap::new();
		insert_primitive("prefix", &mut result7, JsonValue::Null);
		assert_eq!(result7["prefix"], "null");

		let mut result8 = HashMap::new();
		insert_primitive("", &mut result8, JsonValue::Null);
		assert_eq!(result8["value"], "null");
	}

	#[test]
	fn test_json_to_hashmap_with_full_evm_structure() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
			"network": {
				"slug": "ethereum_mainnet"
			},
			"transaction": {
				"hash": "0x1234567890abcdef",
				"from": "0xabc",
				"to": "0xdef",
				"value": "1000000000000000000",
				"nonce": "42",
				"gas": "21000",
				"gasPrice": "20000000000",
				"blockNumber": "12345678",
				"blockHash": "0xblockhash",
				"transactionIndex": "5",
				"input": "0xa9059cbb",
				"type": "2",
				"maxFeePerGas": "30000000000",
				"maxPriorityFeePerGas": "1000000000"
			},
			"receipt": {
				"blockNumber": "12345678",
				"blockHash": "0xblockhash",
				"gasUsed": "21000",
				"status": "1",
				"contractAddress": "0xcontract",
				"effectiveGasPrice": "25000000000",
				"cumulativeGasUsed": "500000",
				"transactionIndex": "5"
			},
			"functions": [],
			"events": []
		});

		let hashmap = json_to_hashmap(&json);

		assert_eq!(hashmap["network.slug"], "ethereum_mainnet");
		assert_eq!(hashmap["transaction.hash"], "0x1234567890abcdef");
		assert_eq!(hashmap["transaction.nonce"], "42");
		assert_eq!(hashmap["transaction.gas"], "21000");
		assert_eq!(hashmap["transaction.gasPrice"], "20000000000");
		assert_eq!(hashmap["transaction.blockNumber"], "12345678");
		assert_eq!(hashmap["transaction.blockHash"], "0xblockhash");
		assert_eq!(hashmap["transaction.transactionIndex"], "5");
		assert_eq!(hashmap["transaction.input"], "0xa9059cbb");
		assert_eq!(hashmap["transaction.type"], "2");
		assert_eq!(hashmap["transaction.maxFeePerGas"], "30000000000");
		assert_eq!(hashmap["transaction.maxPriorityFeePerGas"], "1000000000");
		assert_eq!(hashmap["receipt.blockNumber"], "12345678");
		assert_eq!(hashmap["receipt.gasUsed"], "21000");
		assert_eq!(hashmap["receipt.status"], "1");
		assert_eq!(hashmap["receipt.contractAddress"], "0xcontract");
		assert_eq!(hashmap["receipt.effectiveGasPrice"], "25000000000");
		assert_eq!(hashmap["receipt.cumulativeGasUsed"], "500000");
	}

	#[test]
	fn test_json_to_hashmap_with_stellar_structure() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
			"network": {
				"slug": "stellar_mainnet"
			},
			"transaction": {
				"hash": "abc123",
				"ledger": "12345",
				"ledgerCloseTime": "2024-03-20T12:00:00Z",
				"status": "SUCCESS",
				"applicationOrder": "1"
			},
			"ledger": {
				"sequence": "12345",
				"hash": "ledgerhash",
				"closeTime": "2024-03-20T12:00:00Z"
			},
			"functions": [],
			"events": []
		});

		let hashmap = json_to_hashmap(&json);

		assert_eq!(hashmap["network.slug"], "stellar_mainnet");
		assert_eq!(hashmap["transaction.ledger"], "12345");
		assert_eq!(hashmap["transaction.status"], "SUCCESS");
		assert_eq!(hashmap["ledger.sequence"], "12345");
		assert_eq!(hashmap["ledger.hash"], "ledgerhash");
	}

	#[test]
	fn test_json_to_hashmap_with_solana_structure() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
			"network": {
				"slug": "solana_mainnet"
			},
			"transaction": {
				"signature": "abc123",
				"slot": "123456789",
				"blockTime": "1234567890"
			},
			"block": {
				"slot": "123456789",
				"blockhash": "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn",
				"parentSlot": "123456788",
				"blockTime": "1234567890",
				"blockHeight": "100000000"
			},
			"functions": [],
			"events": []
		});

		let hashmap = json_to_hashmap(&json);

		assert_eq!(hashmap["network.slug"], "solana_mainnet");
		assert_eq!(hashmap["transaction.signature"], "abc123");
		assert_eq!(hashmap["transaction.slot"], "123456789");
		assert_eq!(hashmap["block.slot"], "123456789");
		assert_eq!(
			hashmap["block.blockhash"],
			"4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn"
		);
		assert_eq!(hashmap["block.blockHeight"], "100000000");
	}

	#[test]
	fn test_json_to_hashmap_with_midnight_structure() {
		let json = json!({
			"monitor": {
				"name": "Test Monitor",
			},
			"network": {
				"slug": "midnight_testnet"
			},
			"transaction": {
				"hash": "midnight_tx_hash_123"
			},
			"functions": [],
			"events": []
		});

		let hashmap = json_to_hashmap(&json);

		assert_eq!(hashmap["network.slug"], "midnight_testnet");
		assert_eq!(hashmap["transaction.hash"], "midnight_tx_hash_123");
	}
}
