//! Property-based tests for EVM evaluator functionality (boolean).
//! Tests cover JSON value matching, type detection, and comparison logic.

use crate::properties::filters::evm::strings_evaluator::create_evaluator;
use openzeppelin_monitor::services::filter::{ComparisonOperator, EvaluationError, LiteralValue};
use proptest::{prelude::*, test_runner::Config};

prop_compose! {
	fn generate_valid_boolean_string()(
		variant in prop_oneof![
			Just("true".to_string()),
			Just("false".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_invalid_boolean_string()(
		variant in prop_oneof![
			// Empty/whitespace
			Just("".to_string()),
			Just("   ".to_string()),
			Just("\t".to_string()),
			Just("\n".to_string()),
			// Case variations (Rust's parse::<bool>() is case-sensitive)
			Just("True".to_string()),
			Just("TRUE".to_string()),
			Just("False".to_string()),
			Just("FALSE".to_string()),
			// With spaces
			Just(" true".to_string()),
			Just("true ".to_string()),
			Just(" false ".to_string()),
			// Numbers
			Just("1".to_string()),
			Just("0".to_string()),
			Just("-1".to_string()),
			// Other strings
			Just("yes".to_string()),
			Just("no".to_string()),
			Just("on".to_string()),
			Just("off".to_string()),
			Just("t".to_string()),
			Just("f".to_string()),
			Just("bool".to_string()),
			Just("null".to_string()),
			Just("undefined".to_string()),
			// Invalid formats
			Just("true123".to_string()),
			Just("false456".to_string()),
			Just("abc".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_boolean_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne),
		]
	) -> ComparisonOperator {
		op
	}
}

prop_compose! {
	fn generate_unsupported_boolean_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Gt),
			Just(ComparisonOperator::Gte),
			Just(ComparisonOperator::Lt),
			Just(ComparisonOperator::Lte),
			Just(ComparisonOperator::Contains),
			Just(ComparisonOperator::StartsWith),
			Just(ComparisonOperator::EndsWith),
		]
	) -> ComparisonOperator {
		op
	}
}

prop_compose! {
	fn generate_boolean_literal()(
		value in any::<bool>()
	) -> LiteralValue<'static> {
		LiteralValue::Bool(value)
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn prop_compare_boolean_reflexivity(
		bool_str in generate_valid_boolean_string()
	) {
		let evaluator = create_evaluator();
		let bool_value = bool_str.parse::<bool>().unwrap();

		// A boolean should equal itself
		prop_assert!(evaluator.compare_boolean(
			&bool_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Bool(bool_value)
		).unwrap());

		// A boolean should not be "not equal" to itself
		prop_assert!(!evaluator.compare_boolean(
			&bool_str,
			&ComparisonOperator::Ne,
			&LiteralValue::Bool(bool_value)
		).unwrap());
	}

	/// Property: Boolean comparison should be symmetric for equality
	#[test]
	fn prop_compare_boolean_symmetry(
		bool_str1 in generate_valid_boolean_string(),
		bool_str2 in generate_valid_boolean_string()
	) {
		let evaluator = create_evaluator();
		let bool_val1 = bool_str1.parse::<bool>().unwrap();
		let bool_val2 = bool_str2.parse::<bool>().unwrap();

		let result1 = evaluator.compare_boolean(
			&bool_str1,
			&ComparisonOperator::Eq,
			&LiteralValue::Bool(bool_val2)
		).unwrap();

		let result2 = evaluator.compare_boolean(
			&bool_str2,
			&ComparisonOperator::Eq,
			&LiteralValue::Bool(bool_val1)
		).unwrap();

		// Equality should be symmetric: a == b iff b == a
		prop_assert_eq!(result1, result2);
	}

	/// Property: Boolean logic correctness
	#[test]
	fn prop_compare_boolean_logic_correctness(
		lhs_bool in any::<bool>(),
		rhs_bool in any::<bool>(),
		operator in generate_boolean_operator()
	) {
		let evaluator = create_evaluator();
		let lhs_str = lhs_bool.to_string();

		let result = evaluator.compare_boolean(
			&lhs_str,
			&operator,
			&LiteralValue::Bool(rhs_bool)
		).unwrap();

		let expected = match operator {
			ComparisonOperator::Eq => lhs_bool == rhs_bool,
			ComparisonOperator::Ne => lhs_bool != rhs_bool,
			_ => unreachable!()
		};

		prop_assert_eq!(result, expected,
			"Boolean logic failed: {} {:?} {} should be {}", lhs_bool, operator, rhs_bool, expected);
	}

	/// Property: Invalid boolean strings should produce parse errors
	#[test]
	fn prop_compare_boolean_invalid_string_error(
		invalid_str in generate_invalid_boolean_string(),
		rhs_bool in any::<bool>(),
		operator in generate_boolean_operator()
	) {
		let evaluator = create_evaluator();

		let result = evaluator.compare_boolean(
			&invalid_str,
			&operator,
			&LiteralValue::Bool(rhs_bool)
		);

		// Invalid boolean strings should produce parse error
		prop_assert!(result.is_err());
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::ParseError(_)));
	}

	/// Property: Unsupported operators should produce errors
	#[test]
	fn prop_compare_boolean_unsupported_operators_error(
		bool_str in generate_valid_boolean_string(),
		rhs_bool in any::<bool>(),
		operator in generate_unsupported_boolean_operator()
	) {
		let evaluator = create_evaluator();

		let result = evaluator.compare_boolean(
			&bool_str,
			&operator,
			&LiteralValue::Bool(rhs_bool)
		);

		// Unsupported operators should produce error
		prop_assert!(result.is_err());
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::UnsupportedOperator(_)));
	}
}
