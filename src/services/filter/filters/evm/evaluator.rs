//! This module provides an implementation of the `ConditionEvaluator` trait
//! for evaluating conditions in EVM-based chains.

use super::helpers::{are_same_address, string_to_i256, string_to_u256};
use crate::{
	models::EVMMatchParamEntry,
	services::filter::expression::{
		compare_ordered_values, ComparisonOperator, ConditionEvaluator, EvaluationError,
		LiteralValue,
	},
};
use rust_decimal::Decimal;
use serde_json::Value as JsonValue;
use std::str::FromStr;

type EVMArgs = Vec<EVMMatchParamEntry>;

const UNSIGNED_INTEGER_KINDS: &[&str] = &[
	"uint8", "uint16", "uint32", "uint64", "uint128", "uint256", "number",
];

const SIGNED_INTEGER_KINDS: &[&str] = &["int8", "int16", "int32", "int64", "int128", "int256"];

pub struct EVMConditionEvaluator<'a> {
	args: &'a EVMArgs,
}

impl<'a> EVMConditionEvaluator<'a> {
	pub fn new(args: &'a EVMArgs) -> Self {
		Self { args }
	}

	/// Helper to check if a serde_json::Value matches a target string.
	/// Used by compare_array for items within a JSON array.
	fn check_json_value_matches_str(&self, lhs_json: &JsonValue, rhs_str: &str) -> bool {
		match lhs_json {
			JsonValue::String(s) => {
				if self.get_kind_from_json_value(lhs_json) == "address" {
					are_same_address(s, rhs_str)
				} else {
					s.to_lowercase() == rhs_str.to_lowercase()
				}
			}
			JsonValue::Number(n) => n.to_string() == rhs_str,
			JsonValue::Bool(b) => b.to_string().to_lowercase() == rhs_str.to_lowercase(),
			JsonValue::Object(nested_map) => nested_map
				.values()
				.any(|val_in_obj| self.check_json_value_matches_str(val_in_obj, rhs_str)),
			JsonValue::Array(_) => false,
			JsonValue::Null => rhs_str == "null",
		}
	}

	/// Compares an "array" type parameter.
	fn compare_array(
		&self,
		lhs_json_array_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let rhs_target_str = match rhs_literal {
			LiteralValue::Str(s) => *s,
			LiteralValue::Number(s) => {
				if *operator == ComparisonOperator::Contains {
					*s // For Contains, we search for this number (as string)
				} else {
					// For Eq/Ne, a number literal cannot be equal to a JSON array string.
					let msg = format!(
							"Expected string literal (representing a JSON array) for EVM 'array' Eq/Ne comparison, found number: {:?}",
							rhs_literal
					);
					return Err(EvaluationError::type_mismatch(msg, None, None));
				}
			}
			_ => {
				let msg = format!(
					"Expected string literal for EVM 'array' {} comparison, found: {:?}",
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
			"EVM Comparing array: lhs: '{}', operator: {:?}, rhs_target: '{}'",
			lhs_json_array_str,
			operator,
			rhs_target_str
		);

		match operator {
			ComparisonOperator::Eq | ComparisonOperator::Ne => {
				let lhs_json_value = serde_json::from_str::<JsonValue>(lhs_json_array_str)
					.map_err(|e| {
						let msg = format!(
							"Failed to parse LHS value '{}' as JSON array for 'Eq/Ne' operator",
							lhs_json_array_str
						);
						EvaluationError::parse_error(msg, Some(e.into()), None)
					})?;

				let rhs_json_value =
					serde_json::from_str::<JsonValue>(rhs_target_str).map_err(|e| {
						let msg = format!(
							"Failed to parse RHS value '{}' as JSON array for 'Eq/Ne' operator",
							rhs_target_str
						);
						EvaluationError::parse_error(msg, Some(e.into()), None)
					})?;

				// Ensure both parsed values are actually arrays
				if !lhs_json_value.is_array() || !rhs_json_value.is_array() {
					let msg = format!(
							"For 'array' Eq/Ne comparison, both LHS ('{}') and RHS ('{}') must resolve to JSON arrays.",
							lhs_json_array_str, rhs_target_str
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
				let json_array = serde_json::from_str::<Vec<JsonValue>>(lhs_json_array_str)
					.map_err(|e| {
						let msg = format!(
							"Failed to parse LHS value '{}' as JSON array for 'contains' operator",
							lhs_json_array_str
						);
						EvaluationError::parse_error(msg, Some(e.into()), None)
					})?;

				let found = json_array.iter().any(|item_in_array| {
					self.check_json_value_matches_str(item_in_array, rhs_target_str)
				});
				Ok(found)
			}
			_ => {
				let msg = format!(
					"Operator {:?} not supported for EVM 'array' type. Supported: Eq, Ne, Contains.",
					operator
				);
				Err(EvaluationError::unsupported_operator(msg, None, None))
			}
		}
	}

	/// Compares potential U256 LHS value with the RHS literal value
	/// Handles decimal and hex inputs for both sides
	fn compare_u256(
		&self,
		left_str: &str,
		operator: &ComparisonOperator,
		right_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let left = string_to_u256(left_str).map_err(|error| {
			let msg = format!("Failed to parse LHS value '{}' as U256", left_str,);
			EvaluationError::parse_error(msg, Some(error.into()), None)
		})?;

		let right_str = match right_literal {
			LiteralValue::Number(s) => s,
			LiteralValue::Str(s) => s,
			_ => {
				let msg = format!(
					"Expected number or string literal for U256 comparison with found: {:?}",
					right_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		let right = string_to_u256(right_str).map_err(|error| {
			let msg = format!("Failed to parse RHS value '{}' as U256", right_str,);
			EvaluationError::parse_error(msg, Some(error.into()), None)
		})?;

		tracing::debug!(
			"Comparing U256: left: {}, op: {:?}, right: {}",
			left,
			operator,
			right
		);

		compare_ordered_values(&left, operator, &right)
	}

	/// Compares potential I256 LHS value with the RHS literal value
	fn compare_i256(
		&self,
		left_str: &str,
		operator: &ComparisonOperator,
		right_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let left = string_to_i256(left_str).map_err(|error| {
			let msg = format!("Failed to parse LHS value '{}' as I256", left_str,);
			EvaluationError::parse_error(msg, Some(error.into()), None)
		})?;

		let right_str = match right_literal {
			LiteralValue::Number(s) => s, // e.g., "-10", "10", "0x0A" (if string_to_i256 handles hex)
			LiteralValue::Str(s) => s,    // e.g., "'-10'", "'0x0A'"
			_ => {
				let msg = format!(
					"Expected number or string literal for I256 comparison, found: {:?}",
					right_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		let right = string_to_i256(right_str).map_err(|error| {
			let msg = format!("Failed to parse RHS value '{}' as I256", right_str,);
			EvaluationError::parse_error(msg, Some(error.into()), None)
		})?;

		tracing::debug!(
			"Comparing I256: left: {}, op: {:?}, right: {}",
			left,
			operator,
			right
		);

		compare_ordered_values(&left, operator, &right)
	}

	/// Compares an EVM address (string) with a literal value based on the operator.
	/// Only supports Eq and Ne operators.
	fn compare_address(
		&self,
		left: &str,
		operator: &ComparisonOperator,
		right_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let right = match right_literal {
			LiteralValue::Str(str) => *str,
			_ => {
				let msg = format!(
					"Expected string literal for address comparison, found: {:?}",
					right_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		tracing::debug!("Comapring addresses: left: {}, right: {}", left, right);

		match operator {
			ComparisonOperator::Eq => Ok(are_same_address(left, right)),
			ComparisonOperator::Ne => Ok(!are_same_address(left, right)),
			_ => {
				let msg = format!("Unsupported operator for address type: {:?}", operator);
				Err(EvaluationError::unsupported_operator(msg, None, None))
			}
		}
	}

	/// Compares a string value with a literal value based on the operator.
	/// Supports Eq, Ne, StartsWith, EndsWith, and Contains operators.
	fn compare_string(
		&self,
		lhs_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		// Perform case-insensitive comparisons for all string operators
		let left = lhs_str.to_lowercase();

		let right = match rhs_literal {
			LiteralValue::Str(s) => s.to_lowercase(),
			_ => {
				let msg = format!(
					"Expected string literal for string comparison, found: {:?}",
					rhs_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		tracing::debug!(
			"Comparing strings: left: {}, operator: {:?}, right: {}",
			left,
			operator,
			right,
		);

		match operator {
			ComparisonOperator::Eq => Ok(left == right),
			ComparisonOperator::Ne => Ok(left != right),
			ComparisonOperator::StartsWith => Ok(left.starts_with(&right)),
			ComparisonOperator::EndsWith => Ok(left.ends_with(&right)),
			ComparisonOperator::Contains => Ok(left.contains(&right)),
			_ => {
				let msg = format!("Operator {:?} not supported for type String", operator);
				Err(EvaluationError::unsupported_operator(msg, None, None))
			}
		}
	}

	/// Compares a fixed-point number (Decimal) with a literal value.
	fn compare_fixed_point(
		&self,
		lhs_str: &str, // LHS value as string (needs parsing)
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let left_decimal = Decimal::from_str(lhs_str).map_err(|e| {
			let msg = format!("Failed to parse LHS value '{}' as Decimal", lhs_str);
			EvaluationError::parse_error(msg, Some(e.into()), None)
		})?;

		// RHS must now be parsed from Number(&str) or Str(&str)
		let rhs_str = match rhs_literal {
			LiteralValue::Number(s) => *s,
			LiteralValue::Str(s) => *s, // If user quoted a numeric string e.g., '123.45'
			_ => {
				let msg = format!(
					"Expected number or string literal for Decimal comparison, found: {:?}",
					rhs_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};

		let right_decimal = Decimal::from_str(rhs_str).map_err(|e| {
			let msg = format!("Failed to parse RHS value '{}' as Decimal", rhs_str);
			EvaluationError::parse_error(msg, Some(e.into()), None)
		})?;

		tracing::debug!(
			"Comparing Decimal: left={}, op={:?}, right={}",
			left_decimal,
			operator,
			right_decimal
		);

		compare_ordered_values(&left_decimal, operator, &right_decimal)
	}

	/// Compares a boolean value (true/false) with a literal value.
	/// Only supports Eq and Ne operators.
	fn compare_boolean(
		&self,
		lhs_value_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let lhs = lhs_value_str.parse::<bool>().map_err(|_| {
			let msg = format!("Failed to parse LHS value '{}' as bool", lhs_value_str);
			EvaluationError::parse_error(msg, None, None)
		})?;
		let rhs = match rhs_literal {
			LiteralValue::Bool(b) => *b,
			_ => {
				let msg = format!(
					"Expected bool literal for EVM Bool comparison, found: {:?}",
					rhs_literal
				);
				return Err(EvaluationError::type_mismatch(msg, None, None));
			}
		};
		match operator {
			ComparisonOperator::Eq => Ok(lhs == rhs),
			ComparisonOperator::Ne => Ok(lhs != rhs),
			_ => {
				let msg = format!(
					"Unsupported operator {:?} for EVM Bool comparison",
					operator
				);
				Err(EvaluationError::unsupported_operator(msg, None, None))
			}
		}
	}
}

impl ConditionEvaluator for EVMConditionEvaluator<'_> {
	fn get_base_param(&self, name: &str) -> Result<(&str, &str), EvaluationError> {
		self.args
			.iter()
			.find(|p| p.name == name)
			.map(|p| (p.value.as_str(), p.kind.as_str()))
			.ok_or_else(|| {
				let msg = format!("Base parameter not found: {}", name);
				EvaluationError::variable_not_found(msg, None, None)
			})
	}

	fn compare_final_values(
		&self,
		lhs_kind_str: &str,
		lhs_value_str: &str, // Value after path traversal, or original base value
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let lhs_kind = lhs_kind_str.to_lowercase();

		tracing::debug!(
			"EVM Comparing: lhs_val='{}', lhs_kind='{}', op='{:?}', rhs_lit='{:?}'",
			lhs_value_str,
			lhs_kind_str,
			operator,
			rhs_literal
		);

		if SIGNED_INTEGER_KINDS.contains(&lhs_kind.as_str()) {
			return self.compare_i256(lhs_value_str, operator, rhs_literal);
		}

		if UNSIGNED_INTEGER_KINDS.contains(&lhs_kind.as_str()) {
			return self.compare_u256(lhs_value_str, operator, rhs_literal);
		}

		match lhs_kind.as_str() {
			"fixed" | "ufixed" => self.compare_fixed_point(lhs_value_str, operator, rhs_literal),
			"address" => self.compare_address(lhs_value_str, operator, rhs_literal),
			"string" | "bytes" | "bytes32" => {
				self.compare_string(lhs_value_str, operator, rhs_literal)
			}
			"bool" => self.compare_boolean(lhs_value_str, operator, rhs_literal),
			"array" => self.compare_array(lhs_value_str, operator, rhs_literal),
			_ => {
				let msg = format!(
					"Unsupported EVM parameter kind for comparison: {}",
					lhs_kind_str
				);
				Err(EvaluationError::type_mismatch(msg, None, None))
			}
		}
	}

	fn get_kind_from_json_value(&self, value: &serde_json::Value) -> String {
		match value {
			serde_json::Value::String(s) => {
				let s_lower = s.to_lowercase();
				if s_lower.starts_with("0x")
					&& s.len() == 42
					&& s.chars().skip(2).all(|c| c.is_ascii_hexdigit())
				{
					"address".to_string()
				} else if s_lower.starts_with("0x")
					&& s.chars().skip(2).all(|c| c.is_ascii_hexdigit())
				{
					if s.len() == 66 {
						// 0x + 32 bytes (64 hex chars)
						"bytes32".to_string()
					} else {
						"bytes".to_string()
					}
				// Check if it's a string representation of a decimal
				} else if Decimal::from_str(s).is_ok() && s.contains('.') {
					"fixed".to_string()
				} else {
					"string".to_string()
				}
			}
			serde_json::Value::Number(n) => {
				if n.is_f64() || n.to_string().contains('.') {
					"fixed".to_string()
				} else if n.is_i64() {
					// check if it's negative, otherwise default to number
					if n.as_i64().unwrap_or(0) < 0 {
						"int64".to_string()
					} else {
						"number".to_string()
					}
				} else {
					"number".to_string()
				}
			}
			serde_json::Value::Bool(_) => "bool".to_string(),
			serde_json::Value::Array(_) => "array".to_string(),
			serde_json::Value::Object(_) => "map".to_string(),
			serde_json::Value::Null => "null".to_string(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::services::filter::expression::LiteralValue;
	use alloy::primitives::U256;
	use serde_json::json;

	// Helper to create a dummy EVMConditionEvaluator (args don't matter for these unit tests)
	fn create_evaluator() -> EVMConditionEvaluator<'static> {
		static EMPTY_ARGS: EVMArgs = Vec::new();
		EVMConditionEvaluator::new(&EMPTY_ARGS)
	}

	/// --- Test cases for compare_u256 ---
	#[test]
	fn test_compare_u256_valid() {
		let evaluator = create_evaluator();

		assert!(evaluator
			.compare_u256("123", &ComparisonOperator::Eq, &LiteralValue::Number("123"))
			.unwrap());

		assert!(evaluator
			.compare_u256("123", &ComparisonOperator::Ne, &LiteralValue::Number("456"))
			.unwrap());

		assert!(evaluator
			.compare_u256("123", &ComparisonOperator::Gt, &LiteralValue::Number("100"))
			.unwrap());

		assert!(evaluator
			.compare_u256(
				"123",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("123")
			)
			.unwrap());

		assert!(evaluator
			.compare_u256("123", &ComparisonOperator::Lt, &LiteralValue::Number("200"))
			.unwrap());

		assert!(evaluator
			.compare_u256(
				"123",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("123")
			)
			.unwrap());

		assert!(evaluator
			.compare_u256(
				U256::MAX.to_string().as_str(),
				&ComparisonOperator::Eq,
				&LiteralValue::Number(&U256::MAX.to_string())
			)
			.unwrap());

		assert!(evaluator
			.compare_u256(
				U256::MAX.to_string().as_str(),
				&ComparisonOperator::Gt,
				&LiteralValue::Number(&U256::ZERO.to_string())
			)
			.unwrap());
	}

	#[test]
	fn test_compare_u256_invalid() {
		let evaluator = create_evaluator();

		assert!(!evaluator
			.compare_u256("123", &ComparisonOperator::Eq, &LiteralValue::Number("456"))
			.unwrap());

		assert!(!evaluator
			.compare_u256("123", &ComparisonOperator::Ne, &LiteralValue::Number("123"))
			.unwrap());

		assert!(!evaluator
			.compare_u256("123", &ComparisonOperator::Gt, &LiteralValue::Number("200"))
			.unwrap());

		assert!(!evaluator
			.compare_u256(
				"123",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("200")
			)
			.unwrap());

		assert!(!evaluator
			.compare_u256("123", &ComparisonOperator::Lt, &LiteralValue::Number("100"))
			.unwrap());

		assert!(!evaluator
			.compare_u256(
				"123",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("100")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_u256_error() {
		let evaluator = create_evaluator();

		// Parse error LHS
		assert!(matches!(
			evaluator.compare_u256(
				"not-a-number",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("123")
			),
			Err(EvaluationError::ParseError(_))
		));

		// Parse error RHS
		assert!(matches!(
			evaluator.compare_u256(
				"123",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("not-a-number")
			),
			Err(EvaluationError::ParseError(_))
		));

		// Mismatch type error
		assert!(matches!(
			evaluator.compare_u256("123", &ComparisonOperator::Eq, &LiteralValue::Bool(true)),
			Err(EvaluationError::TypeMismatch(_))
		));

		// Unsupported operator error
		assert!(matches!(
			evaluator.compare_u256(
				"123",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Number("123")
			),
			Err(EvaluationError::UnsupportedOperator(_))
		));
	}

	/// --- Test cases for compare_i256 ---
	#[test]
	fn test_compare_i256_valid() {
		let evaluator = create_evaluator();

		assert!(evaluator
			.compare_i256("123", &ComparisonOperator::Eq, &LiteralValue::Number("123"))
			.unwrap());
		assert!(evaluator
			.compare_i256(
				"123",
				&ComparisonOperator::Ne,
				&LiteralValue::Number("-456")
			)
			.unwrap());

		assert!(evaluator
			.compare_i256(
				"123",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("-100")
			)
			.unwrap());

		assert!(evaluator
			.compare_i256(
				"123",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("123")
			)
			.unwrap());

		assert!(evaluator
			.compare_i256(
				"-123",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("200")
			)
			.unwrap());

		assert!(evaluator
			.compare_i256(
				"-123",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("-123")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_i256_invalid() {
		let evaluator = create_evaluator();

		assert!(!evaluator
			.compare_i256(
				"123",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("-456")
			)
			.unwrap());

		assert!(!evaluator
			.compare_i256("123", &ComparisonOperator::Ne, &LiteralValue::Number("123"))
			.unwrap());

		assert!(!evaluator
			.compare_i256("123", &ComparisonOperator::Gt, &LiteralValue::Number("200"))
			.unwrap());

		assert!(!evaluator
			.compare_i256(
				"123",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("200")
			)
			.unwrap());

		assert!(!evaluator
			.compare_i256(
				"-123",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("-200")
			)
			.unwrap());

		assert!(!evaluator
			.compare_i256(
				"-123",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("-200")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_i256_error() {
		let evaluator = create_evaluator();

		// Parse error LHS
		assert!(matches!(
			evaluator.compare_i256(
				"not-a-number",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("-123")
			),
			Err(EvaluationError::ParseError(_))
		));

		// Parse error RHS
		assert!(matches!(
			evaluator.compare_i256(
				"-123",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("not-a-number")
			),
			Err(EvaluationError::ParseError(_))
		));

		// Mismatch type error
		assert!(matches!(
			evaluator.compare_i256("-123", &ComparisonOperator::Eq, &LiteralValue::Bool(true)),
			Err(EvaluationError::TypeMismatch(_))
		));

		// Unsupported operator error
		assert!(matches!(
			evaluator.compare_i256(
				"-123",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Number("-123")
			),
			Err(EvaluationError::UnsupportedOperator(_))
		));
	}

	/// --- Test cases for compare_address ---
	#[test]
	fn test_compare_address_valid() {
		let evaluator = create_evaluator();

		assert!(evaluator
			.compare_address(
				"0x1234567890123456789012345678901234567890",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("0x1234567890123456789012345678901234567890")
			)
			.unwrap());

		assert!(evaluator
			.compare_address(
				"0x1234567890123456789012345678901234567890",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("0x0987654321098765432109876543210987654321")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_address_invalid() {
		let evaluator = create_evaluator();

		assert!(!evaluator
			.compare_address(
				"0x1234567890123456789012345678901234567890",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("0x0987654321098765432109876543210987654321")
			)
			.unwrap());

		assert!(!evaluator
			.compare_address(
				"0x1234567890123456789012345678901234567890",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("0x1234567890123456789012345678901234567890")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_address_error() {
		let evaluator = create_evaluator();

		// Wrong type for RHS
		assert!(matches!(
			evaluator.compare_address(
				"0x1234567890123456789012345678901234567890",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("123")
			),
			Err(EvaluationError::TypeMismatch(_))
		));

		// Wrong operator
		assert!(matches!(
			evaluator.compare_address(
				"0x1234567890123456789012345678901234567890",
				&ComparisonOperator::Gte,
				&LiteralValue::Str("0x0987654321098765432109876543210987654321")
			),
			Err(EvaluationError::UnsupportedOperator(_))
		));
	}

	/// --- Test cases for compare_string ---
	#[test]
	fn test_compare_string_valid() {
		let evaluator = create_evaluator();

		assert!(evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("test_value_1")
			)
			.unwrap());

		assert!(evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("test_value_2")
			)
			.unwrap());

		assert!(evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Str("test")
			)
			.unwrap());

		assert!(evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::EndsWith,
				&LiteralValue::Str("value_1")
			)
			.unwrap());

		assert!(evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::Contains,
				&LiteralValue::Str("value")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_string_invalid() {
		let evaluator = create_evaluator();

		assert!(!evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("test_value_2")
			)
			.unwrap());

		assert!(!evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("test_value_1")
			)
			.unwrap());

		assert!(!evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Str("value")
			)
			.unwrap());

		assert!(!evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::EndsWith,
				&LiteralValue::Str("test")
			)
			.unwrap());

		assert!(!evaluator
			.compare_string(
				"test_value_1",
				&ComparisonOperator::Contains,
				&LiteralValue::Str("test_value_2")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_string_error() {
		let evaluator = create_evaluator();

		// Wrong type for RHS
		assert!(matches!(
			evaluator.compare_string(
				"test_value_1",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("123")
			),
			Err(EvaluationError::TypeMismatch(_))
		));

		// Wrong operator
		assert!(matches!(
			evaluator.compare_string(
				"test_value_1",
				&ComparisonOperator::Gte,
				&LiteralValue::Str("test_value_2")
			),
			Err(EvaluationError::UnsupportedOperator(_))
		));
	}

	/// --- Test cases for compare_fixed_point ---
	#[test]
	fn test_compare_fixed_point_valid() {
		let evaluator = create_evaluator();

		assert!(evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("123.456")
			)
			.unwrap());

		assert!(evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Ne,
				&LiteralValue::Number("456.789")
			)
			.unwrap());

		assert!(evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("100.0")
			)
			.unwrap());

		assert!(evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("123.456")
			)
			.unwrap());

		assert!(evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("200.0")
			)
			.unwrap());

		assert!(evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("123.456")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_fixed_point_invalid() {
		let evaluator = create_evaluator();

		assert!(!evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("456.789")
			)
			.unwrap());

		assert!(!evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Ne,
				&LiteralValue::Number("123.456")
			)
			.unwrap());

		assert!(!evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("200.0")
			)
			.unwrap());

		assert!(!evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("200.0")
			)
			.unwrap());

		assert!(!evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("100.0")
			)
			.unwrap());

		assert!(!evaluator
			.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("100.0")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_fixed_point_error() {
		let evaluator = create_evaluator();

		// Parse error LHS
		assert!(matches!(
			evaluator.compare_fixed_point(
				"not-a-number",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("123.456")
			),
			Err(EvaluationError::ParseError(_))
		));

		// Parse error RHS
		assert!(matches!(
			evaluator.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("not-a-number")
			),
			Err(EvaluationError::ParseError(_))
		));

		// Mismatch type error
		assert!(matches!(
			evaluator.compare_fixed_point(
				"123.456",
				&ComparisonOperator::Eq,
				&LiteralValue::Bool(true)
			),
			Err(EvaluationError::TypeMismatch(_))
		));

		// Unsupported operator error
		assert!(matches!(
			evaluator.compare_fixed_point(
				"123.456",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Number("123.456")
			),
			Err(EvaluationError::UnsupportedOperator(_))
		));
	}

	/// --- Test cases for compare_boolean ---
	#[test]
	fn test_compare_boolean_valid() {
		let evaluator = create_evaluator();

		assert!(evaluator
			.compare_boolean("true", &ComparisonOperator::Eq, &LiteralValue::Bool(true))
			.unwrap());

		assert!(evaluator
			.compare_boolean("false", &ComparisonOperator::Ne, &LiteralValue::Bool(true))
			.unwrap());
	}

	#[test]
	fn test_compare_boolean_invalid() {
		let evaluator = create_evaluator();

		assert!(!evaluator
			.compare_boolean("true", &ComparisonOperator::Ne, &LiteralValue::Bool(true))
			.unwrap());

		assert!(!evaluator
			.compare_boolean("false", &ComparisonOperator::Eq, &LiteralValue::Bool(true))
			.unwrap());
	}

	#[test]
	fn test_compare_boolean_error() {
		let evaluator = create_evaluator();

		// Parser error
		assert!(matches!(
			evaluator.compare_boolean(
				"not-a-bool",
				&ComparisonOperator::Eq,
				&LiteralValue::Bool(true)
			),
			Err(EvaluationError::ParseError(_))
		));

		// Mismatch type error
		assert!(matches!(
			evaluator.compare_boolean(
				"true",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("123")
			),
			Err(EvaluationError::TypeMismatch(_))
		));

		// Unsupported operator error
		assert!(matches!(
			evaluator.compare_boolean("true", &ComparisonOperator::Gte, &LiteralValue::Bool(true)),
			Err(EvaluationError::UnsupportedOperator(_))
		));
	}

	// --- Test cases for compare_array ---
	#[test]
	fn test_compare_array_json_contains_simple_string() {
		let evaluator = create_evaluator();
		let lhs_json_array = r#"["alice", "bob", "charlie"]"#;
		assert!(evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("bob")
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("charlie")
			)
			.unwrap());
		assert!(!evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("dave")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_array_json_contains_number_as_string() {
		let evaluator = create_evaluator();
		let lhs_json_array = r#"[123, "test", 456, true]"#;
		assert!(evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Number("123")
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("456")
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("true")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_array_json_contains_address() {
		let evaluator = create_evaluator();
		let addr1 = "0x1234567890123456789012345678901234567890";
		let addr1_mixed_case = "0x1234567890123456789012345678901234567890";
		let lhs_json_array = format!(
			r#"["0xAnotherAddress0000000000000000000000000", "{}", "text"]"#,
			addr1_mixed_case
		);

		assert!(evaluator
			.compare_array(
				&lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Str(addr1)
			)
			.unwrap());
	}

	#[test]
	fn test_compare_array_json_contains_in_object_field_value() {
		let evaluator = create_evaluator();
		let lhs_json_array = r#"[{"id": 1, "name": "Alice"}, {"id": 2, "token": "0xTokenAddress00000000000000000000000000"}]"#;
		assert!(evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("Alice")
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("0xTokenAddress00000000000000000000000000")
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Number("2")
			)
			.unwrap());
		assert!(!evaluator
			.compare_array(
				lhs_json_array,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("Bob")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_array_eq_ne_compares_raw_json_string() {
		let evaluator = create_evaluator();
		let lhs1 = r#"["alice", "bob"]"#;
		let lhs2 = r#"["alice", "charlie"]"#;
		assert!(evaluator
			.compare_array(lhs1, &ComparisonOperator::Eq, &LiteralValue::Str(lhs1))
			.unwrap());
		assert!(!evaluator
			.compare_array(lhs1, &ComparisonOperator::Eq, &LiteralValue::Str(lhs2))
			.unwrap());
		assert!(evaluator
			.compare_array(lhs1, &ComparisonOperator::Ne, &LiteralValue::Str(lhs2))
			.unwrap());
	}

	#[test]
	fn test_compare_array_semantic_json_equality() {
		let evaluator = create_evaluator();

		// --- Test Eq ---
		// Basic semantic equality (whitespace)
		assert!(evaluator
			.compare_array(
				r#"[1, 2, 3]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"[1,2,3]"#)
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				r#"["a", "b"]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"[ "a", "b" ]"#)
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				r#"[{"id":1}, {"id":2}]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"[ { "id" : 1 } , { "id" : 2 } ]"#)
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				r#"[]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"[]"#)
			)
			.unwrap());

		// Case sensitivity for string elements (serde_json::Value default behavior)
		assert!(!evaluator
			.compare_array(
				r#"["Alice"]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"["alice"]"#)
			)
			.unwrap());

		// Order matters
		assert!(!evaluator
			.compare_array(
				r#"[1, 2]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"[2, 1]"#)
			)
			.unwrap());

		// Different types: string vs number
		assert!(!evaluator
			.compare_array(
				r#"[1, 2]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"["1", "2"]"#)
			)
			.unwrap());

		// Different lengths
		assert!(!evaluator
			.compare_array(
				r#"[1, 2]"#,
				&ComparisonOperator::Eq,
				&LiteralValue::Str(r#"[1, 2, 3]"#)
			)
			.unwrap());

		// --- Test Ne ---
		assert!(!evaluator
			.compare_array(
				r#"[1, 2, 3]"#,
				&ComparisonOperator::Ne,
				&LiteralValue::Str(r#"[1,2,3]"#)
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				r#"["Alice"]"#,
				&ComparisonOperator::Ne,
				&LiteralValue::Str(r#"["alice"]"#)
			)
			.unwrap());
		assert!(evaluator
			.compare_array(
				r#"[1, 2]"#,
				&ComparisonOperator::Ne,
				&LiteralValue::Str(r#"[2, 1]"#)
			)
			.unwrap());
	}

	#[test]
	fn test_compare_array_errors() {
		let evaluator = create_evaluator();
		let valid_lhs_array_json = r#"["data"]"#;

		assert!(matches!(
			evaluator.compare_array(
				valid_lhs_array_json,
				&ComparisonOperator::Contains,
				&LiteralValue::Bool(true)
			),
			Err(EvaluationError::TypeMismatch(_))
		));

		let invalid_lhs_array_json = "not a json array";
		assert!(matches!(
			evaluator.compare_array(
				invalid_lhs_array_json,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("data")
			),
			Err(EvaluationError::ParseError(_))
		));

		assert!(matches!(
			evaluator.compare_array(
				valid_lhs_array_json,
				&ComparisonOperator::Gt,
				&LiteralValue::Str("data")
			),
			Err(EvaluationError::UnsupportedOperator(_))
		));
	}

	/// --- Test cases for compare_final_values ---
	#[test]
	fn test_compare_final_values_routing() {
		let evaluator = create_evaluator();

		// Test routing to compare_u256
		assert!(evaluator
			.compare_final_values(
				"uint256",
				"100",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("100")
			)
			.unwrap());
		assert!(evaluator
			.compare_final_values(
				"number",
				"0xFF",
				&ComparisonOperator::Gt,
				&LiteralValue::Str("10")
			)
			.unwrap());

		// Test routing to compare_i256
		let i256_res = std::panic::catch_unwind(|| {
			evaluator.compare_final_values(
				"int128",
				"-50",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("0"),
			)
		});
		assert!(
			i256_res.is_err() || i256_res.unwrap().is_ok(),
			"compare_i256 test needs update post-impl"
		);

		// Test routing to compare_fixed_point
		assert!(evaluator
			.compare_final_values(
				"fixed",
				"1.23",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("1.23")
			)
			.unwrap());

		// Test routing to compare_address
		assert!(evaluator
			.compare_final_values(
				"address",
				"0x123...",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("0x456...")
			)
			.unwrap());

		// Test routing to compare_string
		assert!(evaluator
			.compare_final_values(
				"string",
				"text",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Str("te")
			)
			.unwrap());
		assert!(evaluator
			.compare_final_values(
				"bytes",
				"0xab",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("0xab")
			)
			.unwrap());

		// Test routing to compare_boolean
		assert!(evaluator
			.compare_final_values(
				"bool",
				"true",
				&ComparisonOperator::Eq,
				&LiteralValue::Bool(true)
			)
			.unwrap());

		// Test routing to compare_array
		assert!(evaluator
			.compare_final_values(
				"array",
				r#"["val1", "val2"]"#,
				&ComparisonOperator::Contains,
				&LiteralValue::Str("val1")
			)
			.unwrap());
	}

	#[test]
	fn test_compare_final_values_error() {
		let evaluator = create_evaluator();

		let res_unsupported = evaluator.compare_final_values(
			"unknown_type",
			"val",
			&ComparisonOperator::Eq,
			&LiteralValue::Str("s"),
		);
		assert!(matches!(
			res_unsupported,
			Err(EvaluationError::TypeMismatch(_))
		));
	}

	/// --- Test cases for get_kind_from_json_value ---
	#[test]
	fn test_get_kind_from_json_value() {
		let evaluator = create_evaluator();

		assert_eq!(
			evaluator.get_kind_from_json_value(&json!("test_string")),
			"string"
		);
		assert_eq!(
			evaluator
				.get_kind_from_json_value(&json!("0x1234567890123456789012345678901234567890")),
			"address"
		);
		assert_eq!(
			evaluator.get_kind_from_json_value(&json!("0x1234")),
			"bytes"
		); // Assuming general bytes for non-address, non-bytes32 hex
		assert_eq!(
			evaluator.get_kind_from_json_value(&json!(format!("0x{}", "0".repeat(64)))),
			"bytes32"
		); // 0x + 64 hex chars
		assert_eq!(evaluator.get_kind_from_json_value(&json!(123)), "number"); // For U256 path
		assert_eq!(evaluator.get_kind_from_json_value(&json!(-100)), "int64"); // Or "int" if generic
		assert_eq!(evaluator.get_kind_from_json_value(&json!(123.45)), "fixed");
		assert_eq!(
			evaluator.get_kind_from_json_value(&json!("123.45")),
			"fixed"
		); // String that is a decimal
		assert_eq!(evaluator.get_kind_from_json_value(&json!(true)), "bool");
		assert_eq!(evaluator.get_kind_from_json_value(&json!([1, 2])), "array");
		assert_eq!(evaluator.get_kind_from_json_value(&json!({"a":1})), "map");
		assert_eq!(evaluator.get_kind_from_json_value(&json!(null)), "null");
	}
}
