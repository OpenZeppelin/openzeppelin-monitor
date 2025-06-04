//! Error types for the evaluation of expressions.
//! This module defines the `EvaluationError` enum, which represents various errors that can occur
//! during the evaluation of expressions in the context of a filter.
//! The errors include issues related to variable resolution, type mismatches, unsupported operators,
//! parsing errors, and path traversal errors.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvaluationError {
	/// A variable specified in the expression cannot be found.
	#[error("Variable not found: {0}")]
	VariableNotFound(Box<ErrorContext>),

	/// Mismatch between expected and actual types during evaluation.
	#[error("Type mismatch: {0}")]
	TypeMismatch(Box<ErrorContext>),

	/// An operator is used with incompatible types.
	#[error("Unsupported operator: {0}")]
	UnsupportedOperator(Box<ErrorContext>),

	/// A value fails to parse into an expected type.
	#[error("Failed to parse value: {0}")]
	ParseError(Box<ErrorContext>),

	/// An index is out of bounds during array access in a path.
	#[error("Index out of bounds during path traversal: {0}")]
	IndexOutOfBounds(Box<ErrorContext>),

	/// A field/key is not found during object access in a path.
	#[error("Field not found during path traversal: {0}")]
	FieldNotFound(Box<ErrorContext>),
}

impl EvaluationError {
	/// Creates a new `VariableNotFound` error.
	/// The `message` for the `ErrorContext` should be the name of the variable.
	pub fn variable_not_found(
		variable_name: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::VariableNotFound(Box::new(ErrorContext::new_with_log(
			variable_name,
			source,
			metadata,
		)))
	}

	/// Creates a new `TypeMismatch` error.
	/// The `message` for `ErrorContext` should describe the type mismatch.
	pub fn type_mismatch(
		message: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::TypeMismatch(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}

	/// Creates a new `UnsupportedOperator` error.
	/// The `message` for `ErrorContext` should describe why the operator is unsupported,
	/// e.g., "'=' for types String and Number".
	pub fn unsupported_operator(
		message: impl Into<String>, // e.g., format!("'{}' for types {} and {}", op, type1, type2)
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::UnsupportedOperator(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}

	/// Creates a new `ParseError` error.
	/// The `message` for `ErrorContext` should describe the parsing failure.
	pub fn parse_error(
		message: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ParseError(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}

	/// Creates a new `IndexOutOfBounds` error.
	/// The `message` for `ErrorContext` should describe the out-of-bounds access.
	pub fn index_out_of_bounds(
		message: impl Into<String>, // e.g., format!("Index {} out of bounds for length {} at path '{}'", index, len, path)
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::IndexOutOfBounds(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}

	/// Creates a new `FieldNotFound` error.
	/// The `message` for `ErrorContext` should describe the missing field.
	pub fn field_not_found(
		message: impl Into<String>, // e.g., format!("Field '{}' not found at path '{}'", field, path)
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::FieldNotFound(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}
}

impl TraceableError for EvaluationError {
	fn trace_id(&self) -> String {
		match self {
			Self::VariableNotFound(ctx)
			| Self::TypeMismatch(ctx)
			| Self::UnsupportedOperator(ctx)
			| Self::ParseError(ctx)
			| Self::IndexOutOfBounds(ctx)
			| Self::FieldNotFound(ctx) => ctx.trace_id.clone(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	// Mock ErrorContext and TraceableError for testing if they are not available globally for tests
	// If crate::utils::logging::error is a real path and accessible, these mocks are not needed.
	// For this example, I assume they are real and ErrorContext::new_with_log works as expected.

	#[test]
	fn test_variable_not_found_error() {
		let error = EvaluationError::variable_not_found("test_var", None, None);
		assert_eq!(error.to_string(), "Variable not found: test_var");
		assert!(matches!(error, EvaluationError::VariableNotFound(_)));

		let mut meta = HashMap::new();
		meta.insert("scope".to_string(), "global".to_string());
		let error_with_meta =
			EvaluationError::variable_not_found("test_var_meta", None, Some(meta));
		// Assuming ErrorContext's Display includes metadata like "[scope=global]"
		assert_eq!(
			error_with_meta.to_string(),
			"Variable not found: test_var_meta [scope=global]"
		);
	}

	#[test]
	fn test_type_mismatch_error() {
		let error = EvaluationError::type_mismatch("Expected number, got string", None, None);
		assert_eq!(
			error.to_string(),
			"Type mismatch: Expected number, got string"
		);
		assert!(matches!(error, EvaluationError::TypeMismatch(_)));
	}

	#[test]
	fn test_unsupported_operator_error() {
		let error = EvaluationError::unsupported_operator(
			"Operator '>' for types String and Integer",
			None,
			None,
		);
		assert_eq!(
			error.to_string(),
			"Unsupported operator: Operator '>' for types String and Integer"
		);
		assert!(matches!(error, EvaluationError::UnsupportedOperator(_)));
	}

	#[test]
	fn test_parse_error() {
		let source_err = IoError::new(ErrorKind::InvalidData, "bad format");
		let error = EvaluationError::parse_error(
			"Could not parse 'abc' as number",
			Some(Box::new(source_err)),
			None,
		);
		assert_eq!(
			error.to_string(),
			"Failed to parse value: Could not parse 'abc' as number"
		);
		assert!(matches!(error, EvaluationError::ParseError(_)));
		if let EvaluationError::ParseError(ctx) = error {
			assert!(ctx.source.is_some());
			assert_eq!(ctx.source.unwrap().to_string(), "bad format");
		} else {
			panic!("Expected ParseError variant");
		}
	}

	#[test]
	fn test_index_out_of_bounds_error() {
		let error = EvaluationError::index_out_of_bounds(
			"Index 5 out of bounds for array of length 3 at path.to.array",
			None,
			None,
		);
		assert_eq!(
			error.to_string(),
			"Index out of bounds during path traversal: Index 5 out of bounds for array of length 3 at path.to.array"
		);
		assert!(matches!(error, EvaluationError::IndexOutOfBounds(_)));
	}

	#[test]
	fn test_field_not_found_error() {
		let error = EvaluationError::field_not_found(
			"Field 'bar' not found in object at path.to.object",
			None,
			None,
		);
		assert_eq!(
			error.to_string(),
			"Field not found during path traversal: Field 'bar' not found in object at path.to.object"
		);
		assert!(matches!(error, EvaluationError::FieldNotFound(_)));
	}

	#[test]
	fn test_trace_id_retrieval() {
		let error_vnf = EvaluationError::variable_not_found("my_var", None, None);
		let trace_id_vnf_direct: String; // To store the trace_id directly from ErrorContext for comparison

		if let EvaluationError::VariableNotFound(boxed_ctx) = &error_vnf {
			trace_id_vnf_direct = boxed_ctx.trace_id.clone(); // Get trace_id from the inner ErrorContext
			assert!(
				!trace_id_vnf_direct.is_empty(),
				"Trace ID from context should not be empty"
			);
		} else {
			panic!("Expected VariableNotFound to retrieve direct trace_id");
		}
		assert_eq!(
			error_vnf.trace_id(),
			trace_id_vnf_direct,
			"TraceableError trace_id() should match context's trace_id for VariableNotFound"
		);
	}
}
