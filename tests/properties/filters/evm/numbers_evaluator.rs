//! Property-based tests for EVM evaluator functionality (numbers).
//! Tests cover JSON value matching, type detection, and comparison logic.

use crate::properties::filters::evm::strings_evaluator::create_evaluator;
use openzeppelin_monitor::services::filter::{ComparisonOperator, EvaluationError, LiteralValue};
use proptest::{prelude::*, test_runner::Config};

prop_compose! {
	fn generate_valid_u256_string()(
		variant in prop_oneof![
			// Small decimal numbers
			(0u64..1_000_000u64).prop_map(|n| n.to_string()),
			// Large decimal numbers (but not overflow)
			prop::collection::vec(any::<u64>(), 1..4).prop_map(|nums| {
				nums.into_iter().map(|n| (n % 1000).to_string()).collect::<Vec<_>>().join("")
			}),
			// Hex numbers (small)
			(0u64..1_000_000u64).prop_map(|n| format!("0x{:x}", n)),
			// Hex numbers (uppercase)
			(0u64..1_000_000u64).prop_map(|n| format!("0X{:X}", n)),
			// Special values
			Just("0".to_string()),
			Just("0x0".to_string()),
			Just("1".to_string()),
			Just("0x1".to_string()),
			// Max value (using string to avoid overflow in generation)
			Just("115792089237316195423570985008687907853269984665640564039457584007913129639935".to_string()),
			Just("0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_invalid_u256_string()(
		variant in prop_oneof![
			// Empty/whitespace
			Just("".to_string()),
			Just("   ".to_string()),
			Just("\t".to_string()),
			// Invalid characters
			Just("abc".to_string()),
			Just("123abc".to_string()),
			Just("0xGHI".to_string()),
			// Negative numbers
			Just("-1".to_string()),
			Just("-123".to_string()),
			// Overflow
			Just("115792089237316195423570985008687907853269984665640564039457584007913129639936".to_string()),
			// Invalid hex
			Just("0x".to_string()),
			Just("0xZ".to_string()),
			// Mixed formats
			Just("0x123.45".to_string()),
			Just("123.45".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_u256_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne),
			Just(ComparisonOperator::Gt),
			Just(ComparisonOperator::Gte),
			Just(ComparisonOperator::Lt),
			Just(ComparisonOperator::Lte),
		]
	) -> ComparisonOperator {
		op
	}
}

prop_compose! {
	fn generate_u256_literal()(
		value in generate_valid_u256_string(),
		is_number in any::<bool>()
	) -> LiteralValue<'static> {
		// Leak the string so it lives for 'static lifetime in tests
		let leaked_value = Box::leak(value.into_boxed_str());
		if is_number {
			LiteralValue::Number(leaked_value)
		} else {
			LiteralValue::Str(leaked_value)
		}
	}
}

prop_compose! {
	fn generate_valid_i256_string()(
		variant in prop_oneof![
			// Small positive decimal numbers
			(0i64..1_000_000i64).prop_map(|n| n.to_string()),
			// Small negative decimal numbers
			(-1_000_000i64..0i64).prop_map(|n| n.to_string()),
			// Large positive decimal numbers (but not overflow)
			prop::collection::vec(any::<u8>(), 1..4).prop_map(|nums| {
				nums.into_iter().map(|n| (n % 100).to_string()).collect::<Vec<_>>().join("")
			}),
			// Large negative decimal numbers (but not overflow)
			prop::collection::vec(any::<u8>(), 1..4).prop_map(|nums| {
				let num_str = nums.into_iter().map(|n| (n % 100).to_string()).collect::<Vec<_>>().join("");
				format!("-{}", num_str)
			}),
			// Hex numbers (positive)
			(0i64..1_000_000i64).prop_map(|n| format!("0x{:x}", n)),
			// Hex numbers (negative, if supported by string_to_i256)
			(-1_000_000i64..-1i64).prop_map(|n| format!("-0x{:x}", -n)),
			// Hex numbers (uppercase)
			(0i64..1_000_000i64).prop_map(|n| format!("0X{:X}", n)),
			// Special values
			Just("0".to_string()),
			Just("-0".to_string()),
			Just("0x0".to_string()),
			Just("1".to_string()),
			Just("-1".to_string()),
			Just("0x1".to_string()),
			// Max positive value (I256::MAX)
			Just("57896044618658097711785492504343953926634992332820282019728792003956564819967".to_string()),
			// Min negative value (I256::MIN)
			Just("-57896044618658097711785492504343953926634992332820282019728792003956564819968".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_invalid_i256_string()(
		variant in prop_oneof![
			// Empty/whitespace
			Just("".to_string()),
			Just("   ".to_string()),
			Just("\t".to_string()),
			Just("\n".to_string()),
			// Invalid characters
			Just("abc".to_string()),
			Just("123abc".to_string()),
			Just("0xGHI".to_string()),
			Just("--123".to_string()),
			Just("++123".to_string()),
			// Positive overflow (greater than I256::MAX)
			Just("57896044618658097711785492504343953926634992332820282019728792003956564819968".to_string()),
			// Negative overflow (less than I256::MIN)
			Just("-57896044618658097711785492504343953926634992332820282019728792003956564819969".to_string()),
			// Invalid hex
			Just("0x".to_string()),
			Just("-0x".to_string()),
			Just("0xZ".to_string()),
			Just("-0xZ".to_string()),
			// Mixed formats
			Just("0x123.45".to_string()),
			Just("123.45".to_string()),
			Just("-123.45".to_string()),
			// Invalid signs
			Just("- 123".to_string()),
			Just("+ 123".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_i256_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne),
			Just(ComparisonOperator::Gt),
			Just(ComparisonOperator::Gte),
			Just(ComparisonOperator::Lt),
			Just(ComparisonOperator::Lte),
		]
	) -> ComparisonOperator {
		op
	}
}

prop_compose! {
	fn generate_i256_literal()(
		value in generate_valid_i256_string(),
		is_number in any::<bool>()
	) -> LiteralValue<'static> {
		// Leak the string so it lives for 'static lifetime in tests
		let leaked_value = Box::leak(value.into_boxed_str());
		if is_number {
			LiteralValue::Number(leaked_value)
		} else {
			LiteralValue::Str(leaked_value)
		}
	}
}

prop_compose! {
	fn generate_valid_decimal_string()(
		variant in prop_oneof![
			// Simple integers
			(-1000i64..1000i64).prop_map(|n| n.to_string()),
			// Simple decimals with 1-6 decimal places
			(-1000i64..1000i64, 1..7usize).prop_map(|(int_part, decimal_places)| {
				let decimal_part: String = (0..decimal_places)
					.map(|_| char::from(b'0' + (int_part.unsigned_abs() as u8 % 10)))
					.collect();
				format!("{}.{}", int_part, decimal_part)
			}),
			// Zero variations
			Just("0".to_string()),
			Just("0.0".to_string()),
			Just("0.00".to_string()),
			Just("0.000000".to_string()),
			Just("-0".to_string()),
			Just("-0.0".to_string()),
			// Small decimals
			Just("0.1".to_string()),
			Just("0.01".to_string()),
			Just("0.001".to_string()),
			Just("0.123456".to_string()),
			Just("-0.1".to_string()),
			Just("-0.01".to_string()),
			Just("-0.001".to_string()),
			// Large numbers
			Just("999999.999999".to_string()),
			Just("-999999.999999".to_string()),
			Just("1000000000".to_string()),
			Just("-1000000000".to_string()),
			// Numbers with leading zeros
			Just("001.100".to_string()),
			Just("-001.100".to_string()),
			Just("0001".to_string()),
			// Very precise decimals
			Just("123.123456789012345678901234567890".to_string()),
			Just("-123.123456789012345678901234567890".to_string()),
			// Edge cases
			Just("1".to_string()),
			Just("-1".to_string()),
			Just("1.0".to_string()),
			Just("-1.0".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_invalid_decimal_string()(
		variant in prop_oneof![
			// Empty/whitespace
			Just("".to_string()),
			Just("   ".to_string()),
			Just("\t".to_string()),
			Just("\n".to_string()),
			// Invalid characters
			Just("abc".to_string()),
			Just("123abc".to_string()),
			Just("12.34.56".to_string()),
			Just("12..34".to_string()),
			Just("..123".to_string()),
			Just("123..".to_string()),
			Just(".".to_string()),
			Just("-.".to_string()),
			Just("+.".to_string()),
			// Multiple signs
			Just("--123".to_string()),
			Just("++123".to_string()),
			Just("+-123".to_string()),
			Just("-+123".to_string()),
			// Invalid positions of signs
			Just("12-34".to_string()),
			Just("12+34".to_string()),
			Just("12.34-".to_string()),
			// Hex/special formats (if not supported)
			Just("0x123".to_string()),
			Just("NaN".to_string()),
			Just("inf".to_string()),
			Just("infinity".to_string()),
			// Spaces in numbers
			Just("12 34".to_string()),
			Just("12. 34".to_string()),
			Just(" 123.45".to_string()),
			Just("123.45 ".to_string()),
			// Scientific notation (depends on Decimal support)
			Just("1.23e4".to_string()),
			Just("1.23E-4".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_decimal_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne),
			Just(ComparisonOperator::Gt),
			Just(ComparisonOperator::Gte),
			Just(ComparisonOperator::Lt),
			Just(ComparisonOperator::Lte),
		]
	) -> ComparisonOperator {
		op
	}
}

prop_compose! {
	fn generate_unsupported_decimal_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Contains),
			Just(ComparisonOperator::StartsWith),
			Just(ComparisonOperator::EndsWith),
		]
	) -> ComparisonOperator {
		op
	}
}

prop_compose! {
	fn generate_decimal_literal()(
		value in generate_valid_decimal_string(),
		is_number in any::<bool>()
	) -> LiteralValue<'static> {
		let leaked_value = Box::leak(value.into_boxed_str());
		if is_number {
			LiteralValue::Number(leaked_value)
		} else {
			LiteralValue::Str(leaked_value)
		}
	}
}

prop_compose! {
	fn generate_equivalent_decimal_formats()(
		integer_part in -1000i64..1000i64,
		fractional_digits in 1..6usize
	) -> (String, String, String) {
		let base_decimal = format!("{}.{}", integer_part, &"123456"[..fractional_digits]);

		// Different but equivalent representations
		let with_trailing_zeros = format!("{}000", base_decimal);
		let with_leading_zeros = if integer_part >= 0 {
			format!("00{}", base_decimal)
		} else {
			format!("-00{}", base_decimal.strip_prefix('-').unwrap_or(&base_decimal))
		};

		(base_decimal, with_trailing_zeros, with_leading_zeros)
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	/// Property: U256 comparison should be reflexive (value equals itself)
	#[test]
	fn prop_compare_u256_reflexivity(
		value_str in generate_valid_u256_string()
	) {
		let evaluator = create_evaluator();
		let leaked_str = Box::leak(value_str.clone().into_boxed_str());

		// A value should equal itself
		prop_assert!(evaluator.compare_u256(
			&value_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A value should not be "not equal" to itself
		prop_assert!(!evaluator.compare_u256(
			&value_str,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A value should be >= and <= itself
		prop_assert!(evaluator.compare_u256(
			&value_str,
			&ComparisonOperator::Gte,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		prop_assert!(evaluator.compare_u256(
			&value_str,
			&ComparisonOperator::Lte,
			&LiteralValue::Str(leaked_str)
		).unwrap());
	}

	/// Property: Hex and decimal representations of same value should be equal
	#[test]
	fn prop_compare_u256_format_equivalence(
		value in 0u64..1_000_000u64
	) {
		let evaluator = create_evaluator();
		let decimal_str = value.to_string();
		let hex_str = format!("0x{:x}", value);
		let hex_upper_str = format!("0X{:X}", value);

		let leaked_decimal = Box::leak(decimal_str.clone().into_boxed_str());
		let leaked_hex = Box::leak(hex_str.clone().into_boxed_str());
		let leaked_hex_upper = Box::leak(hex_upper_str.clone().into_boxed_str());

		// decimal == hex
		prop_assert!(evaluator.compare_u256(
			&decimal_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_hex)
		).unwrap());

		// hex == decimal
		prop_assert!(evaluator.compare_u256(
			&hex_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_decimal)
		).unwrap());

		// lowercase hex == uppercase hex
		prop_assert!(evaluator.compare_u256(
			&hex_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_hex_upper)
		).unwrap());
	}

	/// Property: Number and String literals should behave identically
	#[test]
	fn prop_compare_u256_literal_type_consistency(
		left_value in generate_valid_u256_string(),
		right_value in generate_valid_u256_string(),
		operator in generate_u256_operator()
	) {
		let evaluator = create_evaluator();

		let leaked_right = Box::leak(right_value.clone().into_boxed_str());

		let result_with_str = evaluator.compare_u256(
			&left_value,
			&operator,
			&LiteralValue::Str(leaked_right)
		);

		let result_with_number = evaluator.compare_u256(
			&left_value,
			&operator,
			&LiteralValue::Number(leaked_right)
		);

		// Both should succeed or both should fail
		prop_assert_eq!(result_with_str.is_ok(), result_with_number.is_ok());

		// If both succeed, results should be identical
		if result_with_str.is_ok() && result_with_number.is_ok() {
			prop_assert_eq!(result_with_str.unwrap(), result_with_number.unwrap());
		}
	}

	/// Property: U256 comparison should reject unsupported operators
	#[test]
	fn prop_compare_u256_unsupported_operators(
		value_str in generate_valid_u256_string()
	) {
		let evaluator = create_evaluator();
		let leaked_str = Box::leak(value_str.clone().into_boxed_str());

		// Test unsupported string operators
		prop_assert!(evaluator.compare_u256(
			&value_str,
			&ComparisonOperator::StartsWith,
			&LiteralValue::Str(leaked_str)
		).is_err());

		prop_assert!(evaluator.compare_u256(
			&value_str,
			&ComparisonOperator::EndsWith,
			&LiteralValue::Str(leaked_str)
		).is_err());

		prop_assert!(evaluator.compare_u256(
			&value_str,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_str)
		).is_err());
	}

	/// Property: I256 comparison should be reflexive (value equals itself)
	#[test]
	fn prop_compare_i256_reflexivity(
		value_str in generate_valid_i256_string()
	) {
		let evaluator = create_evaluator();
		let leaked_str = Box::leak(value_str.clone().into_boxed_str());

		// A value should equal itself
		prop_assert!(evaluator.compare_i256(
			&value_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A value should not be "not equal" to itself
		prop_assert!(!evaluator.compare_i256(
			&value_str,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A value should be >= and <= itself
		prop_assert!(evaluator.compare_i256(
			&value_str,
			&ComparisonOperator::Gte,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		prop_assert!(evaluator.compare_i256(
			&value_str,
			&ComparisonOperator::Lte,
			&LiteralValue::Str(leaked_str)
		).unwrap());
	}

	/// Property: I256 comparison should be symmetric for equality
	#[test]
	fn prop_compare_i256_symmetry(
		left_value in generate_valid_i256_string(),
		right_value in generate_valid_i256_string()
	) {
		let evaluator = create_evaluator();
		let leaked_left = Box::leak(left_value.clone().into_boxed_str());
		let leaked_right = Box::leak(right_value.clone().into_boxed_str());

		let left_eq_right = evaluator.compare_i256(
			&left_value,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_right)
		).unwrap();

		let right_eq_left = evaluator.compare_i256(
			&right_value,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_left)
		).unwrap();

		// Equality should be symmetric: a == b iff b == a
		prop_assert_eq!(left_eq_right, right_eq_left);
	}

	/// Property: Hex and decimal representations of same value should be equal
	#[test]
	fn prop_compare_i256_format_equivalence(
		value in -1_000_000i64..1_000_000i64
	) {
		let evaluator = create_evaluator();
		let decimal_str = value.to_string();
		let hex_str = if value >= 0 {
			format!("0x{:x}", value)
		} else {
			format!("-0x{:x}", -value)
		};
		let hex_upper_str = if value >= 0 {
			format!("0X{:X}", value)
		} else {
			format!("-0X{:X}", -value)
		};

		let leaked_decimal = Box::leak(decimal_str.clone().into_boxed_str());
		let leaked_hex = Box::leak(hex_str.clone().into_boxed_str());
		let leaked_hex_upper = Box::leak(hex_upper_str.clone().into_boxed_str());

		// decimal == hex (if hex is supported by string_to_i256)
		let decimal_eq_hex = evaluator.compare_i256(
			&decimal_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_hex)
		);

		let hex_eq_decimal = evaluator.compare_i256(
			&hex_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_decimal)
		);

		// If hex parsing is supported, they should be equal
		if decimal_eq_hex.is_ok() && hex_eq_decimal.is_ok() {
			prop_assert!(decimal_eq_hex.unwrap());
			prop_assert!(hex_eq_decimal.unwrap());

			// lowercase hex == uppercase hex
			let hex_eq_hex_upper = evaluator.compare_i256(
				&hex_str,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(leaked_hex_upper)
			);

			if hex_eq_hex_upper.is_ok() {
				prop_assert!(hex_eq_hex_upper.unwrap());
			}
		}
	}

	/// Property: Number and String literals should behave identically
	#[test]
	fn prop_compare_i256_literal_type_consistency(
		left_value in generate_valid_i256_string(),
		right_value in generate_valid_i256_string(),
		operator in generate_i256_operator()
	) {
		let evaluator = create_evaluator();

		let leaked_right = Box::leak(right_value.clone().into_boxed_str());

		let result_with_str = evaluator.compare_i256(
			&left_value,
			&operator,
			&LiteralValue::Str(leaked_right)
		);

		let result_with_number = evaluator.compare_i256(
			&left_value,
			&operator,
			&LiteralValue::Number(leaked_right)
		);

		// Both should succeed or both should fail
		prop_assert_eq!(result_with_str.is_ok(), result_with_number.is_ok());

		// If both succeed, results should be identical
		if result_with_str.is_ok() && result_with_number.is_ok() {
			prop_assert_eq!(result_with_str.unwrap(), result_with_number.unwrap());
		}
	}

	/// Property: Unsupported operators should produce errors
	#[test]
	fn prop_compare_i256_unsupported_operators_error(
		left_value in generate_valid_i256_string(),
		right_value in generate_valid_i256_string(),
		operator in prop_oneof![
			Just(ComparisonOperator::Contains),
			Just(ComparisonOperator::StartsWith),
			Just(ComparisonOperator::EndsWith)
		]
	) {
		let evaluator = create_evaluator();
		let leaked_right = Box::leak(right_value.clone().into_boxed_str());

		let result = evaluator.compare_i256(
			&left_value,
			&operator,
			&LiteralValue::Str(leaked_right)
		);

		// Unsupported operators should produce error
		prop_assert!(result.is_err());
	}

	/// Property: Wrong literal types should produce type mismatch errors
	#[test]
	fn prop_compare_i256_wrong_literal_type_error(
		left_value in generate_valid_i256_string(),
		operator in generate_i256_operator()
	) {
		let evaluator = create_evaluator();

		// Bool literal should produce type error
		let result_bool = evaluator.compare_i256(
			&left_value,
			&operator,
			&LiteralValue::Bool(true)
		);
		prop_assert!(result_bool.is_err());
	}

	/// Property: Decimal comparison should be reflexive (value equals itself)
	#[test]
	fn prop_compare_fixed_point_reflexivity(
		decimal_str in generate_valid_decimal_string()
	) {
		let evaluator = create_evaluator();
		let leaked_str = Box::leak(decimal_str.clone().into_boxed_str());

		// A decimal should equal itself
		prop_assert!(evaluator.compare_fixed_point(
			&decimal_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A decimal should not be "not equal" to itself
		prop_assert!(!evaluator.compare_fixed_point(
			&decimal_str,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A decimal should be >= and <= itself
		prop_assert!(evaluator.compare_fixed_point(
			&decimal_str,
			&ComparisonOperator::Gte,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		prop_assert!(evaluator.compare_fixed_point(
			&decimal_str,
			&ComparisonOperator::Lte,
			&LiteralValue::Str(leaked_str)
		).unwrap());
	}

	/// Property: Number and String literals should behave identically
	#[test]
	fn prop_compare_fixed_point_literal_type_consistency(
		left_decimal in generate_valid_decimal_string(),
		right_decimal in generate_valid_decimal_string(),
		operator in generate_decimal_operator()
	) {
		let evaluator = create_evaluator();

		let leaked_right = Box::leak(right_decimal.clone().into_boxed_str());

		let result_with_str = evaluator.compare_fixed_point(
			&left_decimal,
			&operator,
			&LiteralValue::Str(leaked_right)
		);

		let result_with_number = evaluator.compare_fixed_point(
			&left_decimal,
			&operator,
			&LiteralValue::Number(leaked_right)
		);

		// Both should succeed or both should fail
		prop_assert_eq!(result_with_str.is_ok(), result_with_number.is_ok());

		// If both succeed, results should be identical
		if result_with_str.is_ok() && result_with_number.is_ok() {
			prop_assert_eq!(result_with_str.unwrap(), result_with_number.unwrap());
		}
	}

	/// Property: Invalid decimal strings should always produce parse errors
	#[test]
	fn prop_compare_fixed_point_invalid_left_error(
		invalid_left in generate_invalid_decimal_string(),
		valid_right in generate_valid_decimal_string(),
		operator in generate_decimal_operator()
	) {
		let evaluator = create_evaluator();
		let leaked_right = Box::leak(valid_right.clone().into_boxed_str());

		// Invalid left side should produce error
		let result = evaluator.compare_fixed_point(
			&invalid_left,
			&operator,
			&LiteralValue::Str(leaked_right)
		);
		prop_assert!(result.is_err());
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::ParseError(_)));
	}

	/// Property: Unsupported operators should produce errors
	#[test]
	fn prop_compare_fixed_point_unsupported_operators_error(
		left_decimal in generate_valid_decimal_string(),
		right_decimal in generate_valid_decimal_string(),
		operator in generate_unsupported_decimal_operator()
	) {
		let evaluator = create_evaluator();
		let leaked_right = Box::leak(right_decimal.clone().into_boxed_str());

		let result = evaluator.compare_fixed_point(
			&left_decimal,
			&operator,
			&LiteralValue::Str(leaked_right)
		);

		// Unsupported operators should produce error
		prop_assert!(result.is_err());
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::UnsupportedOperator(_)));
	}

	/// Property: Equivalent decimal formats should be equal
	#[test]
	fn prop_compare_fixed_point_format_equivalence(
		(base, with_trailing_zeros, with_leading_zeros) in generate_equivalent_decimal_formats()
	) {
		let evaluator = create_evaluator();

		let leaked_trailing = Box::leak(with_trailing_zeros.clone().into_boxed_str());
		let leaked_leading = Box::leak(with_leading_zeros.clone().into_boxed_str());

		// Different representations of same decimal should be equal
		let base_eq_trailing = evaluator.compare_fixed_point(
			&base,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_trailing)
		);

		let base_eq_leading = evaluator.compare_fixed_point(
			&base,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_leading)
		);

		if base_eq_trailing.is_ok() && base_eq_leading.is_ok() {
			prop_assert!(base_eq_trailing.unwrap(),
				"Format equivalence failed: {} should equal {}", base, with_trailing_zeros);
			prop_assert!(base_eq_leading.unwrap(),
				"Format equivalence failed: {} should equal {}", base, with_leading_zeros);
		}
	}
}
