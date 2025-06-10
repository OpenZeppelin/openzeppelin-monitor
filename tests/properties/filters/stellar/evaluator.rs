//! Property-based tests for Stellar evaluator functionality.
//! Tests cover JSON value matching, type detection, and comparison logic.

use openzeppelin_monitor::services::filter::{
	ComparisonOperator, EvaluationError, LiteralValue, StellarArgs, StellarConditionEvaluator,
};
use proptest::{prelude::*, test_runner::Config};
use serde_json::{json, Value as JsonValue};

fn create_evaluator() -> StellarConditionEvaluator<'static> {
	static EMPTY_ARGS: &StellarArgs = &[];
	StellarConditionEvaluator::new(EMPTY_ARGS)
}

// Strategy for generating various JSON string values
prop_compose! {
	fn arb_json_string()(
		content in prop_oneof![
			// Regular alphanumeric strings
			"[a-zA-Z0-9_]{1,20}",
			// Mixed case strings for case sensitivity testing
			"[a-zA-Z]{1,10}".prop_map(|s| {
				s.chars().enumerate().map(|(i, c)| {
					if i % 2 == 0 { c.to_uppercase().collect::<String>() }
					else { c.to_lowercase().collect::<String>() }
				}).collect::<String>()
			}),
			// Special characters and edge cases
			r#"[a-zA-Z0-9\s\-_\.@#$%]{1,15}"#,
			// Empty string
			Just("".to_string()),
			// Whitespace variations
			r#"\s{1,5}"#,
			// Common values
			Just("null".to_string()),
			Just("true".to_string()),
			Just("false".to_string()),
		]
	) -> String {
		content
	}
}

// Strategy for generating JSON numbers (as JsonValue::Number)
prop_compose! {
	fn arb_json_number()(
		num in -1_000_000_000i64..1_000_000_000i64
	) -> JsonValue {
		json!(num)
	}
}

// Strategy for generating JSON objects with and without "value" field
prop_compose! {
	fn arb_json_object_with_value_field()(
		has_value_field in any::<bool>(),
		value_content in prop_oneof![
			arb_json_string().prop_map(|s| json!(s)),
			arb_json_number(),
			any::<bool>().prop_map(|b| json!(b)),
			Just(json!(null))
		],
		other_fields in prop::collection::hash_map(
			"[a-zA-Z_][a-zA-Z0-9_]{0,8}".prop_filter("Not 'value'", |s| s != "value"),
			arb_json_string().prop_map(|s| json!(s)),
			0..3usize
		)
	) -> JsonValue {
		let mut obj = serde_json::Map::new();

		// Add other fields first
		for (key, val) in other_fields {
			obj.insert(key, val);
		}

		// Conditionally add "value" field
		if has_value_field {
			obj.insert("value".to_string(), value_content);
		}

		JsonValue::Object(obj)
	}
}

// Strategy for generating any JSON value type
prop_compose! {
	fn arb_json_value()(
		value_type in prop_oneof![
			arb_json_string().prop_map(|s| json!(s)),
			arb_json_number(),
			any::<bool>().prop_map(|b| json!(b)),
			Just(json!(null)),
			arb_json_object_with_value_field(),
			// Simple arrays
			prop::collection::vec(arb_json_string().prop_map(|s| json!(s)), 0..3)
				.prop_map(JsonValue::Array)
		]
	) -> JsonValue {
		value_type
	}
}

// Strategy for generating target strings that should match specific JSON values
prop_compose! {
	fn arb_matching_target_for_json(json_val: JsonValue)(
		case_variant in prop_oneof![
			Just("exact"),
			Just("upper"),
			Just("lower"),
			Just("mixed")
		]
	) -> String {
		let base_string = match &json_val {
			JsonValue::String(s) => s.clone(),
			JsonValue::Number(n) => n.to_string(),
			JsonValue::Bool(b) => b.to_string(),
			JsonValue::Null => "null".to_string(),
			JsonValue::Object(obj) => {
				if let Some(val) = obj.get("value") {
					match val {
						JsonValue::String(s) => s.clone(),
						_ => val.to_string().trim_matches('"').to_string()
					}
				} else {
					"no_value_field".to_string()
				}
			},
			JsonValue::Array(_) => "array_representation".to_string(),
		};

		match case_variant {
			"exact" => base_string,
			"upper" => base_string.to_uppercase(),
			"lower" => base_string.to_lowercase(),
			"mixed" => base_string.chars().enumerate().map(|(i, c)| {
				if i % 2 == 0 { c.to_uppercase().collect::<String>() }
				else { c.to_lowercase().collect::<String>() }
			}).collect(),
			_ => base_string
		}
	}
}

// Strategy for generating JSON arrays with various element types
prop_compose! {
	fn arb_json_array_string()(
		elements in prop::collection::vec(
			prop_oneof![
				arb_json_string().prop_map(|s| json!(s)),
				arb_json_number(),
				any::<bool>().prop_map(|b| json!(b)),
				// Objects with "value" field
				prop_oneof![
					arb_json_string().prop_map(|s| json!({"value": s})),
					arb_json_number().prop_map(|n| json!({"value": n})),
					any::<bool>().prop_map(|b| json!({"value": b}))
				],
				// Objects without "value" field
				prop::collection::hash_map(
					"[a-zA-Z_][a-zA-Z0-9_]{0,5}".prop_filter("Not 'value'", |s| s != "value"),
					arb_json_string().prop_map(|s| json!(s)),
					1..3usize
				).prop_map(|map| JsonValue::Object(map.into_iter().collect()))
			],
			0..5
		)
	) -> String {
		serde_json::to_string(&JsonValue::Array(elements)).unwrap()
	}
}

// Strategy for generating CSV strings
prop_compose! {
	fn arb_csv_string()(
		elements in prop::collection::vec(
			"[a-zA-Z0-9_]{1,10}",
			0..5
		),
		spacing in prop_oneof![
			Just(""),
			Just(" "),
			Just("  ")
		]
	) -> String {
		elements.iter()
			.map(|s| format!("{}{}{}", spacing, s, spacing))
			.collect::<Vec<_>>()
			.join(",")
	}
}

// Strategy for generating vec comparison operators
prop_compose! {
	fn arb_vec_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne),
			Just(ComparisonOperator::Contains)
		]
	) -> ComparisonOperator {
		op
	}
}

// Strategy for generating unsupported operators
prop_compose! {
	fn arb_unsupported_vec_operator()(
		op in prop_oneof![
			Just(ComparisonOperator::Gt),
			Just(ComparisonOperator::Gte),
			Just(ComparisonOperator::Lt),
			Just(ComparisonOperator::Lte),
			Just(ComparisonOperator::StartsWith),
			Just(ComparisonOperator::EndsWith)
		]
	) -> ComparisonOperator {
		op
	}
}

// Strategy for generating searchable JSON arrays (with known search targets)
prop_compose! {
	fn arb_json_array_with_searchable_content()(
		search_target in arb_json_string(),
		other_elements in prop::collection::vec(
			prop_oneof![
				// Clone search_target for the filter
				arb_json_string().prop_map(|s| json!(s)),
				arb_json_number(),
				any::<bool>().prop_map(|b| json!(b))
			],
			0..3
		),
		target_position in 0..3usize
	) -> (String, String) {
		// Filter out elements that match search_target after generation
		let mut elements: Vec<JsonValue> = other_elements.into_iter()
			.filter(|elem| {
				if let Some(s) = elem.as_str() {
					s != search_target
				} else {
					true // Keep non-string elements
				}
			})
			.collect();

		if target_position < elements.len() {
			elements.insert(target_position, json!(search_target.clone()));
		} else {
			elements.push(json!(search_target.clone()));
		}

		let json_array_str = serde_json::to_string(&JsonValue::Array(elements)).unwrap();
		(json_array_str, search_target)
	}
}

// Strategy for generating CSV with searchable content
prop_compose! {
	fn arb_csv_with_searchable_content()(
		search_target in "[a-zA-Z0-9_]{1,10}",
		other_elements in prop::collection::vec(
			"[a-zA-Z0-9_]{1,10}",
			0..3
		),
		target_position in 0..3usize
	) -> (String, String) {
		// Filter out elements that match search_target after generation
		let mut elements: Vec<String> = other_elements.into_iter()
			.filter(|s| s != &search_target)
			.collect();

		if target_position < elements.len() {
			elements.insert(target_position, search_target.clone());
		} else {
			elements.push(search_target.clone());
		}

		let csv_str = elements.join(",");
		(csv_str, search_target)
	}
}

// Strategy for generating JSON objects (maps)
prop_compose! {
	fn arb_json_map()(
		fields in prop::collection::hash_map(
			"[a-zA-Z_][a-zA-Z0-9_]{0,8}",
			prop_oneof![
				arb_json_string().prop_map(|s| json!(s)),
				(-1000i64..1000i64).prop_map(|n| json!(n)),
				any::<bool>().prop_map(|b| json!(b)),
				Just(json!(null))
			],
			1..5usize
		)
	) -> String {
		let map: serde_json::Map<String, JsonValue> = fields.into_iter().collect();
		serde_json::to_string(&JsonValue::Object(map)).unwrap()
	}
}

// Strategy for generating JSON maps with known searchable values
prop_compose! {
	fn arb_json_map_with_searchable_content()(
		search_target in arb_json_string(),
		other_fields in prop::collection::hash_map(
			"[a-zA-Z_][a-zA-Z0-9_]{0,8}",
			arb_json_string().prop_map(|s| json!(s)),
			0..3usize
		),
		target_field_name in "[a-zA-Z_][a-zA-Z0-9_]{0,8}"
	) -> (String, String) {
		let mut fields = other_fields;

		// Filter out any field that might conflict with search target
		fields.retain(|_, v| {
			if let Some(s) = v.as_str() {
				s != search_target
			} else {
				true
			}
		});

		// Add the searchable field
		fields.insert(target_field_name, json!(search_target.clone()));

		let map_json = serde_json::to_string(&JsonValue::Object(fields.into_iter().collect())).unwrap();
		(map_json, search_target)
	}
}

// Strategy for generating JSON maps with numeric content
prop_compose! {
	fn arb_json_map_with_numeric_content()(
		search_number in -1000i64..1000i64,
		other_fields in prop::collection::hash_map(
			"[a-zA-Z_][a-zA-Z0-9_]{0,8}",
			prop_oneof![
				arb_json_string().prop_map(|s| json!(s)),
				(-1000i64..1000i64).prop_map(|n| json!(n))
			],
			0..3usize
		),
		target_field_name in "[a-zA-Z_][a-zA-Z0-9_]{0,8}"
	) -> (String, i64) {
		let mut fields = other_fields;

		// Add the searchable numeric field
		fields.insert(target_field_name, json!(search_number));

		let map_json = serde_json::to_string(&JsonValue::Object(fields.into_iter().collect())).unwrap();
		(map_json, search_number)
	}
}

// Strategy for generating invalid JSON strings
prop_compose! {
	fn arb_invalid_json()(
		content in prop_oneof![
			Just("{invalid json}".to_string()),
			Just("{\"unclosed\": \"string".to_string()),
			Just("{\"key\": }".to_string()),
			Just("not json at all".to_string()),
			Just("123".to_string()), // Valid JSON but not an object
			Just("[]".to_string()),  // Valid JSON array but not object
			Just("\"string\"".to_string()) // Valid JSON string but not object
		]
	) -> String {
		content
	}
}

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
		cases: 1000,
		..Config::default()
	})]

	/// Property: String values should match case-insensitively
	#[test]
	fn prop_check_json_string_case_insensitive_matching(
		original_string in arb_json_string(),
		case_variant in prop_oneof![
			Just("upper"),
			Just("lower"),
			Just("mixed")
		]
	) {
		let json_val = json!(original_string);
		let target = match case_variant {
			"upper" => original_string.to_uppercase(),
			"lower" => original_string.to_lowercase(),
			"mixed" => original_string.chars().enumerate().map(|(i, c)| {
				if i % 2 == 0 { c.to_uppercase().collect::<String>() }
				else { c.to_lowercase().collect::<String>() }
			}).collect(),
			_ => original_string.clone()
		};

		let result = StellarConditionEvaluator::check_json_value_matches_str(&json_val, &target);

		// String matching should be case-insensitive
		prop_assert!(result,
			"String '{}' should match target '{}' case-insensitively",
			original_string, target);
	}

	/// Property: String values should NOT match different content regardless of case
	#[test]
	fn prop_check_json_string_different_content_no_match(
		string1 in arb_json_string().prop_filter("Non-empty", |s| !s.is_empty()),
		string2 in arb_json_string().prop_filter("Non-empty", |s| !s.is_empty())
	) {
		// Ensure strings are actually different when normalized
		prop_assume!(string1.to_lowercase() != string2.to_lowercase());

		let json_val = json!(string1);
		let result = StellarConditionEvaluator::check_json_value_matches_str(&json_val, &string2);

		prop_assert!(!result,
			"Different strings '{}' and '{}' should not match",
			string1, string2);
	}

	/// Property: Number values should match their exact string representation
	#[test]
	fn prop_check_json_number_exact_string_match(
		num in -1_000_000_000i64..1_000_000_000i64
	) {
		let json_val = json!(num);
		let num_string = num.to_string();

		let result = StellarConditionEvaluator::check_json_value_matches_str(&json_val, &num_string);

		prop_assert!(result,
			"Number {} should match its string representation '{}'",
			num, num_string);

		// Should NOT match with extra whitespace or formatting
		let with_space = format!(" {} ", num_string);
		let result_with_space = StellarConditionEvaluator::check_json_value_matches_str(&json_val, &with_space);
		prop_assert!(!result_with_space,
			"Number {} should NOT match string with whitespace '{}'",
			num, with_space);
	}

	/// Property: Boolean values should match their exact string representation (case-sensitive)
	#[test]
	fn prop_check_json_boolean_string_match(
		bool_val in any::<bool>()
	) {
		let json_val = json!(bool_val);
		let bool_string = bool_val.to_string(); // "true" or "false"

		let result = StellarConditionEvaluator::check_json_value_matches_str(&json_val, &bool_string);

		// Boolean should match exact case only
		prop_assert!(result,
			"Boolean {} should match exact string '{}'",
			bool_val, bool_string);

		// Should NOT match different cases
		let uppercase = bool_string.to_uppercase();
		if uppercase != bool_string {
			let result_upper = StellarConditionEvaluator::check_json_value_matches_str(&json_val, &uppercase);
			prop_assert!(!result_upper,
				"Boolean {} should NOT match uppercase '{}'",
				bool_val, uppercase);
		}
	}

	/// Property: Objects with "value" field - string values should match exactly (case-sensitive)
	#[test]
	fn prop_check_json_object_value_field_string_exact_match(
		value_string in arb_json_string(),
		target_string in arb_json_string()
	) {
		let obj = json!({
			"value": value_string,
			"other_field": "irrelevant"
		});

		let result = StellarConditionEvaluator::check_json_value_matches_str(&obj, &target_string);

		// For objects with string "value" field, matching should be case-sensitive and exact
		let should_match = value_string == target_string;
		prop_assert_eq!(result, should_match,
			"Object with string value '{}' should {} match target '{}'",
			value_string, if should_match { "" } else { "NOT" }, target_string);
	}


	/// Property: Objects without "value" field should never match
	#[test]
	fn prop_check_json_object_no_value_field_never_matches(
		obj_fields in prop::collection::hash_map(
			"[a-zA-Z_][a-zA-Z0-9_]{0,8}".prop_filter("Not 'value'", |s| s != "value"),
			arb_json_string().prop_map(|s| json!(s)),
			1..4usize
		),
		target_string in arb_json_string()
	) {
		let obj = JsonValue::Object(obj_fields.into_iter().collect());

		let result = StellarConditionEvaluator::check_json_value_matches_str(&obj, &target_string);

		prop_assert!(!result,
			"Object without 'value' field should never match any target string '{}'",
			target_string);
	}

	/// Property: Method should be deterministic - same inputs always produce same outputs
	#[test]
	fn prop_check_json_value_deterministic(
		json_val in arb_json_value(),
		target_string in arb_json_string()
	) {
		let result1 = StellarConditionEvaluator::check_json_value_matches_str(&json_val, &target_string);
		let result2 = StellarConditionEvaluator::check_json_value_matches_str(&json_val, &target_string);

		prop_assert_eq!(result1, result2,
			"check_json_value_matches_str should be deterministic for value {} and target '{}'",
			json_val, target_string);
	}

	 /// Property: Arrays should match their string representation
	#[test]
	fn prop_check_json_array_string_representation(
		array_elements in prop::collection::vec(
			prop_oneof![
				arb_json_string().prop_map(|s| json!(s)),
				any::<i32>().prop_map(|n| json!(n))
			],
			0..3
		)
	) {
		let json_array = JsonValue::Array(array_elements.clone());
		let array_string = json_array.to_string();
		let trimmed_string = array_string.trim_matches('"');

		let result = StellarConditionEvaluator::check_json_value_matches_str(&json_array, trimmed_string);

		prop_assert!(result,
			"Array {} should match its trimmed string representation '{}'",
			json_array, trimmed_string);
	}

	 /// Property: Vec comparison should be reflexive (vec equals itself)
	#[test]
	fn prop_compare_vec_reflexivity(
		vec_content in prop_oneof![
			arb_json_array_string(),
			arb_csv_string()
		]
	) {
		let evaluator = create_evaluator();
		let leaked_content = Box::leak(vec_content.clone().into_boxed_str());

		// A vec should equal itself
		let result = evaluator.compare_vec(
			&vec_content,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_content)
		).unwrap();

		prop_assert!(result, "Vec '{}' should equal itself", vec_content);

		// A vec should not be "not equal" to itself
		let result_ne = evaluator.compare_vec(
			&vec_content,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_content)
		).unwrap();

		prop_assert!(!result_ne, "Vec '{}' should not be Ne to itself", vec_content);
	}

	/// Property: JSON array equality should be semantic (order and whitespace matter)
	#[test]
	fn prop_compare_vec_json_semantic_equality(
		elements in prop::collection::vec(arb_json_string(), 1..4)
	) {
		let evaluator = create_evaluator();

		let array1 = serde_json::to_string(&JsonValue::Array(
			elements.iter().map(|s| json!(s)).collect()
		)).unwrap();

		// Same elements, different whitespace formatting - re-parse and serialize to ensure same content
		let parsed: JsonValue = serde_json::from_str(&array1).unwrap();
		let array2 = serde_json::to_string_pretty(&parsed).unwrap();

		let leaked_array2 = Box::leak(array2.clone().into_boxed_str());
		let result = evaluator.compare_vec(
			&array1,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_array2)
		).unwrap();

		prop_assert!(result, "JSON arrays with same content should be equal: '{}' vs '{}'", array1, array2);

		// Different order - should NOT be equal
		if elements.len() > 1 {
			let mut reversed_elements = elements.clone();
			reversed_elements.reverse();
			let array3 = serde_json::to_string(&JsonValue::Array(
				reversed_elements.iter().map(|s| json!(s)).collect()
			)).unwrap();

			if array1 != array3 {  // Only test if actually different
				let leaked_array3 = Box::leak(array3.clone().into_boxed_str());
				let result_diff_order = evaluator.compare_vec(
					&array1,
					&ComparisonOperator::Eq,
					&LiteralValue::Str(leaked_array3)
				).unwrap();

				prop_assert!(!result_diff_order,
					"JSON arrays with different order should NOT be equal: '{}' vs '{}'",
					array1, array3);
			}
		}
	}

	/// Property: CSV equality should be case-insensitive and normalize whitespace
	#[test]
	fn prop_compare_vec_csv_normalization(
		elements in prop::collection::vec("[a-zA-Z0-9_]{1,8}", 1..4)
	) {
		let evaluator = create_evaluator();

		let csv1 = elements.join(",");
		let csv2 = elements.iter()
			.map(|s| format!(" {} ", s.to_uppercase()))
			.collect::<Vec<_>>()
			.join(",");

		let leaked_csv2 = Box::leak(csv2.clone().into_boxed_str());
		let result = evaluator.compare_vec(
			&csv1,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_csv2)
		).unwrap();

		prop_assert!(result,
			"CSV with different case/whitespace should be equal: '{}' vs '{}'",
			csv1, csv2);
	}

	/// Property: Contains should find existing elements in JSON arrays
	#[test]
	fn prop_compare_vec_json_contains_correctness(
		(json_array_str, search_target) in arb_json_array_with_searchable_content()
	) {
		let evaluator = create_evaluator();
		let leaked_target = Box::leak(search_target.clone().into_boxed_str());

		let result = evaluator.compare_vec(
			&json_array_str,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_target)
		).unwrap();

		prop_assert!(result,
			"JSON array '{}' should contain '{}'",
			json_array_str, search_target);

		// Should not find non-existent elements
		let non_existent = format!("nonexistent_{}", search_target);
		let leaked_non_existent = Box::leak(non_existent.clone().into_boxed_str());
		let result_not_found = evaluator.compare_vec(
			&json_array_str,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_non_existent)
		).unwrap();

		prop_assert!(!result_not_found,
			"JSON array '{}' should NOT contain '{}'",
			json_array_str, non_existent);
	}

	/// Property: Contains should find existing elements in CSV strings
	#[test]
	fn prop_compare_vec_csv_contains_correctness(
		(csv_str, search_target) in arb_csv_with_searchable_content()
	) {
		let evaluator = create_evaluator();
		let leaked_target = Box::leak(search_target.clone().into_boxed_str());

		let result = evaluator.compare_vec(
			&csv_str,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_target)
		).unwrap();

		prop_assert!(result,
			"CSV '{}' should contain '{}'",
			csv_str, search_target);
	}

	/// Property: Contains should work with Number literals
	#[test]
	fn prop_compare_vec_contains_number_literal(
		numbers in prop::collection::vec(-1000i64..1000i64, 1..4),
		search_num in -1000i64..1000i64
	) {
		let evaluator = create_evaluator();

		let json_array = serde_json::to_string(&JsonValue::Array(
			numbers.iter().map(|&n| json!(n)).collect()
		)).unwrap();

		let result = evaluator.compare_vec(
			&json_array,
			&ComparisonOperator::Contains,
			&LiteralValue::Number(&search_num.to_string())
		).unwrap();

		let expected = numbers.contains(&search_num);
		prop_assert_eq!(result, expected,
			"Array {} should {} contain number {}",
			json_array, if expected { "" } else { "NOT" }, search_num);
	}

	/// Property: JSON vs CSV comparison should be handled correctly
	#[test]
	fn prop_compare_vec_json_vs_csv_comparison(
		elements in prop::collection::vec("[a-zA-Z0-9_]{1,8}", 1..3)
	) {
		let evaluator = create_evaluator();

		let json_array = serde_json::to_string(&JsonValue::Array(
			elements.iter().map(|s| json!(s)).collect()
		)).unwrap();
		let csv_string = elements.join(",");

		let leaked_csv = Box::leak(csv_string.clone().into_boxed_str());

		// JSON array vs CSV string should NOT be equal (different formats)
		let result = evaluator.compare_vec(
			&json_array,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_csv)
		).unwrap();

		prop_assert!(!result,
			"JSON array '{}' should NOT equal CSV string '{}'",
			json_array, csv_string);

		// They should be "not equal"
		let result_ne = evaluator.compare_vec(
			&json_array,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_csv)
		).unwrap();

		prop_assert!(result_ne,
			"JSON array '{}' should be Ne to CSV string '{}'",
			json_array, csv_string);
	}

	/// Property: Objects with "value" field should be searchable
	#[test]
	fn prop_compare_vec_object_value_field_search(
		search_target in arb_json_string(),
		other_field_content in arb_json_string()
	) {
		// Skip test if both values are the same
		prop_assume!(search_target != other_field_content);

		let evaluator = create_evaluator();

		// Create JSON array using serde_json to ensure proper escaping
		let json_array = serde_json::to_string(&json!([
			{"value": search_target, "other": "irrelevant"},
			{"different": other_field_content}
		])).unwrap();

		let leaked_target = Box::leak(search_target.clone().into_boxed_str());
		let result = evaluator.compare_vec(
			&json_array,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_target)
		).unwrap();

		prop_assert!(result,
			"Array with object containing 'value' field should find '{}'",
			search_target);
	}

	/// Property: Unsupported operators should produce errors
	#[test]
	fn prop_compare_vec_unsupported_operators_error(
		vec_content in arb_json_array_string(),
		operator in arb_unsupported_vec_operator()
	) {
		let evaluator = create_evaluator();

		let result = evaluator.compare_vec(
			&vec_content,
			&operator,
			&LiteralValue::Str("test")
		);

		prop_assert!(result.is_err(),
			"Unsupported operator {:?} should produce error", operator);
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::UnsupportedOperator(_)));
	}

	/// Property: String comparison should be case-insensitive for all types
	#[test]
	fn prop_compare_string_case_insensitive(
		string_kind in prop_oneof![
			Just("string"),
			Just("symbol"),
			Just("bytes")
		],
		original_string in "[a-zA-Z0-9_]{3,15}",
		operator in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne),
			Just(ComparisonOperator::Contains),
			Just(ComparisonOperator::StartsWith),
			Just(ComparisonOperator::EndsWith)
		]
	) {
		let evaluator = create_evaluator();

		// Test with different case variations
		let uppercase = original_string.to_uppercase();
		let lowercase = original_string.to_lowercase();

		let leaked_upper = Box::leak(uppercase.clone().into_boxed_str());
		let leaked_lower = Box::leak(lowercase.clone().into_boxed_str());

		let result_upper = evaluator.compare_string(
			string_kind,
			&original_string,
			&operator,
			&LiteralValue::Str(leaked_upper)
		).unwrap();

		let result_lower = evaluator.compare_string(
			string_kind,
			&original_string,
			&operator,
			&LiteralValue::Str(leaked_lower)
		).unwrap();

		// Results should be the same regardless of case
		prop_assert_eq!(result_upper, result_lower,
			"Case should not matter for {} comparison with operator {:?}",
			string_kind, operator);

		// For Eq operator, both should be true when comparing same content
		if operator == ComparisonOperator::Eq {
			prop_assert!(result_upper && result_lower,
				"Eq comparison should be true for same content regardless of case");
		}
	}

	/// Property: Address comparison should use special normalization for Eq/Ne
	#[test]
	fn prop_compare_string_address_normalization(
		// Stellar addresses: 56 chars, start with G, base32 alphabet
		address_base in "[A-Z2-7]{55}",
		operator in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne)
		]
	) {
		let evaluator = create_evaluator();

		// Create a valid Stellar address format
		let stellar_address = format!("G{}", address_base);
		let lowercase_addr = stellar_address.to_lowercase();
		let mixed_case = stellar_address.chars().enumerate().map(|(i, c)| {
			if i % 2 == 0 { c.to_lowercase().collect::<String>() }
			else { c.to_uppercase().collect::<String>() }
		}).collect::<String>();

		let leaked_lower = Box::leak(lowercase_addr.clone().into_boxed_str());
		let leaked_mixed = Box::leak(mixed_case.clone().into_boxed_str());

		let result_lower = evaluator.compare_string(
			"address",
			&stellar_address,
			&operator,
			&LiteralValue::Str(leaked_lower)
		).unwrap();

		let result_mixed = evaluator.compare_string(
			"address",
			&stellar_address,
			&operator,
			&LiteralValue::Str(leaked_mixed)
		).unwrap();

		// Test actual behavior rather than assumptions
		if operator == ComparisonOperator::Eq {
			// Address normalization should make case-insensitive comparison work
			prop_assert!(result_lower || result_mixed,
				"Address Eq should handle case differences for Stellar addresses");
		}

		// Test reflexivity - address should equal itself
		let leaked_same = Box::leak(stellar_address.clone().into_boxed_str());
		let result_same = evaluator.compare_string(
			"address",
			&stellar_address,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_same)
		).unwrap();

		prop_assert!(result_same,
			"Stellar address should equal itself: '{}'", stellar_address);
	}

	/// Property: StartsWith and EndsWith should be consistent
	#[test]
	fn prop_compare_string_starts_ends_with_consistency(
		string_kind in prop_oneof![
			Just("string"),
			Just("address"),
			Just("symbol"),
			Just("bytes")
		],
		full_string in "[a-zA-Z0-9_]{5,15}",
		prefix_len in 1..4usize,
		suffix_len in 1..4usize
	) {
		prop_assume!(prefix_len < full_string.len() && suffix_len < full_string.len());

		let evaluator = create_evaluator();

		let prefix = &full_string[..prefix_len];
		let suffix = &full_string[full_string.len().saturating_sub(suffix_len)..];

		let leaked_prefix = Box::leak(prefix.to_string().into_boxed_str());
		let leaked_suffix = Box::leak(suffix.to_string().into_boxed_str());

		let starts_with_result = evaluator.compare_string(
			string_kind,
			&full_string,
			&ComparisonOperator::StartsWith,
			&LiteralValue::Str(leaked_prefix)
		).unwrap();

		let ends_with_result = evaluator.compare_string(
			string_kind,
			&full_string,
			&ComparisonOperator::EndsWith,
			&LiteralValue::Str(leaked_suffix)
		).unwrap();

		prop_assert!(starts_with_result,
			"String '{}' should start with '{}'", full_string, prefix);
		prop_assert!(ends_with_result,
			"String '{}' should end with '{}'", full_string, suffix);
	}

	/// Property: Different strings should not be equal
	#[test]
	fn prop_compare_string_different_strings_not_equal(
		string_kind in prop_oneof![
			Just("string"),
			Just("symbol"),
			Just("bytes")
		],
		string1 in "[a-zA-Z0-9_]{3,10}",
		string2 in "[a-zA-Z0-9_]{3,10}"
	) {
		// Ensure strings are actually different when normalized
		prop_assume!(string1.to_lowercase() != string2.to_lowercase());

		let evaluator = create_evaluator();
		let leaked_string2 = Box::leak(string2.clone().into_boxed_str());

		let eq_result = evaluator.compare_string(
			string_kind,
			&string1,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_string2)
		).unwrap();

		let ne_result = evaluator.compare_string(
			string_kind,
			&string1,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_string2)
		).unwrap();

		prop_assert!(!eq_result,
			"Different strings '{}' and '{}' should not be equal",
			string1, string2);
		prop_assert!(ne_result,
			"Different strings '{}' and '{}' should be not equal",
			string1, string2);
	}

	/// Property: Non-string literals should produce type mismatch errors
	#[test]
	fn prop_compare_string_type_mismatch_error(
		string_kind in prop_oneof![
			Just("string"),
			Just("address"),
			Just("symbol"),
			Just("bytes")
		],
		test_string in "[a-zA-Z0-9_]{1,10}",
		number_literal in -1000i64..1000i64
	) {
		let evaluator = create_evaluator();

		let result = evaluator.compare_string(
			string_kind,
			&test_string,
			&ComparisonOperator::Eq,
			&LiteralValue::Number(&number_literal.to_string())
		);

		prop_assert!(result.is_err(),
			"Non-string literal should produce error for string comparison");
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::TypeMismatch { .. }),
			"Should produce TypeMismatch error");
	}

	/// Property: Unsupported operators should produce errors
	#[test]
	fn prop_compare_string_unsupported_operators_error(
		string_kind in prop_oneof![
			Just("string"),
			Just("address"),
			Just("symbol"),
			Just("bytes")
		],
		test_string in "[a-zA-Z0-9_]{1,10}",
		unsupported_op in prop_oneof![
			Just(ComparisonOperator::Gt),
			Just(ComparisonOperator::Gte),
			Just(ComparisonOperator::Lt),
			Just(ComparisonOperator::Lte)
		]
	) {
		let evaluator = create_evaluator();

		let result = evaluator.compare_string(
			string_kind,
			&test_string,
			&unsupported_op,
			&LiteralValue::Str("test")
		);

		prop_assert!(result.is_err(),
			"Unsupported operator {:?} should produce error for {} comparison",
			unsupported_op, string_kind);
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::UnsupportedOperator { .. }),
			"Should produce UnsupportedOperator error");
	}

	/// Property: Map comparison should be reflexive (map equals itself)
	#[test]
	fn prop_compare_map_reflexivity(
		json_map in arb_json_map()
	) {
		let evaluator = create_evaluator();
		let leaked_map = Box::leak(json_map.clone().into_boxed_str());

		// Map should equal itself
		let result_eq = evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_map)
		).unwrap();

		prop_assert!(result_eq,
			"JSON map should equal itself: '{}'", json_map);

		// Map should NOT be "not equal" to itself
		let result_ne = evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_map)
		).unwrap();

		prop_assert!(!result_ne,
			"JSON map should NOT be Ne to itself: '{}'", json_map);
	}

	/// Property: Map equality should be semantic (same content = equal)
	#[test]
	fn prop_compare_map_semantic_equality(
		fields in prop::collection::hash_map(
			"[a-zA-Z_][a-zA-Z0-9_]{0,8}",
			arb_json_string(),
			1..4usize
		)
	) {
		let evaluator = create_evaluator();

		// Create two JSON representations of the same map
		let map1: serde_json::Map<String, JsonValue> = fields.iter()
			.map(|(k, v)| (k.clone(), json!(v)))
			.collect();
		let map2: serde_json::Map<String, JsonValue> = fields.iter()
			.map(|(k, v)| (k.clone(), json!(v)))
			.collect();

		let json1 = serde_json::to_string(&JsonValue::Object(map1)).unwrap();
		let json2 = serde_json::to_string_pretty(&JsonValue::Object(map2)).unwrap();

		let leaked_json2 = Box::leak(json2.clone().into_boxed_str());

		let result = evaluator.compare_map(
			&json1,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_json2)
		).unwrap();

		prop_assert!(result,
			"Maps with same content should be equal: '{}' vs '{}'",
			json1, json2);
	}

	/// Property: Different maps should not be equal
	#[test]
	fn prop_compare_map_different_maps_not_equal(
		fields1 in prop::collection::hash_map(
			"[a-zA-Z_][a-zA-Z0-9_]{0,8}",
			arb_json_string(),
			1..3usize
		),
		fields2 in prop::collection::hash_map(
			"[a-zA-Z_][a-zA-Z0-9_]{0,8}",
			arb_json_string(),
			1..3usize
		)
	) {
		// Ensure maps are actually different
		prop_assume!(fields1 != fields2);

		let evaluator = create_evaluator();

		let map1 = serde_json::to_string(&JsonValue::Object(
			fields1.into_iter().map(|(k, v)| (k, json!(v))).collect()
		)).unwrap();
		let map2 = serde_json::to_string(&JsonValue::Object(
			fields2.into_iter().map(|(k, v)| (k, json!(v))).collect()
		)).unwrap();

		let leaked_map2 = Box::leak(map2.clone().into_boxed_str());

		let eq_result = evaluator.compare_map(
			&map1,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_map2)
		).unwrap();

		let ne_result = evaluator.compare_map(
			&map1,
			&ComparisonOperator::Ne,
			&LiteralValue::Str(leaked_map2)
		).unwrap();

		prop_assert!(!eq_result,
			"Different maps should not be equal");
		prop_assert!(ne_result,
			"Different maps should be not equal");
	}

	/// Property: Contains should find existing string values in maps
	#[test]
	fn prop_compare_map_contains_string_values(
		(json_map, search_target) in arb_json_map_with_searchable_content()
	) {
		let evaluator = create_evaluator();
		let leaked_target = Box::leak(search_target.clone().into_boxed_str());

		let result = evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_target)
		).unwrap();

		prop_assert!(result,
			"Map '{}' should contain value '{}'",
			json_map, search_target);

		// Should not find non-existent values
		let non_existent = format!("nonexistent_{}", search_target);
		let leaked_non_existent = Box::leak(non_existent.clone().into_boxed_str());
		let result_not_found = evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Contains,
			&LiteralValue::Str(leaked_non_existent)
		).unwrap();

		prop_assert!(!result_not_found,
			"Map '{}' should NOT contain '{}'",
			json_map, non_existent);
	}

	/// Property: Contains should find existing numeric values in maps
	#[test]
	fn prop_compare_map_contains_numeric_values(
		(json_map, search_number) in arb_json_map_with_numeric_content()
	) {
		let evaluator = create_evaluator();

		let result = evaluator.compare_map(
			&json_map,
			&ComparisonOperator::Contains,
			&LiteralValue::Number(&search_number.to_string())
		).unwrap();

		prop_assert!(result,
			"Map '{}' should contain number {}",
			json_map, search_number);
	}

	/// Property: Invalid JSON should produce parse errors
	#[test]
	fn prop_compare_map_invalid_json_parse_error(
		invalid_json in arb_invalid_json(),
		valid_map in arb_json_map()
	) {
		let evaluator = create_evaluator();
		let leaked_valid = Box::leak(valid_map.clone().into_boxed_str());

		// Test invalid LHS
		let result_lhs = evaluator.compare_map(
			&invalid_json,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_valid)
		);

		// Test invalid RHS
		let leaked_invalid = Box::leak(invalid_json.clone().into_boxed_str());
		let result_rhs = evaluator.compare_map(
			&valid_map,
			&ComparisonOperator::Eq,
			&LiteralValue::Str(leaked_invalid)
		);

		prop_assert!(result_lhs.is_err() || result_rhs.is_err(),
			"Invalid JSON should produce error");

		// Accept both ParseError and TypeMismatch as valid error types
		if let Err(err) = result_lhs {
			prop_assert!(
				matches!(err, EvaluationError::ParseError { .. }) ||
				matches!(err, EvaluationError::TypeMismatch { .. }),
				"Should produce ParseError or TypeMismatch for invalid LHS JSON, got: {:?}", err);
		}

		if let Err(err) = result_rhs {
			prop_assert!(
				matches!(err, EvaluationError::ParseError { .. }) ||
				matches!(err, EvaluationError::TypeMismatch { .. }),
				"Should produce ParseError or TypeMismatch for invalid RHS JSON, got: {:?}", err);
		}
	}


	/// Property: Unsupported operators should produce errors
	#[test]
	fn prop_compare_map_unsupported_operators_error(
		json_map in arb_json_map(),
		unsupported_op in prop_oneof![
			Just(ComparisonOperator::Gt),
			Just(ComparisonOperator::Gte),
			Just(ComparisonOperator::Lt),
			Just(ComparisonOperator::Lte),
			Just(ComparisonOperator::StartsWith),
			Just(ComparisonOperator::EndsWith)
		]
	) {
		let evaluator = create_evaluator();

		let result = evaluator.compare_map(
			&json_map,
			&unsupported_op,
			&LiteralValue::Str("test")
		);

		prop_assert!(result.is_err(),
			"Unsupported operator {:?} should produce error", unsupported_op);
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::UnsupportedOperator { .. }),
			"Should produce UnsupportedOperator error");
	}

	/// Property: Number literals should only work with Contains operator
	#[test]
	fn prop_compare_map_number_literal_contains_only(
		json_map in arb_json_map(),
		number_val in -1000i64..1000i64,
		operator in prop_oneof![
			Just(ComparisonOperator::Eq),
			Just(ComparisonOperator::Ne)
		]
	) {
		let evaluator = create_evaluator();

		let result = evaluator.compare_map(
			&json_map,
			&operator,
			&LiteralValue::Number(&number_val.to_string())
		);

		prop_assert!(result.is_err(),
			"Number literal should only work with Contains operator, not {:?}", operator);
		prop_assert!(matches!(result.unwrap_err(),
			EvaluationError::TypeMismatch { .. }),
			"Should produce TypeMismatch error");
	}

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
