//! This module contains the `ConditionEvaluator` trait and the `EvaluationError` enum.
//! The `ConditionEvaluator` trait defines methods for getting base parameters, comparing values,
//! and getting the kind of a value from a JSON value.
//! The `ConditionEvaluator` trait is implemented by specific evaluators that provide the logic
//! for evaluating conditions based on the context of the chain.

use crate::services::filter::expression::ast::{ComparisonOperator, LiteralValue};
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Error)]
pub enum EvaluationError {
	#[error("Variable not found: {0}")]
	VariableNotFound(String),
	#[error("Type mismatch: {0}")]
	TypeMismatch(String),
	#[error("Unsupported operator '{op}' for types")]
	UnsupportedOperator { op: String },
	#[error("Failed to parse value: {0}")]
	ParseError(String),
	#[error("Index out of bounds during path traversal: {0}")]
	IndexOutOfBounds(String),
	#[error("Field not found during path traversal: {0}")]
	FieldNotFound(String),
}

/// The `ConditionEvaluator` trait defines methods for evaluating conditions in filter expressions.
pub trait ConditionEvaluator {
	/// Gets the raw string value and kind for a base variable name
	fn get_base_param(&self, name: &str) -> Result<(&str, &str), EvaluationError>;

	/// Performs the final comparison between the left resolved value (after all path traversal) and the literal value
	fn compare_final_values(
		&self,
		left_kind: &str,
		left_resolved_value: &str,
		operator: &ComparisonOperator,
		right_literal: &LiteralValue,
	) -> Result<bool, EvaluationError>;

	/// Gets the chain-specific kind of a value from a JSON value
	fn get_kind_from_json_value(&self, value: &serde_json::Value) -> String;
}
