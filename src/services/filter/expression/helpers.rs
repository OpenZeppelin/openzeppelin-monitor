//! Utility functions for evaluating expressions and resolving JSON paths

use super::{
	ast::{Accessor, ComparisonOperator, ConditionLeft, Expression, LogicalOperator},
	error::EvaluationError,
	evaluation::ConditionEvaluator,
};

/// Traverses the Expression AST and uses ConditionEvaluator to evaluate conditions
/// Returns true if the expression evaluates to true, false otherwise
/// Returns an error if the evaluation fails
pub fn evaluate(
	expression: &Expression<'_>,
	evaluator: &impl ConditionEvaluator,
) -> Result<bool, EvaluationError> {
	match expression {
		Expression::Condition(condition) => {
			let base_name = condition.left.base_name();
			let accessors = condition.left.accessors();
			let (base_value_str, base_kind_str) = evaluator.get_base_param(base_name)?;

			let final_left_value_str: String;
			let final_left_kind: String;

			if accessors.is_empty() {
				// No accessors, use the base value directly
				// Normalize map format if needed
				final_left_value_str = normalize_map_json(base_value_str, base_kind_str);
				final_left_kind = base_kind_str.to_string();
			} else {
				let resolved_value = resolve_path_to_json_value(
					base_value_str,
					base_kind_str,
					accessors,
					base_name,
					&condition.left,
				)?;

				// Get the kind from the resolved JSON value from chain-specific evaluator
				final_left_kind = evaluator.get_kind_from_json_value(&resolved_value);

				// Convert the resolved JSON value to a string representation
				final_left_value_str = match resolved_value {
					serde_json::Value::String(s) => s,
					serde_json::Value::Number(n) => n.to_string(),
					serde_json::Value::Bool(b) => b.to_string(),
					serde_json::Value::Null => "null".to_string(),
					serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
						// If the resolved value is an array or object, we need to convert it to a string
						resolved_value.to_string()
					}
				};
			}

			evaluator.compare_final_values(
				&final_left_kind,
				&final_left_value_str,
				&condition.operator,
				&condition.right,
			)
		}
		Expression::Logical {
			left,
			operator,
			right,
		} => {
			let left_val = evaluate(left, evaluator)?;
			match operator {
				LogicalOperator::And => {
					if !left_val {
						Ok(false)
					} else {
						evaluate(right, evaluator)
					}
				}
				LogicalOperator::Or => {
					if left_val {
						Ok(true)
					} else {
						evaluate(right, evaluator)
					}
				}
			}
		}
	}
}

/// Compares two values implementing the Ord trait using the specified comparison operator
/// Returns true if the comparison is valid, false otherwise
/// Returns an error if the operator is not supported for the given types
pub fn compare_ordered_values<T: Ord>(
	left: &T,
	op: &ComparisonOperator,
	right: &T,
) -> Result<bool, EvaluationError> {
	match op {
		ComparisonOperator::Eq => Ok(left == right),
		ComparisonOperator::Ne => Ok(left != right),
		ComparisonOperator::Gt => Ok(left > right),
		ComparisonOperator::Gte => Ok(left >= right),
		ComparisonOperator::Lt => Ok(left < right),
		ComparisonOperator::Lte => Ok(left <= right),
		_ => {
			let msg = format!(
				"Unsupported operator '{:?}' for types: {:?} and {:?}",
				op,
				std::any::type_name::<T>(),
				std::any::type_name::<T>()
			);
			Err(EvaluationError::unsupported_operator(msg, None, None))
		}
	}
}

/// Normalizes Stellar-style map format to valid JSON.
/// Converts {key:value} to {"key":"value"} for map types.
/// Only applies if kind contains "map" and JSON parsing fails.
fn normalize_map_json(value: &str, kind: &str) -> String {
	// Only apply to map types
	if !kind.to_lowercase().contains("map") {
		return value.to_string();
	}

	// If it already parses as valid JSON, return as-is
	if serde_json::from_str::<serde_json::Value>(value).is_ok() {
		return value.to_string();
	}

	// Simple normalization for Stellar format
	// Handles: {key:value,key2:value2} -> {"key":"value","key2":"value2"}
	let mut result = String::with_capacity(value.len() * 2);
	let mut chars = value.chars().peekable();
	let mut expect_key = false;

	while let Some(ch) = chars.next() {
		match ch {
			'{' => {
				result.push(ch);
				expect_key = true;
			}
			'}' => {
				result.push(ch);
				expect_key = false;
			}
			':' => {
				result.push(ch);
				expect_key = false;

				// Skip whitespace and read value
				while chars.peek() == Some(&' ') {
					chars.next();
				}

				// Collect value token until ',' or '}'
				let mut token = String::new();
				while let Some(&next_ch) = chars.peek() {
					if next_ch == ',' || next_ch == '}' {
						break;
					}
					token.push(chars.next().unwrap());
				}

				let token = token.trim();

				// Determine if token needs quoting
				let needs_quotes = !token.starts_with('"')
					&& token.parse::<f64>().is_err()
					&& token != "true"
					&& token != "false"
					&& token != "null";

				if needs_quotes {
					result.push('"');
					result.push_str(token);
					result.push('"');
				} else {
					result.push_str(token);
				}
			}
			',' => {
				result.push(ch);
				expect_key = true;
			}
			_ if expect_key && (ch.is_alphanumeric() || ch == '_') => {
				// Reading a key - add quotes around it
				result.push('"');
				result.push(ch);

				while let Some(&next_ch) = chars.peek() {
					if next_ch.is_alphanumeric() || next_ch == '_' {
						result.push(chars.next().unwrap());
					} else {
						break;
					}
				}

				result.push('"');
				expect_key = false;
			}
			' ' if !expect_key => {
				// Skip whitespace outside of keys
			}
			_ => {
				result.push(ch);
			}
		}
	}

	result
}

/// Resolves a JSON path from a base variable name and accessors
/// Returns the resolved JSON value
/// Returns an error if the traversal fails
fn resolve_path_to_json_value(
	base_value_str: &str,
	base_kind_str: &str,
	accessors: &[Accessor],
	base_name_for_error: &str,
	full_lhs_expr_for_error: &ConditionLeft<'_>,
) -> Result<serde_json::Value, EvaluationError> {
	// Parse base value with error context
	let mut current_json_val = parse_base_value(
		base_value_str,
		base_kind_str,
		base_name_for_error,
		full_lhs_expr_for_error,
	)?;

	// Precompute all path segments for error messages
	let path_segments =
		build_path_segments(base_name_for_error, full_lhs_expr_for_error.accessors());

	for (accessor_idx, accessor) in accessors.iter().enumerate() {
		current_json_val =
			access_json_value(current_json_val, accessor, &path_segments[accessor_idx])?;
	}

	Ok(current_json_val)
}

/// Helper to parse the initial JSON value with proper error context
fn parse_base_value(
	base_value_str: &str,
	base_kind_str: &str,
	base_name: &str,
	full_expr: &ConditionLeft<'_>,
) -> Result<serde_json::Value, EvaluationError> {
	// Normalize map format if needed before parsing
	let normalized_value = normalize_map_json(base_value_str, base_kind_str);

	serde_json::from_str(&normalized_value).map_err(|e| {
		let msg = format!(
			"Failed to parse value of base variable '{}' (kind: '{}', value: '{}') as JSON for path traversal. Full LHS: {:?}",
			base_name, base_kind_str, base_value_str, full_expr,
		);
		EvaluationError::parse_error(msg, Some(e.into()), None)
	})
}

/// Precomputes all path segments for error reporting
fn build_path_segments(base_name: &str, accessors: &[Accessor]) -> Vec<String> {
	let mut segments = Vec::with_capacity(accessors.len());
	let mut current_path = base_name.to_string();

	for accessor in accessors {
		current_path = match accessor {
			Accessor::Index(i) => format!("{}[{}]", current_path, i),
			Accessor::Key(k) => format!("{}.{}", current_path, k),
		};
		segments.push(current_path.clone());
	}

	segments
}

/// Helper to access JSON value with proper error handling
fn access_json_value(
	current_json: serde_json::Value,
	accessor: &Accessor,
	path_segment: &str,
) -> Result<serde_json::Value, EvaluationError> {
	match accessor {
		Accessor::Index(idx) => {
			let arr = current_json.as_array().ok_or_else(|| {
				let msg = format!("Array access on non-array at '{}'", path_segment);
				EvaluationError::type_mismatch(msg, None, None)
			})?;

			arr.get(*idx).cloned().ok_or_else(|| {
				let msg = format!(
					"Index {} out of bounds for array of length {} at '{}'",
					idx,
					arr.len(),
					path_segment
				);
				EvaluationError::index_out_of_bounds(msg, None, None)
			})
		}
		Accessor::Key(key) => {
			let obj = current_json.as_object().ok_or_else(|| {
				let msg = format!("Key access on non-object at '{}'", path_segment);
				EvaluationError::type_mismatch(msg, None, None)
			})?;

			obj.get(*key).cloned().ok_or_else(|| {
				let msg = format!("Key '{}' not found at '{}'", key, path_segment);
				EvaluationError::field_not_found(msg, None, None)
			})
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::services::filter::expression::ast::{ComparisonOperator, VariablePath};
	use serde_json::json;

	// --- Tests for `compare_ordered_values` ---
	#[test]
	fn test_compare_ordered_values_integers() {
		assert!(compare_ordered_values(&5, &ComparisonOperator::Eq, &5).unwrap());
		assert!(compare_ordered_values(&10, &ComparisonOperator::Gt, &5).unwrap());
		assert!(compare_ordered_values(&5, &ComparisonOperator::Lt, &10).unwrap());
		assert!(compare_ordered_values(&5, &ComparisonOperator::Gte, &5).unwrap());
		assert!(compare_ordered_values(&5, &ComparisonOperator::Lte, &5).unwrap());
		assert!(compare_ordered_values(&5, &ComparisonOperator::Ne, &10).unwrap());
	}

	#[test]
	fn test_compare_ordered_values_unsupported_operator() {
		let result = compare_ordered_values(&5, &ComparisonOperator::Contains, &5);
		assert!(matches!(
			result,
			Err(EvaluationError::UnsupportedOperator(_))
		));
	}

	// --- Tests for `parse_base_value` ---
	#[test]
	fn test_parse_base_value_ok() {
		let val = parse_base_value(
			r#"{"key": "value"}"#,
			"json_string",
			"data",
			&ConditionLeft::Simple("data"),
		);
		assert_eq!(val.unwrap(), json!({"key": "value"}));
	}

	#[test]
	fn test_parse_base_value_err() {
		let result = parse_base_value("not json", "string", "data", &ConditionLeft::Simple("data"));
		assert!(matches!(result, Err(EvaluationError::ParseError(_))));
	}

	// --- Tests for `access_json_value` ---
	#[test]
	fn test_access_json_value_key() {
		let obj = json!({"user": {"name": "Alice"}});
		let accessed = access_json_value(obj, &Accessor::Key("user"), "obj.user").unwrap();
		assert_eq!(accessed, json!({"name": "Alice"}));
	}

	#[test]
	fn test_access_json_value_index() {
		let arr = json!([10, 20, 30]);
		let accessed = access_json_value(arr, &Accessor::Index(1), "arr[1]").unwrap();
		assert_eq!(accessed, json!(20));
	}

	#[test]
	fn test_access_json_value_errors() {
		// Key not found
		let obj = json!({"name": "Bob"});
		let res1 = access_json_value(obj.clone(), &Accessor::Key("age"), "obj.age");
		assert!(matches!(res1, Err(EvaluationError::FieldNotFound(_))));

		// Index out of bounds
		let arr = json!([1]);
		let res2 = access_json_value(arr.clone(), &Accessor::Index(5), "arr[5]");
		assert!(matches!(res2, Err(EvaluationError::IndexOutOfBounds(_))));

		// Type mismatch (key access on array)
		let res3 = access_json_value(arr.clone(), &Accessor::Key("key"), "arr.key");
		assert!(matches!(res3, Err(EvaluationError::TypeMismatch(_))));

		// Type mismatch (index access on object)
		let res4 = access_json_value(obj.clone(), &Accessor::Index(0), "obj[0]");
		assert!(matches!(res4, Err(EvaluationError::TypeMismatch(_))));
	}

	// --- Tests for `resolve_path_to_json_value` ---
	#[test]
	fn test_resolve_path_simple_key() {
		let base_val_str = r#"{"name": "Alice", "age": 30}"#;
		let accessors = vec![Accessor::Key("age")];
		let lhs = ConditionLeft::Path(VariablePath {
			base: "user",
			accessors: accessors.clone(),
		});
		let resolved =
			resolve_path_to_json_value(base_val_str, "object", &accessors, "user", &lhs).unwrap();
		assert_eq!(resolved, json!(30));
	}

	#[test]
	fn test_resolve_path_nested() {
		let base_val_str = r#"{"user": {"details": {"status": "active"}}}"#;
		let accessors = vec![
			Accessor::Key("user"),
			Accessor::Key("details"),
			Accessor::Key("status"),
		];
		let lhs = ConditionLeft::Path(VariablePath {
			base: "data",
			accessors: accessors.clone(),
		});
		let resolved =
			resolve_path_to_json_value(base_val_str, "object", &accessors, "data", &lhs).unwrap();
		assert_eq!(resolved, json!("active"));
	}

	#[test]
	fn test_resolve_path_array_index() {
		let base_val_str = r#"[{"id": 1}, {"id": 2}]"#;
		let accessors = vec![Accessor::Index(1), Accessor::Key("id")];
		let lhs = ConditionLeft::Path(VariablePath {
			base: "items",
			accessors: accessors.clone(),
		});
		let resolved =
			resolve_path_to_json_value(base_val_str, "array", &accessors, "items", &lhs).unwrap();
		assert_eq!(resolved, json!(2));
	}

	// --- Tests for `build_path_segments` ---
	#[test]
	fn test_build_path_segments_formatting() {
		let segments = build_path_segments("base", &[Accessor::Key("field"), Accessor::Index(0)]);
		assert_eq!(
			segments,
			vec!["base.field".to_string(), "base.field[0]".to_string()]
		);
	}

	// --- Tests for `normalize_map_json` ---
	#[test]
	fn test_normalize_map_json_stellar_format() {
		// Test Stellar's unquoted key:value format
		let input = "{age:34,email:nico_testing@gmail.com,name:Nicolas Molina}";
		let result = normalize_map_json(input, "Map<String,String>");

		// Should convert to valid JSON
		let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
		assert_eq!(parsed["age"], 34); // Numeric value stays as number
		assert_eq!(parsed["email"], "nico_testing@gmail.com");
		assert_eq!(parsed["name"], "Nicolas Molina");
	}

	#[test]
	fn test_normalize_map_json_numeric_values() {
		// Test with numeric values (should not be quoted)
		let input = "{age:34,score:99}";
		let result = normalize_map_json(input, "map");

		let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
		assert_eq!(parsed["age"], 34);
		assert_eq!(parsed["score"], 99);
	}

	#[test]
	fn test_normalize_map_json_already_valid() {
		// Test with already valid JSON - should return unchanged
		let input = r#"{"age":34,"name":"Nicolas"}"#;
		let result = normalize_map_json(input, "Map<String,U32>");

		assert_eq!(result, input);
	}

	#[test]
	fn test_normalize_map_json_non_map_type() {
		// Test with non-map type - should return unchanged
		let input = "{age:34}";
		let result = normalize_map_json(input, "string");

		assert_eq!(result, input);
	}

	#[test]
	fn test_normalize_map_json_empty() {
		// Test empty map
		let input = "{}";
		let result = normalize_map_json(input, "map");

		assert_eq!(result, "{}");
	}

	#[test]
	fn test_normalize_map_json_special_characters() {
		// Test with special characters in values
		let input = "{email:test@example.com,url:https://example.com}";
		let result = normalize_map_json(input, "Map<String,String>");

		let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
		assert_eq!(parsed["email"], "test@example.com");
		assert_eq!(parsed["url"], "https://example.com");
	}
}
