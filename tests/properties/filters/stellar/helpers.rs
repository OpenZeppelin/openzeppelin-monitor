//! Property-based tests for Stellar transaction matching and filtering helpers.

use openzeppelin_monitor::services::filter::stellar_helpers::compare_json_values;
use proptest::prelude::*;

proptest! {
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
}
