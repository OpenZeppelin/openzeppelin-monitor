//! Property-based tests for EVM evaluator functionality.
//! Tests cover JSON value matching, type detection, and comparison logic.

use crate::properties::filters::evm::strings_evaluator::create_evaluator;
use openzeppelin_monitor::services::filter::{ComparisonOperator, EvaluationError, LiteralValue};
use proptest::{prelude::*, test_runner::Config};

prop_compose! {
	fn generate_valid_evm_address()(
		bytes in prop::collection::vec(any::<u8>(), 20),
		case_variant in prop_oneof![
			Just("lowercase"),
			Just("uppercase"),
			Just("mixed"),
			Just("checksum") // Could add EIP-55 checksum if supported
		],
		prefix_variant in prop_oneof![
			Just("0x"),
			Just("0X"),
			Just("")
		]
	) -> String {
		let hex = hex::encode(bytes);
		let hex_with_case = match case_variant {
			"lowercase" => hex.to_lowercase(),
			"uppercase" => hex.to_uppercase(),
			"mixed" => hex.chars().enumerate().map(|(i, c)| {
				if i % 2 == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }
			}).collect(),
			_ => hex.to_lowercase() // Default to lowercase
		};
		format!("{}{}", prefix_variant, hex_with_case)
	}
}

prop_compose! {
	fn generate_address_comparison_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne)
		]
	) -> ComparisonOperator {
		op
	}
}

prop_compose! {
	fn generate_unsupported_address_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Gt),
			Just(ComparisonOperator::Gte),
			Just(ComparisonOperator::Lt),
			Just(ComparisonOperator::Lte),
			Just(ComparisonOperator::Contains),
			Just(ComparisonOperator::StartsWith),
			Just(ComparisonOperator::EndsWith)
		]
	) -> ComparisonOperator {
		op
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	/// Property: Address comparison should be reflexive (address equals itself)
	#[test]
	fn prop_compare_address_reflexivity(
		address in generate_valid_evm_address()
	) {
		let evaluator = create_evaluator();
		let leaked_addr = Box::leak(address.clone().into_boxed_str());

		// An address should equal itself
		prop_assert!(evaluator.compare_address(
			&address,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_addr)
		).unwrap());

		// An address should not be "not equal" to itself
		prop_assert!(!evaluator.compare_address(
			&address,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_addr)
		).unwrap());
	}

	/// Property: Address comparison should be symmetric for equality
	#[test]
	fn prop_compare_address_symmetry(
		addr1 in generate_valid_evm_address(),
		addr2 in generate_valid_evm_address()
	) {
		let evaluator = create_evaluator();
		let leaked_addr1 = Box::leak(addr1.clone().into_boxed_str());
		let leaked_addr2 = Box::leak(addr2.clone().into_boxed_str());

		let addr1_eq_addr2 = evaluator.compare_address(
			&addr1,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_addr2)
		).unwrap();

		let addr2_eq_addr1 = evaluator.compare_address(
			&addr2,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_addr1)
		).unwrap();

		// Equality should be symmetric: addr1 == addr2 iff addr2 == addr1
		prop_assert_eq!(addr1_eq_addr2, addr2_eq_addr1);
	}

	/// Property: Case and space normalization should work correctly
	#[test]
	fn prop_compare_address_normalization(
		base_bytes in prop::collection::vec(any::<u8>(), 20)
	) {
		let evaluator = create_evaluator();

		// Generate different representations of the same address
		let base_hex = hex::encode(&base_bytes);
		let lowercase_addr = format!("0x{}", base_hex.to_lowercase());
		let uppercase_addr = format!("0x{}", base_hex.to_uppercase());
		let mixed_case_addr: String = format!("0x{}", base_hex.chars().enumerate().map(|(i, c)| {
			if i % 2 == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }
		}).collect::<String>());
		let addr_with_spaces = format!("0x{}", base_hex.chars().enumerate().map(|(i, c)| {
			if i > 0 && i % 4 == 0 { format!(" {}", c) } else { c.to_string() }
		}).collect::<String>());

		let leaked_lowercase = Box::leak(lowercase_addr.clone().into_boxed_str());
		let leaked_uppercase = Box::leak(uppercase_addr.clone().into_boxed_str());
		let leaked_mixed = Box::leak(mixed_case_addr.clone().into_boxed_str());
		let leaked_spaces = Box::leak(addr_with_spaces.clone().into_boxed_str());

		// All representations should be equal to each other
		prop_assert!(evaluator.compare_address(
			&lowercase_addr,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_uppercase)
		).unwrap());

		prop_assert!(evaluator.compare_address(
			&uppercase_addr,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_mixed)
		).unwrap());

		prop_assert!(evaluator.compare_address(
			&mixed_case_addr,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_spaces)
		).unwrap());

		prop_assert!(evaluator.compare_address(
			&addr_with_spaces,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_lowercase)
		).unwrap());
	}

	/// Property: Different addresses should not be equal
	#[test]
	fn prop_compare_address_different_addresses_not_equal(
		bytes1 in prop::collection::vec(any::<u8>(), 20),
		bytes2 in prop::collection::vec(any::<u8>(), 20)
	) {
		let evaluator = create_evaluator();

		// Only test if addresses are actually different
		if bytes1 != bytes2 {
			let addr1 = format!("0x{}", hex::encode(&bytes1));
			let addr2 = format!("0x{}", hex::encode(&bytes2));
			let leaked_addr2 = Box::leak(addr2.clone().into_boxed_str());

			// Different addresses should not be equal
			prop_assert!(!evaluator.compare_address(
				&addr1,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(leaked_addr2)
			).unwrap());

			// Different addresses should be "not equal"
			prop_assert!(evaluator.compare_address(
				&addr1,
				&ComparisonOperator::Ne,
				&LiteralValue::Str(leaked_addr2)
			).unwrap());
		}
	}

	/// Property: Unsupported operators should produce errors
	#[test]
	fn prop_compare_address_unsupported_operators_error(
		addr1 in generate_valid_evm_address(),
		addr2 in generate_valid_evm_address(),
		operator in generate_unsupported_address_operator()
	) {
		let evaluator = create_evaluator();
		let leaked_addr2 = Box::leak(addr2.clone().into_boxed_str());

		let result = evaluator.compare_address(
			&addr1,
			&operator,
			&LiteralValue::Str(leaked_addr2)
		);

		// Unsupported operators should produce error
		prop_assert!(result.is_err());
		// Should specifically be an UnsupportedOperator error
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::UnsupportedOperator(_)));
	}

	/// Property: Wrong literal types should produce type mismatch errors
	#[test]
	fn prop_compare_address_wrong_literal_type_error(
		address in generate_valid_evm_address(),
		operator in generate_address_comparison_operator()
	) {
		let evaluator = create_evaluator();

		// Number literal should produce type error
		let result_number = evaluator.compare_address(
			&address,
			&operator,
			&LiteralValue::Number("123")
		);
		prop_assert!(result_number.is_err());
		prop_assert!(matches!(result_number.unwrap_err(),
			EvaluationError::TypeMismatch(_)));

		// Bool literal should produce type error
		let result_bool = evaluator.compare_address(
			&address,
			&operator,
			&LiteralValue::Bool(true)
		);
		prop_assert!(result_bool.is_err());
		prop_assert!(matches!(result_bool.unwrap_err(),
			EvaluationError::TypeMismatch(_)));
	}
}
