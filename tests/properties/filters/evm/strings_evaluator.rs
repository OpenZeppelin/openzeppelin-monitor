//! Property-based tests for EVM evaluator functionality (strings).
//! Tests cover JSON value matching, type detection, and comparison logic.

use openzeppelin_monitor::services::filter::{
	ComparisonOperator, EVMArgs, EVMConditionEvaluator, EvaluationError, LiteralValue,
};
use proptest::{prelude::*, test_runner::Config};
use rust_decimal::Decimal;
use serde_json::Value as JsonValue;
use std::str::FromStr;

// Strategy for generating JSON string values with various patterns
prop_compose! {
	fn generate_json_string_value()(
		content in prop_oneof![
			// Regular strings
			"[a-zA-Z0-9_]{1,20}".prop_map(|s| s.to_string()),
			// Address-like strings (42 chars with 0x prefix)
			prop::collection::vec(any::<u8>(), 20).prop_map(|bytes| format!("0x{}", hex::encode(bytes))),
			// Mixed case addresses for testing normalization
			prop::collection::vec(any::<u8>(), 20).prop_map(|bytes| {
				let hex = hex::encode(bytes);
				let mut result = "0x".to_string();
				for (i, c) in hex.chars().enumerate() {
					if i % 2 == 0 {
						result.push(c.to_ascii_uppercase());
					} else {
						result.push(c.to_ascii_lowercase());
					}
				}
				result
			}),
			// Decimal-like strings
			(any::<i64>(), 1..5u8).prop_map(|(int_part, decimal_places)| {
				format!("{}.{}", int_part, "0".repeat(decimal_places as usize))
			}),
			// Hex strings that aren't addresses
			prop::collection::vec(any::<u8>(), 1..19).prop_map(|bytes| format!("0x{}", hex::encode(bytes))),
			prop::collection::vec(any::<u8>(), 21..32).prop_map(|bytes| format!("0x{}", hex::encode(bytes))),
			// 32-byte hex strings (bytes32)
			prop::collection::vec(any::<u8>(), 32).prop_map(|bytes| format!("0x{}", hex::encode(bytes))),
			// Special strings
			Just("".to_string()),
			Just(" ".to_string()),
			Just("null".to_string()),
			Just("true".to_string()),
			Just("false".to_string()),
		]
	) -> JsonValue {
		JsonValue::String(content)
	}
}

// Strategy for generating JSON number values
prop_compose! {
	fn generate_json_number_value()(
		variant in prop_oneof![
			any::<i64>().prop_map(serde_json::Number::from),
			any::<i64>().prop_map(serde_json::Number::from),
			(any::<f64>().prop_filter("Must be finite", |f| f.is_finite()))
				.prop_map(|f| serde_json::Number::from_f64(f).unwrap())
		]
	) -> JsonValue {
		JsonValue::Number(variant)
	}
}

// Strategy for generating nested JSON objects
prop_compose! {
	fn generate_nested_json_object()(
		keys in prop::collection::vec("[a-zA-Z0-9_]{1,10}", 1..4),
		values in prop::collection::vec(
			prop_oneof![
				generate_json_string_value(),
				generate_json_number_value(),
				any::<bool>().prop_map(JsonValue::Bool),
				Just(JsonValue::Null)
			], 1..4
		)
	) -> JsonValue {
		let mut map = serde_json::Map::new();
		for (key, value) in keys.into_iter().zip(values.into_iter()) {
			map.insert(key, value);
		}
		JsonValue::Object(map)
	}
}

// Strategy for generating deeply nested objects for recursive testing
prop_compose! {
	fn generate_deeply_nested_object()(
		depth in 1..4usize
	) -> JsonValue {
		fn create_nested(depth: usize) -> JsonValue {
			if depth == 0 {
				JsonValue::String("target_value".to_string())
			} else {
				let mut map = serde_json::Map::new();
				map.insert(format!("level_{}", depth), create_nested(depth - 1));
				map.insert("other_field".to_string(), JsonValue::String("other_value".to_string()));
				JsonValue::Object(map)
			}
		}
		create_nested(depth)
	}
}

// Strategy for generating any JSON value type
prop_compose! {
	fn generate_any_json_value()(
		variant in prop_oneof![
			generate_json_string_value(),
			generate_json_number_value(),
			any::<bool>().prop_map(JsonValue::Bool),
			generate_nested_json_object(),
			Just(JsonValue::Null),
			// Simple arrays
			prop::collection::vec(generate_json_string_value(), 0..3).prop_map(JsonValue::Array)
		]
	) -> JsonValue {
		variant
	}
}

// Strategy for generating comparison target strings
prop_compose! {
	fn generate_comparison_target()(
		variant in prop_oneof![
			"[a-zA-Z0-9_]{1,20}".prop_map(|s| s.to_string()),
			any::<i64>().prop_map(|n| n.to_string()),
			any::<u64>().prop_map(|n| n.to_string()),
			any::<bool>().prop_map(|b| b.to_string()),
			Just("null".to_string()),
			Just("NULL".to_string()),
			Just("Null".to_string()),
			// Decimal strings
			(any::<i64>(), 1..5u8).prop_map(|(int_part, decimal_places)| {
				format!("{}.{}", int_part, "0".repeat(decimal_places as usize))
			}),
			// Address-like strings
			prop::collection::vec(any::<u8>(), 20).prop_map(|bytes| format!("0x{}", hex::encode(bytes))),
			// Edge case strings
			Just("".to_string()),
			Just(" ".to_string()),
			Just("\t".to_string()),
			Just("\n".to_string()),
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_string_with_case_variations()(
		base in "[a-zA-Z0-9_\\-\\.]{1,20}"
	) -> String {
		base
	}
}

prop_compose! {
	fn generate_string_with_special_chars()(
		content in prop_oneof![
			"[a-zA-Z0-9_\\-\\.\\s]{1,30}",
			Just("".to_string()),
			Just(" ".to_string()),
			Just("   ".to_string()),
			Just("\t".to_string()),
			Just("\n".to_string()),
			Just("!@#$%^&*()".to_string()),
			Just("Hello, World! 123".to_string()),
			Just("UPPERCASE_ONLY".to_string()),
			Just("lowercase_only".to_string()),
			Just("MiXeD_cAsE_StRiNg".to_string()),
			Just("0123456789".to_string()),
			Just("0x1234abcdef".to_string()),
		]
	) -> String {
		content
	}
}

prop_compose! {
	fn generate_prefix_suffix_string()(
		prefix in "[a-zA-Z]{1,10}",
		middle in "[a-zA-Z0-9]{5,15}",
		suffix in "[a-zA-Z]{1,10}"
	) -> (String, String, String, String) {
		let full_string = format!("{}{}{}", prefix, middle, suffix);
		(full_string, prefix, middle, suffix)
	}
}

prop_compose! {
	fn generate_ascii_string()(
		content in "[a-zA-Z0-9_\\-\\.\\s]{0,50}"
	) -> String {
		content
	}
}

prop_compose! {
	fn generate_string_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne),
			Just(ComparisonOperator::StartsWith),
			Just(ComparisonOperator::EndsWith),
			Just(ComparisonOperator::Contains),
		]
	) -> ComparisonOperator {
		op
	}
}

// Helper to create a dummy EVMConditionEvaluator
pub fn create_evaluator() -> EVMConditionEvaluator<'static> {
	static EMPTY_ARGS: &EVMArgs = &[];
	EVMConditionEvaluator::new(EMPTY_ARGS)
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	/// Property: String comparison should be case-insensitive for non-address strings
	#[test]
	fn prop_check_json_value_string_case_insensitive(
		content in "[a-zA-Z0-9_]{5,20}".prop_filter("Should not look like address", |s| s.len() != 40)
	) {
		let evaluator = create_evaluator();
		let json_string = JsonValue::String(content.clone());
		let lowercase = content.to_lowercase();
		let uppercase = content.to_uppercase();

		// Same content with different cases should match
		prop_assert!(evaluator.check_json_value_matches_str(&json_string, &lowercase));
		prop_assert!(evaluator.check_json_value_matches_str(&json_string, &uppercase));

		// Cross-case matching
		let mixed_case = content.chars().enumerate().map(|(i, c)| {
			if i % 2 == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }
		}).collect::<String>();
		prop_assert!(evaluator.check_json_value_matches_str(&json_string, &mixed_case));
	}

	/// Property: Address comparison should use address normalization logic
	#[test]
	fn prop_check_json_value_address_normalization(
		bytes in prop::collection::vec(any::<u8>(), 20)
	) {
		let evaluator = create_evaluator();
		let hex = hex::encode(&bytes);

		let lowercase_addr = format!("0x{}", hex.to_lowercase());
		let uppercase_addr = format!("0x{}", hex.to_uppercase());

		let json_addr_lower = JsonValue::String(lowercase_addr.clone());
		let json_addr_upper = JsonValue::String(uppercase_addr.clone());

		// Both should match regardless of case when detected as address
		prop_assert!(evaluator.check_json_value_matches_str(&json_addr_lower, &uppercase_addr));
		prop_assert!(evaluator.check_json_value_matches_str(&json_addr_upper, &lowercase_addr));
		prop_assert!(evaluator.check_json_value_matches_str(&json_addr_lower, &lowercase_addr));
	}

	/// Property: Numeric comparison should work with decimal parsing
	#[test]
	fn prop_check_json_value_numeric_comparison(
		num in any::<i64>().prop_filter("Avoid extreme values", |n| n.abs() < 1_000_000_000_000_000_000)
	) {
		let evaluator = create_evaluator();
		let json_number = JsonValue::Number(serde_json::Number::from(num));
		let num_string = num.to_string();

		// Number should match its string representation
		prop_assert!(evaluator.check_json_value_matches_str(&json_number, &num_string));

		// Should not match with whitespace (exact comparison)
		let with_leading_space = format!(" {}", num_string);
		let with_trailing_space = format!("{} ", num_string);
		prop_assert!(!evaluator.check_json_value_matches_str(&json_number, &with_leading_space));
		prop_assert!(!evaluator.check_json_value_matches_str(&json_number, &with_trailing_space));
	}

	/// Property: Boolean comparison should be case-insensitive
	#[test]
	fn prop_check_json_value_boolean_case_insensitive(
		b in any::<bool>()
	) {
		let evaluator = create_evaluator();
		let json_bool = JsonValue::Bool(b);
		let bool_string = b.to_string();
		let bool_string_upper = bool_string.to_uppercase();
		let bool_string_lower = bool_string.to_lowercase();

		prop_assert!(evaluator.check_json_value_matches_str(&json_bool, &bool_string));
		prop_assert!(evaluator.check_json_value_matches_str(&json_bool, &bool_string_upper));
		prop_assert!(evaluator.check_json_value_matches_str(&json_bool, &bool_string_lower));
	}

	/// Property: Null comparison should only match exact "null" string (case-sensitive)
	#[test]
	fn prop_check_json_value_null_comparison(
		test_string in generate_comparison_target()
	) {
		let evaluator = create_evaluator();
		let json_null = JsonValue::Null;
		let should_match = test_string == "null";

		prop_assert_eq!(
			evaluator.check_json_value_matches_str(&json_null, &test_string),
			should_match
		);
	}

	/// Property: Object comparison should recursively search through values
	#[test]
	fn prop_check_json_value_object_recursive_search(
		obj in generate_nested_json_object(),
		search_string in generate_comparison_target()
	) {
		let evaluator = create_evaluator();

		// Manually determine if any value in the object would match
		let expected_match = if let JsonValue::Object(map) = &obj {
			map.values().any(|val| evaluator.check_json_value_matches_str(val, &search_string))
		} else {
			false
		};

		let actual_match = evaluator.check_json_value_matches_str(&obj, &search_string);
		prop_assert_eq!(actual_match, expected_match);
	}

	/// Property: Deeply nested objects should be recursively searched
	#[test]
	fn prop_check_json_value_deep_recursive_search(
		nested_obj in generate_deeply_nested_object()
	) {
		let evaluator = create_evaluator();

		// The deeply nested object always contains "target_value" at some level
		prop_assert!(evaluator.check_json_value_matches_str(&nested_obj, "target_value"));
		prop_assert!(evaluator.check_json_value_matches_str(&nested_obj, "TARGET_VALUE"));
		prop_assert!(!evaluator.check_json_value_matches_str(&nested_obj, "nonexistent_value"));
	}

	/// Property: Array comparison should always return false
	#[test]
	fn prop_check_json_value_array_always_false(
		array_content in prop::collection::vec(generate_any_json_value(), 0..5),
		search_string in generate_comparison_target()
	) {
		let evaluator = create_evaluator();
		let json_array = JsonValue::Array(array_content);

		// Arrays should never match in check_json_value_matches_str
		prop_assert!(!evaluator.check_json_value_matches_str(&json_array, &search_string));
	}

	/// Property: Decimal parsing should be consistent between numbers and strings
	#[test]
	fn prop_check_json_value_decimal_consistency(
		integer_part in -1000i32..1000i32,
		decimal_part in 1..999999u32
	) {
		let evaluator = create_evaluator();
		let decimal_str = format!("{}.{}", integer_part, decimal_part);

		// Test with number JSON value containing the same decimal
		if let Ok(parsed_f64) = decimal_str.parse::<f64>() {
			if let Some(json_number) = serde_json::Number::from_f64(parsed_f64) {
				let json_val = JsonValue::Number(json_number);

				let result = evaluator.check_json_value_matches_str(&json_val, &decimal_str);

				// Check consistency with decimal parsing logic
				let num_str = json_val.as_f64().map(|f| f.to_string()).unwrap_or_default();
				if let (Ok(lhs_dec), Ok(rhs_dec)) = (Decimal::from_str(&num_str), Decimal::from_str(&decimal_str)) {
					let expected_match = lhs_dec == rhs_dec;
					prop_assert_eq!(result, expected_match);
				}
			}
		}
	}

	/// Property: Values should match their own string representations (reflexivity)
	#[test]
	fn prop_check_json_value_reflexivity(
		json_val in prop_oneof![
			generate_json_string_value(),
			any::<bool>().prop_map(JsonValue::Bool),
			Just(JsonValue::Null)
		]
	) {
		let evaluator = create_evaluator();

		let string_repr = match &json_val {
			JsonValue::String(s) => s.clone(),
			JsonValue::Bool(b) => b.to_string(),
			JsonValue::Null => "null".to_string(),
			_ => return Ok(()) // Skip other types
		};

		// A value should match its own string representation (considering case-insensitivity)
		let matches_exact = evaluator.check_json_value_matches_str(&json_val, &string_repr);
		let matches_lower = evaluator.check_json_value_matches_str(&json_val, &string_repr.to_lowercase());

		prop_assert!(matches_exact || matches_lower,
			"Value {:?} should match its string representation '{}'", json_val, string_repr);
	}

	/// Property: Array equality should be reflexive (arrays equal themselves)
	#[test]
	fn prop_compare_array_reflexivity(
		array_content in prop::collection::vec(generate_any_json_value(), 0..5)
	) {
		let evaluator = create_evaluator();
		let json_array_str = serde_json::to_string(&JsonValue::Array(array_content)).unwrap();

		// An array should equal itself
		prop_assert!(evaluator.compare_array(
			&json_array_str,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(&json_array_str)
		).unwrap());

		// And should not be "not equal" to itself
		prop_assert!(!evaluator.compare_array(
			&json_array_str,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(&json_array_str)
		).unwrap());
	}

	/// Property: Contains should find items that exist and not find items that don't
	#[test]
	fn prop_compare_array_contains_correctness(
		array_items in prop::collection::vec(generate_json_string_value(), 1..5),
		search_target in generate_comparison_target()
	) {
		let evaluator = create_evaluator();
		let json_array_str = serde_json::to_string(&JsonValue::Array(array_items.clone())).unwrap();

		let result = evaluator.compare_array(
			&json_array_str,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(&search_target)
		).unwrap();

		let expected = array_items.iter().any(|item| {
			evaluator.check_json_value_matches_str(item, &search_target)
		});

		prop_assert_eq!(result, expected);
	}

	/// Property: Invalid JSON should always produce errors for Eq/Ne operations
	#[test]
	fn prop_compare_array_invalid_json_errors(
		invalid_json in prop_oneof![
			Just("not json at all"),
			Just("{broken json"),
			Just("123"), // Valid JSON but not array
			Just("\"string\""), // Valid JSON but not array
			Just("true"), // Valid JSON but not array
		],
		operator in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne)
		]
	) {
		let evaluator = create_evaluator();

		let result = evaluator.compare_array(
			invalid_json,
			&operator,
			&LiteralValue::Str("[]")
		);

		// Should always be an error
		prop_assert!(result.is_err());
	}

	/// Property: Number literals should work with Contains operator
	#[test]
	fn prop_compare_array_contains_number_literal(
		numbers in prop::collection::vec(any::<i32>(), 1..5),
		search_number in any::<i32>()
	) {
		let evaluator = create_evaluator();

		let json_numbers: Vec<JsonValue> = numbers.iter()
			.map(|&n| JsonValue::Number(serde_json::Number::from(n)))
			.collect();
		let json_array_str = serde_json::to_string(&JsonValue::Array(json_numbers)).unwrap();

		let result = evaluator.compare_array(
			&json_array_str,
			&ComparisonOperator::Contains,
			&LiteralValue::Number(&search_number.to_string())
		).unwrap();

		let expected = numbers.contains(&search_number);
		prop_assert_eq!(result, expected);
	}

	/// Property: String comparison should be reflexive (string equals itself)
	#[test]
	fn prop_compare_string_reflexivity(
		string_val in generate_string_with_special_chars()
	) {
		let evaluator = create_evaluator();
		let leaked_str = Box::leak(string_val.clone().into_boxed_str());

		// A string should equal itself
		prop_assert!(evaluator.compare_string(
			&string_val,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A string should not be "not equal" to itself
		prop_assert!(!evaluator.compare_string(
			&string_val,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A string should start with itself
		prop_assert!(evaluator.compare_string(
			&string_val,
			&ComparisonOperator::StartsWith,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A string should end with itself
		prop_assert!(evaluator.compare_string(
			&string_val,
			&ComparisonOperator::EndsWith,
			&LiteralValue::Str(leaked_str)
		).unwrap());

		// A string should contain itself
		prop_assert!(evaluator.compare_string(
			&string_val,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_str)
		).unwrap());
	}

	/// Property: String comparison should be case-insensitive
	#[test]
	fn prop_compare_string_case_insensitive(
		base_string in generate_string_with_case_variations()
	) {
		let evaluator = create_evaluator();

		let lowercase = base_string.to_lowercase();
		let uppercase = base_string.to_uppercase();
		let mixed_case = base_string.chars().enumerate().map(|(i, c)| {
			if i % 2 == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }
		}).collect::<String>();

		let leaked_lowercase = Box::leak(lowercase.clone().into_boxed_str());
		let leaked_uppercase = Box::leak(uppercase.clone().into_boxed_str());
		let leaked_mixed = Box::leak(mixed_case.clone().into_boxed_str());

		// All case variations should be equal to each other
		prop_assert!(evaluator.compare_string(
			&lowercase,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_uppercase)
		).unwrap());

		prop_assert!(evaluator.compare_string(
			&uppercase,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_mixed)
		).unwrap());

		prop_assert!(evaluator.compare_string(
			&mixed_case,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_lowercase)
		).unwrap());

		// Case variations should also work with other operators
		prop_assert!(evaluator.compare_string(
			&uppercase,
			&ComparisonOperator::StartsWith,
			&LiteralValue::Str(leaked_lowercase)
		).unwrap());

		prop_assert!(evaluator.compare_string(
			&lowercase,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_mixed)
		).unwrap());
	}

	/// Property: StartsWith should work correctly with prefixes
	#[test]
	fn prop_compare_string_starts_with_correctness(
		(full_string, prefix, _middle, _suffix) in generate_prefix_suffix_string()
	) {
		let evaluator = create_evaluator();
		let leaked_prefix = Box::leak(prefix.clone().into_boxed_str());

		// String should start with its own prefix
		prop_assert!(evaluator.compare_string(
			&full_string,
			&ComparisonOperator::StartsWith,
			&LiteralValue::Str(leaked_prefix)
		).unwrap());

		// Test case insensitivity
		let prefix_upper = prefix.to_uppercase();
		let leaked_prefix_upper = Box::leak(prefix_upper.clone().into_boxed_str());

		prop_assert!(evaluator.compare_string(
			&full_string,
			&ComparisonOperator::StartsWith,
			&LiteralValue::Str(leaked_prefix_upper)
		).unwrap());
	}

	/// Property: EndsWith should work correctly with suffixes
	#[test]
	fn prop_compare_string_ends_with_correctness(
		(full_string, _prefix, _middle, suffix) in generate_prefix_suffix_string()
	) {
		let evaluator = create_evaluator();
		let leaked_suffix = Box::leak(suffix.clone().into_boxed_str());

		// String should end with its own suffix
		prop_assert!(evaluator.compare_string(
			&full_string,
			&ComparisonOperator::EndsWith,
			&LiteralValue::Str(leaked_suffix)
		).unwrap());

		// Test case insensitivity
		let suffix_upper = suffix.to_uppercase();
		let leaked_suffix_upper = Box::leak(suffix_upper.clone().into_boxed_str());

		prop_assert!(evaluator.compare_string(
			&full_string,
			&ComparisonOperator::EndsWith,
			&LiteralValue::Str(leaked_suffix_upper)
		).unwrap());
	}

	/// Property: Whitespace handling
	#[test]
	fn prop_compare_string_whitespace_handling(
		base_word in "[a-zA-Z]{3,10}"
	) {
		let evaluator = create_evaluator();

		let string_with_spaces = format!("  {}  ", base_word);
		let string_clean = base_word.clone();

		let leaked_clean = Box::leak(string_clean.clone().into_boxed_str());
		let leaked_spaces = Box::leak(string_with_spaces.clone().into_boxed_str());

		// Strings with whitespace should NOT equal clean strings (exact comparison after lowercase)
		prop_assert!(!evaluator.compare_string(
			&string_with_spaces,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_clean)
		).unwrap());

		// But strings with spaces should contain the clean word
		prop_assert!(evaluator.compare_string(
			&string_with_spaces,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_clean)
		).unwrap());

		// And spaces string should equal itself
		prop_assert!(evaluator.compare_string(
			&string_with_spaces,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_spaces)
		).unwrap());
	}

	/// Property: Special characters should be handled literally
	#[test]
	fn prop_compare_string_special_characters(
		special_chars in prop_oneof![
			Just("!@#$%^&*()".to_string()),
			Just("0x1234abcdef".to_string()),
			Just("hello.world@example.com".to_string()),
			Just("JSON{\"key\":\"value\"}".to_string()),
		]
	) {
		let evaluator = create_evaluator();
		let leaked_chars = Box::leak(special_chars.clone().into_boxed_str());

		// Special characters should equal themselves exactly (after case normalization)
		prop_assert!(evaluator.compare_string(
			&special_chars,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_chars)
		).unwrap());

		// Test case insensitivity with special chars
		let special_upper = special_chars.to_uppercase();
		let leaked_upper = Box::leak(special_upper.clone().into_boxed_str());

		prop_assert!(evaluator.compare_string(
			&special_chars,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_upper)
		).unwrap());
	}

	/// Property: Wrong literal types should produce type mismatch errors
	#[test]
	fn prop_compare_string_wrong_literal_type_error(
		string_val in generate_ascii_string(),
		operator in generate_string_operator()
	) {
		let evaluator = create_evaluator();

		// Number literal should produce type error
		let result_number = evaluator.compare_string(
			&string_val,
			&operator,
			&LiteralValue::Number("123")
		);
		prop_assert!(result_number.is_err());
		prop_assert!(matches!(result_number.unwrap_err(),
			EvaluationError::TypeMismatch(_)));

		// Bool literal should produce type error
		let result_bool = evaluator.compare_string(
			&string_val,
			&operator,
			&LiteralValue::Bool(true)
		);
		prop_assert!(result_bool.is_err());
		prop_assert!(matches!(result_bool.unwrap_err(),
			EvaluationError::TypeMismatch(_)));
	}
}
