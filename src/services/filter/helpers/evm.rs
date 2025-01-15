//! Helper functions for EVM-specific operations.
//!
//! This module provides utility functions for working with EVM-specific data types
//! and formatting, including address and hash conversions, signature normalization,
//! and token value formatting.

use ethabi::Token;
use web3::types::{H160, H256};

/// Converts an H256 hash to its hexadecimal string representation.
///
/// # Arguments
/// * `hash` - The H256 hash to convert
///
/// # Returns
/// A string in the format "0x..." representing the hash
pub fn h256_to_string(hash: H256) -> String {
	format!("0x{}", hex::encode(hash.as_bytes()))
}

/// Converts a hexadecimal string to an H256 hash.
///
/// # Arguments
/// * `hash_string` - The string to convert, with or without "0x" prefix
///
/// # Returns
/// The converted H256 hash or an error if the string is invalid
///
/// # Errors
/// Returns an error if the input string is not valid hexadecimal
pub fn string_to_h256(hash_string: &str) -> Result<H256, Box<dyn std::error::Error>> {
	let hash_without_prefix = hash_string.strip_prefix("0x").unwrap_or(hash_string);
	let hash_bytes = hex::decode(hash_without_prefix)?;
	Ok(H256::from_slice(&hash_bytes))
}

/// Converts an H160 address to its hexadecimal string representation.
///
/// # Arguments
/// * `address` - The H160 address to convert
///
/// # Returns
/// A string in the format "0x..." representing the address
pub fn h160_to_string(address: H160) -> String {
	format!("0x{}", hex::encode(address.as_bytes()))
}

/// Converts a hexadecimal string to an H160 address.
///
/// # Arguments
/// * `address_string` - The string to convert, with or without "0x" prefix
///
/// # Returns
/// The converted H160 address or an error if the string is invalid
///
/// # Errors
/// Returns an error if the input string is not valid hexadecimal
pub fn string_to_h160(address_string: &str) -> Result<H160, Box<dyn std::error::Error>> {
	let address_without_prefix = address_string.strip_prefix("0x").unwrap_or(address_string);
	let address_bytes = hex::decode(address_without_prefix)?;
	Ok(H160::from_slice(&address_bytes))
}

/// Compares two addresses for equality, ignoring case and "0x" prefixes.
///
/// # Arguments
/// * `address1` - First address to compare
/// * `address2` - Second address to compare
///
/// # Returns
/// `true` if the addresses are equivalent, `false` otherwise
pub fn are_same_address(address1: &str, address2: &str) -> bool {
	normalize_address(address1) == normalize_address(address2)
}

/// Normalizes an address string by removing "0x" prefix, spaces, and converting to lowercase.
///
/// # Arguments
/// * `address` - The address string to normalize
///
/// # Returns
/// The normalized address string
pub fn normalize_address(address: &str) -> String {
	address
		.strip_prefix("0x")
		.unwrap_or(address)
		.replace(" ", "")
		.to_lowercase()
}

/// Compares two function signatures for equality, ignoring case and whitespace.
///
/// # Arguments
/// * `signature1` - First signature to compare
/// * `signature2` - Second signature to compare
///
/// # Returns
/// `true` if the signatures are equivalent, `false` otherwise
pub fn are_same_signature(signature1: &str, signature2: &str) -> bool {
	normalize_signature(signature1) == normalize_signature(signature2)
}

/// Normalizes a function signature by removing spaces and converting to lowercase.
///
/// # Arguments
/// * `signature` - The signature string to normalize
///
/// # Returns
/// The normalized signature string
pub fn normalize_signature(signature: &str) -> String {
	signature.replace(" ", "").to_lowercase()
}

/// Formats an ethabi Token into a consistent string representation.
///
/// # Arguments
/// * `token` - The Token to format
///
/// # Returns
/// A string representation of the token value, with appropriate formatting
/// based on the token type
pub fn format_token_value(token: &Token) -> String {
	match token {
		Token::Address(addr) => format!("0x{:x}", addr),
		Token::FixedBytes(bytes) | Token::Bytes(bytes) => format!("0x{}", hex::encode(bytes)),
		Token::Int(num) | Token::Uint(num) => num.to_string(),
		Token::Bool(b) => b.to_string(),
		Token::String(s) => s.clone(),
		Token::Array(arr) => {
			format!(
				"[{}]",
				arr.iter()
					.map(format_token_value)
					.collect::<Vec<String>>()
					.join(",")
			)
		}
		Token::FixedArray(arr) => {
			format!(
				"[{}]",
				arr.iter()
					.map(format_token_value)
					.collect::<Vec<String>>()
					.join(",")
			)
		}
		Token::Tuple(tuple) => {
			format!(
				"({})",
				tuple
					.iter()
					.map(format_token_value)
					.collect::<Vec<String>>()
					.join(",")
			)
		}
	}
}
