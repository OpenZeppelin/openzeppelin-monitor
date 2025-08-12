//! This module provides the `StellarConditionEvaluator` struct, which implements
//! the `ConditionEvaluator` trait for evaluating conditions in Stellar-based chains.

use super::helpers;
use crate::{
	models::StellarMatchParamEntry,
	services::filter::expression::{
		compare_ordered_values, ComparisonOperator, ConditionEvaluator, EvaluationError,
		LiteralValue,
	},
};
use serde_json::Value as JsonValue;

pub type StellarArgs = [StellarMatchParamEntry];

pub struct StellarConditionEvaluator<'a> {
	args: &'a StellarArgs,
}

impl<'a> StellarConditionEvaluator<'a> {
	pub fn new(args: &'a StellarArgs) -> Self {
		Self { args }
	}

	/// Helper to check if a serde_json::Value matches a target string.
	/// Used by compare_vec for items within a JSON array.
	///
	/// Arguments:
	/// - value_to_check: The value to check if it matches the target string.
	/// - target_str: The target string to match against.
	///
	/// Returns:
	/// - true if the value matches the target string, false otherwise.
	pub fn check_json_value_matches_str(value_to_check: &JsonValue, target_str: &str) -> bool {
		match value_to_check {
			JsonValue::String(s) => s.eq_ignore_ascii_case(target_str),
			JsonValue::Object(nested_map) => {
				// If 'value_to_check' is an object - check its "value" field.
				if let Some(val_prop) = nested_map.get("value") {
					return match val_prop {
						JsonValue::String(s_val) => s_val == target_str,
						_ => val_prop.to_string().trim_matches('"') == target_str,
					};
				}
				false
			}
			// For numbers, bools, null - convert to string representation for matching.
			_ => value_to_check.to_string().trim_matches('"') == target_str,
		}
	}

	/// Compares a "vec" type parameter.
	/// LHS (`lhs_str`) can be a JSON array string or a comma-separated string.
	/// Supports "Eq", "Ne", "Contains" operators.
	/// For "Contains":
	///   - If `lhs_str` is a JSON array:
	///     - It iterates through each element of the array.
	///     - If an element is a simple type (string, number, bool), it's compared directly to `rhs_literal`.
	///     - If an element is an object:
	///       - It iterates through each field value of this object element.
	///       - If a field's value is a simple type, it's compared directly.
	///       - If a field's value is *itself another object*, the function checks if this *nested object*
	///         has a key named `"value"`, and if so, compares the content of that `"value"` key.
	///         It does not recursively search all fields of arbitrarily nested objects beyond this specific "value" key check.
	///   - If `lhs_str` is not a JSON array (or fails to parse as one): treats it as a comma-separated list
	///     and checks if `rhs_literal` (as a string) is one of the values in the list.
	///
	/// For "Eq"/"Ne": compares `lhs_str` directly with `rhs_literal` (as string).
	///
	/// Arguments:
	/// - lhs_str: The left-hand side value as a string.
	/// - operator: The operator to use for the comparison.
	/// - rhs_literal: The right-hand side value.
	///
	/// Returns:
	/// - true if the comparison is true, false otherwise.
	pub fn compare_vec(
		&self,
		lhs_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let rhs_target_str = match rhs_literal {
			LiteralValue::Str(s) => *s,
			LiteralValue::Number(s) => *s,
			_ => {
				let msg = format!(
					"Expected string or number literal for 'vec' comparison, found: {:?}",
					rhs_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		tracing::debug!(
			"Comparing vec: lhs: '{}', operator: {:?}, rhs: '{}'",
			lhs_str,
			operator,
			rhs_target_str
		);

		match operator {
			ComparisonOperator::Eq | ComparisonOperator::Ne => {
				let lhs_as_json: Option<JsonValue> = serde_json::from_str(lhs_str).ok();
				let rhs_as_json: Option<JsonValue> = serde_json::from_str(rhs_target_str).ok();

				// Helper closure to compare two strings as normalized CSV lists
				let compare_strings_as_normalized_csv = |s1: &str, s2: &str| {
					let normalize_to_vec = |s: &str| -> Vec<String> {
						s.split(',')
							.map(|part| part.trim().to_lowercase())
							.collect()
					};
					normalize_to_vec(s1) == normalize_to_vec(s2)
				};

				let are_equal = match (lhs_as_json, rhs_as_json) {
					// Both strings parsed successfully as JSON
					(Some(lhs_json_val), Some(rhs_json_val)) => {
						match (lhs_json_val.is_array(), rhs_json_val.is_array()) {
							// Both are JSON arrays - compare them semantically
							(true, true) => lhs_json_val == rhs_json_val,
							// One is a JSON array, the other is valid JSON but not an array. Not equal as 'vec' types.
							(true, false) | (false, true) => false,
							// Both are valid JSON, but NEITHER is an array. Fallback to comparing original string forms as CSV.
							(false, false) => {
								compare_strings_as_normalized_csv(lhs_str, rhs_target_str)
							}
						}
					}
					// Neither string could be parsed as JSON - treat both as CSV
					(None, None) => compare_strings_as_normalized_csv(lhs_str, rhs_target_str),
					// One parsed as JSON, the other didn't - not equal
					(Some(_), None) | (None, Some(_)) => false,
				};

				Ok(if *operator == ComparisonOperator::Eq {
					are_equal
				} else {
					!are_equal
				})
			}
			ComparisonOperator::Contains => {
				// Try to parse lhs_str as a JSON array
				if let Ok(json_array) = serde_json::from_str::<Vec<JsonValue>>(lhs_str) {
					let found = json_array.iter().any(|item| match item {
						JsonValue::Object(map) => {
							// Check each field in the object item
							map.values().any(|val_in_obj| {
								Self::check_json_value_matches_str(val_in_obj, rhs_target_str)
							})
						}
						// For non-object array elements, compare directly
						_ => Self::check_json_value_matches_str(item, rhs_target_str),
					});
					Ok(found)
				} else {
					// Fallback to CSV
					tracing::debug!(
						"LHS for 'vec' ('{}') not valid JSON array, falling back to CSV check for value '{}'",
						lhs_str,
						rhs_target_str
					);
					let csv_values: Vec<&str> = lhs_str.split(',').map(str::trim).collect();
					Ok(csv_values.contains(&rhs_target_str))
				}
			}
			_ => {
				let msg = format!(
					"Operator {:?} not supported for 'vec' type. Supported: Eq, Ne, Contains.",
					operator
				);
				Err(EvaluationError::unsupported_operator(msg, None, None))
			}
		}
	}

	/// Compares two boolean values (true/false) using the specified operator.
	///
	/// Arguments:
	/// - lhs_str: The left-hand side value as a string.
	/// - operator: The operator to use for the comparison.
	/// - rhs_literal: The right-hand side value.
	///
	/// Returns:
	/// - true if the comparison is true, false otherwise.
	pub fn compare_boolean(
		&self,
		lhs_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let Ok(left) = lhs_str.parse::<bool>() else {
			let msg = format!("Failed to parse bool parameter value: {}", lhs_str);
			return Err(EvaluationError::parse_error(msg, None, None));
		};

		let right = match rhs_literal {
			LiteralValue::Bool(b) => *b,
			_ => {
				let msg = format!(
					"Expected bool literal for comparison, found: {:?}",
					rhs_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		tracing::debug!("Comparing bool: left: {}, right: {}", left, right);

		match operator {
			ComparisonOperator::Eq => Ok(left == right),
			ComparisonOperator::Ne => Ok(left != right),
			_ => {
				let msg = format!(
					"Unsupported operator {:?} for Stellar bool comparison",
					operator
				);
				Err(EvaluationError::unsupported_operator(msg, None, None))
			}
		}
	}

	/// Compares two numeric values (u64/i64/u32/i32) using the specified operator.
	///
	/// Arguments:
	/// - lhs_str: The left-hand side value as a string.
	/// - operator: The operator to use for the comparison.
	/// - rhs_literal: The right-hand side value.
	///
	/// Returns:
	/// - true if the comparison is true, false otherwise.
	fn compare_numeric<T: std::str::FromStr + Ord + std::fmt::Display>(
		&self,
		lhs_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError>
	where
		<T as std::str::FromStr>::Err: std::fmt::Debug,
	{
		let left = lhs_str.parse::<T>().map_err(|_| {
			let msg = format!("Failed to parse numeric parameter value: {}", lhs_str);
			EvaluationError::parse_error(msg, None, None)
		})?;

		let rhs_str = match rhs_literal {
			LiteralValue::Number(s) => s,
			_ => {
				let msg = format!(
					"Expected number literal for {} comparison",
					std::any::type_name::<T>()
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		let right = rhs_str.parse::<T>().map_err(|_| {
			let msg = format!(
				"Failed to parse comparison value '{}' as {}",
				rhs_str,
				std::any::type_name::<T>()
			);
			EvaluationError::parse_error(msg, None, None)
		})?;

		compare_ordered_values(&left, operator, &right)
	}

	/// Compares two large integers (u256/i256) as strings.
	///
	/// Arguments:
	/// - lhs_str: The left-hand side value as a string.
	/// - operator: The operator to use for the comparison.
	/// - rhs_literal: The right-hand side value.
	///
	/// Returns:
	/// - true if the comparison is true, false otherwise.
	fn compare_large_int_as_string(
		&self,
		lhs_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let right = match rhs_literal {
			LiteralValue::Number(s) => s,
			LiteralValue::Str(s) => s,
			_ => {
				let msg = format!(
					"Expected number or string literal for i256 comparison, found: {:?}",
					rhs_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		tracing::debug!(
			"Comparing large integer strings: left: {}, right: {}",
			lhs_str,
			right
		);

		match operator {
			ComparisonOperator::Eq => Ok(lhs_str == *right),
			ComparisonOperator::Ne => Ok(lhs_str != *right),
			_ => {
				let msg = format!(
					"Operator {:?} not supported for i256 string comparison",
					operator
				);
				Err(EvaluationError::unsupported_operator(msg, None, None))
			}
		}
	}

	/// Compares two strings (string/address/symbol/bytes) using the specified operator.
	/// The comparison is case-insensitive for string and address types.
	/// For address, it normalizes both sides before comparison.
	/// For symbol and bytes, it performs a case-insensitive comparison.
	///
	/// Arguments:
	/// - lhs_kind: The kind of the left-hand side value.
	/// - lhs_str: The left-hand side value as a string.
	/// - operator: The operator to use for the comparison.
	/// - rhs_literal: The right-hand side value.
	///
	/// Returns:
	/// - true if the comparison is true, false otherwise.
	pub fn compare_string(
		&self,
		lhs_kind: &str, // "string", "address", "symbol", "bytes"
		lhs_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let right_str = match rhs_literal {
			LiteralValue::Str(s) => *s,
			_ => {
				let msg = format!(
					"Expected string literal for {} comparison, found: {:?}",
					lhs_kind, rhs_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		// Normalize based on kind
		let left_normalized;
		let right_normalized;

		let is_address_kind = lhs_kind == "address";
		let is_strict_eq_operator =
			operator == &ComparisonOperator::Eq || operator == &ComparisonOperator::Ne;

		if is_address_kind && is_strict_eq_operator {
			left_normalized = helpers::normalize_address(lhs_str);
			right_normalized = helpers::normalize_address(right_str);
		} else {
			left_normalized = lhs_str.to_lowercase();
			right_normalized = right_str.to_lowercase();
		}

		tracing::debug!(
			"Comparing strings: kind: {}, left: {}, operator: {:?}, right: {}",
			lhs_kind,
			left_normalized,
			operator,
			right_normalized,
		);

		match operator {
			ComparisonOperator::Eq => Ok(left_normalized == right_normalized),
			ComparisonOperator::Ne => Ok(left_normalized != right_normalized),
			ComparisonOperator::StartsWith => Ok(left_normalized.starts_with(&right_normalized)),
			ComparisonOperator::EndsWith => Ok(left_normalized.ends_with(&right_normalized)),
			ComparisonOperator::Contains => Ok(left_normalized.contains(&right_normalized)),
			_ => {
				let msg = format!(
					"Operator {:?} not supported for type {}",
					operator, lhs_kind
				);
				Err(EvaluationError::unsupported_operator(msg, None, None))
			}
		}
	}

	/// Compares a map (JSON object) value with a literal value.
	///
	/// Arguments:
	/// - lhs_json_map_str: The left-hand side value as a JSON map string.
	/// - operator: The operator to use for the comparison.
	/// - rhs_literal: The right-hand side value.
	///
	/// Returns:
	/// - true if the comparison is true, false otherwise.
	pub fn compare_map(
		&self,
		lhs_json_map_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let rhs_target_str = match rhs_literal {
			LiteralValue::Str(s) => *s,
			LiteralValue::Number(s) => {
				if *operator == ComparisonOperator::Contains {
					*s // For Contains, we search for this number (as string)
				} else {
					let msg = format!(
						"Expected string literal (representing a JSON map) for EVM 'map' Eq/Ne comparison, found number: {:?}",
						rhs_literal
					);
					return Err(EvaluationError::type_mismatch(msg, None, None));
				}
			}
			_ => {
				let msg = format!(
					"Expected string literal for EVM 'map' {} comparison, found: {:?}",
					if *operator == ComparisonOperator::Contains {
						"Contains"
					} else {
						"Eq/Ne"
					},
					rhs_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		tracing::debug!(
			"EVM Comparing map: lhs: '{}', operator: {:?}, rhs_target: '{}'",
			lhs_json_map_str,
			operator,
			rhs_target_str
		);

		match operator {
			ComparisonOperator::Eq | ComparisonOperator::Ne => {
				let lhs_json_value =
					serde_json::from_str::<JsonValue>(lhs_json_map_str).map_err(|e| {
						let msg = format!(
							"Failed to parse LHS value '{}' as JSON map for 'Eq/Ne' operator",
							lhs_json_map_str
						);
						EvaluationError::parse_error(msg, Some(e.into()), None)
					})?;

				let rhs_json_value =
					serde_json::from_str::<JsonValue>(rhs_target_str).map_err(|e| {
						let msg = format!(
							"Failed to parse RHS value '{}' as JSON map for 'Eq/Ne' operator",
							rhs_target_str
						);
						EvaluationError::parse_error(msg, Some(e.into()), None)
					})?;

				// Ensure both parsed values are actually objects
				if !lhs_json_value.is_object() || !rhs_json_value.is_object() {
					let msg = format!(
						"For 'map' Eq/Ne comparison, both LHS ('{}') and RHS ('{}') must resolve to JSON objects.",
						lhs_json_map_str, rhs_target_str
					);
					return Err(EvaluationError::type_mismatch(msg, None, None));
				}

				let are_equal = lhs_json_value == rhs_json_value;

				Ok(if *operator == ComparisonOperator::Eq {
					are_equal
				} else {
					!are_equal
				})
			}
			ComparisonOperator::Contains => {
				let json_map =
					serde_json::from_str::<serde_json::Map<String, JsonValue>>(lhs_json_map_str)
						.map_err(|e| {
							let msg = format!(
								"Failed to parse LHS value '{}' as JSON map for 'contains' operator",
								lhs_json_map_str
							);
							EvaluationError::parse_error(msg, Some(e.into()), None)
						})?;

				let found = json_map.values().any(|item_in_map| {
					Self::check_json_value_matches_str(item_in_map, rhs_target_str)
				});
				Ok(found)
			}
			_ => {
				let msg = format!(
					"Operator {:?} not supported for EVM 'map' type. Supported: Eq, Ne, Contains.",
					operator
				);
				Err(EvaluationError::unsupported_operator(msg, None, None))
			}
		}
	}
}

impl ConditionEvaluator for StellarConditionEvaluator<'_> {
	/// This method is used to get the base parameter of the Stellar condition evaluator.
	///
	/// Arguments:
	/// - name: The name of the parameter to get.
	///
	/// Returns:
	/// - The base parameter.
	fn get_base_param(&self, name: &str) -> Result<(&str, &str), EvaluationError> {
		self.args
			.iter()
			.find(|entry| entry.name == name)
			.map(|entry| (entry.value.as_str(), entry.kind.as_str()))
			.ok_or_else(|| {
				let msg = format!("Base parameter not found: {}", name);
				EvaluationError::variable_not_found(msg, None, None)
			})
	}

	/// This method is used to get the kind of the value from the JSON value.
	///
	/// Arguments:
	/// - value: The JSON value to get the kind from.
	///
	/// Returns:
	/// - The kind of the value.
	fn get_kind_from_json_value(&self, value: &serde_json::Value) -> String {
		helpers::get_kind_from_value(value)
	}

	/// This method is used to compare the final values of the Stellar condition evaluator.
	///
	/// Arguments:
	/// - lhs_kind: The kind of the left-hand side value.
	/// - lhs_str: The left-hand side value as a string.
	/// - operator: The operator to use for the comparison.
	/// - rhs_literal: The right-hand side value.
	fn compare_final_values(
		&self,
		lhs_kind: &str,
		lhs_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		match lhs_kind.to_lowercase().as_str() {
			"bool" => self.compare_boolean(lhs_str, operator, rhs_literal),
			"u32" => self.compare_numeric::<u32>(lhs_str, operator, rhs_literal),
			"u64" | "timepoint" | "duration" => {
				self.compare_numeric::<u64>(lhs_str, operator, rhs_literal)
			}
			"i32" => self.compare_numeric::<i32>(lhs_str, operator, rhs_literal),
			"i64" => self.compare_numeric::<i64>(lhs_str, operator, rhs_literal),
			"u128" => self.compare_numeric::<u128>(lhs_str, operator, rhs_literal),
			"i128" => self.compare_numeric::<i128>(lhs_str, operator, rhs_literal),
			"u256" | "i256" => self.compare_large_int_as_string(lhs_str, operator, rhs_literal),
			"string" | "symbol" | "address" | "bytes" => self.compare_string(
				lhs_kind.to_ascii_lowercase().as_str(),
				lhs_str,
				operator,
				rhs_literal,
			),
			"vec" => self.compare_vec(lhs_str, operator, rhs_literal),
			"map" => self.compare_map(lhs_str, operator, rhs_literal),
			unknown_type => {
				let msg = format!("Unknown parameter type: {}", unknown_type);
				Err(EvaluationError::type_mismatch(msg, None, None))
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// Helper to create a dummy StellarConditionEvaluator (args don't matter for these unit tests)
	fn create_evaluator() -> StellarConditionEvaluator<'static> {
		static EMPTY_ARGS: &StellarArgs = &[];
		StellarConditionEvaluator::new(EMPTY_ARGS)
	}

	/// --- Test cases for compare_bool method ---
	#[test]
	fn test_compare_bool_valid() {
		let evaluator = create_evaluator();

		assert!(evaluator
			.compare_boolean("true", &ComparisonOperator::Eq, &LiteralValue::Bool(true))
			.unwrap());
		assert!(!evaluator
			.compare_boolean("true", &ComparisonOperator::Eq, &LiteralValue::Bool(false))
			.unwrap());
	}

	#[test]
	fn test_compare_bool_invalid() {
		let evaluator = create_evaluator();

		assert!(!evaluator
			.compare_boolean("true", &ComparisonOperator::Ne, &LiteralValue::Bool(true))
			.unwrap());
		assert!(evaluator
			.compare_boolean("true", &ComparisonOperator::Ne, &LiteralValue::Bool(false))
			.unwrap());
	}

	#[test]
	fn test_compare_bool_error() {
		let evaluator = create_evaluator();

		// Test TypeMismatch for RHS
		let type_mismatch_result = evaluator.compare_boolean(
			"true",
			&ComparisonOperator::Eq,
			&LiteralValue::Number("123"),
		);
		assert!(matches!(
			type_mismatch_result,
			Err(EvaluationError::TypeMismatch(_))
		));

		// Test ParseError for LHS
		let parse_error_result = evaluator.compare_boolean(
			"notabool",
			&ComparisonOperator::Eq,
			&LiteralValue::Bool(true),
		);
		assert!(matches!(
			parse_error_result,
			Err(EvaluationError::ParseError(_))
		));

		// Test UnsupportedOperator
		let unsupported_op_result = evaluator.compare_boolean(
			"true",
			&ComparisonOperator::Gt, // Gt is not supported for bool
			&LiteralValue::Bool(false),
		);
		assert!(matches!(
			unsupported_op_result,
			Err(EvaluationError::UnsupportedOperator { .. })
		));
	}

	#[test]
	fn test_compare_bool_case_sensitivity() {
		let args = vec![];
		let evaluator = StellarConditionEvaluator::new(&args);

		// Test TRUE (uppercase)
		assert!(evaluator
			.compare_boolean("TRUE", &ComparisonOperator::Eq, &LiteralValue::Bool(true))
			.is_err());

		// Test False (mixed case)
		assert!(evaluator
			.compare_boolean("False", &ComparisonOperator::Eq, &LiteralValue::Bool(false))
			.is_err());

		// Test TRUE == TRUE (both uppercase)
		assert!(evaluator
			.compare_boolean("TRUE", &ComparisonOperator::Eq, &LiteralValue::Bool(true))
			.is_err());
	}

	/// --- Test cases for compare_numeric method ---
	#[test]
	fn test_compare_numeric_valid() {
		let evaluator = create_evaluator();

		assert!(evaluator
			.compare_numeric::<u64>("100", &ComparisonOperator::Gt, &LiteralValue::Number("50"))
			.unwrap());

		assert!(evaluator
			.compare_numeric::<u64>("100", &ComparisonOperator::Lt, &LiteralValue::Number("150"))
			.unwrap());

		assert!(evaluator
			.compare_numeric::<u64>("100", &ComparisonOperator::Eq, &LiteralValue::Number("100"))
			.unwrap());

		assert!(evaluator
			.compare_numeric::<u64>("100", &ComparisonOperator::Ne, &LiteralValue::Number("50"))
			.unwrap());

		assert!(evaluator
			.compare_numeric::<u64>(
				"100",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("100")
			)
			.unwrap());

		assert!(evaluator
			.compare_numeric::<u64>(
				"100",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("150")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_numeric_invalid() {
		let evaluator = create_evaluator();

		assert!(!evaluator
			.compare_numeric::<u64>("100", &ComparisonOperator::Gt, &LiteralValue::Number("150"))
			.unwrap());

		assert!(!evaluator
			.compare_numeric::<u64>("100", &ComparisonOperator::Lt, &LiteralValue::Number("50"))
			.unwrap());

		assert!(!evaluator
			.compare_numeric::<u64>("100", &ComparisonOperator::Eq, &LiteralValue::Number("50"))
			.unwrap());

		assert!(!evaluator
			.compare_numeric::<u64>("100", &ComparisonOperator::Ne, &LiteralValue::Number("100"))
			.unwrap());

		assert!(!evaluator
			.compare_numeric::<u64>(
				"100",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("150")
			)
			.unwrap());

		assert!(!evaluator
			.compare_numeric::<u64>("100", &ComparisonOperator::Lte, &LiteralValue::Number("50"))
			.unwrap());
	}

	#[test]
	fn test_compare_numeric_error() {
		let evaluator = create_evaluator();

		// Type Mismatch
		assert!(matches!(
			evaluator.compare_numeric::<u64>(
				"100",
				&ComparisonOperator::Gt,
				&LiteralValue::Bool(true)
			),
			Err(EvaluationError::TypeMismatch(_))
		));

		// Parse Error LHS
		assert!(matches!(
			evaluator.compare_numeric::<u64>(
				"abc",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("50")
			),
			Err(EvaluationError::ParseError(_))
		));

		// Parse Error RHS
		assert!(matches!(
			evaluator.compare_numeric::<u64>(
				"100",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("xyz")
			),
			Err(EvaluationError::ParseError(_))
		));

		// Unsupported Operator
		assert!(matches!(
			evaluator.compare_numeric::<u64>(
				"100",
				&ComparisonOperator::Contains,
				&LiteralValue::Number("50")
			),
			Err(EvaluationError::UnsupportedOperator { .. })
		));
	}

	/// --- Test cases for compare_large_int_as_string method ---
	#[test]
	fn test_compare_i256() {
		let evaluator = create_evaluator();

		// Eq
		assert!(evaluator
			.compare_large_int_as_string(
				"12345",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("12345")
			)
			.unwrap());
		assert!(evaluator
			.compare_large_int_as_string(
				"12345",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("12345")
			)
			.unwrap());
		assert!(!evaluator
			.compare_large_int_as_string(
				"12345",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("54321")
			)
			.unwrap());

		// Ne
		assert!(evaluator
			.compare_large_int_as_string(
				"12345",
				&ComparisonOperator::Ne,
				&LiteralValue::Number("54321")
			)
			.unwrap());
		assert!(!evaluator
			.compare_large_int_as_string(
				"12345",
				&ComparisonOperator::Ne,
				&LiteralValue::Number("12345")
			)
			.unwrap());

		// Unsupported operator
		assert!(matches!(
			evaluator.compare_large_int_as_string(
				"12345",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("54321")
			),
			Err(EvaluationError::UnsupportedOperator { .. })
		));

		// Type Mismatch RHS
		assert!(matches!(
			evaluator.compare_large_int_as_string(
				"12345",
				&ComparisonOperator::Eq,
				&LiteralValue::Bool(true)
			),
			Err(EvaluationError::TypeMismatch(_))
		));
	}

	/// --- Test cases for compare_string method ---
	#[test]
	fn test_compare_string_valid() {
		let evaluator = create_evaluator();

		// String Eq
		assert!(evaluator
			.compare_string(
				"string",
				"hello",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("hello")
			)
			.unwrap());

		// String Ne
		assert!(evaluator
			.compare_string(
				"string",
				"hello",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("world")
			)
			.unwrap());

		// String StartsWith
		assert!(evaluator
			.compare_string(
				"string",
				"hello world",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Str("hello")
			)
			.unwrap());

		// String EndsWith
		assert!(evaluator
			.compare_string(
				"string",
				"hello world",
				&ComparisonOperator::EndsWith,
				&LiteralValue::Str("world")
			)
			.unwrap());

		// String Contains
		assert!(evaluator
			.compare_string(
				"string",
				"hello world",
				&ComparisonOperator::Contains,
				&LiteralValue::Str("world")
			)
			.unwrap());

		// Address Eq (normalized)
		assert!(evaluator
			.compare_string(
				"address",
				"GABC...", // Assume normalize_address makes it GABC...
				&ComparisonOperator::Eq,
				&LiteralValue::Str("gabc...")
			)
			.unwrap()); // This depends on normalize_address

		// Address Ne (normalized)
		assert!(evaluator
			.compare_string(
				"address",
				"GABC...",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("something...")
			)
			.unwrap());

		// Address StartsWith (normalized)
		assert!(evaluator
			.compare_string(
				"address",
				"GABC...",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Str("GAB")
			)
			.unwrap());

		// Address EndsWith (normalized)
		assert!(evaluator
			.compare_string(
				"address",
				"GABC...",
				&ComparisonOperator::EndsWith,
				&LiteralValue::Str("C...")
			)
			.unwrap());

		// Address Contains (normalized)
		assert!(evaluator
			.compare_string(
				"address",
				"GABC...",
				&ComparisonOperator::Contains,
				&LiteralValue::Str("AB")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_string_invalid() {
		let evaluator = create_evaluator();

		// String Eq
		assert!(!evaluator
			.compare_string(
				"string",
				"hello",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("world")
			)
			.unwrap());

		// String Ne
		assert!(!evaluator
			.compare_string(
				"string",
				"hello",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("hello")
			)
			.unwrap());

		// String StartsWith
		assert!(!evaluator
			.compare_string(
				"string",
				"hello world",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Str("world")
			)
			.unwrap());

		// String EndsWith
		assert!(!evaluator
			.compare_string(
				"string",
				"hello world",
				&ComparisonOperator::EndsWith,
				&LiteralValue::Str("hello")
			)
			.unwrap());

		// String Contains
		assert!(!evaluator
			.compare_string(
				"string",
				"hello world",
				&ComparisonOperator::Contains,
				&LiteralValue::Str("foo")
			)
			.unwrap());

		// Address Eq (normalized)
		assert!(!evaluator
			.compare_string(
				"address",
				"GABC...",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("something...")
			)
			.unwrap());

		// Address Ne (normalized)
		assert!(!evaluator
			.compare_string(
				"address",
				"GABC...",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("GABC...")
			)
			.unwrap());

		// Address StartsWith (normalized)
		assert!(!evaluator
			.compare_string(
				"address",
				"GABC...",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Str("XYZ")
			)
			.unwrap());

		// Address EndsWith (normalized)
		assert!(!evaluator
			.compare_string(
				"address",
				"GABC...",
				&ComparisonOperator::EndsWith,
				&LiteralValue::Str("XYZ")
			)
			.unwrap());

		// Address Contains (normalized)
		assert!(!evaluator
			.compare_string(
				"address",
				"GABC...",
				&ComparisonOperator::Contains,
				&LiteralValue::Str("XYZ")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_string_error() {
		let evaluator = create_evaluator();

		// Type Mismatch
		assert!(matches!(
			evaluator.compare_string(
				"string",
				"hello",
				&ComparisonOperator::Eq,
				&LiteralValue::Bool(true)
			),
			Err(EvaluationError::TypeMismatch(_))
		));

		// Unsupported Operator
		assert!(matches!(
			evaluator.compare_string(
				"string",
				"hello",
				&ComparisonOperator::Gte,
				&LiteralValue::Str("world")
			),
			Err(EvaluationError::UnsupportedOperator { .. })
		));
	}

	// --- Test cases for compare_vec method ---
	#[test]
	fn test_compare_vec_json_array_contains_string() {
		let evaluator = create_evaluator();
		let lhs = r#"["item1", "item2", "item3"]"#;
		assert!(evaluator
			.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("item2")
			)
			.unwrap());
		assert!(!evaluator
			.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("items5")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_vec_json_array_contains_number_as_string() {
		let evaluator = create_evaluator();
		let lhs = r#"[123, "test", 456]"#;
		assert!(evaluator
			.compare_vec(lhs, &ComparisonOperator::Contains, &LiteralValue::Number("123")) // RHS is Number("123")
			.unwrap());
		assert!(evaluator
			.compare_vec(lhs, &ComparisonOperator::Contains, &LiteralValue::Str("456")) // RHS is Str("456")
			.unwrap());
	}

	#[test]
	fn test_compare_vec_json_array_contains_in_object_direct_value() {
		let evaluator = create_evaluator();
		let lhs = r#"[{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]"#;
		assert!(evaluator // Search for string value
			.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("Alice")
			)
			.unwrap());
		assert!(evaluator // Search for number value (as string)
			.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Number("2")
			)
			.unwrap());
		assert!(!evaluator
			.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("Charlie")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_vec_json_array_contains_in_object_value_field() {
		let evaluator = create_evaluator();
		let lhs = r#"[{"type": "user", "value": "alice"}, {"type": "item", "value": 789}]"#;
		assert!(evaluator // String in "value" field
			.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("alice")
			)
			.unwrap());
		assert!(evaluator // Number in "value" field (compared as string)
			.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Number("789")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_vec_csv_fallback_contains() {
		let evaluator = create_evaluator();
		let lhs = "alpha, beta, gamma"; // Not a valid JSON array
		assert!(evaluator
			.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("beta")
			)
			.unwrap());
		assert!(!evaluator
			.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("delta")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_vec_eq_ne() {
		let evaluator = create_evaluator();
		let lhs_json = r#"["a", "b"]"#;
		let lhs_csv = "a,b";

		// Eq
		assert!(evaluator
			.compare_vec(
				lhs_json,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"["a", "b"]"#)
			)
			.unwrap());
		assert!(!evaluator
			.compare_vec(
				lhs_json,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"["a", "c"]"#)
			)
			.unwrap());
		assert!(evaluator
			.compare_vec(lhs_csv, &ComparisonOperator::Eq, &LiteralValue::Str("a,b"))
			.unwrap());

		// Ne
		assert!(evaluator
			.compare_vec(
				lhs_json,
				&ComparisonOperator::Ne,
				&LiteralValue::Str(r#"["a", "c"]"#)
			)
			.unwrap());
	}

	#[test]
	fn test_compare_vec_eq_ne_json_vs_json_semantic() {
		let evaluator = create_evaluator();

		// --- Eq: Both JSON arrays ---
		assert!(evaluator
			.compare_vec(
				r#"[1, 2, "hello"]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"[1,2, "hello"]"#)
			)
			.unwrap());
		assert!(evaluator
			.compare_vec(
				r#"["Foo", "Bar"]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"[ "Foo", "Bar" ]"#)
			)
			.unwrap(),);

		// Case sensitivity for string elements (serde_json::Value default behavior)
		assert!(!evaluator
			.compare_vec(
				r#"["Alice"]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"["alice"]"#)
			)
			.unwrap(),);

		// Order matters
		assert!(!evaluator
			.compare_vec(
				r#"[1, 2]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"[2, 1]"#)
			)
			.unwrap(),);

		// --- Ne: Both JSON arrays ---
		assert!(!evaluator
			.compare_vec(
				r#"[1, 2, 3]"#,
				&ComparisonOperator::Ne,
				&LiteralValue::Str(r#"[1,2,3]"#)
			)
			.unwrap(),);
		assert!(evaluator
			.compare_vec(
				r#"["Alice"]"#,
				&ComparisonOperator::Ne,
				&LiteralValue::Str(r#"["alice"]"#)
			)
			.unwrap(),);
	}

	#[test]
	fn test_compare_vec_eq_ne_csv_vs_csv_normalized() {
		let evaluator = create_evaluator();

		// --- Eq: Both CSV-like ---
		assert!(evaluator
			.compare_vec(
				"alice, bob, charlie",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("alice,bob,charlie")
			)
			.unwrap());
		assert!(evaluator
			.compare_vec(
				"ALICE, BOB",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("alice,bob")
			)
			.unwrap());
		assert!(evaluator
			.compare_vec(
				"  leading,trailing  ",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("leading,trailing")
			)
			.unwrap());
		assert!(evaluator
			.compare_vec("one", &ComparisonOperator::Eq, &LiteralValue::Str("ONE"))
			.unwrap(),);
		assert!(evaluator
			.compare_vec(
				"", // LHS empty CSV
				&ComparisonOperator::Eq,
				&LiteralValue::Str("") // RHS empty CSV
			)
			.unwrap(),);

		// Order matters for CSV as well after normalization
		assert!(!evaluator
			.compare_vec(
				"alice,bob",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("bob,alice")
			)
			.unwrap(),);

		// Different content
		assert!(!evaluator
			.compare_vec(
				"alice,bob",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("alice,charlie")
			)
			.unwrap(),);

		// --- Ne: Both CSV-like ---
		assert!(!evaluator
			.compare_vec(
				"alice, bob",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("ALICE,BOB")
			)
			.unwrap(),);
		assert!(evaluator
			.compare_vec(
				"alice,bob",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("bob,alice")
			)
			.unwrap(),);
	}

	#[test]
	fn test_compare_vec_eq_ne_json_vs_csv() {
		let evaluator = create_evaluator();

		// Eq: JSON vs CSV - should be false
		assert!(!evaluator
			.compare_vec(
				r#"["alice", "bob"]"#, // LHS JSON
				&ComparisonOperator::Eq,
				&LiteralValue::Str("alice,bob") // RHS CSV
			)
			.unwrap(),);
		assert!(!evaluator
			.compare_vec(
				"alice,bob", // LHS CSV
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"["alice", "bob"]"#) // RHS JSON
			)
			.unwrap(),);

		// Ne: JSON vs CSV - should be true
		assert!(evaluator
			.compare_vec(
				r#"["alice", "bob"]"#,
				&ComparisonOperator::Ne,
				&LiteralValue::Str("alice,bob")
			)
			.unwrap(),);
	}

	#[test]
	fn test_compare_vec_errors() {
		let evaluator = create_evaluator();
		let lhs = r#"["data"]"#;

		// RHS TypeMismatch
		assert!(matches!(
			evaluator.compare_vec(
				lhs,
				&ComparisonOperator::Contains,
				&LiteralValue::Bool(true)
			),
			Err(EvaluationError::TypeMismatch(_))
		));

		// Unsupported Operator
		assert!(matches!(
			evaluator.compare_vec(lhs, &ComparisonOperator::Gt, &LiteralValue::Str("data")),
			Err(EvaluationError::UnsupportedOperator(_))
		));
	}

	/// --- Test cases for compare_map ---
	#[test]
	fn test_compare_map_contains_value() {
		let evaluator = create_evaluator();
		let lhs_json_map = r#"{"key1": "value1", "key2": "value2"}"#;
		assert!(evaluator
			.compare_map(
				lhs_json_map,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("value1")
			)
			.unwrap());
		assert!(!evaluator
			.compare_map(
				lhs_json_map,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("value3")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_map_semantic_equality() {
		let evaluator = create_evaluator();

		// Test Eq
		assert!(evaluator
			.compare_map(
				r#"{"key1": "value1", "key2": "value2"}"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"{"key2":"value2","key1":"value1"}"#)
			)
			.unwrap());

		// Test Ne
		assert!(!evaluator
			.compare_map(
				r#"{"key1": "value1", "key2": "value2"}"#,
				&ComparisonOperator::Ne,
				&LiteralValue::Str(r#"{"key1":"value1","key2":"value2"}"#)
			)
			.unwrap());
	}

	/// --- Test cases for compare_final_values method ---
	#[test]
	fn test_compare_final_values_routing() {
		let evaluator = create_evaluator();

		// Test routing to compare_bool
		assert!(evaluator
			.compare_final_values(
				"bool",
				"true",
				&ComparisonOperator::Eq,
				&LiteralValue::Bool(true)
			)
			.unwrap());

		// Test routing to compare_numeric
		assert!(evaluator
			.compare_final_values(
				"u64",
				"100",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("50")
			)
			.unwrap());

		// Test routing to compare_large_int_as_string
		assert!(evaluator
			.compare_final_values(
				"i256",
				"12345678901234567890",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("12345678901234567890")
			)
			.unwrap());

		// Test routing to compare_string
		assert!(evaluator
			.compare_final_values(
				"string",
				"hello",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("hello")
			)
			.unwrap());

		// Test routing to compare_string with address
		assert!(evaluator
			.compare_final_values(
				"address",
				"GABC...",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("gabc...")
			)
			.unwrap());

		// Test routing to compare_vec
		assert!(evaluator
			.compare_final_values(
				"vec",
				r#"["item1", "item2"]"#,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("item1")
			)
			.unwrap());

		assert!(evaluator
			.compare_final_values(
				"vec",
				"apple,banana",
				&ComparisonOperator::Contains,
				&LiteralValue::Str("apple")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_final_values_error() {
		let evaluator = create_evaluator();

		// Test TypeMismatch
		assert!(matches!(
			evaluator.compare_final_values(
				"bool",
				"true",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("123")
			),
			Err(EvaluationError::TypeMismatch(_))
		));
	}
}
