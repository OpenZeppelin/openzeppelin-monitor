//! Property-based tests for EVM transaction matching and filtering.
//! Tests cover signature/address normalization, expression evaluation, and transaction matching.

use std::str::FromStr;

use crate::properties::filters::evm::strings_evaluator::create_evaluator;
use ethabi::Token;
use openzeppelin_monitor::services::filter::{
	evm_helpers::{format_token_value, string_to_h256},
	ComparisonOperator, ConditionEvaluator, LiteralValue,
};
use proptest::{prelude::*, test_runner::Config};
use rust_decimal::Decimal;
use serde_json::json;

// Generator for ethabi Token values
prop_compose! {
	fn generate_token()(
		token_type in prop_oneof![
			Just("address"),
			Just("bytes"),
			Just("uint"),
			Just("bool"),
			Just("string"),
			Just("array"),
		],
		value in any::<u64>(),
		string_value in "[a-zA-Z0-9]{1,10}",
		bytes_len in 1..32usize
	) -> Token {
		match token_type {
			"address" => Token::Address(ethabi::Address::from_low_u64_be(value)),
			"bytes" => {
				let bytes = (0..bytes_len).map(|i| ((i as u64 + value) % 256) as u8).collect::<Vec<u8>>();
				Token::Bytes(bytes)
			},
			"uint" => Token::Uint(ethabi::Uint::from(value)),
			"bool" => Token::Bool(value % 2 == 0),
			"string" => Token::String(string_value),
			"array" => {
				let elements = vec![
					Token::Uint(ethabi::Uint::from(value)),
					Token::Uint(ethabi::Uint::from(value + 1)),
				];
				Token::Array(elements)
			},
			_ => Token::Uint(ethabi::Uint::from(0)),
		}
	}
}

prop_compose! {
	/// Generate valid Ethereum addresses (0x + 40 hex chars)
	fn arb_eth_address()(s in "[0-9a-fA-F]{40}") -> String {
		format!("0x{}", s)
	}
}

prop_compose! {
	/// Generate valid bytes32 values (0x + 64 hex chars)
	fn arb_bytes32()(s in "[0-9a-fA-F]{64}") -> String {
		format!("0x{}", s)
	}
}

prop_compose! {
	/// Generate valid hex bytes of various lengths (not address or bytes32)
	fn arb_hex_bytes()(
		len in 1usize..100usize,
		s in "[0-9a-fA-F]*"
	) -> String {
		let trimmed = if s.len() > len { &s[..len] } else { &s };
		// Ensure it's not 40 or 64 chars to avoid address/bytes32 classification
		let adjusted_len = if trimmed.len() == 40 || trimmed.len() == 64 {
			trimmed.len() + 1
		} else {
			trimmed.len()
		};
		let hex_part = format!("{:0<width$}", trimmed, width = adjusted_len);
		format!("0x{}", hex_part)
	}
}

prop_compose! {
	/// Generate decimal strings with fractional parts
	fn arb_decimal_string()(
		int_part in -999999i64..999999i64,
		frac_part in 0u32..999999u32
	) -> String {
		format!("{}.{}", int_part, frac_part)
	}
}

prop_compose! {
	/// Generate arbitrary string that's not hex, not decimal
	fn arb_regular_string()(s in "[^0][a-zA-Z][a-zA-Z0-9 _-]*") -> String {
		s
	}
}

// Generator for valid hex strings (32 bytes = 64 hex chars for B256)
prop_compose! {
	fn generate_valid_hex_string()(
		bytes in prop::collection::vec(any::<u8>(), 32),
		include_prefix in any::<bool>()
	) -> String {
		let hex_string = hex::encode(bytes);
		if include_prefix {
			format!("0x{}", hex_string)
		} else {
			hex_string
		}
	}
}

// Generator for valid hex strings (20 bytes = 40 hex chars for Address/H160)
prop_compose! {
	fn generate_valid_address_hex_string()(
		bytes in prop::collection::vec(any::<u8>(), 20),
		include_prefix in any::<bool>()
	) -> String {
		let hex_string = hex::encode(bytes);
		if include_prefix {
			format!("0x{}", hex_string)
		} else {
			hex_string
		}
	}
}

// Generator for invalid hex strings for addresses
prop_compose! {
	fn generate_invalid_address_hex_string()(
		variant in prop_oneof![
			// Too short
			prop::collection::vec(any::<u8>(), 1..20).prop_map(hex::encode),
			// Too long
			prop::collection::vec(any::<u8>(), 21..50).prop_map(hex::encode),
			// Invalid characters
			Just("0xZZZZ456789abcdef0123456789abcdef01234567".to_string()),
			Just("not_hex_at_all".to_string()),
			Just("0x".to_string()),
			Just("".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_value_for_kind()(
		kind in "[a-z]+",
		seed in any::<u32>()
	) -> String {
		match kind.as_str() {
			"uint256" | "number" => {
				if seed % 3 == 0 {
					format!("0x{:x}", seed % 1000000)
				} else {
					(seed % 1000000).to_string()
				}
			},
			"int256" => {
				let val = (seed % 1000000) as i32 - 500000;
				if seed % 3 == 0 {
					if val >= 0 {
						format!("0x{:x}", val)
					} else {
						format!("-0x{:x}", -val)
					}
				} else {
					val.to_string()
				}
			},
			"address" => {
				let bytes: Vec<u8> = (0..20).map(|i| ((seed + i) % 256) as u8).collect();
				format!("0x{}", hex::encode(bytes))
			},
			"string" | "bytes" | "bytes32" => {
				format!("test_string_{}", seed % 1000)
			},
			"bool" => {
				if seed % 2 == 0 { "true".to_string() } else { "false".to_string() }
			},
			"fixed" | "ufixed" => {
				let int_part = (seed % 1000) as i32 - 500;
				let frac_part = seed % 1000000;
				format!("{}.{}", int_part, frac_part)
			},
			"array" => {
				format!("[{}, {}, {}]", seed % 100, (seed + 1) % 100, (seed + 2) % 100)
			},
			"map" => {
				format!("{{\"key1\": {}, \"key2\": \"value_{}\"}}", seed % 100, seed % 1000)
			},
			_ => "default_value".to_string()
		}
	}
}

prop_compose! {
	fn generate_compatible_operator()(
		kind in "[a-z]+",
		seed in any::<u8>()
	) -> ComparisonOperator {
		match kind.as_str() {
			"uint256" | "int256" | "number" | "fixed" | "ufixed" => {
				match seed % 6 {
					0 => ComparisonOperator::Eq,
					1 => ComparisonOperator::Ne,
					2 => ComparisonOperator::Gt,
					3 => ComparisonOperator::Gte,
					4 => ComparisonOperator::Lt,
					_ => ComparisonOperator::Lte,
				}
			},
			"address" | "bool" => {
				if seed % 2 == 0 { ComparisonOperator::Eq } else { ComparisonOperator::Ne }
			},
			"string" | "bytes" | "bytes32" => {
				match seed % 5 {
					0 => ComparisonOperator::Eq,
					1 => ComparisonOperator::Ne,
					2 => ComparisonOperator::StartsWith,
					3 => ComparisonOperator::EndsWith,
					_ => ComparisonOperator::Contains,
				}
			},
			"array" | "map" => {
				match seed % 3 {
					0 => ComparisonOperator::Eq,
					1 => ComparisonOperator::Ne,
					_ => ComparisonOperator::Contains,
				}
			},
			_ => ComparisonOperator::Eq
		}
	}
}

prop_compose! {
	fn generate_compatible_literal()(
		kind in "[a-z]+",
		seed in any::<u32>()
	) -> LiteralValue<'static> {
		let leaked_str = match kind.as_str() {
			"uint256" | "int256" | "number" | "fixed" | "ufixed" => {
				let val = (seed % 1000000).to_string();
				Box::leak(val.into_boxed_str())
			},
			"address" => {
				let bytes: Vec<u8> = (0..20).map(|i| ((seed + i + 100) % 256) as u8).collect();
				let addr = format!("0x{}", hex::encode(bytes));
				Box::leak(addr.into_boxed_str())
			},
			"string" | "bytes" | "bytes32" | "array" | "map" => {
				let val = format!("literal_value_{}", seed % 1000);
				Box::leak(val.into_boxed_str())
			},
			"bool" => {
				return LiteralValue::Bool(seed % 2 == 0);
			},
			_ => {
				let val = "default_literal".to_string();
				Box::leak(val.into_boxed_str())
			}
		};

		// Mix Number and Str literals
		if seed % 2 == 0 && matches!(kind.as_str(), "uint256" | "int256" | "number" | "fixed" | "ufixed") {
			LiteralValue::Number(leaked_str)
		} else if kind == "bool" {
			LiteralValue::Bool(seed % 2 == 0)
		} else {
			LiteralValue::Str(leaked_str)
		}
	}
}

prop_compose! {
	fn generate_valid_evm_kind()(
		variant in prop_oneof![
			// Unsigned integers
			Just("uint8"), Just("uint16"), Just("uint32"), Just("uint64"),
			Just("uint128"), Just("uint256"), Just("number"),
			// Signed integers
			Just("int8"), Just("int16"), Just("int32"), Just("int64"),
			Just("int128"), Just("int256"),
			// Arrays
			Just("array"), Just("uint8[]"), Just("uint16[]"), Just("uint32[]"),
			Just("uint64[]"), Just("uint128[]"), Just("uint256[]"),
			Just("int8[]"), Just("int16[]"), Just("int32[]"), Just("int64[]"),
			Just("int128[]"), Just("int256[]"), Just("string[]"),
			Just("address[]"), Just("bool[]"), Just("fixed[]"), Just("ufixed[]"),
			Just("bytes[]"), Just("bytes32[]"), Just("tuple[]"),
			// Other types
			Just("fixed"), Just("ufixed"), Just("address"), Just("string"),
			Just("bytes"), Just("bytes32"), Just("bool"), Just("map"),
		]
	) -> &'static str {
		variant
	}
}

prop_compose! {
	fn generate_case_variant()(
		base_kind in generate_valid_evm_kind(),
		case_type in prop_oneof![
			Just("lowercase"),
			Just("UPPERCASE"),
			Just("MiXeD"),
			Just("Capitalized")
		]
	) -> String {
		match case_type {
			"lowercase" => base_kind.to_lowercase(),
			"UPPERCASE" => base_kind.to_uppercase(),
			"MiXeD" => base_kind.chars().enumerate().map(|(i, c)| {
				if i % 2 == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }
			}).collect(),
			"Capitalized" => {
				let mut chars: Vec<char> = base_kind.chars().collect();
				if !chars.is_empty() {
					chars[0] = chars[0].to_ascii_uppercase();
				}
				chars.into_iter().collect()
			},
			_ => base_kind.to_string()
		}
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn test_format_token_value(
		token in generate_token()
	) {
		let formatted = format_token_value(&token);

		// Result should be a non-empty string
		prop_assert!(!formatted.is_empty());

		// Type-specific assertions
		match token {
			Token::Address(_) => prop_assert!(formatted.starts_with("0x")),
			Token::Bytes(_) | Token::FixedBytes(_) => prop_assert!(formatted.starts_with("0x")),
			Token::Array(_) => {
				prop_assert!(formatted.starts_with('['));
				prop_assert!(formatted.ends_with(']'));
			}
			Token::Tuple(_) => {
				prop_assert!(formatted.starts_with('('));
				prop_assert!(formatted.ends_with(')'));
			}
			_ => {} // Other types don't have specific format requirements
		}

		// The formatted string should be parseable based on the token type
		match token {
			Token::Uint(num) => {
				let parsed: Result<u64, _> = formatted.parse();
				prop_assert!(parsed.is_ok());
				prop_assert_eq!(parsed.unwrap(), num.as_u64());
			}
			Token::Bool(b) => {
				prop_assert_eq!(formatted, b.to_string());
			}
			Token::String(s) => {
				prop_assert_eq!(formatted, s);
			}
			_ => {} // Other types need more complex parsing
		}
	}

	#[test]
	fn test_string_to_h256_valid_input(
		hex_string in generate_valid_hex_string()
	) {
		let result = string_to_h256(&hex_string);

		// Valid hex strings should always succeed
		prop_assert!(result.is_ok());

		let hash = result.unwrap();

		// The result should be a valid B256 (32 bytes)
		prop_assert_eq!(hash.len(), 32);

		// Test idempotency: same input should produce same output
		let result2 = string_to_h256(&hex_string);
		prop_assert!(result2.is_ok());
		prop_assert_eq!(hash, result2.unwrap());

		// Test prefix handling: with and without "0x" should produce same result
		let without_prefix = hex_string.strip_prefix("0x").unwrap_or(&hex_string);
		let with_prefix = if hex_string.starts_with("0x") {
			hex_string.clone()
		} else {
			format!("0x{}", hex_string)
		};

		let result_without = string_to_h256(without_prefix);
		let result_with = string_to_h256(&with_prefix);

		prop_assert!(result_without.is_ok());
		prop_assert!(result_with.is_ok());
		prop_assert_eq!(result_without.unwrap(), result_with.unwrap());
	}

	#[test]
	fn test_string_to_h256_round_trip(
		original_bytes in prop::collection::vec(any::<u8>(), 32)
	) {
		// Convert bytes to hex string and back
		let hex_string = hex::encode(&original_bytes);
		let result = string_to_h256(&hex_string);

		prop_assert!(result.is_ok());
		let parsed_hash = result.unwrap();

		// Should get back the original bytes
		prop_assert_eq!(parsed_hash.as_slice(), original_bytes.as_slice());

		// Test with 0x prefix too
		let hex_with_prefix = format!("0x{}", hex_string);
		let result_with_prefix = string_to_h256(&hex_with_prefix);

		prop_assert!(result_with_prefix.is_ok());
		let hash_with_prefix = result_with_prefix.unwrap();
		prop_assert_eq!(hash_with_prefix.as_slice(), original_bytes.as_slice());
	}

	#[test]
	fn prop_compare_final_values_routing_consistency(
		kind in generate_valid_evm_kind(),
		value in generate_value_for_kind(),
		operator in generate_compatible_operator(),
		literal in generate_compatible_literal()
	) {
		let evaluator = create_evaluator();

		// Result via router
		let router_result = evaluator.compare_final_values(kind, &value, &operator, &literal);

		// Result via direct call (match on normalized kind)
		let normalized_kind = kind.to_lowercase();
		let direct_result = match normalized_kind.as_str() {
			k if ["uint8", "uint16", "uint32", "uint64", "uint128", "uint256", "number"].contains(&k) => {
				evaluator.compare_u256(&value, &operator, &literal)
			},
			k if ["int8", "int16", "int32", "int64", "int128", "int256"].contains(&k) => {
				evaluator.compare_i256(&value, &operator, &literal)
			},
			"address" => evaluator.compare_address(&value, &operator, &literal),
			"string" | "bytes" | "bytes32" => evaluator.compare_string(&value, &operator, &literal),
			"bool" => evaluator.compare_boolean(&value, &operator, &literal),
			"fixed" | "ufixed" => evaluator.compare_fixed_point(&value, &operator, &literal),
			"map" => evaluator.compare_map(&value, &operator, &literal),
			k if k.ends_with("[]") || k == "array" => {
				evaluator.compare_array(&value, &operator, &literal)
			},
			_ => {
				Ok(false)
			}
		};

		// Both should succeed or both should fail
		prop_assert_eq!(router_result.is_ok(), direct_result.is_ok(),
			"Routing consistency failed for kind '{}': router={:?}, direct={:?}",
			kind, router_result.is_ok(), direct_result.is_ok());

		// If both succeed, results should be identical
		if router_result.is_ok() && direct_result.is_ok() {
			prop_assert_eq!(router_result.unwrap(), direct_result.unwrap(),
				"Routing consistency failed for kind '{}': different results", kind);
		}
	}

	/// Property: Case insensitive kind matching
	#[test]
	fn prop_compare_final_values_case_insensitive(
		case_variant in generate_case_variant()
	) {
		let evaluator = create_evaluator();
		let value = "test_value";
		let operator = ComparisonOperator::Eq;
		let literal = LiteralValue::Str("test_value");

		// Get the base kind (lowercase)
		let base_kind = case_variant.to_lowercase();

		// Skip if it's an invalid kind after normalization
		if !["uint8", "uint16", "uint32", "uint64", "uint128", "uint256", "number",
			  "int8", "int16", "int32", "int64", "int128", "int256",
			  "address", "string", "bytes", "bytes32", "bool", "fixed", "ufixed", "map",
			  "array", "uint8[]", "uint16[]", "uint32[]", "uint64[]", "uint128[]", "uint256[]",
			  "int8[]", "int16[]", "int32[]", "int64[]", "int128[]", "int256[]",
			  "string[]", "address[]", "bool[]", "fixed[]", "ufixed[]", "bytes[]", "bytes32[]", "tuple[]"].contains(&base_kind.as_str()) {
			return Ok(());
		}

		let result_case_variant = evaluator.compare_final_values(&case_variant, value, &operator, &literal);
		let result_lowercase = evaluator.compare_final_values(&base_kind, value, &operator, &literal);

		// Both should have same success/failure
		prop_assert_eq!(result_case_variant.is_ok(), result_lowercase.is_ok(),
			"Case sensitivity failed: '{}' vs '{}' have different success status",
			case_variant, base_kind);

		// If both succeed, results should be identical
		if result_case_variant.is_ok() && result_lowercase.is_ok() {
			prop_assert_eq!(result_case_variant.unwrap(), result_lowercase.unwrap(),
				"Case sensitivity failed: '{}' vs '{}' produce different results",
				case_variant, base_kind);
		}
	}

	#[test]
	fn prop_addresses_are_classified_correctly(addr in arb_eth_address()) {
		let evaluator = create_evaluator();
		let json_val = json!(addr);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		prop_assert_eq!(kind, "address");
	}

	#[test]
	fn prop_bytes32_are_classified_correctly(bytes32 in arb_bytes32()) {
		let evaluator = create_evaluator();
		let json_val = json!(bytes32);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		prop_assert_eq!(kind, "bytes32");
	}

	#[test]
	fn prop_hex_bytes_are_classified_correctly(hex_bytes in arb_hex_bytes()) {
		let evaluator = create_evaluator();
		let json_val = json!(hex_bytes);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		// Should be "bytes" since it's hex but not address or bytes32 length
		prop_assert_eq!(kind, "bytes");
	}

	#[test]
	fn prop_decimal_strings_are_classified_as_fixed(decimal_str in arb_decimal_string()) {
		let evaluator = create_evaluator();
		// Only test if it's a valid decimal (some edge cases might not be)
		if Decimal::from_str(&decimal_str).is_ok() {
			let json_val = json!(decimal_str);
			let kind = evaluator.get_kind_from_json_value(&json_val);
			prop_assert_eq!(kind, "fixed");
		}
	}

	#[test]
	fn prop_regular_strings_are_classified_as_string(s in arb_regular_string()) {
		let evaluator = create_evaluator();
		let json_val = json!(s);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		prop_assert_eq!(kind, "string");
	}

	#[test]
	fn prop_positive_integers_are_classified_as_number(n in 0i64..i64::MAX) {
		let evaluator = create_evaluator();
		let json_val = json!(n);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		prop_assert_eq!(kind, "number");
	}

	#[test]
	fn prop_negative_integers_are_classified_as_int64(n in i64::MIN..-1i64) {
		let evaluator = create_evaluator();
		let json_val = json!(n);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		prop_assert_eq!(kind, "int64");
	}

	#[test]
	fn prop_floating_point_numbers_are_classified_as_fixed(n in -1000.0f64..1000.0f64) {
		let evaluator = create_evaluator();
		// Only test finite numbers
		if n.is_finite() {
			let json_val = json!(n);
			let kind = evaluator.get_kind_from_json_value(&json_val);
			prop_assert_eq!(kind, "fixed");
		}
	}

	#[test]
	fn prop_booleans_are_classified_correctly(b in any::<bool>()) {
		let evaluator = create_evaluator();
		let json_val = json!(b);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		prop_assert_eq!(kind, "bool");
	}

	#[test]
	fn prop_arrays_are_classified_correctly(arr in prop::collection::vec(any::<i32>(), 0..10)) {
		let evaluator = create_evaluator();
		let json_val = json!(arr);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		prop_assert_eq!(kind, "array");
	}

	#[test]
	fn prop_objects_are_classified_as_map(
		keys in prop::collection::vec("[a-z]+", 0..5),
		values in prop::collection::vec(any::<i32>(), 0..5)
	) {
		let evaluator = create_evaluator();
		// Create a map from keys and values
		let mut map = serde_json::Map::new();
		for (k, v) in keys.iter().zip(values.iter()) {
			map.insert(k.clone(), json!(v));
		}
		let json_val = serde_json::Value::Object(map);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		prop_assert_eq!(kind, "map");
	}

	#[test]
	fn prop_null_is_classified_correctly(_unit in Just(())) {
		let evaluator = create_evaluator();
		let json_val = json!(null);
		let kind = evaluator.get_kind_from_json_value(&json_val);
		prop_assert_eq!(kind, "null");
	}

	#[test]
	fn prop_address_case_insensitive(
		addr_lower in "[0-9a-f]{40}",
		addr_upper in "[0-9A-F]{40}"
	) {
		let evaluator = create_evaluator();

		let lower_addr = format!("0x{}", addr_lower);
		let upper_addr = format!("0x{}", addr_upper);

		let lower_json = json!(lower_addr);
		let upper_json = json!(upper_addr);

		let lower_kind = evaluator.get_kind_from_json_value(&lower_json);
		let upper_kind = evaluator.get_kind_from_json_value(&upper_json);

		prop_assert_eq!(lower_kind, "address");
		prop_assert_eq!(upper_kind, "address");
	}

	#[test]
	fn prop_large_numbers_classification(
		// Use strings to represent very large numbers that might not fit in standard types
		large_num_str in r"[1-9][0-9]{20,100}"
	) {
		let evaluator = create_evaluator();

		// Test as string - should be "string" since it's a numeric string without decimal
		let json_str = json!(large_num_str);
		let kind_str = evaluator.get_kind_from_json_value(&json_str);
		prop_assert_eq!(kind_str, "string");

		// Test with decimal point - should be "fixed" if it parses as Decimal
		let large_decimal_str = format!("{}.0", large_num_str);
		if Decimal::from_str(&large_decimal_str).is_ok() {
			let json_decimal = json!(large_decimal_str);
			let kind_decimal = evaluator.get_kind_from_json_value(&json_decimal);
			prop_assert_eq!(kind_decimal, "fixed");
		}
	}
}
