//! This module provides the `SolanaConditionEvaluator` struct, which implements
//! the `ConditionEvaluator` trait for evaluating conditions in Solana-based chains.

use crate::{
	models::SolanaMatchParamEntry,
	services::filter::expression::{
		compare_ordered_values, ComparisonOperator, ConditionEvaluator, EvaluationError,
		LiteralValue,
	},
};

pub type SolanaArgs = [SolanaMatchParamEntry];

/// Evaluator for Solana condition expressions.
///
/// This evaluator handles the evaluation of filter expressions against
/// Solana transaction and instruction data.
pub struct SolanaConditionEvaluator<'a> {
	args: &'a SolanaArgs,
}

impl<'a> SolanaConditionEvaluator<'a> {
	/// Creates a new SolanaConditionEvaluator with the given arguments.
	pub fn new(args: &'a SolanaArgs) -> Self {
		Self { args }
	}

	/// Gets the parameter by name.
	fn get_param(&self, name: &str) -> Option<&SolanaMatchParamEntry> {
		self.args.iter().find(|p| p.name == name)
	}

	/// Compares string/pubkey values
	fn compare_string(
		&self,
		lhs_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let rhs_str = match rhs_literal {
			LiteralValue::Str(s) => *s,
			LiteralValue::Number(n) => *n,
			_ => {
				return Err(EvaluationError::type_mismatch(
					format!("Expected string for comparison, got {:?}", rhs_literal),
					None,
					None,
				))
			}
		};

		match operator {
			ComparisonOperator::Eq => Ok(lhs_str == rhs_str),
			ComparisonOperator::Ne => Ok(lhs_str != rhs_str),
			ComparisonOperator::Contains => Ok(lhs_str.contains(rhs_str)),
			_ => Err(EvaluationError::type_mismatch(
				format!(
					"Operator {:?} not supported for string comparison",
					operator
				),
				None,
				None,
			)),
		}
	}

	/// Compares boolean values
	fn compare_bool(
		&self,
		lhs_str: &str,
		operator: &ComparisonOperator,
		rhs_literal: &LiteralValue<'_>,
	) -> Result<bool, EvaluationError> {
		let lhs_bool = lhs_str.parse::<bool>().map_err(|_| {
			EvaluationError::type_mismatch(
				format!("Cannot parse '{}' as boolean", lhs_str),
				None,
				None,
			)
		})?;

		let rhs_bool = match rhs_literal {
			LiteralValue::Bool(b) => *b,
			LiteralValue::Str(s) => s.parse::<bool>().map_err(|_| {
				EvaluationError::type_mismatch(
					format!("Cannot parse '{}' as boolean", s),
					None,
					None,
				)
			})?,
			_ => {
				return Err(EvaluationError::type_mismatch(
					format!("Expected boolean for comparison, got {:?}", rhs_literal),
					None,
					None,
				))
			}
		};

		match operator {
			ComparisonOperator::Eq => Ok(lhs_bool == rhs_bool),
			ComparisonOperator::Ne => Ok(lhs_bool != rhs_bool),
			_ => Err(EvaluationError::type_mismatch(
				format!(
					"Operator {:?} not supported for boolean comparison",
					operator
				),
				None,
				None,
			)),
		}
	}
}

impl ConditionEvaluator for SolanaConditionEvaluator<'_> {
	/// Gets the raw string value and kind for a base variable name
	fn get_base_param(&self, name: &str) -> Result<(&str, &str), EvaluationError> {
		let param = self.get_param(name).ok_or_else(|| {
			EvaluationError::type_mismatch(format!("Unknown parameter: {}", name), None, None)
		})?;

		Ok((&param.value, &param.kind))
	}

	/// Performs the final comparison between the left resolved value and the literal value
	fn compare_final_values(
		&self,
		left_kind: &str,
		left_resolved_value: &str,
		operator: &ComparisonOperator,
		right_literal: &LiteralValue,
	) -> Result<bool, EvaluationError> {
		match left_kind.to_lowercase().as_str() {
			// Unsigned numeric types
			"u8" | "u16" | "u32" | "u64" | "u128" => {
				let rhs_str = match right_literal {
					LiteralValue::Number(n) => *n,
					LiteralValue::Str(s) => *s,
					_ => {
						return Err(EvaluationError::type_mismatch(
							format!("Expected number for comparison, got {:?}", right_literal),
							None,
							None,
						))
					}
				};
				let left: u128 = left_resolved_value.parse().map_err(|_| {
					EvaluationError::type_mismatch(
						format!("Cannot parse '{}' as unsigned integer", left_resolved_value),
						None,
						None,
					)
				})?;
				let right: u128 = rhs_str.parse().map_err(|_| {
					EvaluationError::type_mismatch(
						format!("Cannot parse '{}' as unsigned integer", rhs_str),
						None,
						None,
					)
				})?;
				compare_ordered_values(&left, operator, &right)
			}
			// Signed numeric types
			"i8" | "i16" | "i32" | "i64" | "i128" => {
				let rhs_str = match right_literal {
					LiteralValue::Number(n) => *n,
					LiteralValue::Str(s) => *s,
					_ => {
						return Err(EvaluationError::type_mismatch(
							format!("Expected number for comparison, got {:?}", right_literal),
							None,
							None,
						))
					}
				};
				let left: i128 = left_resolved_value.parse().map_err(|_| {
					EvaluationError::type_mismatch(
						format!("Cannot parse '{}' as signed integer", left_resolved_value),
						None,
						None,
					)
				})?;
				let right: i128 = rhs_str.parse().map_err(|_| {
					EvaluationError::type_mismatch(
						format!("Cannot parse '{}' as signed integer", rhs_str),
						None,
						None,
					)
				})?;
				compare_ordered_values(&left, operator, &right)
			}
			// Boolean type
			"bool" => self.compare_bool(left_resolved_value, operator, right_literal),
			// String/pubkey types
			"pubkey" | "string" | "bytes" => {
				self.compare_string(left_resolved_value, operator, right_literal)
			}
			// Default case for unknown types
			_ => self.compare_string(left_resolved_value, operator, right_literal),
		}
	}

	/// Gets the chain-specific kind of a value from a JSON value
	fn get_kind_from_json_value(&self, value: &serde_json::Value) -> String {
		match value {
			serde_json::Value::Bool(_) => "bool".to_string(),
			serde_json::Value::Number(n) => {
				if n.is_i64() {
					"i64".to_string()
				} else if n.is_u64() {
					"u64".to_string()
				} else {
					"f64".to_string()
				}
			}
			serde_json::Value::String(_) => "string".to_string(),
			serde_json::Value::Array(_) => "vec".to_string(),
			serde_json::Value::Object(_) => "object".to_string(),
			serde_json::Value::Null => "null".to_string(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn create_test_args() -> Vec<SolanaMatchParamEntry> {
		vec![
			SolanaMatchParamEntry {
				name: "amount".to_string(),
				value: "1000000".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			SolanaMatchParamEntry {
				name: "recipient".to_string(),
				value: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				kind: "pubkey".to_string(),
				indexed: false,
			},
			SolanaMatchParamEntry {
				name: "is_initialized".to_string(),
				value: "true".to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
		]
	}

	#[test]
	fn test_get_base_param() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		let (value, kind) = evaluator.get_base_param("amount").unwrap();
		assert_eq!(value, "1000000");
		assert_eq!(kind, "u64");

		let (value, kind) = evaluator.get_base_param("recipient").unwrap();
		assert_eq!(value, "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
		assert_eq!(kind, "pubkey");
	}

	#[test]
	fn test_numeric_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Equal
		assert!(evaluator
			.compare_final_values(
				"u64",
				"1000000",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("1000000")
			)
			.unwrap());

		// Not equal
		assert!(evaluator
			.compare_final_values(
				"u64",
				"1000000",
				&ComparisonOperator::Ne,
				&LiteralValue::Number("500000")
			)
			.unwrap());

		// Greater than
		assert!(evaluator
			.compare_final_values(
				"u64",
				"1000000",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("500000")
			)
			.unwrap());

		// Less than
		assert!(evaluator
			.compare_final_values(
				"u64",
				"1000000",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("2000000")
			)
			.unwrap());
	}

	#[test]
	fn test_string_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Equal
		assert!(evaluator
			.compare_final_values(
				"pubkey",
				"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA")
			)
			.unwrap());

		// Not equal
		assert!(evaluator
			.compare_final_values(
				"pubkey",
				"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("11111111111111111111111111111111")
			)
			.unwrap());

		// Contains
		assert!(evaluator
			.compare_final_values(
				"pubkey",
				"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
				&ComparisonOperator::Contains,
				&LiteralValue::Str("Tokenkeg")
			)
			.unwrap());
	}

	#[test]
	fn test_bool_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Equal true
		assert!(evaluator
			.compare_final_values(
				"bool",
				"true",
				&ComparisonOperator::Eq,
				&LiteralValue::Bool(true)
			)
			.unwrap());

		// Not equal false
		assert!(evaluator
			.compare_final_values(
				"bool",
				"true",
				&ComparisonOperator::Ne,
				&LiteralValue::Bool(false)
			)
			.unwrap());
	}

	#[test]
	fn test_unknown_param() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		let result = evaluator.get_base_param("unknown_param");
		assert!(result.is_err());
	}

	#[test]
	fn test_get_kind_from_json_value() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		assert_eq!(
			evaluator.get_kind_from_json_value(&serde_json::json!(true)),
			"bool"
		);
		assert_eq!(
			evaluator.get_kind_from_json_value(&serde_json::json!(123)),
			"i64"
		);
		assert_eq!(
			evaluator.get_kind_from_json_value(&serde_json::json!("hello")),
			"string"
		);
		assert_eq!(
			evaluator.get_kind_from_json_value(&serde_json::json!([1, 2, 3])),
			"vec"
		);
		assert_eq!(
			evaluator.get_kind_from_json_value(&serde_json::json!({"key": "value"})),
			"object"
		);
		assert_eq!(
			evaluator.get_kind_from_json_value(&serde_json::Value::Null),
			"null"
		);
	}
}
