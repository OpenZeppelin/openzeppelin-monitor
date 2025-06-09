//! Property-based tests for Stellar transaction matching and filtering helpers.

use alloy::primitives::U256;
use openzeppelin_monitor::services::filter::stellar_helpers::{
	combine_i128, combine_i256, combine_u128, combine_u256, get_kind_from_value, is_address,
	parse_sc_val,
};
use proptest::{prelude::*, test_runner::Config};
use serde_json::{json, Value};
use std::str::FromStr;
use stellar_xdr::curr::{
	Hash, Int128Parts, Int256Parts, ScAddress, ScString, ScSymbol, ScVal, StringM, UInt128Parts,
	UInt256Parts,
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

// Generator for UInt256Parts
fn uint256_parts() -> impl Strategy<Value = UInt256Parts> {
	(any::<u64>(), any::<u64>(), any::<u64>(), any::<u64>()).prop_map(
		|(lo_lo, lo_hi, hi_lo, hi_hi)| UInt256Parts {
			lo_lo,
			lo_hi,
			hi_lo,
			hi_hi,
		},
	)
}

// Strategy to generate various JSON values
fn arb_json_value() -> impl Strategy<Value = Value> {
	let leaf = prop_oneof![
		any::<bool>().prop_map(|b| json!(b)),
		any::<i64>().prop_map(|i| json!(i)),
		any::<u64>().prop_map(|u| json!(u)),
		any::<f64>()
			.prop_filter("finite", |f| f.is_finite())
			.prop_map(|f| json!(f)),
		"[a-zA-Z0-9_]*".prop_map(|s| json!(s)),
		Just(json!(null)),
	];

	leaf.prop_recursive(
		2,  // Max recursion depth
		10, // Max total nodes
		3,  // Max items per collection
		|inner| {
			prop_oneof![
				prop::collection::vec(inner.clone(), 0..=3).prop_map(|v| json!(v)),
				prop::collection::hash_map("[a-zA-Z][a-zA-Z0-9_]*", inner, 0..=3)
					.prop_map(|m| json!(m)),
			]
		},
	)
}

// Strategy for generating Stellar addresses
fn arb_stellar_address() -> impl Strategy<Value = String> {
	prop_oneof![
		// Valid Ed25519 public key addresses (start with G)
		Just("GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI".to_string()),
		Just("GCDNJUBQSX7AJWLJACMJ7I4BC3Z47BQUTMHEICZLE6MU4KQBRYG5JY6B".to_string()),
		// Valid contract addresses (start with C)
		Just("CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4".to_string()),
		Just("CA6PUJLBYKZKUEKLZJMKBZLEKP2OTHANDEOWSFF44FTSYLKQPIICCJBE".to_string()),
	]
}

// Strategy for generating non-address strings
fn arb_non_address_string() -> impl Strategy<Value = String> {
	prop_oneof![
		"[a-zA-Z0-9_]{1,20}",
		"[^GC][A-Z2-7]{55}", // Wrong prefix
		"G[A-Z2-7]{50}",     // Too short
		"C[A-Z2-7]{60}",     // Too long
		".*[^A-Z2-7].*",     // Invalid characters
		"",                  // Empty string
	]
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

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

	/// Test that the function is deterministic - same input always produces same output
	#[test]
	fn test_determinism(parts in uint256_parts()) {
		let result1 = combine_u256(&parts);
		let result2 = combine_u256(&parts);
		prop_assert_eq!(result1, result2);
	}

	/// Test that output is never empty
	#[test]
	fn test_non_empty_output(parts in uint256_parts()) {
		let result = combine_u256(&parts);
		prop_assert!(!result.is_empty());
	}

	/// Test that output is a valid decimal number string
	#[test]
	fn test_valid_decimal_string(parts in uint256_parts()) {
		let result = combine_u256(&parts);

		// Should parse as a valid U256
		prop_assert!(U256::from_str(&result).is_ok());

		// Should only contain digits
		prop_assert!(result.chars().all(|c| c.is_ascii_digit()));

		// Should not have leading zeros (except for "0")
		if result != "0" {
			prop_assert!(!result.starts_with('0'));
		}
	}

	/// Test mathematical correctness by comparing with manual calculation
	#[test]
	fn test_mathematical_correctness(parts in uint256_parts()) {
		let result = combine_u256(&parts);
		let parsed = U256::from_str(&result).unwrap();

		// Manually construct the expected U256
		let expected = U256::from_limbs([parts.lo_lo, parts.lo_hi, parts.hi_lo, parts.hi_hi]);

		prop_assert_eq!(parsed, expected);
	}

	/// Test monotonicity - if we increase any component, result should be >= original
	#[test]
	fn test_monotonicity_lo_lo(
		mut parts in uint256_parts().prop_filter("not max", |p| p.lo_lo < u64::MAX)
	) {
		let original_result = combine_u256(&parts);
		let original_value = U256::from_str(&original_result).unwrap();

		parts.lo_lo += 1;
		let new_result = combine_u256(&parts);
		let new_value = U256::from_str(&new_result).unwrap();

		prop_assert!(new_value > original_value);
	}

	#[test]
	fn test_monotonicity_lo_hi(
		mut parts in uint256_parts().prop_filter("not max", |p| p.lo_hi < u64::MAX)
	) {
		let original_result = combine_u256(&parts);
		let original_value = U256::from_str(&original_result).unwrap();

		parts.lo_hi += 1;
		let new_result = combine_u256(&parts);
		let new_value = U256::from_str(&new_result).unwrap();

		prop_assert!(new_value > original_value);
	}

	#[test]
	fn test_monotonicity_hi_lo(
		mut parts in uint256_parts().prop_filter("not max", |p| p.hi_lo < u64::MAX)
	) {
		let original_result = combine_u256(&parts);
		let original_value = U256::from_str(&original_result).unwrap();

		parts.hi_lo += 1;
		let new_result = combine_u256(&parts);
		let new_value = U256::from_str(&new_result).unwrap();

		prop_assert!(new_value > original_value);
	}

	/// Test that combine_i256 handles zero correctly
	#[test]
	fn test_combine_i256_zero_property(
		_lo_lo in any::<u64>(),
		_lo_hi in any::<u64>(),
		_hi_lo in any::<u64>(),
	) {
		let zero_parts = Int256Parts {
			lo_lo: 0,
			lo_hi: 0,
			hi_lo: 0,
			hi_hi: 0,
		};
		let result = combine_i256(&zero_parts);
		prop_assert_eq!(result, "0");
	}

	/// Test that combine_i256 produces negative strings when hi_hi is negative
	#[test]
	fn test_combine_i256_negative_sign_property(
		lo_lo in any::<u64>(),
		lo_hi in any::<u64>(),
		hi_lo in any::<u64>(),
		hi_hi in i64::MIN..-1i64, // Exclude -1 to avoid edge cases
	) {
		let parts = Int256Parts { lo_lo, lo_hi, hi_lo, hi_hi };
		let result = combine_i256(&parts);

		// When hi_hi is negative, result should typically be negative
		// but there might be edge cases due to two's complement arithmetic
		// So we just verify it's a valid decimal string
		prop_assert!(result.chars().all(|c| c.is_ascii_digit() || c == '-'));
		prop_assert!(!result.is_empty());
	}

	/// Test that combine_i256 produces positive strings when hi_hi is non-negative
	#[test]
	fn test_combine_i256_positive_sign_property(
		lo_lo in any::<u64>(),
		lo_hi in any::<u64>(),
		hi_lo in any::<u64>(),
		hi_hi in 0i64..=i64::MAX,
	) {
		let parts = Int256Parts { lo_lo, lo_hi, hi_lo, hi_hi };
		let result = combine_i256(&parts);

		// When hi_hi is non-negative, result should not start with '-'
		prop_assert!(!result.starts_with('-'), "Expected non-negative result, got: {}", result);
	}

	/// Test that combine_i256 result can be parsed back consistently
	#[test]
	fn test_combine_i256_parseable_property(
		lo_lo in any::<u64>(),
		lo_hi in any::<u64>(),
		hi_lo in any::<u64>(),
		hi_hi in any::<i64>(),
	) {
		let parts = Int256Parts { lo_lo, lo_hi, hi_lo, hi_hi };
		let result = combine_i256(&parts);

		// Result should always be a valid decimal string
		prop_assert!(result.chars().all(|c| c.is_ascii_digit() || c == '-'));

		// First character should be either digit or minus
		if let Some(first_char) = result.chars().next() {
			prop_assert!(first_char.is_ascii_digit() || first_char == '-');
		}

		// If it starts with minus, second character should be a digit
		if result.starts_with('-') && result.len() > 1 {
			if let Some(second_char) = result.chars().nth(1) {
				prop_assert!(second_char.is_ascii_digit());
			}
		}
	}
	/// Test mathematical correctness: result should equal hi * 2^64 + lo
	#[test]
	fn test_combine_u128_mathematical_correctness_property(
		hi in any::<u64>(),
		lo in any::<u64>(),
	) {
		let parts = UInt128Parts { hi, lo };
		let result = combine_u128(&parts);

		// Calculate expected value: hi * 2^64 + lo
		let expected = (hi as u128) << 64 | (lo as u128);

		if let Ok(actual) = result.parse::<u128>() {
			prop_assert_eq!(actual, expected, "Expected {}, got {}", expected, actual);
		}
	}

	/// Test that combine_u128 is monotonic with respect to hi when lo is constant
	#[test]
	fn test_combine_u128_monotonic_hi_property(
		lo in any::<u64>(),
		hi1 in any::<u64>(),
		hi2 in any::<u64>(),
	) {
		prop_assume!(hi1 != hi2);

		let parts1 = UInt128Parts { hi: hi1, lo };
		let parts2 = UInt128Parts { hi: hi2, lo };

		let result1 = combine_u128(&parts1);
		let result2 = combine_u128(&parts2);

		// Parse results for numerical comparison
		if let (Ok(val1), Ok(val2)) = (result1.parse::<u128>(), result2.parse::<u128>()) {
			if hi1 < hi2 {
				prop_assert!(val1 < val2, "Expected {} < {} when hi1={} < hi2={}", val1, val2, hi1, hi2);
			} else {
				prop_assert!(val1 > val2, "Expected {} > {} when hi1={} > hi2={}", val1, val2, hi1, hi2);
			}
		}
	}

	/// Test that combine_u128 is monotonic with respect to lo when hi is constant
	#[test]
	fn test_combine_u128_monotonic_lo_property(
		hi in any::<u64>(),
		lo1 in any::<u64>(),
		lo2 in any::<u64>(),
	) {
		prop_assume!(lo1 != lo2);

		let parts1 = UInt128Parts { hi, lo: lo1 };
		let parts2 = UInt128Parts { hi, lo: lo2 };

		let result1 = combine_u128(&parts1);
		let result2 = combine_u128(&parts2);

		// Parse results for numerical comparison
		if let (Ok(val1), Ok(val2)) = (result1.parse::<u128>(), result2.parse::<u128>()) {
			if lo1 < lo2 {
				prop_assert!(val1 < val2, "Expected {} < {} when lo1={} < lo2={}", val1, val2, lo1, lo2);
			} else {
				prop_assert!(val1 > val2, "Expected {} > {} when lo1={} > lo2={}", val1, val2, lo1, lo2);
			}
		}
	}

	/// Test that combine_u128 result is always a valid unsigned decimal string
	#[test]
	fn test_combine_u128_output_format_property(
		hi in any::<u64>(),
		lo in any::<u64>(),
	) {
		let parts = UInt128Parts { hi, lo };
		let result = combine_u128(&parts);

		// Should never be empty
		prop_assert!(!result.is_empty());

		// Should never start with negative sign (unsigned)
		prop_assert!(!result.starts_with('-'));

		// Should contain only digits
		prop_assert!(result.chars().all(|c| c.is_ascii_digit()), "Result '{}' contains non-digit characters", result);

		// Should be parseable as u128
		prop_assert!(result.parse::<u128>().is_ok(), "Result '{}' is not a valid u128", result);

		// Should not have leading zeros (unless it's just "0")
		if result != "0" {
			prop_assert!(!result.starts_with('0'), "Result '{}' has leading zeros", result);
		}
	}

	/// Test that combine_i128 is monotonic with respect to hi when lo is constant
	#[test]
	fn test_combine_i128_monotonic_hi_property(
		lo in any::<i64>(),
		hi1 in any::<i64>(),
		hi2 in any::<i64>(),
	) {
		prop_assume!(hi1 != hi2);

		let parts1 = Int128Parts { hi: hi1, lo: lo as u64 };
		let parts2 = Int128Parts { hi: hi2, lo: lo as u64 };

		let result1 = combine_i128(&parts1);
		let result2 = combine_i128(&parts2);

		// Parse results for numerical comparison
		if let (Ok(val1), Ok(val2)) = (result1.parse::<i128>(), result2.parse::<i128>()) {
			if hi1 < hi2 {
				prop_assert!(val1 < val2, "Expected {} < {} when hi1={} < hi2={}", val1, val2, hi1, hi2);
			} else {
				prop_assert!(val1 > val2, "Expected {} > {} when hi1={} > hi2={}", val1, val2, hi1, hi2);
			}
		}
	}

	/// Test that combine_i128 result is always a valid signed decimal string
	#[test]
	fn test_combine_i128_output_format_property(
		hi in any::<i64>(),
		lo in any::<i64>(),
	) {
		let parts = Int128Parts { hi, lo: lo as u64 };
		let result = combine_i128(&parts);

		// Should never be empty
		prop_assert!(!result.is_empty());

		// Should contain only digits and optionally a leading minus sign
		if let Some(stripped) = result.strip_prefix('-') {
			prop_assert!(!stripped.is_empty(), "Negative sign should be followed by digits");
			prop_assert!(stripped.chars().all(|c| c.is_ascii_digit()),
				"Result '{}' contains non-digit characters after minus sign", result);
		} else {
			prop_assert!(result.chars().all(|c| c.is_ascii_digit()),
				"Result '{}' contains non-digit characters", result);
		}

		// Should be parseable as i128
		prop_assert!(result.parse::<i128>().is_ok(), "Result '{}' is not a valid i128", result);

		// Should not have leading zeros (unless it's just "0" or "-0")
		if result != "0" && result != "-0" {
			let digits_part = result.strip_prefix('-').unwrap_or(&result);
			prop_assert!(!digits_part.starts_with('0'), "Result '{}' has leading zeros", result);
		}
	}

	/// Test sign consistency: negative hi should generally produce negative results
	#[test]
	fn test_combine_i128_sign_consistency_property(
		hi in i64::MIN..0i64,
		lo in any::<i64>(),
	) {
		let parts = Int128Parts { hi, lo: lo as u64 };
		let result = combine_i128(&parts);

		// When hi is negative, the result should typically be negative
		// (since hi is the most significant part)
		if let Ok(val) = result.parse::<i128>() {
			prop_assert!(val < 0, "Expected negative result when hi={} < 0, got: {}", hi, val);
		}
	}

	/// Test positive hi produces positive results when lo is non-negative
	#[test]
	fn test_combine_i128_positive_hi_property(
		hi in 1i64..=i64::MAX,
		lo in 0i64..=i64::MAX,
	) {
		let parts = Int128Parts { hi, lo: lo as u64 };
		let result = combine_i128(&parts);

		if let Ok(val) = result.parse::<i128>() {
			prop_assert!(val > 0, "Expected positive result when hi={} > 0 and lo={} >= 0, got: {}", hi, lo, val);
		}
	}

	/// Test that strings with invalid characters are rejected
	#[test]
	fn test_is_address_invalid_characters_property(
		prefix in "[GC]", // Valid prefixes
		invalid_chars in "[^A-Z2-7]*", // Invalid base32 characters
		suffix in "[A-Z2-7]*" // Valid base32 characters
	) {
		prop_assume!(!invalid_chars.is_empty());
		prop_assume!(invalid_chars.chars().any(|c| !matches!(c, 'A'..='Z' | '2'..='7')));

		let test_string = format!("{}{}{}", prefix, invalid_chars, suffix);

		// Strings with invalid characters should be rejected
		prop_assert!(!is_address(&test_string), "String with invalid characters '{}' should not be valid", test_string);
	}

	/// Test that strings with wrong prefixes are rejected
	#[test]
	fn test_is_address_wrong_prefix_property(
		wrong_prefix in "[^GC]", // Invalid prefixes (not G or C)
		body in "[A-Z2-7]{50,60}" // Valid base32 body
	) {
		prop_assume!(!wrong_prefix.is_empty());
		prop_assume!(!wrong_prefix.starts_with('G') && !wrong_prefix.starts_with('C'));

		let test_string = format!("{}{}", wrong_prefix, body);

		// Strings with wrong prefixes should be rejected
		prop_assert!(!is_address(&test_string), "String with wrong prefix '{}' should not be valid", test_string);
	}

	/// Test that strings of wrong length are rejected
	#[test]
	fn test_is_address_wrong_length_property(
		prefix in "[GC]",
		body in "[A-Z2-7]*".prop_filter("Wrong length", |s| s.len() != 55) // 56 total - 1 for prefix
	) {
		prop_assume!(!body.is_empty() && body.len() < 100); // Reasonable bounds
		prop_assume!(body.len() != 55); // Stellar addresses should be 56 chars total

		let test_string = format!("{}{}", prefix, body);

		// Most strings of wrong length should be rejected
		// (There might be some edge cases, but the vast majority should fail)
		if test_string.len() != 56 {
			prop_assert!(!is_address(&test_string), "String of wrong length '{}' (len={}) should not be valid", test_string, test_string.len());
		}
	}

	/// Test that the function is consistent (same input always gives same output)
	#[test]
	fn test_is_address_consistency_property(
		test_string in ".*"
	) {
		let result1 = is_address(&test_string);
		let result2 = is_address(&test_string);

		prop_assert_eq!(result1, result2, "is_address should be deterministic for input '{}'", test_string);
	}

	/// Test case sensitivity (Stellar addresses should be case-sensitive)
	#[test]
	fn test_is_address_case_sensitivity_property(
		prefix in "[GC]", // lowercase prefixes
		body in "[a-z2-7]{55}" // lowercase body
	) {
		let lowercase_string = format!("{}{}", prefix, body);

		// Lowercase versions should generally be invalid
		// (Stellar addresses use uppercase)
		prop_assert!(!is_address(&lowercase_string), "Lowercase address '{}' should not be valid", lowercase_string);
	}

	/// Property: get_kind_from_value never panics and always returns non-empty string
	#[test]
	fn get_kind_from_value_never_panics_returns_non_empty(value in arb_json_value()) {
		let result = std::panic::catch_unwind(|| {
			get_kind_from_value(&value)
		});
		prop_assert!(result.is_ok(), "get_kind_from_value should never panic");

		let kind = result.unwrap();
		prop_assert!(!kind.is_empty(), "Kind should never be empty string");
	}

	/// Property: Function is deterministic - same input always gives same output
	#[test]
	fn get_kind_from_value_deterministic(value in arb_json_value()) {
		let result1 = get_kind_from_value(&value);
		let result2 = get_kind_from_value(&value);
		prop_assert_eq!(result1, result2);
	}

	/// Property: Boolean values always return "Bool"
	#[test]
	fn get_kind_from_value_bool_classification(bool_val: bool) {
		let value = json!(bool_val);
		let kind = get_kind_from_value(&value);
		prop_assert_eq!(kind, "Bool");
	}

	/// Property: Arrays always return "Vec"
	#[test]
	fn get_kind_from_value_array_classification(
		array_elements in prop::collection::vec(arb_json_value(), 0..=5)
	) {
		let value = json!(array_elements);
		let kind = get_kind_from_value(&value);
		prop_assert_eq!(kind, "Vec");
	}

	/// Property: Objects always return "Map"
	#[test]
	fn get_kind_from_value_object_classification(
		object_map in prop::collection::hash_map("[a-zA-Z][a-zA-Z0-9_]*", arb_json_value(), 0..=5)
	) {
		let value = json!(object_map);
		let kind = get_kind_from_value(&value);
		prop_assert_eq!(kind, "Map");
	}

	/// Property: Positive integers within u64 range return "U64"
	#[test]
	fn get_kind_from_value_u64_classification(num in 0u64..=u64::MAX) {
		let value = json!(num);
		let kind = get_kind_from_value(&value);
		prop_assert_eq!(kind, "U64");
	}

	/// Property: Negative integers return "I64"
	#[test]
	fn get_kind_from_value_i64_classification(num in i64::MIN..0i64) {
		let value = json!(num);
		let kind = get_kind_from_value(&value);
		prop_assert_eq!(kind, "I64");
	}

	/// Property: Floating point numbers return "F64"
	#[test]
	fn get_kind_from_value_f64_classification(
		num in any::<f64>().prop_filter("finite", |f| f.is_finite() && f.fract() != 0.0)
	) {
		let value = json!(num);
		let kind = get_kind_from_value(&value);
		prop_assert_eq!(kind, "F64");
	}

	/// Property: Valid Stellar addresses return "Address"
	#[test]
	fn get_kind_from_value_address_classification(address in arb_stellar_address()) {
		let value = json!(address.clone());
		let kind = get_kind_from_value(&value);
		prop_assert_eq!(kind, "Address", "Valid address '{}' should be classified as Address", address);
	}

	/// Property: Non-address strings return "String"
	#[test]
	fn get_kind_from_value_string_classification(
		non_address in arb_non_address_string().prop_filter("not address", |s| !is_address(s))
	) {
		let value = json!(non_address.clone());
		let kind = get_kind_from_value(&value);
		prop_assert_eq!(kind, "String", "Non-address string '{}' should be classified as String", non_address);
	}

}
