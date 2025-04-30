//! Property-based tests for Stellar transaction matching and filtering.
//! Tests cover signature/address normalization, expression evaluation, and transaction matching.

use base64::Engine;
use openzeppelin_monitor::{
	models::{
		AddressWithABI, FunctionCondition, Monitor, StellarDecodedTransaction,
		StellarMatchArguments, StellarMatchParamEntry, StellarTransaction, StellarTransactionInfo,
		TransactionStatus,
	},
	services::{
		blockchain::{StellarClient, StellarTransportClient},
		filter::{
			stellar_helpers::{
				are_same_address, are_same_signature, normalize_address, normalize_signature,
			},
			StellarBlockFilter,
		},
	},
	utils::tests::stellar::monitor::MonitorBuilder,
};
use proptest::{prelude::*, test_runner::Config};
use serde_json::{json, Value};
use std::{marker::PhantomData, str::FromStr};
use stellar_strkey::Contract;
use stellar_xdr::curr::{
	AccountId, HostFunction, Int128Parts, InvokeContractArgs, InvokeHostFunctionOp, Memo,
	Operation, OperationBody, Preconditions, ScAddress, ScVal, StringM,
	Transaction as XdrTransaction, TransactionEnvelope, TransactionExt, TransactionV1Envelope,
	VecM,
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
		format!("{} {} {}", param_position, operator, value)
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
			.function(format!("{}({})", function_name, param_type).as_str(), Some(format!("0 >= {}", min_value)))
			.function(format!("not_{}({})", function_name, param_type).as_str(), Some(format!("0 >= {}", min_value)))
			.build()
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
		let param_name = "0";
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
		let result = filter.evaluate_expression(&expr, &Some(params));

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
		let param_name = "0";
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
		let result = filter.evaluate_expression(&expr, &Some(params));

		let expected = match operator {
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
		let param_name = "0";
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

	// Tests vector operations (contains, equality) in filter expressions
	#[test]
	fn test_vec_expression_evaluation(
		values in prop::collection::vec(any::<i64>(), 0..5),
		operator in prop_oneof![Just("contains"), Just("=="), Just("!=")],
		compare_to in any::<i64>(),
	) {
		let param_name = "0";
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
		let result = filter.evaluate_expression(&expr, &Some(params));

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

	// Tests map/object property access in filter expressions
	#[test]
	fn test_map_expression_evaluation(
		key in "[a-zA-Z][a-zA-Z0-9_]*",
		value in any::<u64>(),
		operator in prop_oneof![Just("=="), Just("!=")],
		compare_to in any::<u64>(),
	) {
		let param_name = "0";
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
		let result = filter.evaluate_expression(&expr, &Some(params));

		let expected = match operator {
			"==" => value == compare_to,
			"!=" => value != compare_to,
			_ => false
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
		let expr = format!("0 >= {} AND 1 == {}", threshold, addr);

		let params = vec![
			StellarMatchParamEntry {
				name: "0".to_string(),
				value: amount.to_string(),
				kind: "I128".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "1".to_string(),
				value: addr.clone(),
				kind: "Address".to_string(),
				indexed: false,
			}
		];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &Some(params));

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
		let expr = format!("0 < {} OR 0 > {}", threshold1, threshold2);

		let params = vec![StellarMatchParamEntry {
			name: "0".to_string(),
			value: amount.to_string(),
			kind: "I128".to_string(),
			indexed: false,
		}];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &Some(params));

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
			"(0 > {} AND 1 < {}) OR (2 == {} AND 3 == {})",
			threshold, threshold, addr1, addr2
		);

		let params = vec![
			StellarMatchParamEntry {
				name: "0".to_string(),
				value: value1.to_string(),
				kind: "I128".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "1".to_string(),
				value: value2.to_string(),
				kind: "I128".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "2".to_string(),
				value: addr1.clone(),
				kind: "address".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "3".to_string(),
				value: addr2.clone(),
				kind: "address".to_string(),
				indexed: false,
			}
		];

		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let result = filter.evaluate_expression(&expr, &Some(params));

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
				name: "0".to_string(),
				value: value.to_string(),
				kind: "I128".to_string(),
				indexed: false,
			},
			StellarMatchParamEntry {
				name: "1".to_string(),
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
		let invalid_operator = format!("0 <=> {}", value);
		prop_assert!(!filter.evaluate_expression(&invalid_operator, &Some(params.clone())));

		// 2. Non-existent parameter reference
		let invalid_param = format!("2 == {}", value);
		prop_assert!(!filter.evaluate_expression(&invalid_param, &Some(params.clone())));

		// 3. Type mismatch in comparison
		let invalid_comparison = format!("1 > {}", value);
		prop_assert!(!filter.evaluate_expression(&invalid_comparison, &Some(params.clone())));

		// 4. Syntactically incomplete expression
		let malformed = "0 > ".to_string();
		prop_assert!(!filter.evaluate_expression(&malformed, &Some(params)));
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
					filter.evaluate_expression(expr, &Some(tx_params))
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

	// Tests function matching in transactions against monitor conditions
	#[test]
	fn test_find_matching_function_for_transaction(
		monitor in generate_monitor_with_function(),
		envelope in generate_envelope(),
		tx in generate_transaction(),
	) {
		let filter = StellarBlockFilter::<StellarClient<StellarTransportClient>> {
			_client: PhantomData,
		};
		let mut matched_functions = Vec::new();
		let mut matched_args = StellarMatchArguments {
			events: None,
			functions: Some(Vec::new()),
		};

		// Inject test envelope into transaction
		let mut transaction = tx;
		transaction.0.decoded = Some(StellarDecodedTransaction {
			envelope: Some(envelope),
			result: None,
			meta: None,
		});

		let mut updated_monitor = monitor;

		// Add test cases for known contract addresses and function signatures
		if rand::random() {
			updated_monitor.addresses.push(AddressWithABI {
				address: "CAVLP5DH2GJPZMVO7IJY4CVOD5MWEFTJFVPD2YY2FQXOQHRGHK4D6HLP".to_string(),
				abi: None,
			});
			updated_monitor.match_conditions.functions.push(FunctionCondition {
				signature: "transfer(I128)".to_string(),
				expression: Some("0 >= 100".to_string()),
			});
		}

		let monitored_addresses = updated_monitor.addresses.iter()
			.map(|addr| normalize_address(&addr.address))
			.collect::<Vec<String>>();

		filter.find_matching_functions_for_transaction(
			&monitored_addresses,
			&transaction,
			&updated_monitor,
			&mut matched_functions,
			&mut matched_args,
		);

		// Determine if transaction should match by checking:
		// 1. Transaction has decoded data
		// 2. Contains valid host function invocation
		// 3. Contract address matches monitored addresses
		// 4. Function signature matches conditions
		let should_match = if let Some(decoded) = &transaction.0.decoded {
			if let Some(TransactionEnvelope::Tx(tx)) = &decoded.envelope {
				if let Some(operation) = tx.tx.operations.first() {
					if let OperationBody::InvokeHostFunction(invoke) = &operation.body {
						if let HostFunction::InvokeContract(args) = &invoke.host_function {
							let arg_type = args.args.first().map(|arg| match arg {
								ScVal::Address(_) => "Address",
								ScVal::I128(_) => "I128",
								ScVal::U128(_) => "U128",
								ScVal::String(_) => "String",
								ScVal::Bool(_) => "Bool",
								ScVal::Bytes(_) => "Bytes",
								ScVal::Symbol(_) => "Symbol",
								_ => "Unknown"
							}).unwrap_or("Unknown");

							let function_signature = format!("{}({})", args.function_name.0, arg_type);
							let contract_address = normalize_address(&args.contract_address.to_string());

							let address_matches = monitored_addresses.contains(&contract_address);
							let function_matches = updated_monitor.match_conditions.functions.iter().any(|condition| {
								are_same_signature(&condition.signature, &function_signature)
							});

							address_matches && function_matches
						} else {
							false
						}
					} else {
						false
					}
				} else {
					false
				}
			} else {
				false
			}
		} else {
			false
		};

		prop_assert_eq!(!matched_functions.is_empty(), should_match);
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
			Value::Number(serde_json::Number::from(int_value)),
			Value::Number(serde_json::Number::from(uint_value)),
			Value::Bool(bool_value),
			Value::String(string_value.to_string()),
		];


		let params = filter.convert_arguments_to_match_param_entry(&arguments);

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

		let params = filter.convert_arguments_to_match_param_entry(&arguments);

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

		let params = filter.convert_arguments_to_match_param_entry(&arguments);

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

		let typed_params = filter.convert_arguments_to_match_param_entry(&typed_arguments);

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

		let params = filter.convert_arguments_to_match_param_entry(&arguments);

		// Verify empty input produces empty output
		prop_assert!(params.is_empty());
	}

}
