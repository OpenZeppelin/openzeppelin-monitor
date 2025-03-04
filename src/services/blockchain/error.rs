//! Blockchain error types and handling.
//!
//! This module provides a comprehensive error handling system for blockchain operations,
//! including network connectivity, request processing, and blockchain-specific errors.

use std::collections::HashMap;

use crate::utils::{new_error, new_error_with_source, ErrorContext, ErrorContextProvider};

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
	fn target() -> &'static str {
		"blockchain"
	}
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
	/// Creates a new connection error with logging and optional source error
	pub fn connection_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::ConnectionError,
			"Connection Error",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new connection error with logging and source error
	pub fn connection_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::ConnectionError,
			"Connection Error",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new request error with logging
	pub fn request_error<T: ErrorContextProvider + 'static>(
		msg: impl Into<String>,
		source: Option<T>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		if let Some(source) = source {
			new_error_with_source(
				Self::RequestError,
				"Request Error",
				msg,
				source,
				metadata,
				target,
			)
		} else {
			new_error(Self::RequestError, "Request Error", msg, metadata, target)
		}
	}

	/// Creates a new block not found error with logging
	pub fn block_not_found(
		number: u64,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::BlockNotFound,
			"Block Not Found Error",
			number.to_string(),
			metadata,
			target,
		)
	}

	/// Creates a new block not found error with logging and source error
	pub fn block_not_found_with_source(
		number: u64,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::BlockNotFound,
			"Block Not Found Error",
			number.to_string(),
			source,
			metadata,
			target,
		)
	}

	/// Creates a new transaction error with logging
	pub fn transaction_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::TransactionError,
			"Transaction Error",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new transaction error with logging and source error
	pub fn transaction_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::TransactionError,
			"Transaction Error",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new internal error with logging
	pub fn internal_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(Self::InternalError, "Internal Error", msg, metadata, target)
	}

	/// Creates a new internal error with logging and source error
	pub fn internal_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::InternalError,
			"Internal Error",
			msg,
			source,
			metadata,
			target,
		)
	}

	/// Creates a new client pool error with logging
	pub fn client_pool_error(
		msg: impl Into<String>,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error(
			Self::ClientPoolError,
			"Client Pool Error",
			msg,
			metadata,
			target,
		)
	}

	/// Creates a new client pool error with logging and source error
	pub fn client_pool_error_with_source(
		msg: impl Into<String>,
		source: impl ErrorContextProvider + 'static,
		metadata: Option<HashMap<String, String>>,
		target: Option<&str>,
	) -> Self {
		new_error_with_source(
			Self::ClientPoolError,
			"Client Pool Error",
			msg,
			source,
			metadata,
			target,
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
		Self::request_error::<ErrorContext<String>>(err.to_string(), None, None, None)
	}
}

#[cfg(test)]
mod tests {
	use crate::utils::format_target_with_source;

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
		let error =
			BlockChainError::request_error::<BlockChainError>("test error", None, None, None);
		assert_eq!(error.to_string(), "test error");

		let error = BlockChainError::request_error(
			"test error",
			Some(std::io::Error::new(
				std::io::ErrorKind::NotFound,
				"test source",
			)),
			None,
			None,
		);
		assert_eq!(error.to_string(), "test error (test source)");

		let error = BlockChainError::request_error(
			"test error",
			Some(std::io::Error::new(
				std::io::ErrorKind::NotFound,
				"test source",
			)),
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

	#[test]
	fn test_error_context_display() {
		let inner_error = BlockChainError::request_error::<ErrorContext<String>>(
			"HTTP error 429 Too Many Requests",
			None,
			None,
			Some("send_raw_request"),
		);

		let err_ctx = inner_error.provide_error_context();
		let target = format_target_with_source(Some("get_transactions"), err_ctx);
		let middle_error = BlockChainError::request_error::<BlockChainError>(
			inner_error.to_string(),
			None,
			None,
			Some(&target),
		);

		let outer_error = BlockChainError::request_error::<BlockChainError>(
			"Failed to get transactions",
			Some(middle_error),
			None,
			Some("filter_block"),
		);

		// Get the target of the outer error
		let target = outer_error
			.provide_error_context()
			.unwrap()
			.get_recursive_source_target();

		assert_eq!(
			target,
			"blockchain::filter_block::blockchain::get_transactions::blockchain::send_raw_request"
		);
	}
}
