//! Blockchain error types and handling.
//!
//! This module provides a comprehensive error handling system for blockchain operations,
//! including network connectivity, request processing, and blockchain-specific errors.

use std::collections::HashMap;

use crate::utils::{EnhancedContext, ErrorContext};

/// Represents possible errors that can occur during blockchain operations
#[derive(Debug)]
pub enum BlockChainError {
	/// Errors related to network connectivity issues
	ConnectionError(ErrorContext<String>),

	/// Errors related to malformed requests or invalid responses
	RequestError(ErrorContext<String>),

	/// When a requested block cannot be found on the blockchain
	///
	/// Contains the block number that was not found
	BlockNotFound(ErrorContext<String>),

	/// Errors related to transaction processing
	TransactionError(ErrorContext<String>),

	/// Internal errors within the blockchain client
	InternalError(ErrorContext<String>),
}

impl BlockChainError {
	const TARGET: &str = "blockchain::error";

	/// Creates a new connection error with logging and optional source error
	pub fn connection_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConnectionError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Connection Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new connection error with logging and source error
	pub fn connection_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::ConnectionError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Connection Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new request error with logging
	pub fn request_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::RequestError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Request Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new request error with logging and source error
	pub fn request_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::RequestError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Request Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new block not found error with logging
	pub fn block_not_found(number: u64, metadata: Option<HashMap<String, String>>) -> Self {
		Self::BlockNotFound(
			ErrorContext::new(
				number.to_string(),
				EnhancedContext::new("Block Not Found Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new block not found error with logging and source error
	pub fn block_not_found_with_source(
		number: u64,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::BlockNotFound(
			ErrorContext::new(
				number.to_string(),
				EnhancedContext::new("Block Not Found Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new transaction error with logging
	pub fn transaction_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::TransactionError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Transaction Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new transaction error with logging and source error
	pub fn transaction_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::TransactionError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Transaction Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new internal error with logging
	pub fn internal_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Internal Error").with_metadata(metadata),
			)
			.with_target(Self::TARGET),
		)
	}

	/// Creates a new internal error with logging and source error
	pub fn internal_error_with_source(
		msg: impl Into<String>,
		source: impl std::error::Error + Send + Sync + 'static,
		metadata: Option<HashMap<String, String>>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				msg.into(),
				EnhancedContext::new("Internal Error").with_metadata(metadata),
			)
			.with_source(source)
			.with_target(Self::TARGET),
		)
	}
}

impl std::error::Error for BlockChainError {}

// Standard error trait implementations
impl std::fmt::Display for BlockChainError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::ConnectionError(ctx) => ctx.fmt(f),
			Self::RequestError(ctx) => ctx.fmt(f),
			Self::BlockNotFound(ctx) => ctx.fmt(f),
			Self::TransactionError(ctx) => ctx.fmt(f),
			Self::InternalError(ctx) => ctx.fmt(f),
		}
	}
}

/// Conversion from Web3 errors to BlockChainError
impl From<web3::Error> for BlockChainError {
	fn from(err: web3::Error) -> Self {
		Self::request_error(err.to_string(), None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_connection_error_formatting() {
		let error = BlockChainError::connection_error("test error", None);
		assert!(error.to_string().contains("Connection Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::connection_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Connection Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::connection_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Connection Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_request_error_formatting() {
		let error = BlockChainError::request_error("test error", None);
		assert!(error.to_string().contains("Request Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::request_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Request Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::request_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Request Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_block_not_found_formatting() {
		let error = BlockChainError::block_not_found(1, None);
		assert!(error.to_string().contains("Block Not Found Error: 1"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::block_not_found_with_source(
			1,
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Block Not Found Error: 1"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::block_not_found_with_source(
			1,
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Block Not Found Error: 1"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_transaction_error_formatting() {
		let error = BlockChainError::transaction_error("test error", None);
		assert!(error.to_string().contains("Transaction Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::transaction_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Transaction Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::transaction_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Transaction Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = BlockChainError::internal_error("test error", None);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
		);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("[timestamp="));

		let error = BlockChainError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
		);
		assert!(error.to_string().contains("Internal Error: test error"));
		assert!(error.to_string().contains("(test source)"));
		assert!(error.to_string().contains("timestamp="));
		assert!(error.to_string().contains("[key1=value1"));
	}

	#[test]
	fn test_from_web3_error() {
		let error = web3::Error::InvalidResponse("test error".to_string());
		let block_chain_error: BlockChainError = error.into();
		assert!(block_chain_error
			.to_string()
			.contains("Request Error: Got invalid response: test error"));
		assert!(block_chain_error.to_string().contains("[timestamp="));
	}
}
