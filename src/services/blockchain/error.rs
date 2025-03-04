//! Blockchain error types and handling.
//!
//! This module provides a comprehensive error handling system for blockchain operations,
//! including network connectivity, request processing, and blockchain-specific errors.

use std::collections::HashMap;

use crate::utils::{EnhancedContext, ErrorContext, ErrorContextProvider};

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

	/// Errors related to client pool
	ClientPoolError(ErrorContext<String>),
}

impl ErrorContextProvider for BlockChainError {
	fn provide_error_context(&self) -> Option<&ErrorContext<String>> {
		match self {
			Self::ConnectionError(ctx) => Some(ctx),
			Self::RequestError(ctx) => Some(ctx),
			Self::BlockNotFound(ctx) => Some(ctx),
			Self::TransactionError(ctx) => Some(ctx),
			Self::InternalError(ctx) => Some(ctx),
			Self::ClientPoolError(ctx) => Some(ctx),
		}
	}
}

impl BlockChainError {
	const TARGET: &str = "blockchain";

	fn format_target(target: Option<&str>) -> String {
		if let Some(target) = target {
			format!("{}::{}", Self::TARGET, target)
		} else {
			Self::TARGET.to_string()
		}
	}

	/// Creates a new connection error with logging and optional source error
	pub fn connection_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::ConnectionError(
			ErrorContext::new(
				"Connection Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new connection error with logging and source error
	pub fn connection_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::ConnectionError(
			ErrorContext::new(
				"Connection Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new request error with logging
	pub fn request_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::RequestError(
			ErrorContext::new(
				"Request Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new request error with logging and source error
	pub fn request_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::RequestError(
			ErrorContext::new(
				"Request Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new block not found error with logging
	pub fn block_not_found(
		number: u64,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::BlockNotFound(
			ErrorContext::new(
				"Block Not Found Error",
				number.to_string(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new block not found error with logging and source error
	pub fn block_not_found_with_source(
		number: u64,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::BlockNotFound(
			ErrorContext::new(
				"Block Not Found Error",
				number.to_string(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new transaction error with logging
	pub fn transaction_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::TransactionError(
			ErrorContext::new(
				"Transaction Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new transaction error with logging and source error
	pub fn transaction_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::TransactionError(
			ErrorContext::new(
				"Transaction Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new internal error with logging
	pub fn internal_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				"Internal Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new internal error with logging and source error
	pub fn internal_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::InternalError(
			ErrorContext::new(
				"Internal Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new client pool error with logging
	pub fn client_pool_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::ClientPoolError(
			ErrorContext::new(
				"Client Pool Error",
				msg.into(),
				EnhancedContext::new(None).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
		)
	}

	/// Creates a new client pool error with logging and source error
	pub fn client_pool_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		Self::ClientPoolError(
			ErrorContext::new(
				"Client Pool Error",
				msg.into(),
				EnhancedContext::new(Some(Box::new(source))).with_metadata(metadata),
			)
			.with_target(Self::format_target(target)),
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
			Self::ClientPoolError(ctx) => ctx.fmt(f),
		}
	}
}

/// Conversion from Web3 errors to BlockChainError
impl From<web3::Error> for BlockChainError {
	fn from(err: web3::Error) -> Self {
		Self::request_error(err.to_string(), None, None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_connection_error_formatting() {
		let error = BlockChainError::connection_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockChainError::connection_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = BlockChainError::connection_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_request_error_formatting() {
		let error = BlockChainError::request_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockChainError::request_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = BlockChainError::request_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_block_not_found_formatting() {
		let error = BlockChainError::block_not_found(1, None, None);
		assert_eq!(error.to_string(), "1");

		let error = BlockChainError::block_not_found_with_source(
			1,
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "1 (test source)");

		let error = BlockChainError::block_not_found_with_source(
			1,
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "1 (test source [key1=value1])");
	}

	#[test]
	fn test_transaction_error_formatting() {
		let error = BlockChainError::transaction_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockChainError::transaction_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = BlockChainError::transaction_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_internal_error_formatting() {
		let error = BlockChainError::internal_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockChainError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = BlockChainError::internal_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_client_pool_error_formatting() {
		let error = BlockChainError::client_pool_error("test error", None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockChainError::client_pool_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = BlockChainError::client_pool_error_with_source(
			"test error",
			std::io::Error::new(std::io::ErrorKind::NotFound, "test source"),
			Some(HashMap::from([("key1".to_string(), "value1".to_string())])),
			None,
		);
		assert_eq!(error.to_string(), "test error (test source [key1=value1])");
	}

	#[test]
	fn test_from_web3_error() {
		let error = web3::Error::InvalidResponse("test error".to_string());
		let block_chain_error: BlockChainError = error.into();
		assert_eq!(
			block_chain_error.to_string(),
			"Got invalid response: test error"
		);
	}
}
