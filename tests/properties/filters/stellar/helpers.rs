//! Property-based tests for Stellar transaction matching and filtering helpers.

use openzeppelin_monitor::services::filter::stellar_helpers::{compare_json_values, parse_sc_val};
use proptest::{prelude::*, test_runner::Config};
use std::str::FromStr;
use stellar_xdr::curr::{
	Hash, Int128Parts, ScAddress, ScString, ScSymbol, ScVal, StringM, UInt128Parts,
};

// Generator for ScVal values
prop_compose! {
	fn generate_sc_val()(
		val_type in 0..11usize,
		bool_val in any::<bool>(),
		u32_val in any::<u32>(),
		i32_val in any::<i32>(),
		u64_val in any::<u64>(),
		i64_val in any::<i64>(),
		u128_hi in any::<u64>(),
		u128_lo in any::<u64>(),
		i128_hi in any::<i64>(),
		i128_lo in any::<u64>(),
		bytes in prop::collection::vec(any::<u8>(), 1..32),
		str_val in "[a-zA-Z0-9]{1,20}"
	) -> ScVal {
		match val_type {
			0 => ScVal::Bool(bool_val),
			1 => ScVal::U32(u32_val),
			2 => ScVal::I32(i32_val),
			3 => ScVal::U64(u64_val),
			4 => ScVal::I64(i64_val),
			5 => ScVal::U128(UInt128Parts { hi: u128_hi, lo: u128_lo }),
			6 => ScVal::I128(Int128Parts { hi: i128_hi, lo: i128_lo }),
			7 => {
				let bytes = if bytes.is_empty() { vec![1, 2, 3] } else { bytes };
				ScVal::Bytes(bytes.try_into().unwrap_or_else(|_| vec![1, 2, 3].try_into().unwrap()))
			},
			8 => {
				let s = if str_val.is_empty() { "test".to_string() } else { str_val };
				let str_m = StringM::<{ u32::MAX }>::from_str(&s)
					.unwrap_or_else(|_| StringM::<{ u32::MAX }>::from_str("test").unwrap());
				ScVal::String(ScString(str_m))
			},
			9 => {
				let s = if str_val.is_empty() { "test".to_string() } else { str_val };
				let sym_m = StringM::<32>::from_str(&s).unwrap_or_else(|_| StringM::<32>::from_str("test").unwrap());
				ScVal::Symbol(ScSymbol(sym_m))
			},
			10 => {
				// Generate actual random hash for contract address
				let mut hash_data = [0u8; 32];
				for (i, byte) in bytes.iter().take(32).enumerate() {
					hash_data[i] = *byte;
				}
				ScVal::Address(ScAddress::Contract(Hash(hash_data)))
			},
			_ => ScVal::Void
		}
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]
	// Tests property-based validation of compare_json_values function
	#[test]
	fn test_compare_json_values_property(
		// Generate numeric values for comparison
		value1 in -1000i64..1000i64,
		value2 in -1000i64..1000i64,
		// Generate comparison operators
		operator in prop_oneof![
			Just("=="), Just("!="), Just(">"),
			Just(">="), Just("<"), Just("<=")
		],
	) {
		// Create JSON values for testing
		let json_value1 = serde_json::json!(value1);
		let json_value2 = serde_json::json!(value2);

		// Test equality comparison
		if operator == "==" {
			let eq_result = compare_json_values(&json_value1, "==", &json_value1);
			prop_assert!(eq_result);

			// Different values should not be equal
			if value1 != value2 {
				let not_eq_result = compare_json_values(&json_value1, "==", &json_value2);
				prop_assert!(!not_eq_result);
			}
		}

		// Test inequality comparison
		if operator == "!=" {
			let neq_result = compare_json_values(&json_value1, "!=", &json_value2);
			// Should be true if values are different
			prop_assert_eq!(neq_result, value1 != value2);
		}

		// Test greater than
		if operator == ">" {
			let gt_result = compare_json_values(&json_value1, ">", &json_value2);
			prop_assert_eq!(gt_result, value1 > value2);
		}

		// Test greater than or equal
		if operator == ">=" {
			let gte_result = compare_json_values(&json_value1, ">=", &json_value2);
			prop_assert_eq!(gte_result, value1 >= value2);
		}

		// Test less than
		if operator == "<" {
			let lt_result = compare_json_values(&json_value1, "<", &json_value2);
			prop_assert_eq!(lt_result, value1 < value2);
		}

		// Test less than or equal
		if operator == "<=" {
			let lte_result = compare_json_values(&json_value1, "<=", &json_value2);
			prop_assert_eq!(lte_result, value1 <= value2);
		}

		// Test string values (should only support == and !=)
		let str_value1 = serde_json::json!("test1");
		let str_value2 = serde_json::json!("test2");

		// String equality
		let str_eq_result = compare_json_values(&str_value1, "==", &str_value1);
		prop_assert!(str_eq_result);

		// String inequality
		let str_neq_result = compare_json_values(&str_value1, "!=", &str_value2);
		prop_assert!(str_neq_result);

		// Numeric operators should not work with string values
		if operator == ">" || operator == ">=" || operator == "<" || operator == "<=" {
			let unsupported_result = compare_json_values(&str_value1, operator, &str_value2);
			prop_assert!(!unsupported_result);
		}

		// Test unsupported operator
		let invalid_operator_result = compare_json_values(&json_value1, "invalid", &json_value2);
		prop_assert!(!invalid_operator_result);

		// Test with complex objects
		let obj_value1 = serde_json::json!({ "key": value1 });
		let obj_value2 = serde_json::json!({ "key": value2 });

		// Object equality (only == and != should work)
		let obj_eq_result = compare_json_values(&obj_value1, "==", &obj_value1);
		prop_assert!(obj_eq_result);

		if value1 != value2 {
			let obj_neq_result = compare_json_values(&obj_value1, "!=", &obj_value2);
			prop_assert!(obj_neq_result);
		}

		// Numeric operators should not work with objects
		if operator == ">" || operator == ">=" || operator == "<" || operator == "<=" {
			let obj_num_result = compare_json_values(&obj_value1, operator, &obj_value2);
			prop_assert!(!obj_num_result);
		}
	}

	#[test]
	fn test_parse_sc_val(
		sc_val in generate_sc_val(),
		indexed in any::<bool>()
	) {
		let result = parse_sc_val(&sc_val, indexed);

		// For most ScVal types we should get a valid result
		match sc_val {
			ScVal::Bool(_) | ScVal::U32(_) | ScVal::I32(_) | ScVal::U64(_) | ScVal::I64(_) |
			ScVal::U128(_) | ScVal::I128(_) | ScVal::Bytes(_) | ScVal::String(_) |
			ScVal::Symbol(_) | ScVal::Address(_) | ScVal::Timepoint(_) | ScVal::Duration(_) => {
				prop_assert!(result.is_some());

				// Verify the indexed flag is correctly set
				let entry = result.unwrap();
				prop_assert_eq!(entry.indexed, indexed);

				// Kind should match the type
				match sc_val {
					ScVal::Bool(_) => prop_assert_eq!(entry.kind, "Bool"),
					ScVal::U32(_) => prop_assert_eq!(entry.kind, "U32"),
					ScVal::I32(_) => prop_assert_eq!(entry.kind, "I32"),
					ScVal::U64(_) => prop_assert_eq!(entry.kind, "U64"),
					ScVal::I64(_) => prop_assert_eq!(entry.kind, "I64"),
					ScVal::U128(_) => prop_assert_eq!(entry.kind, "U128"),
					ScVal::I128(_) => prop_assert_eq!(entry.kind, "I128"),
					ScVal::Bytes(_) => prop_assert_eq!(entry.kind, "Bytes"),
					ScVal::String(_) => prop_assert_eq!(entry.kind, "String"),
					ScVal::Symbol(_) => prop_assert_eq!(entry.kind, "Symbol"),
					ScVal::Address(_) => prop_assert_eq!(entry.kind, "Address"),
					ScVal::Timepoint(_) => prop_assert_eq!(entry.kind, "Timepoint"),
					ScVal::Duration(_) => prop_assert_eq!(entry.kind, "Duration"),
					_ => {},
				}

				// Value should not be empty
				prop_assert!(!entry.value.to_string().is_empty());
			},
			_ => {
				// For unsupported types, we should get None
				prop_assert!(result.is_none());
			}
		}
	}
}
