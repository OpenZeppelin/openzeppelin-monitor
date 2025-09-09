//! Property-based tests for EVM evaluator functionality (maps).
//! Tests cover JSON value matching, type detection, and comparison logic.

use openzeppelin_monitor::services::filter::ComparisonOperator;
use proptest::prelude::*;
use serde_json::json;
use serde_json::Value as JsonValue;

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

}
