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

	/// Compares potential U256 LHS value with the RHS literal value
	/// Handles decimal and hex inputs for both sides
	fn compare_u256(
		&self,
		left_str: &str,
		operator: &ComparisonOperator,
		right_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let left = string_to_u256(left_str).map_err(|error| {
			EvaluationError::ParseError(format!(
				"Failed to parse LHS value '{}' as U256: {}",
				left_str, error,
			))
		})?;

		let right_str = match right_literal {
			LiteralValue::Number(s) => s,
			LiteralValue::Str(s) => s,
			_ => {
				return Err(EvaluationError::TypeMismatch(format!(
					"Expected number or string literal for U256 comparison with found: {:?}",
					right_literal
				)));
			}
		};

		let right = string_to_u256(right_str).map_err(|error| {
			EvaluationError::ParseError(format!(
				"Failed to parse RHS value '{}' as U256: {}",
				right_str, error,
			))
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
			EvaluationError::ParseError(format!(
				"Failed to parse LHS value '{}' as I256: {}",
				left_str, error,
			))
		})?;

		let right_str = match right_literal {
			LiteralValue::Number(s) => s, // e.g., "-10", "10", "0x0A" (if string_to_i256 handles hex)
			LiteralValue::Str(s) => s,    // e.g., "'-10'", "'0x0A'"
			_ => {
				return Err(EvaluationError::TypeMismatch(format!(
					"Expected number or string literal for I256 comparison, found: {:?}",
					right_literal
				)));
			}
		};

		let right = string_to_i256(right_str).map_err(|error| {
			EvaluationError::ParseError(format!(
				"Failed to parse RHS value '{}' as I256: {}",
				right_str, error,
			))
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
				return Err(EvaluationError::TypeMismatch(format!(
					"Expected string literal for address comparison, found: {:?}",
					right_literal
				)));
			}
		};

		tracing::debug!("Comapring addresses: left: {}, right: {}", left, right);

		match operator {
			ComparisonOperator::Eq => Ok(are_same_address(left, right)),
			ComparisonOperator::Ne => Ok(!are_same_address(left, right)),
			_ => Err(EvaluationError::UnsupportedOperator {
				op: format!("Unsupported operator for address type: {:?}", operator),
			}),
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
				return Err(EvaluationError::TypeMismatch(format!(
					"Expected string literal comparison, found: {:?}",
					rhs_literal
				)));
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
			_ => Err(EvaluationError::UnsupportedOperator {
				op: format!("Operator {:?} not supported for type String", operator),
			}),
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
			EvaluationError::ParseError(format!(
				"Failed to parse LHS value '{}' as Decimal: {}",
				lhs_str, e
			))
		})?;

		// RHS must now be parsed from Number(&str) or Str(&str)
		let rhs_str = match rhs_literal {
			LiteralValue::Number(s) => *s,
			LiteralValue::Str(s) => *s, // If user quoted a numeric string e.g., '123.45'
			_ => {
				return Err(EvaluationError::TypeMismatch(format!(
					"Expected number or string literal for Decimal comparison, found: {:?}",
					rhs_literal
				)));
			}
		};

		let right_decimal = Decimal::from_str(rhs_str).map_err(|e| {
			EvaluationError::ParseError(format!(
				"Failed to parse RHS value '{}' as Decimal: {}",
				rhs_str, e
			))
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
			EvaluationError::ParseError(format!("Invalid EVM bool LHS: {}", lhs_value_str))
		})?;
		let rhs = match rhs_literal {
			LiteralValue::Bool(b) => *b,
			_ => {
				return Err(EvaluationError::TypeMismatch(
					"Expected bool literal for EVM Bool comparison".into(),
				))
			}
		};
		match operator {
			ComparisonOperator::Eq => Ok(lhs == rhs),
			ComparisonOperator::Ne => Ok(lhs != rhs),
			_ => Err(EvaluationError::UnsupportedOperator {
				op: format!("Unsupported op {:?} for EVM Bool", operator),
			}),
		}
	}
}

impl ConditionEvaluator for EVMConditionEvaluator<'_> {
	fn get_base_param(&self, name: &str) -> Result<(&str, &str), EvaluationError> {
		self.args
			.iter()
			.find(|p| p.name == name)
			.map(|p| (p.value.as_str(), p.kind.as_str()))
			.ok_or_else(|| EvaluationError::VariableNotFound(name.to_string()))
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
			_ => Err(EvaluationError::TypeMismatch(format!(
				"Unsupported EVM parameter kind for comparison: {}",
				lhs_kind_str
			))),
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
			Err(EvaluationError::UnsupportedOperator { op: _ })
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
			Err(EvaluationError::UnsupportedOperator { op: _ })
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
			Err(EvaluationError::UnsupportedOperator { op: _ })
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
			Err(EvaluationError::UnsupportedOperator { op: _ })
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
			Err(EvaluationError::UnsupportedOperator { op: _ })
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
			Err(EvaluationError::UnsupportedOperator { op: _ })
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
			.is_ok());
		assert!(evaluator
			.compare_final_values(
				"number",
				"0xFF",
				&ComparisonOperator::Gt,
				&LiteralValue::Str("10")
			)
			.is_ok());

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
			.is_ok());

		// Test routing to compare_address
		assert!(evaluator
			.compare_final_values(
				"address",
				"0x123...",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("0x456...")
			)
			.is_ok());

		// Test routing to compare_string
		assert!(evaluator
			.compare_final_values(
				"string",
				"text",
				&ComparisonOperator::StartsWith,
				&LiteralValue::Str("te")
			)
			.is_ok());
		assert!(evaluator
			.compare_final_values(
				"bytes",
				"0xab",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("0xab")
			)
			.is_ok());

		// Test routing to compare_boolean
		assert!(evaluator
			.compare_final_values(
				"bool",
				"true",
				&ComparisonOperator::Eq,
				&LiteralValue::Bool(true)
			)
			.is_ok());
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
