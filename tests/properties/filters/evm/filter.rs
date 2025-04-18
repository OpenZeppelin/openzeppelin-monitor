//! Property-based tests for EVM transaction matching and filtering.
//! Tests cover signature/address normalization, expression evaluation, and transaction matching.

use alloy::primitives::{Address, Bytes, U256};
use std::marker::PhantomData;

use openzeppelin_monitor::{
	models::{
		AddressWithABI, EVMBaseTransaction, EVMMatchArguments, EVMMatchParamEntry, EVMTransaction,
		FunctionCondition, MatchConditions, Monitor, TransactionCondition, TransactionStatus,
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
};
use proptest::{prelude::*, test_runner::Config};
use serde_json::json;

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
		Monitor {
			name: "Test Monitor".to_string(),
			addresses: vec![AddressWithABI {
				address,
				abi: None,
			}],
			..Default::default()
		}
	}
}

// Generates monitor configured with transaction value thresholds and status conditions
prop_compose! {
	fn generate_monitor_with_transaction()(
		address in valid_address(),
		min_value in 0u128..500000u128,
		max_value in 500001u128..1000000u128
	) -> Monitor {
		Monitor {
			name: "Test Monitor".to_string(),
			addresses: vec![AddressWithABI {
				address,
				abi: None,
			}],
			match_conditions: MatchConditions {
				transactions: vec![
					TransactionCondition {
						expression: Some(format!("value >= {}", min_value)),
						status: TransactionStatus::Success,
					},
					TransactionCondition {
						expression: Some(format!("value < {}", max_value)),
						status: TransactionStatus::Any,
					},
				],
				functions: vec![],
				events: vec![],
			},
			..Default::default()
		}
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
		Monitor {
			name: "Test Monitor".to_string(),
			addresses: vec![AddressWithABI {
				address,
				abi: Some(json!([
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
						"inputs": [],
						"name": "retrieve",
						"outputs": [
						  {
							"internalType": "uint256",
							"name": "",
							"type": "uint256"
						  }
						],
						"stateMutability": "view",
						"type": "function"
					  },
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
				])),
			}],
			match_conditions: MatchConditions {
				transactions: vec![],
				functions: vec![
					FunctionCondition {
						signature: format!("{}({})", function_name, param_type),
						expression: Some(format!("value >= {}", min_value)),
					},
					// Extra function that should never match
					FunctionCondition {
						signature: format!("not_{}({})", function_name, param_type),
						expression: Some(format!("value >= {}", min_value)),
					},
				],
				events: vec![],
			},
			..Default::default()
		}
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
		let result = filter.evaluate_expression(&expr, &Some(params));

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
		let result = filter.evaluate_expression(&expr, &Some(params));

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
		let result = filter.evaluate_expression(&expr, &Some(params));

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
		let result = filter.evaluate_expression(&expr, &Some(params));

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
		let result = filter.evaluate_expression(&expr, &Some(params));

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
		prop_assert!(!filter.evaluate_expression(&invalid_operator, &Some(params.clone())));

		let invalid_param = format!("nonexistent == {}", value);
		prop_assert!(!filter.evaluate_expression(&invalid_param, &Some(params.clone())));

		let invalid_comparison = format!("recipient > {}", value);
		prop_assert!(!filter.evaluate_expression(&invalid_comparison, &Some(params.clone())));

		let malformed = "amount > ".to_string();
		prop_assert!(!filter.evaluate_expression(&malformed, &Some(params)));
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
					expr_matches = filter.evaluate_expression(expr, &Some(vec![
						EVMMatchParamEntry {
							name: "value".to_string(),
							value: value.to_string(),
							kind: "uint256".to_string(),
							indexed: false,
						}
					]))
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
		let monitor = Monitor {
			match_conditions: MatchConditions {
				transactions: vec![],
				functions: vec![],
				events: vec![],
			},
			..Default::default()
		};

		filter.find_matching_transaction(
			&TransactionStatus::Success,
			&tx,
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

		filter.find_matching_functions_for_transaction(
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
}
