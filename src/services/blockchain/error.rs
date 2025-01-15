//! Blockchain error types and handling.
//!
//! This module provides a comprehensive error handling system for blockchain operations,
//! including network connectivity, request processing, and blockchain-specific errors.

use crate::services::blockwatcher::BlockWatcherError;

use log::error;

/// Represents possible errors that can occur during blockchain operations
#[derive(Debug)]
pub enum BlockChainError {
	/// Errors related to network connectivity issues
	ConnectionError(String),

	/// Errors related to malformed requests or invalid responses
	RequestError(String),

	/// When a requested block cannot be found on the blockchain
	///
	/// Contains the block number that was not found
	BlockNotFound(u64),

	/// Errors related to transaction processing
	TransactionError(String),

	/// Internal errors within the blockchain client
	InternalError(String),
}

impl BlockChainError {
	/// Formats the error message based on the error type
	fn format_message(&self) -> String {
		match self {
			Self::ConnectionError(msg) => format!("Connection error: {}", msg),
			Self::RequestError(msg) => format!("Request error: {}", msg),
			Self::BlockNotFound(number) => format!("Block not found: {}", number),
			Self::TransactionError(msg) => format!("Transaction error: {}", msg),
			Self::InternalError(msg) => format!("Internal error: {}", msg),
		}
	}

	/// Creates a new connection error with logging
	pub fn connection_error(msg: impl Into<String>) -> Self {
		let error = Self::ConnectionError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new request error with logging
	pub fn request_error(msg: impl Into<String>) -> Self {
		let error = Self::RequestError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new block not found error with logging
	pub fn block_not_found(number: u64) -> Self {
		let error = Self::BlockNotFound(number);
		error!("{}", error.format_message());
		error
	}

	/// Creates a new transaction error with logging
	pub fn transaction_error(msg: impl Into<String>) -> Self {
		let error = Self::TransactionError(msg.into());
		error!("{}", error.format_message());
		error
	}

	/// Creates a new internal error with logging
	pub fn internal_error(msg: impl Into<String>) -> Self {
		let error = Self::InternalError(msg.into());
		error!("{}", error.format_message());
		error
	}
}

// Standard error trait implementations
impl std::fmt::Display for BlockChainError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.format_message())
	}
}

impl std::error::Error for BlockChainError {}

/// Conversion from Web3 errors to BlockChainError
impl From<web3::Error> for BlockChainError {
	fn from(err: web3::Error) -> Self {
		Self::request_error(err.to_string())
	}
}

/// Conversion from BlockChainError to BlockWatcherError
impl From<BlockChainError> for BlockWatcherError {
	fn from(err: BlockChainError) -> Self {
		BlockWatcherError::network_error(err.to_string())
	}
}
