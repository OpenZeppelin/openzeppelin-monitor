//! Property-based tests for EVM evaluator functionality (maps).
//! Tests cover JSON value matching, type detection, and comparison logic.

use openzeppelin_monitor::services::filter::{ComparisonOperator, EvaluationError, LiteralValue};
use proptest::{prelude::*, test_runner::Config};
use serde_json::json;
use serde_json::Value as JsonValue;

use crate::properties::filters::evm::strings_evaluator::create_evaluator;

prop_compose! {
	fn generate_simple_json_object()(
		keys in prop::collection::vec("[a-zA-Z][a-zA-Z0-9_]{0,10}", 1..5),
		values in prop::collection::vec(
			prop_oneof![
				"[a-zA-Z0-9_]{1,15}".prop_map(|s| json!(s)),
				any::<i32>().prop_map(|n| json!(n)),
				any::<bool>().prop_map(|b| json!(b)),
				Just(json!(null))
			], 1..5
		)
	) -> String {
		let mut obj = serde_json::Map::new();
		for (key, value) in keys.into_iter().zip(values.into_iter()) {
			obj.insert(key, value);
		}
		serde_json::to_string(&JsonValue::Object(obj)).unwrap()
	}
}

prop_compose! {
	fn generate_nested_json_object()(
		depth in 1..3usize
	) -> String {
		fn create_nested_object(depth: usize) -> JsonValue {
			if depth == 0 {
				json!({
					"leaf_key": "leaf_value",
					"number": 42,
					"boolean": true
				})
			} else {
				json!({
					"level": depth,
					"nested": create_nested_object(depth - 1),
					"data": format!("level_{}_data", depth),
					"count": depth * 10
				})
			}
		}
		serde_json::to_string(&create_nested_object(depth)).unwrap()
	}
}

prop_compose! {
	fn generate_json_object_with_searchable_values()(
		search_target in "[a-zA-Z0-9_]{3,10}",
		other_values in prop::collection::vec("[a-zA-Z0-9_]{1,15}", 2..5)
	) -> (String, String) {
		let mut obj = serde_json::Map::new();
		obj.insert("target_key".to_string(), json!(search_target.clone()));
		obj.insert("number_key".to_string(), json!(123));
		obj.insert("bool_key".to_string(), json!(false));

		for (i, value) in other_values.into_iter().enumerate() {
			obj.insert(format!("key_{}", i), json!(value));
		}

		let json_str = serde_json::to_string(&JsonValue::Object(obj)).unwrap();
		(json_str, search_target)
	}
}

prop_compose! {
	fn generate_invalid_json_string()(
		variant in prop_oneof![
			Just("".to_string()),
			Just("{".to_string()),
			Just("}".to_string()),
			Just("{invalid}".to_string()),
			Just("{\"key\":}".to_string()),
			Just("{\"key\": value}".to_string()),
			Just("{key: \"value\"}".to_string()),
			Just("not json at all".to_string()),
			Just("123".to_string()),
			Just("\"string\"".to_string()),
			Just("true".to_string()),
			Just("[]".to_string()),  // Array, not object
			Just("[{\"key\": \"value\"}]".to_string()),  // Array, not object
		]
	) -> String {
		variant
	}
}

prop_compose! {
	fn generate_map_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne),
			Just(ComparisonOperator::Contains),
		]
	) -> ComparisonOperator {
		op
	}
}

prop_compose! {
	fn generate_unsupported_map_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Gt),
			Just(ComparisonOperator::Gte),
			Just(ComparisonOperator::Lt),
			Just(ComparisonOperator::Lte),
			Just(ComparisonOperator::StartsWith),
			Just(ComparisonOperator::EndsWith),
		]
	) -> ComparisonOperator {
		op
	}
}

prop_compose! {
	fn generate_equivalent_json_objects()(
		keys in prop::collection::vec("[a-zA-Z][a-zA-Z0-9_]{0,8}", 2..4),
		values in prop::collection::vec(any::<i32>(), 2..4)
	) -> (String, String) {
		let mut obj1 = serde_json::Map::new();
		let mut obj2 = serde_json::Map::new();

		// Same content, different order and formatting
		for (key, value) in keys.iter().zip(values.iter()) {
			obj1.insert(key.clone(), json!(value));
			obj2.insert(key.clone(), json!(value));
		}

		// Serialize with different formatting
		let json1 = serde_json::to_string(&JsonValue::Object(obj1)).unwrap();
		let json2 = serde_json::to_string_pretty(&JsonValue::Object(obj2)).unwrap();

		(json1, json2)
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	/// Property: Map comparison should be reflexive (map equals itself)
	#[test]
	fn prop_compare_map_reflexivity(
		json_map in generate_simple_json_object()
	) {
		let evaluator = create_evaluator();
		let leaked_map = Box::leak(json_map.clone().into_boxed_str());

		// A map should equal itself
		prop_assert!(evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_map)
		).unwrap());

		// A map should not be "not equal" to itself
		prop_assert!(!evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_map)
		).unwrap());
	}

	/// Property: Map equality should be symmetric
	#[test]
	fn prop_compare_map_symmetry(
		map1 in generate_simple_json_object(),
		map2 in generate_simple_json_object()
	) {
		let evaluator = create_evaluator();
		let leaked_map1 = Box::leak(map1.clone().into_boxed_str());
		let leaked_map2 = Box::leak(map2.clone().into_boxed_str());

		let map1_eq_map2 = evaluator.compare_map(
			&map1,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_map2)
		).unwrap();

		let map2_eq_map1 = evaluator.compare_map(
			&map2,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_map1)
		).unwrap();

		// Equality should be symmetric: map1 == map2 iff map2 == map1
		prop_assert_eq!(map1_eq_map2, map2_eq_map1);
	}

	/// Property: Contains should find values that exist
	#[test]
	fn prop_compare_map_contains_correctness(
		(json_map, search_target) in generate_json_object_with_searchable_values()
	) {
		let evaluator = create_evaluator();
		let leaked_target = Box::leak(search_target.clone().into_boxed_str());

		// Should find the target value
		prop_assert!(evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_target)
		).unwrap());

		// Should also work with number literals for Contains
		prop_assert!(evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Contains,
			&LiteralValue::Number("123")
		).unwrap());

		// Should not find non-existent values
		let non_existent = "definitely_not_in_map_12345";
		let leaked_non_existent = Box::leak(non_existent.to_string().into_boxed_str());
		prop_assert!(!evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_non_existent)
		).unwrap());
	}

	/// Property: Unsupported operators should produce errors
	#[test]
	fn prop_compare_map_unsupported_operators_error(
		map1 in generate_simple_json_object(),
		map2 in generate_simple_json_object(),
		operator in generate_unsupported_map_operator()
	) {
		let evaluator = create_evaluator();
		let leaked_map2 = Box::leak(map2.clone().into_boxed_str());

		let result = evaluator.compare_map(
			&map1,
			&operator,
			&LiteralValue::Str(leaked_map2)
		);

		// Unsupported operators should produce error
		prop_assert!(result.is_err());
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::UnsupportedOperator(_)));
	}

	/// Property: Wrong literal types should produce type mismatch errors
	#[test]
	fn prop_compare_map_wrong_literal_type_error(
		json_map in generate_simple_json_object()
	) {
		let evaluator = create_evaluator();

		// Bool literal should produce type error (except for Contains with Number)
		let result_bool = evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Eq,
			&LiteralValue::Bool(true)
		);
		prop_assert!(result_bool.is_err());
		prop_assert!(matches!(result_bool.unwrap_err(),
			EvaluationError::TypeMismatch(_)));

		// Number literal for Eq should produce type error
		let result_number_eq = evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Eq,
			&LiteralValue::Number("123")
		);
		prop_assert!(result_number_eq.is_err());
		prop_assert!(matches!(result_number_eq.unwrap_err(),
			EvaluationError::TypeMismatch(_)));

		// But Number literal for Contains should work
		let result_number_contains = evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Contains,
			&LiteralValue::Number("123")
		);
		prop_assert!(result_number_contains.is_ok());
	}

	/// Property: Nested objects should be handled correctly
	#[test]
	fn prop_compare_map_nested_objects(
		nested_map in generate_nested_json_object()
	) {
		let evaluator = create_evaluator();
		let leaked_map = Box::leak(nested_map.clone().into_boxed_str());

		// Nested object should equal itself
		prop_assert!(evaluator.compare_map(
			&nested_map,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_map)
		).unwrap());

		// Should be able to find values in nested structure with Contains
		prop_assert!(evaluator.compare_map(
			&nested_map,
			&ComparisonOperator::Contains,
			&LiteralValue::Str("leaf_value")
		).unwrap());

		prop_assert!(evaluator.compare_map(
			&nested_map,
			&ComparisonOperator::Contains,
			&LiteralValue::Number("42")
		).unwrap());
	}
}
