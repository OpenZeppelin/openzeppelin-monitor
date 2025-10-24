//! Stellar client error types
//!
//! Provides error handling for Stellar RPC requests, response parsing, input validation and out-of-retention errors.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error;

/// Stellar client error type
#[derive(Debug, Error)]
pub enum StellarClientError {
	/// Requested data is outside of the Stellar RPC retention window
	#[error("Data for '{ledger_info}' is outside of Stellar RPC retention window")]
	OutsideRetentionWindow {
		rpc_code: i64,       // Code from RPC response
		rpc_message: String, // Message from RPC response
		ledger_info: String, // Information about the ledger (e.g., start_sequence, end_sequence)
		context: Box<ErrorContext>,
	},

	/// Failure in making an RPC request
	#[error("Stellar RPC request failed: {0}")]
	RpcError(Box<ErrorContext>),

	/// Failure in parsing the Stellar RPC response
	#[error("Failed to parse Stellar RPC response: {0}")]
	ResponseParseError(Box<ErrorContext>),

	/// Invalid input provided to the Stellar client
	#[error("Invalid input: {0}")]
	InvalidInput(Box<ErrorContext>),

	/// The response from the Stellar RPC does not match the expected format.
	#[error("Unexpected response structure from Stellar RPC: {0}")]
	UnexpectedResponseStructure(Box<ErrorContext>),
}

impl StellarClientError {
	pub fn outside_retention_window(
		rpc_code: i64,
		rpc_message: String,
		ledger_info: String,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		let message = format!(
			"Data for '{}' is outside of Stellar RPC retention window: {} (code {})",
			&ledger_info.clone(),
			&rpc_message.clone(),
			&rpc_code
		);
		Self::OutsideRetentionWindow {
			rpc_code,
			rpc_message,
			ledger_info,
			context: Box::new(ErrorContext::new_with_log(message, source, metadata)),
		}
	}

	pub fn rpc_error(
		message: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::RpcError(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}

	pub fn response_parse_error(
		message: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ResponseParseError(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}

	pub fn invalid_input(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InvalidInput(Box::new(ErrorContext::new_with_log(msg, source, metadata)))
	}

	pub fn unexpected_response_structure(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::UnexpectedResponseStructure(Box::new(ErrorContext::new_with_log(
			msg, source, metadata,
		)))
	}
}

impl TraceableError for StellarClientError {
	fn trace_id(&self) -> String {
		match self {
			StellarClientError::OutsideRetentionWindow { context, .. } => context.trace_id.clone(),
			StellarClientError::RpcError(context) => context.trace_id.clone(),
			StellarClientError::ResponseParseError(context) => context.trace_id.clone(),
			StellarClientError::InvalidInput(context) => context.trace_id.clone(),
			StellarClientError::UnexpectedResponseStructure(context) => context.trace_id.clone(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_outside_retention_window_error_formatting() {
		let rpc_error_code = 1234;
		let rpc_error_message = "Random RPC error".to_string();
		let error_ledger_info = "start_sequence=123 end_sequence=456".to_string();
		let error = StellarClientError::outside_retention_window(
			rpc_error_code,
			rpc_error_message.clone(),
			error_ledger_info.clone(),
			None,
			None,
		);
		assert_eq!(
			error.to_string(),
			format!(
				"Data for '{}' is outside of Stellar RPC retention window",
				error_ledger_info
			)
		);
		if let StellarClientError::OutsideRetentionWindow {
			rpc_code,
			rpc_message,
			ledger_info,
			context,
		} = error
		{
			assert_eq!(rpc_code, rpc_error_code);
			assert_eq!(rpc_message, rpc_error_message);
			assert_eq!(ledger_info, error_ledger_info);
			assert!(!context.trace_id.is_empty());
		} else {
			panic!("Expected OutsideRetentionWindow variant");
		}
	}

	#[test]
	fn test_rpc_error_formatting() {
		let error_message = "Random Stellar RPC error".to_string();
		let error = StellarClientError::rpc_error(error_message.clone(), None, None);
		assert_eq!(
			error.to_string(),
			format!("Stellar RPC request failed: {}", error_message)
		);
		if let StellarClientError::RpcError(context) = error {
			assert_eq!(context.message, error_message);
			assert!(!context.trace_id.is_empty());
		} else {
			panic!("Expected RpcError variant");
		}
	}

	#[test]
	fn test_response_parse_error_formatting() {
		let error_message = "Failed to parse Stellar RPC response".to_string();
		let error = StellarClientError::response_parse_error(error_message.clone(), None, None);
		assert_eq!(
			error.to_string(),
			format!("Failed to parse Stellar RPC response: {}", error_message)
		);
		if let StellarClientError::ResponseParseError(context) = error {
			assert_eq!(context.message, error_message);
			assert!(!context.trace_id.is_empty());
		} else {
			panic!("Expected ResponseParseError variant");
		}
	}

	#[test]
	fn test_invalid_input_error_formatting() {
		let error_message = "Invalid input provided to Stellar client".to_string();
		let error = StellarClientError::invalid_input(error_message.clone(), None, None);
		assert_eq!(
			error.to_string(),
			format!("Invalid input: {}", error_message)
		);
		if let StellarClientError::InvalidInput(context) = error {
			assert_eq!(context.message, error_message);
			assert!(!context.trace_id.is_empty());
		} else {
			panic!("Expected InvalidInput variant");
		}
	}

	#[test]
	fn test_unexpected_response_structure_error_formatting() {
		let error_message = "Unexpected response structure from Stellar RPC".to_string();
		let error =
			StellarClientError::unexpected_response_structure(error_message.clone(), None, None);
		assert_eq!(
			error.to_string(),
			format!(
				"Unexpected response structure from Stellar RPC: {}",
				error_message
			)
		);
		if let StellarClientError::UnexpectedResponseStructure(context) = error {
			assert_eq!(context.message, error_message);
			assert!(!context.trace_id.is_empty());
		} else {
			panic!("Expected UnexpectedResponseStructure variant");
		}
	}

	#[test]
	fn test_error_source_chain() {
		let io_error = std::io::Error::other("while reading config");

		let outer_error =
			StellarClientError::rpc_error("Failed to initialize", Some(Box::new(io_error)), None);

		// Just test the string representation instead of the source chain
		assert!(outer_error.to_string().contains("Failed to initialize"));

		// For StellarClientError::RpcError, we know the implementation details
		if let StellarClientError::RpcError(context) = &outer_error {
			// Check that the context has the right message
			assert_eq!(context.message, "Failed to initialize");

			// Check that the context has the source error
			assert!(context.source.is_some());

			if let Some(src) = &context.source {
				assert_eq!(src.to_string(), "while reading config");
			}
		} else {
			panic!("Expected RpcError variant");
		}
	}

	#[test]
	fn test_all_error_variants_have_and_propagate_consistent_trace_id() {
		let create_context_with_id = || {
			let context = ErrorContext::new("test message", None, None);
			let original_id = context.trace_id.clone();
			(context, original_id)
		};

		let errors_with_ids: Vec<(StellarClientError, String)> = vec![
			{
				let (ctx, id) = create_context_with_id();
				(StellarClientError::RpcError(Box::new(ctx)), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(StellarClientError::ResponseParseError(Box::new(ctx)), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(StellarClientError::InvalidInput(Box::new(ctx)), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(
					StellarClientError::OutsideRetentionWindow {
						rpc_code: 0,
						rpc_message: "".to_string(),
						ledger_info: "".to_string(),
						context: Box::new(ctx),
					},
					id,
				)
			},
			{
				let (ctx, id) = create_context_with_id();
				(
					StellarClientError::UnexpectedResponseStructure(Box::new(ctx)),
					id,
				)
			},
		];

		for (error, original_id) in errors_with_ids {
			let propagated_id = error.trace_id();
			assert!(
				!propagated_id.is_empty(),
				"Error {:?} should have a non-empty trace_id",
				error
			);
			assert_eq!(
				propagated_id, original_id,
				"Trace ID for {:?} was not propagated consistently. Expected: {}, Got: {}",
				error, original_id, propagated_id
			);
		}
	}
}
