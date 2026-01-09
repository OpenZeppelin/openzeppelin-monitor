//! Solana client error types
//!
//! Provides error handling for Solana RPC requests, response parsing, input validation,
//! and Solana-specific error conditions.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error;

/// Solana client error type
#[derive(Debug, Error)]
pub enum SolanaClientError {
	/// Requested slot is not available (skipped or not yet produced)
	#[error("Slot {slot} is not available: {reason}")]
	SlotNotAvailable {
		slot: u64,
		reason: String,
		context: Box<ErrorContext>,
	},

	/// Block data not available for the requested slot
	#[error("Block not available for slot {slot}: {reason}")]
	BlockNotAvailable {
		slot: u64,
		reason: String,
		context: Box<ErrorContext>,
	},

	/// Transaction not found
	#[error("Transaction not found: {signature}")]
	TransactionNotFound {
		signature: String,
		context: Box<ErrorContext>,
	},

	/// Failure in making an RPC request
	#[error("Solana RPC request failed: {0}")]
	RpcError(Box<ErrorContext>),

	/// Failure in parsing the Solana RPC response
	#[error("Failed to parse Solana RPC response: {0}")]
	ResponseParseError(Box<ErrorContext>),

	/// Invalid input provided to the Solana client
	#[error("Invalid input: {0}")]
	InvalidInput(Box<ErrorContext>),

	/// The response from the Solana RPC does not match the expected format
	#[error("Unexpected response structure from Solana RPC: {0}")]
	UnexpectedResponseStructure(Box<ErrorContext>),

	/// Program/IDL not found
	#[error("Program IDL not found for: {program_id}")]
	IdlNotFound {
		program_id: String,
		context: Box<ErrorContext>,
	},

	/// Instruction decoding error
	#[error("Failed to decode instruction: {0}")]
	InstructionDecodeError(Box<ErrorContext>),
}

impl SolanaClientError {
	/// Creates a SlotNotAvailable error
	pub fn slot_not_available(
		slot: u64,
		reason: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		let reason = reason.into();
		let message = format!("Slot {} is not available: {}", slot, &reason);
		Self::SlotNotAvailable {
			slot,
			reason,
			context: Box::new(ErrorContext::new_with_log(message, source, metadata)),
		}
	}

	/// Creates a BlockNotAvailable error
	pub fn block_not_available(
		slot: u64,
		reason: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		let reason = reason.into();
		let message = format!("Block not available for slot {}: {}", slot, &reason);
		Self::BlockNotAvailable {
			slot,
			reason,
			context: Box::new(ErrorContext::new_with_log(message, source, metadata)),
		}
	}

	/// Creates a TransactionNotFound error
	pub fn transaction_not_found(
		signature: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		let signature = signature.into();
		let message = format!("Transaction not found: {}", &signature);
		Self::TransactionNotFound {
			signature,
			context: Box::new(ErrorContext::new_with_log(message, source, metadata)),
		}
	}

	/// Creates an RPC error
	pub fn rpc_error(
		message: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::RpcError(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}

	/// Creates a response parse error
	pub fn response_parse_error(
		message: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ResponseParseError(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}

	/// Creates an invalid input error
	pub fn invalid_input(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InvalidInput(Box::new(ErrorContext::new_with_log(msg, source, metadata)))
	}

	/// Creates an unexpected response structure error
	pub fn unexpected_response_structure(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::UnexpectedResponseStructure(Box::new(ErrorContext::new_with_log(
			msg, source, metadata,
		)))
	}

	/// Creates an IDL not found error
	pub fn idl_not_found(
		program_id: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		let program_id = program_id.into();
		let message = format!("Program IDL not found for: {}", &program_id);
		Self::IdlNotFound {
			program_id,
			context: Box::new(ErrorContext::new_with_log(message, source, metadata)),
		}
	}

	/// Creates an instruction decode error
	pub fn instruction_decode_error(
		message: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InstructionDecodeError(Box::new(ErrorContext::new_with_log(
			message, source, metadata,
		)))
	}

	/// Checks if this is a slot not available error
	pub fn is_slot_not_available(&self) -> bool {
		matches!(self, Self::SlotNotAvailable { .. })
	}

	/// Checks if this is a block not available error
	pub fn is_block_not_available(&self) -> bool {
		matches!(self, Self::BlockNotAvailable { .. })
	}

	/// Checks if this is a transaction not found error
	pub fn is_transaction_not_found(&self) -> bool {
		matches!(self, Self::TransactionNotFound { .. })
	}
}

impl TraceableError for SolanaClientError {
	fn trace_id(&self) -> String {
		match self {
			SolanaClientError::SlotNotAvailable { context, .. } => context.trace_id.clone(),
			SolanaClientError::BlockNotAvailable { context, .. } => context.trace_id.clone(),
			SolanaClientError::TransactionNotFound { context, .. } => context.trace_id.clone(),
			SolanaClientError::RpcError(context) => context.trace_id.clone(),
			SolanaClientError::ResponseParseError(context) => context.trace_id.clone(),
			SolanaClientError::InvalidInput(context) => context.trace_id.clone(),
			SolanaClientError::UnexpectedResponseStructure(context) => context.trace_id.clone(),
			SolanaClientError::IdlNotFound { context, .. } => context.trace_id.clone(),
			SolanaClientError::InstructionDecodeError(context) => context.trace_id.clone(),
		}
	}
}

/// Known Solana RPC error codes
pub mod error_codes {
	/// Block not available (slot was skipped or not produced yet)
	pub const BLOCK_NOT_AVAILABLE: i64 = -32004;
	/// Slot was skipped
	pub const SLOT_SKIPPED: i64 = -32007;
	/// Long-term storage query error
	pub const LONG_TERM_STORAGE_SLOT_SKIPPED: i64 = -32009;
	/// Transaction version not supported
	#[allow(dead_code)]
	pub const UNSUPPORTED_TRANSACTION_VERSION: i64 = -32015;
	/// Invalid parameters
	#[allow(dead_code)]
	pub const INVALID_PARAMS: i64 = -32602;
	/// Internal error
	#[allow(dead_code)]
	pub const INTERNAL_ERROR: i64 = -32603;
}

/// Checks if the given RPC error code indicates a skipped/unavailable slot
pub fn is_slot_unavailable_error(code: i64) -> bool {
	matches!(
		code,
		error_codes::BLOCK_NOT_AVAILABLE
			| error_codes::SLOT_SKIPPED
			| error_codes::LONG_TERM_STORAGE_SLOT_SKIPPED
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_slot_not_available_error_formatting() {
		let error = SolanaClientError::slot_not_available(12345, "Slot was skipped", None, None);
		assert_eq!(
			error.to_string(),
			"Slot 12345 is not available: Slot was skipped"
		);
		if let SolanaClientError::SlotNotAvailable {
			slot,
			reason,
			context,
		} = error
		{
			assert_eq!(slot, 12345);
			assert_eq!(reason, "Slot was skipped");
			assert!(!context.trace_id.is_empty());
		} else {
			panic!("Expected SlotNotAvailable variant");
		}
	}

	#[test]
	fn test_block_not_available_error_formatting() {
		let error = SolanaClientError::block_not_available(12345, "Block cleaned up", None, None);
		assert_eq!(
			error.to_string(),
			"Block not available for slot 12345: Block cleaned up"
		);
		if let SolanaClientError::BlockNotAvailable {
			slot,
			reason,
			context,
		} = error
		{
			assert_eq!(slot, 12345);
			assert_eq!(reason, "Block cleaned up");
			assert!(!context.trace_id.is_empty());
		} else {
			panic!("Expected BlockNotAvailable variant");
		}
	}

	#[test]
	fn test_transaction_not_found_error_formatting() {
		let sig = "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW";
		let error = SolanaClientError::transaction_not_found(sig, None, None);
		assert_eq!(error.to_string(), format!("Transaction not found: {}", sig));
	}

	#[test]
	fn test_rpc_error_formatting() {
		let error_message = "Random Solana RPC error".to_string();
		let error = SolanaClientError::rpc_error(error_message.clone(), None, None);
		assert_eq!(
			error.to_string(),
			format!("Solana RPC request failed: {}", error_message)
		);
		if let SolanaClientError::RpcError(context) = error {
			assert_eq!(context.message, error_message);
			assert!(!context.trace_id.is_empty());
		} else {
			panic!("Expected RpcError variant");
		}
	}

	#[test]
	fn test_response_parse_error_formatting() {
		let error_message = "Failed to parse Solana RPC response".to_string();
		let error = SolanaClientError::response_parse_error(error_message.clone(), None, None);
		assert_eq!(
			error.to_string(),
			format!("Failed to parse Solana RPC response: {}", error_message)
		);
	}

	#[test]
	fn test_invalid_input_error_formatting() {
		let error_message = "Invalid input provided to Solana client".to_string();
		let error = SolanaClientError::invalid_input(error_message.clone(), None, None);
		assert_eq!(
			error.to_string(),
			format!("Invalid input: {}", error_message)
		);
	}

	#[test]
	fn test_idl_not_found_error_formatting() {
		let program_id = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
		let error = SolanaClientError::idl_not_found(program_id, None, None);
		assert_eq!(
			error.to_string(),
			format!("Program IDL not found for: {}", program_id)
		);
	}

	#[test]
	fn test_error_type_checks() {
		let slot_error = SolanaClientError::slot_not_available(123, "skipped", None, None);
		assert!(slot_error.is_slot_not_available());
		assert!(!slot_error.is_block_not_available());
		assert!(!slot_error.is_transaction_not_found());

		let block_error = SolanaClientError::block_not_available(123, "cleaned", None, None);
		assert!(!block_error.is_slot_not_available());
		assert!(block_error.is_block_not_available());

		let tx_error = SolanaClientError::transaction_not_found("abc", None, None);
		assert!(tx_error.is_transaction_not_found());
	}

	#[test]
	fn test_is_slot_unavailable_error() {
		assert!(is_slot_unavailable_error(error_codes::BLOCK_NOT_AVAILABLE));
		assert!(is_slot_unavailable_error(error_codes::SLOT_SKIPPED));
		assert!(is_slot_unavailable_error(
			error_codes::LONG_TERM_STORAGE_SLOT_SKIPPED
		));
		assert!(!is_slot_unavailable_error(error_codes::INVALID_PARAMS));
		assert!(!is_slot_unavailable_error(error_codes::INTERNAL_ERROR));
	}

	#[test]
	fn test_all_error_variants_have_trace_id() {
		let create_context_with_id = || {
			let context = ErrorContext::new("test message", None, None);
			let original_id = context.trace_id.clone();
			(context, original_id)
		};

		let errors_with_ids: Vec<(SolanaClientError, String)> = vec![
			{
				let (ctx, id) = create_context_with_id();
				(
					SolanaClientError::SlotNotAvailable {
						slot: 0,
						reason: "".to_string(),
						context: Box::new(ctx),
					},
					id,
				)
			},
			{
				let (ctx, id) = create_context_with_id();
				(
					SolanaClientError::BlockNotAvailable {
						slot: 0,
						reason: "".to_string(),
						context: Box::new(ctx),
					},
					id,
				)
			},
			{
				let (ctx, id) = create_context_with_id();
				(
					SolanaClientError::TransactionNotFound {
						signature: "".to_string(),
						context: Box::new(ctx),
					},
					id,
				)
			},
			{
				let (ctx, id) = create_context_with_id();
				(SolanaClientError::RpcError(Box::new(ctx)), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(SolanaClientError::ResponseParseError(Box::new(ctx)), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(SolanaClientError::InvalidInput(Box::new(ctx)), id)
			},
			{
				let (ctx, id) = create_context_with_id();
				(
					SolanaClientError::UnexpectedResponseStructure(Box::new(ctx)),
					id,
				)
			},
			{
				let (ctx, id) = create_context_with_id();
				(
					SolanaClientError::IdlNotFound {
						program_id: "".to_string(),
						context: Box::new(ctx),
					},
					id,
				)
			},
			{
				let (ctx, id) = create_context_with_id();
				(SolanaClientError::InstructionDecodeError(Box::new(ctx)), id)
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
				"Trace ID for {:?} was not propagated consistently",
				error
			);
		}
	}
}
