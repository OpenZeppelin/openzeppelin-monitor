//! Helper functions for EVM-specific operations.
//!
//! This module provides utility functions for working with EVM-specific data types
//! and formatting, including address and hash conversions, signature normalization,
//! and token value formatting.

use alloy::primitives::{Address, B256};
use ethabi::{Hash, Token};

/// Converts an H256 hash to its hexadecimal string representation.
///
/// # Arguments
/// * `hash` - The H256 hash to convert
///
/// # Returns
/// A string in the format "0x..." representing the hash
pub fn h256_to_string(hash: Hash) -> String {
	format!("0x{}", hex::encode(hash.as_bytes()))
}

/// Converts an B256 hash to its hexadecimal string representation.
///
/// # Arguments
/// * `hash` - The B256 hash to convert
///
/// # Returns
/// A string in the format "0x..." representing the hash
pub fn b256_to_string(hash: B256) -> String {
	format!("0x{}", hex::encode(hash.as_slice()))
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
pub fn string_to_h256(hash_string: &str) -> Result<B256, Box<dyn std::error::Error>> {
	let hash_without_prefix = hash_string.strip_prefix("0x").unwrap_or(hash_string);
	let hash_bytes = hex::decode(hash_without_prefix)?;
	Ok(B256::from_slice(&hash_bytes))
}

/// Converts an H160 address to its hexadecimal string representation.
///
/// # Arguments
/// * `address` - The H160 address to convert
///
/// # Returns
/// A string in the format "0x..." representing the address
pub fn h160_to_string(address: Address) -> String {
	format!("0x{}", hex::encode(address.as_slice()))
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
pub fn string_to_h160(address_string: &str) -> Result<Address, Box<dyn std::error::Error>> {
	let address_without_prefix = address_string.strip_prefix("0x").unwrap_or(address_string);
	let address_bytes = hex::decode(address_without_prefix)?;
	Ok(Address::from_slice(&address_bytes))
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

#[cfg(test)]
mod tests {
	use super::*;
	use alloy::primitives::{hex, Address, B256};
	use ethabi::Token;

	#[test]
	fn test_h256_to_string() {
		let hash_bytes =
			hex::decode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
				.unwrap();
		let hash = Hash::from_slice(&hash_bytes);
		let result = h256_to_string(hash);
		assert_eq!(
			result,
			"0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
		);
	}

	#[test]
	fn test_b256_to_string() {
		let hash_bytes =
			hex::decode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
				.unwrap();
		let hash = B256::from_slice(&hash_bytes);
		let result = b256_to_string(hash);
		assert_eq!(
			result,
			"0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
		);
	}

	#[test]
	fn test_string_to_h256() {
		let hash_str = "0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
		let result = string_to_h256(hash_str).unwrap();
		assert_eq!(
			b256_to_string(result),
			"0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
		);

		// Test without 0x prefix
		let hash_str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
		let result = string_to_h256(hash_str).unwrap();
		assert_eq!(
			b256_to_string(result),
			"0x000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
		);

		// Test invalid hex string
		let result = string_to_h256("invalid_hex");
		assert!(result.is_err());
	}

	#[test]
	fn test_h160_to_string() {
		let address_bytes = hex::decode("0123456789abcdef0123456789abcdef01234567").unwrap();
		let address = Address::from_slice(&address_bytes);
		let result = h160_to_string(address);
		assert_eq!(result, "0x0123456789abcdef0123456789abcdef01234567");
	}

	#[test]
	fn test_string_to_h160() {
		let address_str = "0x0123456789abcdef0123456789abcdef01234567";
		let result = string_to_h160(address_str).unwrap();
		assert_eq!(
			h160_to_string(result),
			"0x0123456789abcdef0123456789abcdef01234567"
		);

		// Test without 0x prefix
		let address_str = "0123456789abcdef0123456789abcdef01234567";
		let result = string_to_h160(address_str).unwrap();
		assert_eq!(
			h160_to_string(result),
			"0x0123456789abcdef0123456789abcdef01234567"
		);

		// Test invalid hex string
		let result = string_to_h160("invalid_hex");
		assert!(result.is_err());
	}

	#[test]
	fn test_are_same_address() {
		assert!(are_same_address(
			"0x0123456789abcdef0123456789abcdef01234567",
			"0x0123456789ABCDEF0123456789ABCDEF01234567"
		));
		assert!(are_same_address(
			"0123456789abcdef0123456789abcdef01234567",
			"0x0123456789abcdef0123456789abcdef01234567"
		));
		assert!(!are_same_address(
			"0x0123456789abcdef0123456789abcdef01234567",
			"0x0123456789abcdef0123456789abcdef01234568"
		));
	}

	#[test]
	fn test_normalize_address() {
		assert_eq!(
			normalize_address("0x0123456789ABCDEF0123456789ABCDEF01234567"),
			"0123456789abcdef0123456789abcdef01234567"
		);
		assert_eq!(
			normalize_address("0123456789ABCDEF0123456789ABCDEF01234567"),
			"0123456789abcdef0123456789abcdef01234567"
		);
		assert_eq!(
			normalize_address("0x0123456789abcdef 0123456789abcdef01234567"),
			"0123456789abcdef0123456789abcdef01234567"
		);
	}

	#[test]
	fn test_are_same_signature() {
		assert!(are_same_signature(
			"transfer(address,uint256)",
			"transfer(address, uint256)"
		));
		assert!(are_same_signature(
			"TRANSFER(address,uint256)",
			"transfer(address,uint256)"
		));
		assert!(!are_same_signature(
			"transfer(address,uint256)",
			"transfer(address,uint128)"
		));
	}

	#[test]
	fn test_normalize_signature() {
		assert_eq!(
			normalize_signature("transfer(address, uint256)"),
			"transfer(address,uint256)"
		);
		assert_eq!(
			normalize_signature("TRANSFER(address,uint256)"),
			"transfer(address,uint256)"
		);
		assert_eq!(
			normalize_signature("transfer (address , uint256 )"),
			"transfer(address,uint256)"
		);
	}

	#[test]
	fn test_format_token_value() {
		// Test Address
		let address = ethabi::Address::from_slice(
			&hex::decode("0123456789abcdef0123456789abcdef01234567").unwrap(),
		);
		assert_eq!(
			format_token_value(&Token::Address(address)),
			"0x0123456789abcdef0123456789abcdef01234567"
		);

		// Test Bytes
		let bytes = hex::decode("0123456789").unwrap();
		assert_eq!(
			format_token_value(&Token::Bytes(bytes.clone())),
			"0x0123456789"
		);
		assert_eq!(
			format_token_value(&Token::FixedBytes(bytes)),
			"0x0123456789"
		);

		// Test Numbers
		assert_eq!(
			format_token_value(&Token::Int(ethabi::Int::from(123))),
			"123"
		);
		assert_eq!(
			format_token_value(&Token::Uint(ethabi::Uint::from(456))),
			"456"
		);

		// Test Bool
		assert_eq!(format_token_value(&Token::Bool(true)), "true");
		assert_eq!(format_token_value(&Token::Bool(false)), "false");

		// Test String
		assert_eq!(
			format_token_value(&Token::String("test".to_string())),
			"test"
		);

		// Test Array
		let arr = vec![
			Token::Uint(ethabi::Uint::from(1)),
			Token::Uint(ethabi::Uint::from(2)),
		];
		assert_eq!(format_token_value(&Token::Array(arr.clone())), "[1,2]");
		assert_eq!(format_token_value(&Token::FixedArray(arr)), "[1,2]");

		// Test Tuple
		let tuple = vec![
			Token::String("test".to_string()),
			Token::Uint(ethabi::Uint::from(123)),
		];
		assert_eq!(format_token_value(&Token::Tuple(tuple)), "(test,123)");
	}
}
