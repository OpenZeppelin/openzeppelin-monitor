//! Property-based tests for EVM transaction matching and filtering.
//! Tests cover signature/address normalization, expression evaluation, and transaction matching.

use alloy::primitives::{Address, Bytes, LogData, B256, U256};
use std::marker::PhantomData;
use std::str::FromStr;

use openzeppelin_monitor::{
	models::{
		ContractSpec, EVMBaseTransaction, EVMContractSpec, EVMMatchArguments, EVMMatchParamEntry,
		EVMReceiptLog, EVMTransaction, Monitor, TransactionStatus,
	},
	services::{
		blockchain::{EVMTransportClient, EvmClient},
		filter::{
			evm_helpers::{
				are_same_address, are_same_signature, normalize_address, normalize_signature,
			},
			EVMBlockFilter,
		},
	},
	utils::tests::evm::{monitor::MonitorBuilder, receipt::ReceiptBuilder},
};
use proptest::{prelude::*, test_runner::Config};
use serde_json::{json, Value as JsonValue};

// Generates valid EVM function signatures with random parameters
prop_compose! {
	fn valid_signatures()(
		name in "[a-zA-Z][a-zA-Z0-9_]*",
		count in 0..5usize
	)(
		name in Just(name),
		params in prop::collection::vec(
			prop_oneof![
				Just("address"),
				Just("uint256"),
				Just("string"),
				Just("bool"),
				Just("bytes32")
			],
			count..=count
		)
	) -> String {
		format!("{}({})", name, params.join(","))
	}
}

// Generates valid comparison expressions for testing parameter matching
prop_compose! {
	fn valid_expression()(
		param_name in "[a-zA-Z][a-zA-Z0-9_]*",
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		value in 0u128..1000000u128
	) -> String {
		format!("{} {} {}", param_name, operator, value)
	}
}

// Generates valid EVM addresses in both checksummed and lowercase formats
prop_compose! {
	fn valid_address()(hex in "[0-9a-fA-F]{40}") -> String {
		format!("0x{}", hex)
	}
}

// Generates mock EVM transactions with random values and addresses
prop_compose! {
	fn generate_transaction()(
		value in 0u128..1000000u128,
		from_addr in valid_address(),
		to_addr in valid_address(),
		input_data in prop::collection::vec(any::<u8>(), 0..100)
	) -> EVMTransaction {
		EVMTransaction(EVMBaseTransaction {
			from: Some(Address::from_slice(&hex::decode(&from_addr[2..]).unwrap())),
			to: Some(Address::from_slice(&hex::decode(&to_addr[2..]).unwrap())),
			value: U256::from(value),
			input: Bytes::from(input_data),
			..Default::default()
		})
	}
}

// Generates basic monitor configuration
prop_compose! {
	fn generate_base_monitor()(
		address in valid_address(),
	) -> Monitor {
		MonitorBuilder::new()
			.name("Test Monitor")
			.addresses(vec![address])
			.build()
	}
}

// Generates monitor configured with transaction value thresholds and status conditions
prop_compose! {
	fn generate_monitor_with_transaction()(
		address in valid_address(),
		min_value in 0u128..500000u128,
		max_value in 500001u128..1000000u128
	) -> Monitor {
		MonitorBuilder::new()
			.name("Test Monitor")
			.addresses(vec![address])
			.transaction(TransactionStatus::Success, Some(format!("value >= {}", min_value)))
			.transaction(TransactionStatus::Any, Some(format!("value < {}", max_value)))
			.build()
	}
}

// Generates monitor configured with function matching conditions and ABI
prop_compose! {
	fn generate_monitor_with_function()(
		address in valid_address(),
		function_name in prop_oneof![
			Just("store"),
			Just("retrieve"),
			Just("approve"),
		],
		param_type in prop_oneof![
			Just("address"),
			Just("uint256")
		],
		min_value in 0u128..500000u128
	) -> Monitor {
		MonitorBuilder::new()
			.name("Test Monitor")
			.address_with_spec(address.as_str(), Some(ContractSpec::EVM(EVMContractSpec::from(json!([
				{
					"anonymous": false,
					"inputs": [
						{
							"indexed": false,
							"internalType": "uint256",
							"name": "value",
							"type": "uint256"
						}
					],
					"name": "ValueChanged",
					"type": "event"
				},
				{
					"inputs": [
						{
							"internalType": "address",
							"name": "recipient",
							"type": "address"
						},
						{
							"internalType": "uint256",
							"name": "amount",
							"type": "uint256"
						}
					],
					"name": "transfer",
					"outputs": [
						{
							"internalType": "bool",
							"name": "",
							"type": "bool"
						}
					],
					"stateMutability": "nonpayable",
					"type": "function"
				}
			])))))
			.function(format!("{}({})", function_name, param_type).as_str(), Some(format!("value >= {}", min_value)))
			.function(format!("not_{}({})", function_name, param_type).as_str(), Some(format!("value >= {}", min_value)))
			.build()
	}
}

// Generates monitor configured with function matching conditions and ABI
prop_compose! {
	fn generate_monitor_with_event()(
		address in valid_address(),
		min_value in 0u128..500000u128
	) -> (Monitor, U256) {
		let monitor = MonitorBuilder::new()
			.name("Test Monitor")
			.address_with_spec(address.as_str(), Some(ContractSpec::EVM(EVMContractSpec::from(json!([
				{
					"anonymous": false,
					"name": "Transfer",
					"type": "event",
					"inputs": [
						{
							"indexed": true,
							"internalType": "address",
							"name": "from",
							"type": "address"
						},
						{
							"indexed": true,
							"internalType": "address",
							"name": "to",
							"type": "address"
						},
						{
							"indexed": false,
							"internalType": "uint256",
							"name": "value",
							"type": "uint256"
						},
					],
				}
			])))))
			.event("Transfer(address,address,uint256)", Some(format!("value >= {}", min_value)))
			.build();
		(monitor, U256::from(min_value))
	}
}

// Generates event logs and ABIs for decode_events function testing
prop_compose! {
	fn generate_event_data()(
		address in valid_address(),
		event_type in prop_oneof![
			Just("Transfer"),
			Just("Approval"),
			Just("ValueChanged")
		],
		value in 0u128..1000000u128
	) -> (ContractSpec, EVMReceiptLog) {
		// Create address instances
		let contract_addr = Address::from_slice(&hex::decode(&address[2..]).unwrap());
		let from_addr = Address::from_slice(&hex::decode("0000000000000000000000000000000000000001").unwrap());
		let to_addr = Address::from_slice(&hex::decode("0000000000000000000000000000000000000002").unwrap());

		// Create contract spec with the proper ABI
		let abi = match event_type {
			"Transfer" => json!([{
				"anonymous": false,
				"name": "Transfer",
				"type": "event",
				"inputs": [
					{
						"indexed": true,
						"name": "from",
						"type": "address",
						"internalType": "address"
					},
					{
						"indexed": true,
						"name": "to",
						"type": "address",
						"internalType": "address"
					},
					{
						"indexed": false,
						"name": "value",
						"type": "uint256",
						"internalType": "uint256"
					}
				]
			}]),
			"Approval" => json!([{
				"anonymous": false,
				"name": "Approval",
				"type": "event",
				"inputs": [
					{
						"indexed": true,
						"name": "owner",
						"type": "address",
						"internalType": "address"
					},
					{
						"indexed": true,
						"name": "spender",
						"type": "address",
						"internalType": "address"
					},
					{
						"indexed": false,
						"name": "value",
						"type": "uint256",
						"internalType": "uint256"
					}
				]
			}]),
			"ValueChanged" => json!([{
				"anonymous": false,
				"name": "ValueChanged",
				"type": "event",
				"inputs": [
					{
						"indexed": false,
						"name": "value",
						"type": "uint256",
						"internalType": "uint256"
					}
				]
			}]),
			_ => unreachable!()
		};

		let contract_spec = ContractSpec::EVM(EVMContractSpec::from(abi));

		// Create receipt with the log
		let receipt_builder = ReceiptBuilder::new()
			.contract_address(contract_addr)
			.from(from_addr)
			.to(to_addr);

		// Special case for Transfer since ReceiptBuilder has direct support
		let receipt = if event_type == "Transfer" {
			receipt_builder.value(U256::from(value)).build()
		} else {
			// For other event types, create a custom log
			let event_signature = match event_type {
				"Approval" => "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925",
				"ValueChanged" => "0x93fe6d397c74fdf1402a8b72e47b68512f0510d7b98a4bc4cbdf6ac7108b3c59",
				_ => unreachable!()
			};

			let value_hex = format!("{:064x}", value);

			let alloy_log = alloy::primitives::Log {
				address: contract_addr,
				data: LogData::new_unchecked(
					match event_type {
						"ValueChanged" => vec![
							B256::from_str(event_signature).unwrap(),
						],
						_ => vec![
							B256::from_str(event_signature).unwrap(),
							B256::from_slice(&[&[0u8; 12], from_addr.as_slice()].concat()),
							B256::from_slice(&[&[0u8; 12], to_addr.as_slice()].concat()),
						],
					},
					Bytes(hex::decode(value_hex).unwrap().into()),
				),
			};

			let base_log = EVMReceiptLog::from(alloy_log);
			receipt_builder.logs(vec![base_log]).build()
		};

		// Return the contract spec and the first log from the receipt
		(contract_spec, receipt.0.logs[0].clone())
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	// Tests that function signatures match regardless of whitespace and case variations
	#[test]
	fn test_signature_normalization(
		sig1 in valid_signatures(),
		spaces in " *",
	) {
		// Test that function signatures match regardless of whitespace and case variations
		let with_spaces = sig1.chars()
			.flat_map(|c| vec![c, spaces.chars().next().unwrap_or(' ')])
			.collect::<String>();

		let sig2 = with_spaces.chars()
			.map(|c| if c.is_alphabetic() && rand::random() {
				c.to_ascii_uppercase()
			} else {
				c
			})
			.collect::<String>();

		prop_assert!(are_same_signature(&sig1, &sig2));
		prop_assert_eq!(normalize_signature(&sig1), normalize_signature(&sig2));
	}

	// Tests that addresses match regardless of checksum and prefix variations
	#[test]
	fn test_address_normalization(
		addr in "[0-9a-fA-F]{40}",
		prefix in prop_oneof![Just("0x"), Just("")],
	) {
		// Test that addresses match regardless of prefix and case
		let addr1 = format!("{}{}", prefix, addr);
		let addr2 = format!("0x{}", addr.to_uppercase());

		prop_assert!(are_same_address(&addr1, &addr2));
		prop_assert_eq!(
			normalize_address(&addr1),
			normalize_address(&addr2)
		);
	}

	// Tests that different function signatures don't incorrectly match
	#[test]
	fn test_invalid_signature(
		name1 in "[a-zA-Z][a-zA-Z0-9_]*",
		name2 in "[a-zA-Z][a-zA-Z0-9_]*",
		params in prop::collection::vec(
			prop_oneof![
				Just("address"),
				Just("uint256"),
				Just("string"),
				Just("bool"),
				Just("bytes32")
			],
			0..5
		),
	) {
		// Skip test if names happen to be identical
		prop_assume!(name1 != name2);

		// Test that different function names with same parameters don't match
		let sig1 = format!("{}({})", name1, params.join(","));
		let sig2 = format!("{}({})", name2, params.join(","));
		prop_assert!(!are_same_signature(&sig1, &sig2));

		// Test that same function name with different parameter counts don't match
		if !params.is_empty() {
			let shorter_params = params[..params.len()-1].join(",");
			let sig3 = format!("{}({})", name1, shorter_params);
			prop_assert!(!are_same_signature(&sig1, &sig3));
		}
	}

	// Tests address comparison expressions with equality operators
	#[test]
	fn test_address_expression_evaluation(
		addr1 in valid_address(),
		addr2 in valid_address(),
		operator in prop_oneof![Just("=="), Just("!=")],
	) {
		// Test address comparison expressions with equality operators
		let param_name = "from";
		let expr = format!("{} {} {}", param_name, operator, addr2);

		let params = vec![EVMMatchParamEntry {
			name: param_name.to_string(),
			value: addr1.clone(),
			kind: "address".to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = match operator {
			"==" => are_same_address(&addr1, &addr2),
			"!=" => !are_same_address(&addr1, &addr2),
			_ => false
		};

		prop_assert_eq!(result, expected);
	}

	// Tests numeric comparison expressions for uint256 values
	// Verifies all comparison operators work correctly with numeric values
	#[test]
	fn test_uint256_expression_evaluation(
		value in 0u128..1000000u128,
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to in 0u128..1000000u128,
	) {
		// Test numeric comparison expressions for uint256 values
		let expr = format!("amount {} {}", operator, compare_to);

		let params = vec![EVMMatchParamEntry {
			name: "amount".to_string(),
			value: value.to_string(),
			kind: "uint256".to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = match operator {
			">" => value > compare_to,
			">=" => value >= compare_to,
			"<" => value < compare_to,
			"<=" => value <= compare_to,
			"==" => value == compare_to,
			"!=" => value != compare_to,
			_ => false
		};

		prop_assert_eq!(result, expected);
	}

	// Tests string comparison expressions in filter conditions
	// Note: String comparisons are case-insensitive
	#[test]
	fn test_string_expression_evaluation(
		value_orig in "[a-zA-Z0-9_]+",
		operator in prop_oneof![
			Just("=="),
			Just("!="),
			Just("starts_with"),
			Just("ends_with"),
			Just("contains")
		],
		compare_to_orig in "[a-zA-Z0-9_]+",
	) {
		let rhs_for_expr = format!("'{}'", compare_to_orig.replace('\'', "\\'"));
		let expr = format!("name {} {}", operator, rhs_for_expr);

		let params = vec![EVMMatchParamEntry {
			name: "name".to_string(),
			value: value_orig.clone(),
			kind: "string".to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};

		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let value_normalized = value_orig.to_lowercase();
		let compare_to_normalized = compare_to_orig.to_lowercase();

		let expected = match operator {
			"==" => value_normalized == compare_to_normalized,
			"!=" => value_normalized != compare_to_normalized,
			"starts_with" => value_normalized.starts_with(&compare_to_normalized),
			"ends_with" => value_normalized.ends_with(&compare_to_normalized),
			"contains" => value_normalized.contains(&compare_to_normalized),
			_ => false
		};

		prop_assert_eq!(result, expected,
			"\nExpression: '{}'\nOriginal LHS: '{}'\nOriginal RHS: '{}'\nNormalized LHS: '{}'\nNormalized RHS: '{}'\nEvaluated: {}, Expected: {}",
			expr, value_orig, compare_to_orig, value_normalized, compare_to_normalized, result, expected
		);
	}

	// Tests boolean comparison expressions for true/false values
	// Verifies that boolean expressions are evaluated correctly
	#[test]
	fn test_bool_expression_evaluation(
		value in prop_oneof![Just("true"), Just("false")],
		operator in prop_oneof![Just("=="), Just("!=")],
		compare_to in prop_oneof![Just("true"), Just("false")],
	) {
		let expr = format!("is_active {} {}", operator, compare_to);

		let params = vec![EVMMatchParamEntry {
			name: "is_active".to_string(),
			value: value.to_string(),
			kind: "bool".to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = match operator {
			"==" => value == compare_to,
			"!=" => value != compare_to,
			_ => false
		};

		prop_assert_eq!(result, expected);
	}

	// Tests fixed-point number comparison expressions
	// Verifies all comparison operators work correctly with fixed-point numbers
	#[test]
	fn test_fixed_point_numbers_expression_evaluation(
		value in 0.1_f64..1000000.0_f64,
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to in 0.1_f64..1000000.0_f64,
	) {
		let expr = format!("amount {} {}", operator, compare_to);

		let params = vec![EVMMatchParamEntry {
			name: "amount".to_string(),
			value: value.to_string(),
			kind: "fixed".to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = match operator {
			">" => value > compare_to,
			">=" => value >= compare_to,
			"<" => value < compare_to,
			"<=" => value <= compare_to,
			"==" => value == compare_to,
			"!=" => value != compare_to,
			_ => false
		};

		prop_assert_eq!(result, expected);
	}

	// Tests signed integer comparison expressions (int8 to int256)
	// Verifies all comparison operators work correctly
	#[test]
	fn test_signed_int_expression_evaluation(
		value_i128 in (i128::MIN / 2)..(i128::MAX / 2),
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to_i128 in (i128::MIN / 2)..(i128::MAX / 2),
		signed_kind_str in prop_oneof![
			Just("int8"), Just("int16"), Just("int32"), Just("int64"),
			Just("int128"), Just("int256")
		]
	) {
		let param_name = "signedValue";
		let expr = format!("{} {} {}", param_name, operator, compare_to_i128);

		let params = vec![EVMMatchParamEntry {
			name: param_name.to_string(),
			value: value_i128.to_string(),
			kind: signed_kind_str.to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = match operator {
			">" => value_i128 > compare_to_i128,
			">=" => value_i128 >= compare_to_i128,
			"<" => value_i128 < compare_to_i128,
			"<=" => value_i128 <= compare_to_i128,
			"==" => value_i128 == compare_to_i128,
			"!=" => value_i128 != compare_to_i128,
			_ => false,
		};

		prop_assert_eq!(result, expected,
			"Expr: '{}', LHS Value: {}, Kind: {}, RHS Value: {}, Evaluated: {}, Expected: {}",
			expr, value_i128, signed_kind_str, compare_to_i128, result, expected
		);
	}

	// Tests unsigned integer comparison expressions (uint8 to uint128, and "number")
	// Verifies all comparison operators work correctly
	#[test]
	fn test_unsigned_int_expression_evaluation(
		value_u64 in 0u64..u64::MAX / 2,
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to_u64 in 0u64..u64::MAX / 2,
		unsigned_kind_str in prop_oneof![
			Just("uint8"), Just("uint16"), Just("uint32"), Just("uint64"),
			Just("uint128"),
			Just("number")
		]
	) {
		let param_name = "unsignedValue";
		let lhs_value_str = if unsigned_kind_str == "uint128" {
			(value_u64 as u128 * 1_000_000_000_000u128).to_string()
		} else {
			value_u64.to_string()
		};
		let rhs_value_str = if unsigned_kind_str == "uint128" {
			(compare_to_u64 as u128 * 1_000_000_000_000u128).to_string()
		} else {
			compare_to_u64.to_string()
		};

		let expr = format!("{} {} {}", param_name, operator, rhs_value_str);

		let params = vec![EVMMatchParamEntry {
			name: param_name.to_string(),
			value: lhs_value_str.clone(),
			kind: unsigned_kind_str.to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();
		let lhs_as_u128 = lhs_value_str.parse::<u128>().unwrap_or_default();
		let rhs_as_u128 = rhs_value_str.parse::<u128>().unwrap_or_default();

		let expected = match operator {
			">" => lhs_as_u128 > rhs_as_u128,
			">=" => lhs_as_u128 >= rhs_as_u128,
			"<" => lhs_as_u128 < rhs_as_u128,
			"<=" => lhs_as_u128 <= rhs_as_u128,
			"==" => lhs_as_u128 == rhs_as_u128,
			"!=" => lhs_as_u128 != rhs_as_u128,
			_ => false,
		};

		prop_assert_eq!(result, expected,
			"Expr: '{}', LHS Value: {}, Kind: {}, RHS Value: {}, Evaluated: {}, Expected: {}",
			expr, lhs_value_str, unsigned_kind_str, rhs_value_str, result, expected
		);
	}

	// Tests JSON array contains operation with integer values
	#[test]
	fn test_array_contains_i64_expression_evaluation(
			values in prop::collection::vec(any::<i64>(), 0..5),
			target in any::<i64>(),
	) {
			let param_name = "array_param";
			let value_str = serde_json::to_string(&values).unwrap();

			let expr = format!("{} contains {}", param_name, target);

			let params = vec![EVMMatchParamEntry {
					name: param_name.to_string(),
					value: value_str,
					kind: "array".to_string(),
					indexed: false,
			}];

			let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
					_client: PhantomData,
			};
			let result = filter.evaluate_expression(&expr, &params).unwrap();

			let expected = values.contains(&target);
			prop_assert_eq!(
					result, expected,
					"Failed on values: {:?}, target: {}, expected: {}",
					values, target, expected
			);
	}

	// Tests JSON array contains operation with string values
	#[test]
	fn test_array_contains_string_expression_evaluation(
			values in prop::collection::vec("[a-zA-Z0-9_]{1,8}", 0..5),
			target in "[a-zA-Z0-9_]{1,8}",
	) {
			let param_name = "array_param";
			let value_str = serde_json::to_string(&values).unwrap();

			let expr = format!(r#"{} contains "{}""#, param_name, target);

			let params = vec![EVMMatchParamEntry {
					name: param_name.to_string(),
					value: value_str,
					kind: "array".to_string(),
					indexed: false,
			}];

			let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
					_client: PhantomData,
			};
			let result = filter.evaluate_expression(&expr, &params).unwrap();
			// Normalize the target for comparison
			let target_lowercase = target.to_lowercase();
			let expected = values.iter().any(|v| v.to_lowercase() == target_lowercase);
			prop_assert_eq!(
					result, expected,
					"Failed on values: {:?}, target: {}, expected: {}",
					values, target, expected
			);
	}


	// Tests JSON array contains operation with mixed types
	#[test]
	fn test_vec_json_array_mixed_types_expression_evaluation(
			int_values in prop::collection::vec(any::<i64>(), 0..2),
			string_values in prop::collection::vec("[a-zA-Z0-9_]{1,8}", 0..2),
			target in prop_oneof![any::<i64>().prop_map(|v| v.to_string()), "[a-zA-Z0-9_]{1,8}"],
	) {
			let param_name = "array_param";

			// Create mixed type array with proper JSON representation
			let mut mixed_array = Vec::new();
			for v in &int_values {
					mixed_array.push(json!(v));
			}
			for v in &string_values {
					mixed_array.push(json!(v));
			}
			let value_str = serde_json::to_string(&mixed_array).unwrap();

			// Create expression with proper quoting based on target type
			let expr = if target.parse::<i64>().is_ok() {
					format!("{} contains {}", param_name, target)
			} else {
					format!(r#"{} contains "{}""#, param_name, target)
			};

			let params = vec![EVMMatchParamEntry {
					name: param_name.to_string(),
					value: value_str,
					kind: "array".to_string(),
					indexed: false,
			}];

			let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
					_client: PhantomData,
			};
			let result = filter.evaluate_expression(&expr, &params).unwrap();

			// Manually check for presence in original values
			let expected_str_match = string_values.iter().any(|s_val| s_val.eq_ignore_ascii_case(&target));
			let expected = int_values.iter().any(|v| v.to_string() == target) || expected_str_match;

			prop_assert_eq!(
					result, expected,
					"Failed on values: {:?}, target: {}, expected: {}",
					mixed_array, target, expected
			);
	}

	// Tests JSON array equality comparison
	#[test]
	fn test_vec_json_array_equality_expression_evaluation(
			values1 in prop::collection::vec(any::<i64>(), 0..5),
			values2 in prop::collection::vec(any::<i64>(), 0..5),
	) {
		let param_name = "array_param";
		let value_str1 = serde_json::to_string(&values1).unwrap();
		let value_str2 = serde_json::to_string(&values2).unwrap();

		// For single-quoted string literal in expression, escape single quotes
		let escaped_rhs_for_single_quotes = value_str2.replace('\'', r#"\'"#);
		let expr = format!(r#"{} == '{}'"#, param_name, escaped_rhs_for_single_quotes);

		let params = vec![EVMMatchParamEntry {
			name: param_name.to_string(),
			value: value_str1.clone(),
			kind: "array".to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = value_str1.eq_ignore_ascii_case(&value_str2);

		prop_assert_eq!(
			result, expected,
			"Failed on values1: {:?}, values2: {:?}, expected: {}",
			values1, values2, expected
		);
	}

	// Tests direct object comparison expressions
	#[test]
	fn test_map_json_object_eq_ne_expression_evaluation(
		lhs_json_map_str in prop_oneof![
			Just("{\"name\":\"alice\", \"id\":1}".to_string()),
			Just("{\"id\":1, \"name\":\"alice\"}".to_string()), // Same as above, different order
			Just("{\"name\":\"bob\", \"id\":2}".to_string()),
			Just("{\"city\":\"london\"}".to_string()),
			Just("{}".to_string()) // Empty object
		],
		rhs_json_map_str in prop_oneof![
			Just("{\"name\":\"alice\", \"id\":1}".to_string()),
			Just("{\"id\":1, \"name\":\"alice\"}".to_string()),
			Just("{\"name\":\"bob\", \"id\":2}".to_string()),
			Just("{\"city\":\"london\"}".to_string()),
			Just("{}".to_string()),
			Just("{\"name\":\"alice\"}".to_string()) // Partially different
		],
		operator in prop_oneof![Just("=="), Just("!=")],
	) {
		let expr = format!("map_param {} '{}'", operator, rhs_json_map_str);

		let params = vec![EVMMatchParamEntry {
			name: "map_param".to_string(),
			value: lhs_json_map_str.clone(),
			kind: "map".to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();
		let lhs_json_val = serde_json::from_str::<JsonValue>(&lhs_json_map_str).unwrap();
		let rhs_json_val = serde_json::from_str::<JsonValue>(&rhs_json_map_str).unwrap();

		let expected = match operator {
			"==" => lhs_json_val == rhs_json_val,
			"!=" => lhs_json_val != rhs_json_val,
			_ => unreachable!(),
		};

		prop_assert_eq!(result, expected);
	}

	// Tests logical AND combinations with mixed types
	// Verifies that combining numeric and address comparisons works correctly
	#[test]
	fn test_and_expression_evaluation(
		amount in 0u128..1000000u128,
		threshold in 0u128..1000000u128,
		addr in valid_address(),
	) {
		// Test logical AND combinations with mixed types (numeric and address)
		let expr = format!("amount >= {} AND recipient == {}", threshold, addr);

		let params = vec![
			EVMMatchParamEntry {
				name: "amount".to_string(),
				value: amount.to_string(),
				kind: "uint256".to_string(),
				indexed: false,
			},
			EVMMatchParamEntry {
				name: "recipient".to_string(),
				value: addr.clone(),
				kind: "address".to_string(),
				indexed: false,
			}
		];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = amount >= threshold && are_same_address(&addr, &addr);
		prop_assert_eq!(result, expected);
	}

	// Tests logical OR with range conditions
	// Verifies that value ranges can be properly checked using OR conditions
	#[test]
	fn test_or_expression_evaluation(
		amount in 0u128..1000000u128,
		threshold1 in 0u128..500000u128,
		threshold2 in 500001u128..1000000u128,
	) {
		// Test logical OR with range conditions
		let expr = format!("amount < {} OR amount > {}", threshold1, threshold2);

		let params = vec![EVMMatchParamEntry {
			name: "amount".to_string(),
			value: amount.to_string(),
			kind: "uint256".to_string(),
			indexed: false,
		}];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = amount < threshold1 || amount > threshold2;
		prop_assert_eq!(result, expected);
	}

	// Tests complex expressions combining AND/OR with parentheses
	// Verifies that nested logical operations work correctly with different types
	#[test]
	fn test_and_or_expression_evaluation(
		value1 in 0u128..1000000u128,
		value2 in 0u128..1000000u128,
		addr1 in valid_address(),
		addr2 in valid_address(),
		threshold in 500000u128..1000000u128,
	) {
		// Test complex expression combining AND/OR with parentheses
		let expr = format!(
			"(value1 > {} AND value2 < {}) OR (from == {} AND to == {})",
			threshold, threshold, addr1, addr2
		);

		let params = vec![
			EVMMatchParamEntry {
				name: "value1".to_string(),
				value: value1.to_string(),
				kind: "uint256".to_string(),
				indexed: false,
			},
			EVMMatchParamEntry {
				name: "value2".to_string(),
				value: value2.to_string(),
				kind: "uint256".to_string(),
				indexed: false,
			},
			EVMMatchParamEntry {
				name: "from".to_string(),
				value: addr1.clone(),
				kind: "address".to_string(),
				indexed: false,
			},
			EVMMatchParamEntry {
				name: "to".to_string(),
				value: addr2.clone(),
				kind: "address".to_string(),
				indexed: false,
			}
		];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = (value1 > threshold && value2 < threshold) ||
					  (are_same_address(&addr1, &addr1) && are_same_address(&addr2, &addr2));

		prop_assert_eq!(result, expected);
	}

	// Tests various invalid expression scenarios
	// Verifies proper handling of:
	// - Invalid operators
	// - Non-existent parameters
	// - Type mismatches
	// - Malformed expressions
	#[test]
	fn test_invalid_expressions(
		value in 0u128..1000000u128,
		addr in valid_address(),
	) {
		let params = vec![
			EVMMatchParamEntry {
				name: "amount".to_string(),
				value: value.to_string(),
				kind: "uint256".to_string(),
				indexed: false,
			},
			EVMMatchParamEntry {
				name: "recipient".to_string(),
				value: addr.clone(),
				kind: "address".to_string(),
				indexed: false,
			}
		];

		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};

		// Test various invalid expression scenarios
		let invalid_operator = format!("amount <=> {}", value);
		prop_assert!(filter.evaluate_expression(&invalid_operator, &params).is_err());

		let invalid_param = format!("nonexistent == {}", value);
		prop_assert!(filter.evaluate_expression(&invalid_param, &params).is_err());

		let invalid_comparison = format!("recipient > {}", value);
		prop_assert!(filter.evaluate_expression(&invalid_comparison, &params).is_err());

		let malformed = "amount > ".to_string();
		prop_assert!(filter.evaluate_expression(&malformed, &params).is_err());
	}

	// Tests transaction matching against monitor conditions
	// Verifies that transactions are correctly matched based on:
	// - Transaction status
	// - Value conditions
	// - Expression evaluation
	#[test]
	fn test_find_matching_transaction(
		tx in generate_transaction(),
		monitor in generate_monitor_with_transaction()
	) {
		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};

		// Test transaction matching across different status types
		for status in [TransactionStatus::Success, TransactionStatus::Failure, TransactionStatus::Any] {
			let mut matched_transactions = Vec::new();
			filter.find_matching_transaction(
				&status,
				&tx,
				&Some(ReceiptBuilder::new().build()),
				&monitor,
				&mut matched_transactions
			);

			// Verify matches based on monitor conditions and transaction status
			let value = tx.value.to::<u128>();
			let should_match = monitor.match_conditions.transactions.iter().any(|condition| {
				let status_matches = matches!(condition.status, TransactionStatus::Any) ||
								   condition.status == status;
				let mut expr_matches = true;

				if let Some(expr) = &condition.expression {
					expr_matches = filter.evaluate_expression(expr, &[
						EVMMatchParamEntry {
							name: "value".to_string(),
							value: value.to_string(),
							kind: "uint256".to_string(),
							indexed: false,
						}
					]).unwrap()
				}

				status_matches && expr_matches
			});

			prop_assert_eq!(!matched_transactions.is_empty(), should_match);
		}
	}

	// Tests transaction matching with empty conditions
	// Verifies default matching behavior when no conditions are specified
	#[test]
	fn test_find_matching_transaction_empty_conditions(
		tx in generate_transaction()
	) {
		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let mut matched_transactions = Vec::new();

		// Test that transactions match when no conditions are specified
		let monitor = MonitorBuilder::new().build();

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx,
			&Some(ReceiptBuilder::new().build()),
			&monitor,
			&mut matched_transactions
		);

		prop_assert_eq!(matched_transactions.len(), 1);
		prop_assert!(matched_transactions[0].expression.is_none());
		prop_assert!(matched_transactions[0].status == TransactionStatus::Any);
	}

	// Tests function matching in transactions
	// Verifies that function calls are correctly identified and matched based on:
	// - Function signatures
	// - Input data decoding
	// - Parameter evaluation
	#[test]
	fn test_find_matching_function_for_transaction(
		monitor in generate_monitor_with_function()
	) {
		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let mut matched_functions = Vec::new();
		let mut matched_args = EVMMatchArguments {
			events: None,
			functions: Some(Vec::new()),
		};

		// Create transaction with specific function call data
		let monitor_address = Address::from_slice(&hex::decode(&monitor.addresses[0].address[2..]).unwrap());
		let store_signature = [96, 87, 54, 29];  // store(uint256) function selector
		let mut input_data = store_signature.to_vec();
		let value = U256::from(600000u128);
		let bytes: [u8; 32] = value.to_be_bytes();
		input_data.extend_from_slice(&bytes);

		let tx = EVMTransaction(EVMBaseTransaction {
			to: Some(monitor_address),
			input: Bytes::from(input_data),
			..Default::default()
		});

		// Create contract spec matching the function
		let contract_specs = vec![(
			monitor.addresses[0].address.clone(),
			EVMContractSpec::from(json!([
				{
					"inputs": [
						{
							"internalType": "uint256",
							"name": "value",
							"type": "uint256"
						}
					],
					"name": "store",
					"outputs": [],
					"stateMutability": "nonpayable",
					"type": "function"
				}
			]))
		)];

		filter.find_matching_functions_for_transaction(
			&contract_specs,
			&tx,
			&monitor,
			&mut matched_functions,
			&mut matched_args
		);

		let should_match = monitor.match_conditions.functions.iter().any(|f|
			f.signature == "store(uint256)"
		);

		prop_assert_eq!(!matched_functions.is_empty(), should_match);
	}

	// Tests event matching in transactions
	// Verifies that event logs are correctly identified and matched based on:
	// - Event signatures
	// - Log data decoding
	// - Parameter evaluation
	#[test]
	fn test_find_matching_event_for_transaction(
		(monitor, min_value) in generate_monitor_with_event()
	) {
		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};
		let mut matched_events = Vec::new();
		let mut matched_args = EVMMatchArguments {
			events: Some(Vec::new()),
			functions: None,
		};

		// Create transaction with specific function call data
		let monitor_address = Address::from_slice(&hex::decode(&monitor.addresses[0].address[2..]).unwrap());

		let tx_receipt = ReceiptBuilder::new()
			.contract_address(monitor_address)
			.from(Address::from_slice(&hex::decode("0000000000000000000000000000000000001234").unwrap()))
			.to(Address::from_slice(&hex::decode("0000000000000000000000000000000000005678").unwrap()))
			.value(U256::from(min_value))
			.build();

		filter.find_matching_events_for_transaction(
			&tx_receipt.logs,
			&monitor,
			&mut matched_events,
			&mut matched_args,
			&mut monitor.addresses.iter().map(|a| a.address.clone()).collect()
		);


		let should_match = monitor.match_conditions.events.iter().any(|e|
			e.signature == "Transfer(address,address,uint256)"
		);
		prop_assert_eq!(matched_events.len(), 1);
		prop_assert_eq!(!matched_events.is_empty(), should_match);
	}

	// Tests the decode_events function with different event types
	// Verifies that event logs are correctly decoded based on the ABI
	#[test]
	fn test_decode_events(
		(contract_spec, log) in generate_event_data()
	) {
		let filter = EVMBlockFilter::<EvmClient<EVMTransportClient>> {
			_client: PhantomData,
		};

		// Decode the event
		let decoded = filter.decode_events(&contract_spec, &log);
		prop_assert!(decoded.is_some());

		if let Some(result) = decoded {
			// Verify signature is properly formatted
			prop_assert!(result.signature.contains('('));
			prop_assert!(result.signature.contains(')'));

			// Verify we have arguments
			prop_assert!(result.args.is_some());
			let args = result.args.unwrap();
			prop_assert!(!args.is_empty());

			// Check parameters by event type
			match result.signature.as_str() {
				signature if signature.starts_with("Transfer") => {
					// Check that we have the right number of parameters and types
					prop_assert_eq!(args.len(), 3);
					prop_assert_eq!(args[0].kind.as_str(), "address");
					prop_assert_eq!(args[1].kind.as_str(), "address");
					prop_assert_eq!(args[2].kind.as_str(), "uint256");
				},
				signature if signature.starts_with("Approval") => {
					// Check that we have the right number of parameters and types
					prop_assert_eq!(args.len(), 3);
					prop_assert_eq!(args[0].kind.as_str(), "address");
					prop_assert_eq!(args[1].kind.as_str(), "address");
					prop_assert_eq!(args[2].kind.as_str(), "uint256");
				},
				signature if signature.starts_with("ValueChanged") => {
					// Check that we have the right number of parameters and types
					prop_assert_eq!(args.len(), 1);
					prop_assert_eq!(args[0].kind.as_str(), "uint256");
				},
				_ => {
					// Should not reach here with our test data
					prop_assert!(false, "Unexpected event signature: {}", result.signature);
				}
			}

			// Verify hex signature is present
			prop_assert!(result.hex_signature.is_some());
		}
	}
}
