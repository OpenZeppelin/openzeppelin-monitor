//! Blockchain service error types and handling.
//!
//! Provides a comprehensive error handling system for blockchain operations,
//! including network connectivity, request processing, and blockchain-specific errors.

use crate::utils::logging::error::{ErrorContext, TraceableError};
use std::collections::HashMap;
use thiserror::Error as ThisError;
use uuid::Uuid;

/// Represents possible errors that can occur during blockchain operations
#[derive(ThisError, Debug)]
pub enum BlockChainError {
	/// Errors related to network connectivity issues
	#[error("Connection error: {0}")]
	ConnectionError(ErrorContext),

	/// Errors related to malformed requests or invalid responses
	#[error("Request error: {0}")]
	RequestError(ErrorContext),

	/// When a requested block cannot be found on the blockchain
	#[error("Block not found: {0}")]
	BlockNotFound(ErrorContext),

	/// Errors related to transaction processing
	#[error("Transaction error: {0}")]
	TransactionError(ErrorContext),

	/// Internal errors within the blockchain client
	#[error("Internal error: {0}")]
	InternalError(ErrorContext),

	/// Errors related to client pool
	#[error("Client pool error: {0}")]
	ClientPoolError(ErrorContext),

	/// Other errors that don't fit into the categories above
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

impl BlockChainError {
	// Connection error
	pub fn connection_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConnectionError(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Request error
	pub fn request_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::RequestError(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Block not found
	pub fn block_not_found(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::BlockNotFound(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Transaction error
	pub fn transaction_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::TransactionError(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Internal error
	pub fn internal_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(ErrorContext::new_with_log(msg, source, metadata))
	}

	// Client pool error
	pub fn client_pool_error(
		msg: impl Into<String>,
		source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ClientPoolError(ErrorContext::new_with_log(msg, source, metadata))
	}
}

impl TraceableError for BlockChainError {
	fn trace_id(&self) -> String {
		match self {
			Self::ConnectionError(ctx) => ctx.trace_id.clone(),
			Self::RequestError(ctx) => ctx.trace_id.clone(),
			Self::BlockNotFound(ctx) => ctx.trace_id.clone(),
			Self::TransactionError(ctx) => ctx.trace_id.clone(),
			Self::InternalError(ctx) => ctx.trace_id.clone(),
			Self::ClientPoolError(ctx) => ctx.trace_id.clone(),
			Self::Other(_) => Uuid::new_v4().to_string(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::{Error as IoError, ErrorKind};

	#[test]
	fn test_connection_error_formatting() {
		let error = BlockChainError::connection_error("test error", None, None);
		assert_eq!(error.to_string(), "Connection error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockChainError::connection_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Connection error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_request_error_formatting() {
		let error = BlockChainError::request_error("test error", None, None);
		assert_eq!(error.to_string(), "Request error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockChainError::request_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Request error: test error [key1=value1]");
	}

	#[test]
	fn test_block_not_found_formatting() {
		let error = BlockChainError::block_not_found("1".to_string(), None, None);
		assert_eq!(error.to_string(), "Block not found: 1");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockChainError::block_not_found(
			"1".to_string(),
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(error.to_string(), "Block not found: 1 [key1=value1]");
	}

	#[test]
	fn test_transaction_error_formatting() {
		let error = BlockChainError::transaction_error("test error", None, None);
		assert_eq!(error.to_string(), "Transaction error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockChainError::transaction_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Transaction error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = BlockChainError::internal_error("test error", None, None);
		assert_eq!(error.to_string(), "Internal error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockChainError::internal_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Internal error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_client_pool_error_formatting() {
		let error = BlockChainError::client_pool_error("test error", None, None);
		assert_eq!(error.to_string(), "Client pool error: test error");

		let source_error = IoError::new(ErrorKind::NotFound, "test source");
		let error = BlockChainError::client_pool_error(
			"test error",
			Some(Box::new(source_error)),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert_eq!(
			error.to_string(),
			"Client pool error: test error [key1=value1]"
		);
	}

	#[test]
	fn test_from_anyhow_error() {
		let anyhow_error = anyhow::anyhow!("test anyhow error");
		let block_chain_error: BlockChainError = anyhow_error.into();
		assert!(matches!(block_chain_error, BlockChainError::Other(_)));
		assert_eq!(block_chain_error.to_string(), "test anyhow error");
	}

	#[test]
	fn test_error_source_chain() {
		let io_error = std::io::Error::new(std::io::ErrorKind::Other, "while reading config");

		let outer_error =
			BlockChainError::request_error("Failed to initialize", Some(Box::new(io_error)), None);

		// Just test the string representation instead of the source chain
		assert!(outer_error.to_string().contains("Failed to initialize"));

		// For BlockChainError::RequestError, we know the implementation details
		if let BlockChainError::RequestError(ctx) = &outer_error {
			// Check that the context has the right message
			assert_eq!(ctx.message, "Failed to initialize");

			// Check that the context has the source error
			assert!(ctx.source.is_some());

			if let Some(src) = &ctx.source {
				assert_eq!(src.to_string(), "while reading config");
			}
		} else {
			panic!("Expected RequestError variant");
		}
	}

	#[test]
	fn test_trace_id_propagation() {
		// Create an error context with a known trace ID
		let error_context = ErrorContext::new("Inner error", None, None);
		let original_trace_id = error_context.trace_id.clone();

		// Wrap it in a BlockChainError
		let block_chain_error = BlockChainError::RequestError(error_context);

		// Verify the trace ID is preserved
		assert_eq!(block_chain_error.trace_id(), original_trace_id);

		// Test trace ID propagation through error chain
		let source_error = IoError::new(ErrorKind::Other, "Source error");
		let error_context = ErrorContext::new("Middle error", Some(Box::new(source_error)), None);
		let original_trace_id = error_context.trace_id.clone();

		let block_chain_error = BlockChainError::RequestError(error_context);
		assert_eq!(block_chain_error.trace_id(), original_trace_id);

		// Test Other variant
		let anyhow_error = anyhow::anyhow!("Test anyhow error");
		let block_chain_error: BlockChainError = anyhow_error.into();

		// Other variant should generate a new UUID
		assert!(!block_chain_error.trace_id().is_empty());
	}
}
