//! Property-based tests for Stellar transaction matching and filtering helpers.

use alloy::primitives::U256;
use openzeppelin_monitor::services::filter::stellar_helpers::{combine_u256, parse_sc_val};
use proptest::{prelude::*, test_runner::Config};
use std::str::FromStr;
use stellar_xdr::curr::{
	Hash, Int128Parts, ScAddress, ScString, ScSymbol, ScVal, StringM, UInt128Parts, UInt256Parts,
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
}
