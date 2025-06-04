//! This module contains the `ConditionEvaluator` trait and the `EvaluationError` enum.
//! The `ConditionEvaluator` trait defines methods for getting base parameters, comparing values,
//! and getting the kind of a value from a JSON value.
//! The `ConditionEvaluator` trait is implemented by specific evaluators that provide the logic
//! for evaluating conditions based on the context of the chain.

use super::error::EvaluationError;
use crate::services::filter::expression::ast::{ComparisonOperator, LiteralValue};

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
