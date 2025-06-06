//! Property-based tests for Stellar transaction matching and filtering.
//! Tests cover signature/address normalization, expression evaluation, and transaction matching.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use openzeppelin_monitor::{
	models::{
		Monitor, StellarContractFunction, StellarContractInput, StellarDecodedTransaction,
		StellarEvent, StellarFormattedContractSpec, StellarMatchArguments, StellarMatchParamEntry,
		StellarMatchParamsMap, StellarTransaction, StellarTransactionInfo, TransactionStatus,
	},
	services::{
		blockchain::{StellarClient, StellarTransportClient},
		filter::{
			stellar_helpers::{
				are_same_address, are_same_signature, normalize_address, normalize_signature,
			},
			EventMap, StellarBlockFilter,
		},
	},
	utils::tests::stellar::monitor::MonitorBuilder,
};
use proptest::{prelude::*, test_runner::Config};
use serde_json::{json, Value as JsonValue};
use std::{marker::PhantomData, str::FromStr};
use stellar_strkey::Contract;
use stellar_xdr::curr::{
	AccountId, Hash, HostFunction, Int128Parts, InvokeContractArgs, InvokeHostFunctionOp, Memo,
	MuxedAccount, Operation, OperationBody, Preconditions, ScAddress, ScString, ScSymbol, ScVal,
	StringM, Transaction as XdrTransaction, TransactionEnvelope, TransactionExt,
	TransactionV1Envelope, UInt128Parts, Uint256, VecM, WriteXdr,
};

prop_compose! {
	// Generates valid Stellar function signatures with random parameters
	fn valid_signatures()(
		name in "[a-zA-Z][a-zA-Z0-9_]*",
		count in 0..5usize
	)(
		name in Just(name),
		params in prop::collection::vec(
			prop_oneof![
				Just("Address"),
				Just("I128"),
				Just("U128"),
				Just("String"),
				Just("Bool"),
				Just("Bytes"),
				Just("Symbol"),
				Just("Vec<Address>"),
				Just("Vec<I128>"),
				Just("Map<String,I128>")
			],
			count..=count
		)
	) -> String {
		format!("{}({})", name, params.join(","))
	}
}

prop_compose! {
	// Generates random valid Stellar contract addresses
	fn valid_address()(_: ()) -> String {
		let random_bytes: [u8; 32] = rand::random();
		Contract(random_bytes).to_string()
	}
}

prop_compose! {
	// Generates comparison expressions for testing parameter matching
	fn valid_expression()(
		param_position in 0..3,
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		value in 0u128..1000000u128
	) -> String {
		format!("param{} {} {}", param_position, operator, value)
	}
}

prop_compose! {
	// Generates Stellar transaction envelopes with common contract functions
	fn generate_envelope()(
		address in prop_oneof![
			Just("CAVLP5DH2GJPZMVO7IJY4CVOD5MWEFTJFVPD2YY2FQXOQHRGHK4D6HLP"),
			Just("CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA"),
		],
		function_name in prop_oneof![
			Just("transfer"),
			Just("transferFrom"),
			Just("setApprovalForAll"),
			Just("isApprovedForAll"),
			Just("balanceOf"),
			Just("allowance"),
		],
		value in 0u128..1000000u128,
	) -> TransactionEnvelope {
		let arg = ScVal::I128(Int128Parts {
			hi: value as i64,
			lo: 0,
		});
		let args = VecM::<ScVal, { u32::MAX }>::try_from(vec![arg]).unwrap();
		let invoke_host_function = InvokeHostFunctionOp {
			host_function: HostFunction::InvokeContract(InvokeContractArgs {
				contract_address: ScAddress::Contract(Contract::from_str(address).unwrap().0.into()),
				function_name: StringM::<32>::from_str(function_name).unwrap().into(),
				args,
			}),
			auth: Default::default(),
		};

		let operation = Operation {
			source_account: None,
			body: OperationBody::InvokeHostFunction(invoke_host_function),
		};

		let operations = VecM::<Operation, 100>::try_from(vec![operation]).unwrap();

		let account_seed: [u8; 32] = rand::random();
		let source_account = stellar_strkey::ed25519::PublicKey(account_seed).to_string();

		let xdr_tx = XdrTransaction {
			source_account: AccountId::from_str(&source_account).unwrap().into(),
			fee: 100,
			seq_num: 1.into(),
			operations,
			cond: Preconditions::None,
			memo: Memo::None,
			ext: TransactionExt::V0,
		};

		TransactionEnvelope::Tx(TransactionV1Envelope {
			tx: xdr_tx,
			signatures: Default::default(),
		})
	}
}

prop_compose! {
	// Generates mock Stellar transactions with various states and metadata
	fn generate_transaction()(
		hash in "[a-zA-Z0-9]{64}",
		value in 0u128..1000000u128,
		from_addr in valid_address(),
		to_addr in valid_address(),
		input_data in prop::collection::vec(any::<u8>(), 0..100),
		status in prop_oneof![Just("SUCCESS"), Just("FAILED")]
	) -> StellarTransaction {
		let envelope_json = serde_json::json!({
			"type": "ENVELOPE_TYPE_TX",
			"value": {
				"tx": {
					"sourceAccount": from_addr,
					"operations": [{
						"type": "INVOKE_HOST_FUNCTION",
						"value": value,
						"auth": [{
							"address": to_addr
						}]
					}]
				}
			}
		});

		let transaction_info = StellarTransactionInfo {
				status: status.to_string(),
				transaction_hash: hash,
				application_order: 1,
				fee_bump: false,
				envelope_xdr: Some(base64::engine::general_purpose::STANDARD.encode(&input_data)),
				envelope_json: Some(envelope_json.clone()),
				result_xdr: Some(base64::engine::general_purpose::STANDARD.encode(&input_data)),
				result_json: Some(serde_json::json!({
					"result": status
				})),
				result_meta_xdr: Some(base64::engine::general_purpose::STANDARD.encode(&input_data)),
				result_meta_json: Some(serde_json::json!({
					"meta": "data"
				})),
				diagnostic_events_xdr: Some(vec![
					base64::engine::general_purpose::STANDARD.encode(&input_data)
				]),
				diagnostic_events_json: Some(vec![
					serde_json::json!({
						"event": "diagnostic",
						"sourceAccount": from_addr,
						"targetAccount": to_addr
					})
				]),
			ledger: 1234,
			ledger_close_time: 1234567890,
			decoded: None,
		};

		StellarTransaction::from(transaction_info)
	}
}

prop_compose! {
	// Generates basic monitor configuration
	fn generate_base_monitor()(
		address in valid_address(),
	) -> Monitor {
		MonitorBuilder::new()
			.name("Test Monitor")
			.addresses(vec![address])
			.build()
	}
}

prop_compose! {
	// Generates monitor configured to match specific transaction hashes
	fn generate_monitor_with_transaction()(
		address in valid_address(),
		hash in "[a-zA-Z0-9]{64}",
	) -> Monitor {
		MonitorBuilder::new()
			.name("Test Monitor")
			.addresses(vec![address])
			.transaction(TransactionStatus::Success, Some(format!("hash == {}", hash)))
			.transaction(TransactionStatus::Failure, Some(format!("hash != {}", hash)))
			.build()
	}
}

prop_compose! {
	// Generates monitor configured to match specific contract functions and parameters
	fn generate_monitor_with_function()(
		address in valid_address(),
		function_name in prop_oneof![
			Just("transfer"),
			Just("transferFrom"),
			Just("setApprovalForAll"),
			Just("isApprovedForAll"),
			Just("balanceOf"),
			Just("allowance"),
		],
		param_type in prop_oneof![
			Just("Address"),
			Just("I128"),
			Just("U128"),
			Just("String"),
			Just("Bool"),
			Just("Bytes"),
			Just("Symbol"),
		],
		min_value in 0u128..500000u128
	) -> Monitor {
		MonitorBuilder::new()
			.name("Test Monitor")
			.addresses(vec![address])
			.function(format!("{}({})", function_name, param_type).as_str(), Some(format!("param0 >= {}", min_value)))
			.function(format!("not_{}({})", function_name, param_type).as_str(), Some(format!("param0 >= {}", min_value)))
			.build()
	}
}

// Generates valid JSON objects of different complexity
prop_compose! {
	fn valid_json_objects()(
		num_fields in 1..5usize
	) -> String {
		let mut obj = serde_json::Map::new();
		for i in 0..num_fields {
			let key = format!("key{}", i);
			let value = match i % 3 {
				0 => serde_json::Value::String(format!("value{}", i)),
				1 => serde_json::Value::Number(serde_json::Number::from(i)),
				_ => {
					let mut nested = serde_json::Map::new();
					nested.insert("nested_key".into(), serde_json::Value::String(format!("nested_value{}", i)));
					serde_json::Value::Object(nested)
				}
			};
			obj.insert(key, value);
		}
		serde_json::Value::Object(obj).to_string()
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	// Tests signature normalization across different whitespace and case variations
	#[test]
	fn test_signature_normalization(
		sig1 in valid_signatures(),
		spaces in " *",
	) {
		// Create signature variation with random spaces between characters
		let with_spaces = sig1.chars()
			.flat_map(|c| vec![c, spaces.chars().next().unwrap_or(' ')])
			.collect::<String>();

		// Create signature variation with random case changes
		let sig2 = with_spaces.chars()
			.map(|c| if c.is_alphabetic() && rand::random() {
				c.to_ascii_uppercase()
			} else {
				c
			})
			.collect::<String>();

		// Test that signatures match regardless of spacing and case
		prop_assert!(are_same_signature(&sig1, &sig2));
		prop_assert_eq!(normalize_signature(&sig1), normalize_signature(&sig2));
	}

	// Tests address normalization across different formats and case variations
	#[test]
	fn test_address_normalization(
		base_address in valid_address(),
		spaces in " \t\n\r*",
	) {
		// Create variations of the address with different case and whitespace
		let address_with_spaces = format!("{}{}{}{}", spaces, base_address, spaces, spaces);
		let address_mixed_case = base_address.chars()
			.enumerate()
			.map(|(i, c)| if i % 2 == 0 { c.to_ascii_lowercase() } else { c.to_ascii_uppercase() })
			.collect::<String>();

		// Verify address normalization handles whitespace and case variations
		prop_assert!(are_same_address(&base_address, &address_with_spaces));
		prop_assert!(are_same_address(&base_address, &address_mixed_case));
		prop_assert!(are_same_address(&address_with_spaces, &address_mixed_case));

		let normalized = normalize_address(&base_address);
		prop_assert_eq!(normalized.clone(), normalize_address(&address_with_spaces));
		prop_assert_eq!(normalized, normalize_address(&address_mixed_case));
	}

	// Verifies that different function signatures don't incorrectly match
	#[test]
	fn test_invalid_signature(
		name1 in "[a-zA-Z][a-zA-Z0-9_]*",
		name2 in "[a-zA-Z][a-zA-Z0-9_]*",
		params in prop::collection::vec(
			prop_oneof![
				Just("Address"),
				Just("I128"),
				Just("U128"),
				Just("String"),
				Just("Bool"),
				Just("Bytes"),
				Just("Symbol"),
			],
			0..5
		),
	) {
		prop_assume!(name1 != name2);

		// Test different function names with same parameters
		let sig1 = format!("{}({})", name1, params.join(","));
		let sig2 = format!("{}({})", name2, params.join(","));
		prop_assert!(!are_same_signature(&sig1, &sig2));

		// Test same function name with different parameter counts
		if !params.is_empty() {
			let shorter_params = params[..params.len()-1].join(",");
			let sig3 = format!("{}({})", name1, shorter_params);
			prop_assert!(!are_same_signature(&sig1, &sig3));
		}
	}

	// Tests address comparison expressions in filter conditions
	#[test]
	fn test_address_expression_evaluation(
		addr1 in valid_address(),
		addr2 in valid_address(),
		operator in prop_oneof![Just("=="), Just("!=")],
	) {
		let param_name = "param0";
		let expr = format!("{} {} {}", param_name, operator, addr2);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: addr1.clone(),
			kind: "address".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		// Test address comparison based on normalized form
		let expected = match operator {
			"==" => are_same_address(&addr1, &addr2),
			"!=" => !are_same_address(&addr1, &addr2),
			_ => false
		};

		prop_assert_eq!(result, expected);
	}

	// Tests boolean expression evaluation in filter conditions
	#[test]
	fn test_bool_expression_evaluation(
		value in any::<bool>(),
		operator in prop_oneof![Just("=="), Just("!=")],
		compare_to in any::<bool>(),
	) {
		let param_name = "param0";
		let expr = format!("{} {} {}", param_name, operator, compare_to);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: value.to_string(),
			kind: "bool".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

		let params = vec![StellarMatchParamEntry {
			name: "name".to_string(),
			value: value_orig.clone(),
			kind: "string".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};

		let result = filter.evaluate_expression(&expr, &params).unwrap();

		// Normalize for expected result according to StellarConditionEvaluator::compare_string logic
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

	// Tests numeric comparison expressions for i32 values
	#[test]
	fn test_i32_expression_evaluation(
		value in any::<i32>(),
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to in any::<i32>(),
	) {
		let param_name = "param0";
		let expr = format!("{} {} {}", param_name, operator, compare_to);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: value.to_string(),
			kind: "i32".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

	// Tests numeric comparison expressions for i64 values
	#[test]
	fn test_i64_expression_evaluation(
		value in any::<i64>(),
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to in any::<i64>(),
	) {
		let param_name = "param0";
		let expr = format!("{} {} {}", param_name, operator, compare_to);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: value.to_string(),
			kind: "i64".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

	// Tests numeric comparison expressions for i128 values
	#[test]
	fn test_i128_expression_evaluation(
		value in any::<i128>(),
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to in any::<i128>(),
	) {
		let param_name = "param0";
		let expr = format!("{} {} {}", param_name, operator, compare_to);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: value.to_string(),
			kind: "i128".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

	// Tests numeric comparison expressions for u32 values
	#[test]
	fn test_u32_expression_evaluation(
		value in any::<u32>(),
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to in any::<u32>(),
	) {
		let param_name = "param0";
		let expr = format!("{} {} {}", param_name, operator, compare_to);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: value.to_string(),
			kind: "u32".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

	// Tests numeric comparison expressions for u64 values
	#[test]
	fn test_u64_expression_evaluation(
		value in any::<u64>(),
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to in any::<u64>(),
	) {
		let param_name = "param0";
		let expr = format!("{} {} {}", param_name, operator, compare_to);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: value.to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

	// Tests numeric comparison expressions for u128 values
	#[test]
	fn test_u128_expression_evaluation(
		value in any::<u128>(),
		operator in prop_oneof![
			Just(">"), Just(">="), Just("<"), Just("<="),
			Just("=="), Just("!=")
		],
		compare_to in any::<u128>(),
	) {
		let param_name = "param0";
		let expr = format!("{} {} {}", param_name, operator, compare_to);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: value.to_string(),
			kind: "u128".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

	// Tests vector operations (contains, equality) in filter expressions with CSV format
	#[test]
	fn test_vec_csv_expression_evaluation(
		values in prop::collection::vec(any::<i64>(), 0..5),
		operator in prop_oneof![Just("contains"), Just("=="), Just("!=")],
		compare_to in any::<i64>(),
	) {
		let param_name = "param0";
		// Convert vector to comma-separated string for parameter value
		let value_str = values.iter()
			.map(|v| v.to_string())
			.collect::<Vec<_>>()
			.join(",");

		let expr = format!("{} {} {}", param_name, operator, compare_to);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: value_str.clone(),
			kind: "vec".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		// Handle different vector operations: contains checks for membership,
		// equality operators compare string representation
		let expected = match operator {
			"contains" => values.contains(&compare_to),
			"==" => value_str == compare_to.to_string(),
			"!=" => value_str != compare_to.to_string(),
			_ => false
		};

		prop_assert_eq!(result, expected);
	}

	// Tests JSON array contains operation with integer values
	#[test]
	fn test_vec_json_array_contains_i64_expression_evaluation(
			values in prop::collection::vec(any::<i64>(), 0..5),
			target in any::<i64>(),
	) {
			let param_name = "vec_param";
			let value_str = serde_json::to_string(&values).unwrap();

			let expr = format!("{} contains {}", param_name, target);

			let params = vec![StellarMatchParamEntry {
					name: param_name.to_string(),
					value: value_str,
					kind: "Vec".to_string(),
					indexed: false,
			}];

			let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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
	fn test_vec_json_array_contains_string_expression_evaluation(
			values in prop::collection::vec("[a-zA-Z0-9_]{1,8}", 0..5),
			target in "[a-zA-Z0-9_]{1,8}",
	) {
			let param_name = "vec_param";
			let value_str = serde_json::to_string(&values).unwrap();

			let expr = format!(r#"{} contains "{}""#, param_name, target);

			let params = vec![StellarMatchParamEntry {
					name: param_name.to_string(),
					value: value_str,
					kind: "Vec".to_string(),
					indexed: false,
			}];

			let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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
			let param_name = "vec_param";

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

			let params = vec![StellarMatchParamEntry {
					name: param_name.to_string(),
					value: value_str,
					kind: "Vec".to_string(),
					indexed: false,
			}];

			let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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
		let param_name = "vec_param";
		let value_str1 = serde_json::to_string(&values1).unwrap();
		let value_str2 = serde_json::to_string(&values2).unwrap();

		// For single-quoted string literal in expression, escape single quotes
		let escaped_rhs_for_single_quotes = value_str2.replace('\'', r#"\'"#);
		let expr = format!(r#"{} == '{}'"#, param_name, escaped_rhs_for_single_quotes);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: value_str1.clone(),
			kind: "Vec".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

	// Tests nested JSON array contains operation
	#[test]
	fn test_vec_nested_json_array_contains_expression_evaluation(
			outer_values in prop::collection::vec(
					prop::collection::vec(any::<i64>(), 1..3),
					1..3
			),
			target in any::<i64>(),
	) {
			let param_name = "vec_param";
			let value_str = serde_json::to_string(&outer_values).unwrap();

			let expr = format!("{} contains {}", param_name, target);

			let params = vec![StellarMatchParamEntry {
					name: param_name.to_string(),
					value: value_str,
					kind: "Vec".to_string(),
					indexed: false,
			}];

			let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
					_client: PhantomData,
			};
			let result = filter.evaluate_expression(&expr, &params).unwrap();

			// Check if target exists in any nested array
			let expected = outer_values.iter()
					.any(|inner| inner.contains(&target));

			prop_assert_eq!(
					result, expected,
					"Failed on values: {:?}, target: {}, expected: {}",
					outer_values, target, expected
			);
	}

	// Tests map/object property access in filter expressions
	#[test]
	fn test_map_expression_evaluation(
		key in "[a-zA-Z][a-zA-Z0-9_]*",
		value in any::<u64>(),
		operator in prop_oneof![Just("=="), Just("!=")],
		compare_to in any::<u64>(),
	) {
		let param_name = "param0";
		// Create JSON object with single key-value pair
		let map_value = serde_json::json!({
			&key: value
		});

		// Test property access using dot notation
		let expr = format!("{}.{} {} {}", param_name, key, operator, compare_to);

		let params = vec![StellarMatchParamEntry {
			name: param_name.to_string(),
			value: map_value.to_string(),
			kind: "U64".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

		let params = vec![StellarMatchParamEntry {
			name: "map_param".to_string(),
			value: lhs_json_map_str.clone(),
			kind: "map".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
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

	// Tests logical AND combinations in filter expressions
	#[test]
	fn test_and_expression_evaluation(
		amount in 0u128..1000000u128,
		threshold in 0u128..1000000u128,
		addr in valid_address(),
	) {
		let expr = format!("param0 >= {} AND param1 == {}", threshold, addr);

		let params = vec![
			StellarMatchParamEntry {
				name: "param0".to_string(),
				value: amount.to_string(),
				kind: "I128".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "param1".to_string(),
				value: addr.clone(),
				kind: "Address".to_string(),
				indexed: false,
			}
		];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = amount >= threshold && are_same_address(&addr, &addr);
		prop_assert_eq!(result, expected);
	}

	// Tests logical OR combinations in filter expressions
	#[test]
	fn test_or_expression_evaluation(
		amount in 0u128..1000000u128,
		threshold1 in 0u128..500000u128,
		threshold2 in 500001u128..1000000u128,

	) {
		let expr = format!("param0 < {} OR param0 > {}", threshold1, threshold2);

		let params = vec![StellarMatchParamEntry {
			name: "param0".to_string(),
			value: amount.to_string(),
			kind: "I128".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		let expected = amount < threshold1 || amount > threshold2;
		prop_assert_eq!(result, expected);
	}


	// Tests complex combinations of AND/OR expressions
	#[test]
	fn test_and_or_expression_evaluation(
		value1 in 0u128..1000000u128,
		value2 in 0u128..1000000u128,
		addr1 in valid_address(),
		addr2 in valid_address(),
		threshold in 500000u128..1000000u128,
	) {
		// Tests complex expression: (numeric comparison AND numeric comparison) OR (address equality AND address equality)
		let expr = format!(
			"(param0 > {} AND param1 < {}) OR (param2 == {} AND param3 == {})",
			threshold, threshold, addr1, addr2
		);

		let params = vec![
			StellarMatchParamEntry {
				name: "param0".to_string(),
				value: value1.to_string(),
				kind: "I128".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "param1".to_string(),
				value: value2.to_string(),
				kind: "I128".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "param2".to_string(),
				value: addr1.clone(),
				kind: "address".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "param3".to_string(),
				value: addr2.clone(),
				kind: "address".to_string(),
				indexed: false,
			}
		];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &params).unwrap();

		// Expected result combines numeric threshold checks with address equality checks
		let expected = (value1 > threshold && value2 < threshold) ||
					  (are_same_address(&addr1, &addr1) && are_same_address(&addr2, &addr2));

		prop_assert_eq!(result, expected);
	}

	// Verifies proper handling of malformed/invalid expressions
	#[test]
	fn test_invalid_expressions(
		value in 0u128..1000000u128,
		addr in valid_address(),
	) {
		let params = vec![
			StellarMatchParamEntry {
				name: "param0".to_string(),
				value: value.to_string(),
				kind: "I128".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "param1".to_string(),
				value: addr.clone(),
				kind: "address".to_string(),
				indexed: false,
			}
		];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};

		// Test cases for expression validation:
		// 1. Invalid operator syntax
		let invalid_operator = format!("param0 <=> {}", value);
		prop_assert!(filter.evaluate_expression(&invalid_operator, &params).is_err());

		// 2. Non-existent parameter reference
		let invalid_param = format!("param2 == {}", value);
		prop_assert!(filter.evaluate_expression(&invalid_param, &params).is_err());

		// 3. Type mismatch in comparison
		let invalid_comparison = format!("param1 > {}", value);
		prop_assert!(filter.evaluate_expression(&invalid_comparison, &params).is_err());

		// 4. Syntactically incomplete expression
		let malformed = "param0 > ".to_string();
		prop_assert!(filter.evaluate_expression(&malformed, &params).is_err());
	}

	// Tests transaction matching against monitor conditions
	#[test]
	fn test_find_matching_transaction(
		tx in generate_transaction(),
		monitor in generate_monitor_with_transaction(),
	) {
		let mut matched_transactions = Vec::new();
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};

		filter.find_matching_transaction(&tx, &monitor, &mut matched_transactions);

		// Determine match by checking:
		// 1. Status match (Any, Success, or Failure)
		// 2. Expression evaluation if present
		let expected_matches = monitor.match_conditions.transactions.iter().any(|condition| {
			let status_matches = match condition.status {
				TransactionStatus::Any => true,
				required_status => {
					let tx_status = match tx.status.as_str() {
						"SUCCESS" => TransactionStatus::Success,
						"FAILED" | "NOT_FOUND" => TransactionStatus::Failure,
						_ => TransactionStatus::Any,
					};
					required_status == tx_status
				}
			};

			if status_matches {
				if let Some(expr) = &condition.expression {
					let tx_params = vec![
						StellarMatchParamEntry {
							name: "hash".to_string(),
							value: tx.hash().to_string(),
							kind: "string".to_string(),
							indexed: false,
						}
					];
					filter.evaluate_expression(expr, &tx_params).unwrap()
				} else {
					true
				}
			} else {
				false
			}
		});

		prop_assert_eq!(!matched_transactions.is_empty(), expected_matches);
	}

	// Verifies default matching behavior with empty conditions
	#[test]
	fn test_find_matching_transaction_empty_conditions(
		tx in generate_transaction()

	) {
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let mut matched_transactions = Vec::new();

		// Create monitor with empty conditions
		let monitor = MonitorBuilder::new().build();

		filter.find_matching_transaction(
			&tx,
			&monitor,
			&mut matched_transactions
		);

		// Should match when no conditions are specified
		prop_assert_eq!(matched_transactions.len(), 1);
		prop_assert!(matched_transactions[0].expression.is_none());
		prop_assert!(matched_transactions[0].status == TransactionStatus::Any);
	}

	// Tests function for finding matching functions for transactions
	#[test]
	fn test_find_matching_functions_for_transactions(
		// Generate a base contract address
		contract_address in valid_address(),
		// Generate a function name from common Stellar contract functions
		function_name in prop_oneof![
			Just("transfer"),
			Just("approve"),
			Just("mint"),
			Just("burn"),
			Just("setAdmin")
		],
		// Generate parameter type
		param_type in prop_oneof![
			Just("Address"),
			Just("I128"),
			Just("U128"),
			Just("String"),
			Just("Bool"),
			Just("Bytes"),
			Just("Symbol")
		],
		// Generate a value for expression testing
		value in 0u128..1000000u128,
		// Generate a threshold for expression testing
		threshold in 0u128..1000000u128,
	) {
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};

		// Create the function signature
		let function_signature = format!("{}({})", function_name, param_type);

		// Create monitor with the function condition
		let monitor = MonitorBuilder::new()
			.addresses(vec![contract_address.clone()])
			.function(&function_signature, Some(format!("param0 >= {}", threshold)))
			.build();

		let monitored_addresses = vec![normalize_address(&contract_address)];

		// Create contract spec matching the function
		let contract_specs = vec![(
			contract_address.clone(),
			StellarFormattedContractSpec {
				functions: vec![StellarContractFunction {
					signature: function_signature.clone(),
					name: function_name.to_string(),
					inputs: vec![StellarContractInput {
						name: "param0".to_string(),
						kind: param_type.to_string(),
						index: 0,
					}],
				}],
			},
		)];

		// Create ScVal argument based on param_type
		let arg = match param_type {
			"I128" => ScVal::I128(Int128Parts {
				hi: (value >> 64) as i64,
				lo: value as u64,
			}),
			"U128" => ScVal::U128(UInt128Parts {
				hi: ((value >> 64) & 0x7FFFFFFFFFFFFFFF) as u64,
				lo: (value & 0xFFFFFFFFFFFFFFFF) as u64,
			}),
			"Bool" => ScVal::Bool(value % 2 == 0),
			"String" => ScVal::String(ScString(format!("value_{}", value).try_into().unwrap())),
			"Bytes" => ScVal::Bytes(vec![value as u8].try_into().unwrap()),
			"Symbol" => ScVal::Symbol(ScSymbol(StringM::<32>::from_str(&format!("SYM_{}", value)).unwrap())),
			"Address" => ScVal::Address(ScAddress::Contract(Hash([0u8; 32]))),
			_ => ScVal::I128(Int128Parts { hi: 0, lo: 0 }),
		};

		// Create transaction with host function invocation
		let invoke_host_function = InvokeHostFunctionOp {
			host_function: HostFunction::InvokeContract(InvokeContractArgs {
				contract_address: ScAddress::Contract(Contract::from_str(&contract_address).unwrap().0.into()),
				function_name: StringM::<32>::from_str(function_name).unwrap().into(),
				args: vec![arg].try_into().unwrap(),
			}),
			auth: Default::default(),
		};

		let operation = Operation {
			source_account: None,
			body: OperationBody::InvokeHostFunction(invoke_host_function),
		};

		let tx = XdrTransaction {
			source_account: MuxedAccount::Ed25519(Uint256::from([0; 32])),
			fee: 100,
			seq_num: 1.into(),
			operations: vec![operation].try_into().unwrap(),
			cond: Preconditions::None,
			memo: Memo::None,
			ext: TransactionExt::V0,
		};

		let envelope = TransactionEnvelope::Tx(TransactionV1Envelope {
			tx,
			signatures: Default::default(),
		});

		let transaction = StellarTransaction(StellarTransactionInfo {
			status: "SUCCESS".to_string(),
			transaction_hash: "test_hash".to_string(),
			application_order: 1,
			fee_bump: false,
			envelope_xdr: None,
			envelope_json: None,
			result_xdr: None,
			result_json: None,
			result_meta_xdr: None,
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
		});

		let mut matched_functions = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: None,
			functions: Some(Vec::new()),
		};

		// Call the function under test
		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
			&contract_specs,
			&transaction,
			&monitor,
			&mut matched_functions,
			&mut matched_args,
		);

		// Determine if we should have a match
		let should_match = match param_type {
			"I128" | "U128" => value >= threshold,
			_ => false, // Expression evaluation only works for numeric types
		};

		// Verify the results
		if should_match {
			// Should have exactly one match
			prop_assert_eq!(matched_functions.len(), 1);
			prop_assert_eq!(matched_functions[0].signature.clone(), function_signature.clone());
			prop_assert_eq!(matched_functions[0].expression.clone(), Some(format!("param0 >= {}", threshold)));

			// Verify matched arguments
			if let Some(functions) = &matched_args.functions {
				prop_assert_eq!(functions.len(), 1);
				prop_assert_eq!(functions[0].signature.clone(), function_signature.clone());
				prop_assert!(functions[0].args.is_some());
			}
		} else {
			// Should have no matches
			prop_assert!(matched_functions.is_empty());
			if let Some(functions) = &matched_args.functions {
				prop_assert!(functions.is_empty());
			}
		}
	}

	// Tests conversion of primitive types to match parameters
	#[test]
	fn test_convert_primitive_arguments(
		// Generate only negative numbers for int_value
		int_value in (-1000000i64..=-1i64),
		uint_value in any::<u64>(),
		bool_value in any::<bool>(),
		string_value in "[a-zA-Z0-9]*",
	) {
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};

		// Create array of JSON values with explicit types
		let arguments = vec![
			JsonValue::Number(serde_json::Number::from(int_value)),
			JsonValue::Number(serde_json::Number::from(uint_value)),
			JsonValue::Bool(bool_value),
			JsonValue::String(string_value.to_string()),
		];

		let function_spec = StellarContractFunction::default();
		let params = filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		// Verify correct number of parameters
		prop_assert_eq!(params.len(), 4);

		// Check integer parameter
		prop_assert_eq!(&params[0].name, "0");
		prop_assert_eq!(&params[0].kind, "I64");
		prop_assert_eq!(&params[0].value, &int_value.to_string());
		prop_assert!(!params[0].indexed);

		// Check unsigned integer parameter
		prop_assert_eq!(&params[1].name, "1");
		prop_assert_eq!(&params[1].kind, "U64");
		prop_assert_eq!(&params[1].value, &uint_value.to_string());
		prop_assert!(!params[1].indexed);

		// Check boolean parameter
		prop_assert_eq!(&params[2].name, "2");
		prop_assert_eq!(&params[2].kind, "Bool");
		prop_assert_eq!(&params[2].value, &bool_value.to_string());
		prop_assert!(!params[2].indexed);

		// Check string parameter
		prop_assert_eq!(&params[3].name, "3");
		prop_assert_eq!(&params[3].kind, "String");
		prop_assert_eq!(&params[3].value, &string_value);
		prop_assert!(!params[3].indexed);
	}

	// Tests conversion of array arguments to match parameters
	#[test]
	fn test_convert_array_arguments(
		values in prop::collection::vec(any::<i64>(), 1..5),
	) {
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let arguments = vec![json!(values)];

		let function_spec = StellarContractFunction::default();
		let params = filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		// Verify array conversion to parameter entry
		prop_assert_eq!(params.len(), 1);
		prop_assert_eq!(&params[0].name, "0");
		prop_assert_eq!(&params[0].kind, "Vec");

		let expected_value = serde_json::to_string(&values).unwrap();
		prop_assert_eq!(&params[0].value, &expected_value);
		prop_assert!(!params[0].indexed);
	}

	// Tests conversion of object/map arguments to match parameters
	#[test]
	fn test_convert_object_arguments(
		key in "[a-zA-Z][a-zA-Z0-9_]*",
		value in any::<i64>(),
	) {
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};

		// Test regular object to parameter conversion
		let map = json!({
			key: value
		});
		let arguments = vec![map.clone()];

		let function_spec = StellarContractFunction::default();
		let params = filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		prop_assert_eq!(params.len(), 1);
		prop_assert_eq!(&params[0].name, "0");
		prop_assert_eq!(&params[0].kind, "Map");
		let expected_value = serde_json::to_string(&map).unwrap();
		prop_assert_eq!(&params[0].value, &expected_value);
		prop_assert!(!params[0].indexed);

		// Test typed object structure conversion
		let typed_obj = json!({
			"type": "Address",
			"value": "GBXGQJWVLWOYHFLPTKWV3FUHH7LYGHJPHGMODPXX2JYG2LOHG5EDPIWP"
		});
		let typed_arguments = vec![typed_obj];

		let function_spec = StellarContractFunction::default();
		let typed_params = filter.convert_arguments_to_match_param_entry(&typed_arguments, Some(&function_spec));

		prop_assert_eq!(typed_params.len(), 1);
		prop_assert_eq!(&typed_params[0].name, "0");
		prop_assert_eq!(&typed_params[0].kind, "Address");
		prop_assert_eq!(&typed_params[0].value, "GBXGQJWVLWOYHFLPTKWV3FUHH7LYGHJPHGMODPXX2JYG2LOHG5EDPIWP");
		prop_assert!(!typed_params[0].indexed);
	}


	// Verifies proper handling of empty argument lists
	#[test]
	fn test_convert_empty_arguments(_ in prop::collection::vec(any::<i64>(), 0..1)) {
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let arguments = Vec::new();

		let function_spec = StellarContractFunction::default();
		let params = filter.convert_arguments_to_match_param_entry(&arguments, Some(&function_spec));

		// Verify empty input produces empty output
		prop_assert!(params.is_empty());
	}

	// Tests property-based matching for event functions
	#[test]
	fn test_find_matching_events_for_transaction_property(
		// Generate a transaction hash
		tx_hash in "[a-zA-Z0-9]{64}",
		// Generate an event name
		event_name in prop_oneof![
			Just("Transfer"),
			Just("Approval"),
			Just("Mint"),
			Just("Burn"),
			Just("AdminChanged")
		],
		// Generate parameter type (only use numeric types for simplicity)
		param_type in prop_oneof![
			Just("I128"),
			Just("U128"),
		],
		// Generate a value for expression testing
		value in 0u128..1000000u128,
		// Generate a threshold for expression testing
		threshold in 0u128..1000000u128,
	) {
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};

		// Create the event signature
		let event_signature = format!("{}({})", event_name, param_type);

		// Create monitor with the event condition
		let monitor = MonitorBuilder::new()
			.event(&event_signature, Some(format!("param0 >= {}", threshold)))
			.build();

		// Create transaction
		let transaction = StellarTransaction::from(StellarTransactionInfo {
			status: "SUCCESS".to_string(),
			transaction_hash: tx_hash.clone(),
			application_order: 1,
			fee_bump: false,
			envelope_xdr: None,
			envelope_json: None,
			result_xdr: None,
			result_json: None,
			result_meta_xdr: None,
			result_meta_json: None,
			diagnostic_events_xdr: None,
			diagnostic_events_json: None,
			ledger: 1234,
			ledger_close_time: 1234567890,
			decoded: None,
		});

		// Create event with the matching transaction hash
		let test_event = EventMap {
			event: StellarMatchParamsMap {
				signature: event_signature.clone(),
				args: Some(vec![
					StellarMatchParamEntry {
						name: "param0".to_string(),
						value: value.to_string(),
						kind: param_type.to_string(),
						indexed: false,
					}
				]),
			},
			tx_hash: tx_hash.clone(),
		};

		let events = vec![test_event];
		let mut matched_events = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: Some(Vec::new()),
			functions: Some(Vec::new()),
		};

		// Call the function under test
		filter.find_matching_events_for_transaction(
			&events,
			&transaction,
			&monitor,
			&mut matched_events,
			&mut matched_args,
		);

		// Determine if we should have a match based on the expression
		let should_match = value >= threshold;

		// Verify the results
		if should_match {
			// Should have exactly one match
			prop_assert_eq!(matched_events.len(), 1);
			prop_assert_eq!(&matched_events[0].signature, &event_signature);
			prop_assert_eq!(&matched_events[0].expression, &Some(format!("param0 >= {}", threshold)));

			// Verify matched arguments
			if let Some(events) = &matched_args.events {
				prop_assert_eq!(events.len(), 1);
				prop_assert_eq!(&events[0].signature, &event_signature);
				prop_assert!(events[0].args.is_some());
			}
		} else {
			// Should have no matches
			prop_assert!(matched_events.is_empty());
			if let Some(events) = &matched_args.events {
				prop_assert!(events.is_empty());
			}
		}
	}

	// Tests property-based matching for decode_events
	#[test]
	fn test_decode_events_property(
		// Generate a contract address
		contract_address in valid_address(),
		// Generate an event name
		event_name in prop_oneof![
			Just("Transfer"),
			Just("Approval"),
			Just("Mint"),
			Just("Burn")
		],
		// Generate a transaction hash
		tx_hash in "[a-zA-Z0-9]{64}",
		// Generate a value for expression testing
		value in 0u64..u64::MAX,
	) {
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};

		// Create a buffer for event name encoding (8 byte prefix + name)
		let mut event_name_buffer = vec![0u8; 8];
		event_name_buffer.extend_from_slice(event_name.as_bytes());
		let encoded_event_name = BASE64.encode(event_name_buffer);

		// Create I128 value separately
		let value_i128 = Int128Parts { hi: 0, lo: value };
		let sc_val = ScVal::I128(value_i128);
		let encoded_value = sc_val.to_xdr_base64(stellar_xdr::curr::Limits::none()).unwrap();

		// Create test event
		let stellar_event = StellarEvent {
			contract_id: contract_address.clone(),
			transaction_hash: tx_hash.clone(),
			topic_xdr: Some(vec![encoded_event_name.clone()]),
			value_xdr: Some(encoded_value.clone()),
			event_type: "contract".to_string(),
			ledger: 1234,
			ledger_closed_at: "2023-01-01T00:00:00Z".to_string(),
			id: "0".to_string(),
			paging_token: "0".to_string(),
			in_successful_contract_call: true,
			topic_json: None,
			value_json: None,
		};

		let monitored_addresses = vec![normalize_address(&contract_address)];
		let events = vec![stellar_event];
		let contract_specs = Vec::new();

		// Run the function under test
		let decoded_events = filter.decode_events(&events, &monitored_addresses, &contract_specs);
		// Verify the results
		prop_assert_eq!(decoded_events.len(), 1);
		prop_assert_eq!(&decoded_events[0].tx_hash, &tx_hash);

		let decoded_event = &decoded_events[0].event;
		prop_assert!(decoded_event.signature.starts_with(event_name));
		prop_assert!(decoded_event.signature.contains("I128"));

		// Verify the args are properly decoded
		if let Some(args) = &decoded_event.args {
			prop_assert_eq!(args.len(), 1);
			prop_assert_eq!(&args[0].kind, "I128");
			prop_assert_eq!(&args[0].name, "0");
			prop_assert!(!args[0].indexed);
			prop_assert_eq!(&args[0].value, &value.to_string());
		}
	}
}
