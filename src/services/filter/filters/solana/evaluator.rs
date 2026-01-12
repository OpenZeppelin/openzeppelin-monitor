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

	// ============================================================================
	// Signed integer tests
	// ============================================================================

	#[test]
	fn test_signed_integer_i8_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Positive i8
		assert!(evaluator
			.compare_final_values(
				"i8",
				"100",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("100")
			)
			.unwrap());

		// Negative i8
		assert!(evaluator
			.compare_final_values(
				"i8",
				"-50",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("-50")
			)
			.unwrap());

		// i8 less than
		assert!(evaluator
			.compare_final_values(
				"i8",
				"-100",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("50")
			)
			.unwrap());

		// i8 greater than
		assert!(evaluator
			.compare_final_values(
				"i8",
				"50",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("-100")
			)
			.unwrap());
	}

	#[test]
	fn test_signed_integer_i16_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Positive i16
		assert!(evaluator
			.compare_final_values(
				"i16",
				"10000",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("10000")
			)
			.unwrap());

		// Negative i16
		assert!(evaluator
			.compare_final_values(
				"i16",
				"-5000",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("0")
			)
			.unwrap());
	}

	#[test]
	fn test_signed_integer_i32_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Large positive i32
		assert!(evaluator
			.compare_final_values(
				"i32",
				"1000000000",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("999999999")
			)
			.unwrap());

		// Large negative i32
		assert!(evaluator
			.compare_final_values(
				"i32",
				"-1000000000",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("-999999999")
			)
			.unwrap());

		// Negative not equal
		assert!(evaluator
			.compare_final_values(
				"i32",
				"-12345",
				&ComparisonOperator::Ne,
				&LiteralValue::Number("12345")
			)
			.unwrap());
	}

	#[test]
	fn test_signed_integer_i64_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Very large positive i64
		assert!(evaluator
			.compare_final_values(
				"i64",
				"9223372036854775807",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("9223372036854775807")
			)
			.unwrap());

		// Very large negative i64
		assert!(evaluator
			.compare_final_values(
				"i64",
				"-9223372036854775808",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("0")
			)
			.unwrap());
	}

	#[test]
	fn test_signed_integer_i128_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Very large i128 (beyond i64 range)
		assert!(evaluator
			.compare_final_values(
				"i128",
				"170141183460469231731687303715884105727",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("0")
			)
			.unwrap());

		// Negative i128
		assert!(evaluator
			.compare_final_values(
				"i128",
				"-170141183460469231731687303715884105728",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("0")
			)
			.unwrap());
	}

	// ============================================================================
	// Gte and Lte operator tests
	// ============================================================================

	#[test]
	fn test_gte_operator_unsigned() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Greater than or equal (greater)
		assert!(evaluator
			.compare_final_values(
				"u64",
				"1000000",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("500000")
			)
			.unwrap());

		// Greater than or equal (equal)
		assert!(evaluator
			.compare_final_values(
				"u64",
				"1000000",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("1000000")
			)
			.unwrap());

		// Greater than or equal (less - should be false)
		assert!(!evaluator
			.compare_final_values(
				"u64",
				"500000",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("1000000")
			)
			.unwrap());
	}

	#[test]
	fn test_lte_operator_unsigned() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Less than or equal (less)
		assert!(evaluator
			.compare_final_values(
				"u64",
				"500000",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("1000000")
			)
			.unwrap());

		// Less than or equal (equal)
		assert!(evaluator
			.compare_final_values(
				"u64",
				"1000000",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("1000000")
			)
			.unwrap());

		// Less than or equal (greater - should be false)
		assert!(!evaluator
			.compare_final_values(
				"u64",
				"1000000",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("500000")
			)
			.unwrap());
	}

	#[test]
	fn test_gte_operator_signed() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Gte with negative numbers
		assert!(evaluator
			.compare_final_values(
				"i64",
				"-100",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("-100")
			)
			.unwrap());

		assert!(evaluator
			.compare_final_values(
				"i64",
				"0",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("-100")
			)
			.unwrap());

		assert!(!evaluator
			.compare_final_values(
				"i64",
				"-200",
				&ComparisonOperator::Gte,
				&LiteralValue::Number("-100")
			)
			.unwrap());
	}

	#[test]
	fn test_lte_operator_signed() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Lte with negative numbers
		assert!(evaluator
			.compare_final_values(
				"i64",
				"-200",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("-100")
			)
			.unwrap());

		assert!(evaluator
			.compare_final_values(
				"i64",
				"-100",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("-100")
			)
			.unwrap());

		assert!(!evaluator
			.compare_final_values(
				"i64",
				"0",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("-100")
			)
			.unwrap());
	}

	// ============================================================================
	// Additional unsigned integer type tests
	// ============================================================================

	#[test]
	fn test_unsigned_integer_u8_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		assert!(evaluator
			.compare_final_values(
				"u8",
				"255",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("255")
			)
			.unwrap());

		assert!(evaluator
			.compare_final_values(
				"u8",
				"0",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("255")
			)
			.unwrap());
	}

	#[test]
	fn test_unsigned_integer_u16_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		assert!(evaluator
			.compare_final_values(
				"u16",
				"65535",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("65535")
			)
			.unwrap());
	}

	#[test]
	fn test_unsigned_integer_u32_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		assert!(evaluator
			.compare_final_values(
				"u32",
				"4294967295",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("4294967295")
			)
			.unwrap());
	}

	#[test]
	fn test_unsigned_integer_u128_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Very large u128 (beyond u64 range)
		assert!(evaluator
			.compare_final_values(
				"u128",
				"340282366920938463463374607431768211455",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("0")
			)
			.unwrap());
	}

	// ============================================================================
	// Type mismatch and error tests
	// ============================================================================

	#[test]
	fn test_type_mismatch_unsigned_parse_error() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Invalid unsigned integer (negative)
		let result = evaluator.compare_final_values(
			"u64",
			"-100",
			&ComparisonOperator::Eq,
			&LiteralValue::Number("100"),
		);
		assert!(result.is_err());
		let err_msg = format!("{:?}", result.unwrap_err());
		assert!(err_msg.contains("unsigned integer") || err_msg.contains("parse"));
	}

	#[test]
	fn test_type_mismatch_signed_parse_error() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Invalid signed integer (non-numeric)
		let result = evaluator.compare_final_values(
			"i64",
			"not_a_number",
			&ComparisonOperator::Eq,
			&LiteralValue::Number("100"),
		);
		assert!(result.is_err());
		let err_msg = format!("{:?}", result.unwrap_err());
		assert!(err_msg.contains("signed integer") || err_msg.contains("parse"));
	}

	#[test]
	fn test_type_mismatch_bool_invalid_literal() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Invalid boolean literal type
		let result = evaluator.compare_final_values(
			"bool",
			"true",
			&ComparisonOperator::Eq,
			&LiteralValue::Number("123"),
		);
		assert!(result.is_err());
		let err_msg = format!("{:?}", result.unwrap_err());
		assert!(err_msg.contains("boolean") || err_msg.contains("Expected"));
	}

	#[test]
	fn test_type_mismatch_bool_parse_error() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Invalid boolean value
		let result = evaluator.compare_final_values(
			"bool",
			"not_bool",
			&ComparisonOperator::Eq,
			&LiteralValue::Bool(true),
		);
		assert!(result.is_err());
		let err_msg = format!("{:?}", result.unwrap_err());
		assert!(err_msg.contains("boolean") || err_msg.contains("parse"));
	}

	#[test]
	fn test_type_mismatch_numeric_with_bool_literal() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Numeric comparison with boolean literal
		let result = evaluator.compare_final_values(
			"u64",
			"100",
			&ComparisonOperator::Eq,
			&LiteralValue::Bool(true),
		);
		assert!(result.is_err());
		let err_msg = format!("{:?}", result.unwrap_err());
		assert!(err_msg.contains("Expected") || err_msg.contains("number"));
	}

	#[test]
	fn test_string_unsupported_operator() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// String with Gt operator (not supported)
		let result = evaluator.compare_final_values(
			"string",
			"hello",
			&ComparisonOperator::Gt,
			&LiteralValue::Str("world"),
		);
		assert!(result.is_err());
		let err_msg = format!("{:?}", result.unwrap_err());
		assert!(err_msg.contains("not supported") || err_msg.contains("Operator"));
	}

	#[test]
	fn test_bool_unsupported_operator() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Boolean with Gt operator (not supported)
		let result = evaluator.compare_final_values(
			"bool",
			"true",
			&ComparisonOperator::Gt,
			&LiteralValue::Bool(false),
		);
		assert!(result.is_err());
		let err_msg = format!("{:?}", result.unwrap_err());
		assert!(err_msg.contains("not supported") || err_msg.contains("Operator"));
	}

	// ============================================================================
	// Bytes type tests
	// ============================================================================

	#[test]
	fn test_bytes_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Bytes equal
		assert!(evaluator
			.compare_final_values(
				"bytes",
				"0x123456",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("0x123456")
			)
			.unwrap());

		// Bytes not equal
		assert!(evaluator
			.compare_final_values(
				"bytes",
				"0x123456",
				&ComparisonOperator::Ne,
				&LiteralValue::Str("0xabcdef")
			)
			.unwrap());

		// Bytes contains
		assert!(evaluator
			.compare_final_values(
				"bytes",
				"0x123456789abc",
				&ComparisonOperator::Contains,
				&LiteralValue::Str("345678")
			)
			.unwrap());
	}

	// ============================================================================
	// Edge case tests
	// ============================================================================

	#[test]
	fn test_zero_comparison() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Zero unsigned
		assert!(evaluator
			.compare_final_values(
				"u64",
				"0",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("0")
			)
			.unwrap());

		// Zero signed
		assert!(evaluator
			.compare_final_values(
				"i64",
				"0",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("0")
			)
			.unwrap());

		// Zero comparison with negative
		assert!(evaluator
			.compare_final_values(
				"i64",
				"0",
				&ComparisonOperator::Gt,
				&LiteralValue::Number("-1")
			)
			.unwrap());
	}

	#[test]
	fn test_boundary_values() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// i8 boundaries
		assert!(evaluator
			.compare_final_values(
				"i8",
				"-128",
				&ComparisonOperator::Lt,
				&LiteralValue::Number("127")
			)
			.unwrap());

		// u8 boundaries
		assert!(evaluator
			.compare_final_values(
				"u8",
				"0",
				&ComparisonOperator::Lte,
				&LiteralValue::Number("255")
			)
			.unwrap());
	}

	#[test]
	fn test_string_with_number_literal() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// String comparison can accept Number literal (it's treated as string)
		assert!(evaluator
			.compare_final_values(
				"string",
				"12345",
				&ComparisonOperator::Eq,
				&LiteralValue::Number("12345")
			)
			.unwrap());
	}

	#[test]
	fn test_bool_with_string_literal() {
		let args = create_test_args();
		let evaluator = SolanaConditionEvaluator::new(&args);

		// Boolean comparison with string literal "true"
		assert!(evaluator
			.compare_final_values(
				"bool",
				"true",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("true")
			)
			.unwrap());

		// Boolean comparison with string literal "false"
		assert!(evaluator
			.compare_final_values(
				"bool",
				"false",
				&ComparisonOperator::Eq,
				&LiteralValue::Str("false")
			)
			.unwrap());
	}
}
