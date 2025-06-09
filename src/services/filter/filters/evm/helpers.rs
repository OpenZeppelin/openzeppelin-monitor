//! Helper functions for EVM-specific operations.
//!
//! This module provides utility functions for working with EVM-specific data types
//! and formatting, including address and hash conversions, signature normalization,
//! and token value formatting.

use alloy::primitives::{Address, B256, I256, U256};
use ethabi::{Hash, Token};
use std::str::FromStr;

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

/// Converts a string to a U256 value.
pub fn string_to_u256(value_str: &str) -> Result<U256, String> {
	let trimmed = value_str.trim();

	if trimmed.is_empty() {
		return Err("Input string is empty".to_string());
	}

	if let Some(hex_val) = trimmed
		.strip_prefix("0x")
		.or_else(|| trimmed.strip_prefix("0X"))
	{
		// Hexadecimal parsing
		if hex_val.is_empty() {
			return Err("Hex string '0x' is missing value digits".to_string());
		}
		U256::from_str_radix(hex_val, 16)
			.map_err(|e| format!("Failed to parse hex '{}': {}", hex_val, e))
	} else {
		// Decimal parsing
		U256::from_str(trimmed).map_err(|e| format!("Failed to parse decimal '{}': {}", trimmed, e))
	}
}

/// Converts a string to an I256 value.
pub fn string_to_i256(value_str: &str) -> Result<I256, String> {
	let trimmed = value_str.trim();
	if trimmed.is_empty() {
		return Err("Input string is empty".to_string());
	}

	if let Some(hex_val_no_sign) = trimmed
		.strip_prefix("0x")
		.or_else(|| trimmed.strip_prefix("0X"))
	{
		if hex_val_no_sign.is_empty() {
			return Err("Hex string '0x' is missing value digits".to_string());
		}
		// Parse hex as U256 first
		U256::from_str_radix(hex_val_no_sign, 16)
			.map_err(|e| format!("Failed to parse hex magnitude '{}': {}", hex_val_no_sign, e))
			.map(I256::from_raw)
	} else {
		I256::from_str(trimmed).map_err(|e| format!("Failed to parse decimal '{}': {}", trimmed, e))
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
	fn test_string_to_u256() {
		// --- Helpers ---
		fn u256_hex_val(hex_str: &str) -> U256 {
			U256::from_str_radix(hex_str.strip_prefix("0x").unwrap_or(hex_str), 16).unwrap()
		}

		// --- Constants for testing ---
		const U256_MAX_STR: &str =
			"115792089237316195423570985008687907853269984665640564039457584007913129639935";
		const U256_MAX_HEX_STR: &str =
			"0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
		const U256_OVERFLOW_STR: &str =
			"115792089237316195423570985008687907853269984665640564039457584007913129639936";
		const U256_HEX_OVERFLOW_STR: &str =
			"0x10000000000000000000000000000000000000000000000000000000000000000";
		const ZERO_STR: &str = "0";
		const SMALL_NUM_STR: &str = "123";
		const SMALL_NUM_HEX_STR: &str = "0x7b"; // 123 in hex

		// --- Valid numbers cases ---
		assert_eq!(string_to_u256(ZERO_STR), Ok(U256::ZERO));
		assert_eq!(
			string_to_u256(SMALL_NUM_STR),
			Ok(U256::from_str(SMALL_NUM_STR).unwrap())
		);
		assert_eq!(string_to_u256(U256_MAX_STR), Ok(U256::MAX));

		// --- Valid hex cases ---
		assert_eq!(string_to_u256("0x0"), Ok(U256::ZERO));
		assert_eq!(string_to_u256("0X0"), Ok(U256::ZERO)); // Case insensitive
		assert_eq!(
			string_to_u256(SMALL_NUM_HEX_STR),
			Ok(u256_hex_val(SMALL_NUM_HEX_STR))
		);
		assert_eq!(string_to_u256(U256_MAX_HEX_STR), Ok(U256::MAX));

		// --- Invalid cases ---
		assert!(string_to_u256("").is_err());
		assert!(string_to_u256("   ").is_err());
		assert!(string_to_u256("0x").is_err());
		assert!(string_to_u256("abc").is_err());
		assert!(string_to_u256("-123").is_err());
		assert!(string_to_u256(U256_OVERFLOW_STR).is_err());
		assert!(string_to_u256(U256_HEX_OVERFLOW_STR).is_err());
	}

	#[test]
	fn test_string_to_i256() {
		// --- Constants for testing ---
		const I256_MAX_STR: &str =
			"57896044618658097711785492504343953926634992332820282019728792003956564819967";
		const I256_MAX_HEX_STR: &str =
			"0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
		const I256_MIN_STR: &str =
			"-57896044618658097711785492504343953926634992332820282019728792003956564819968";
		const I256_MIN_HEX_STR: &str =
			"0x8000000000000000000000000000000000000000000000000000000000000000";
		const I256_POS_OVERFLOW_STR: &str =
			"57896044618658097711785492504343953926634992332820282019728792003956564819968";
		const I256_NEG_OVERFLOW_STR: &str =
			"-57896044618658097711785492504343953926634992332820282019728792003956564819969";
		const I256_HEX_OVERFLOW_STR: &str =
			"0x10000000000000000000000000000000000000000000000000000000000000000";

		// --- Valid numbers cases ---
		assert_eq!(string_to_i256("0"), Ok(I256::ZERO));
		assert_eq!(string_to_i256("123"), Ok(I256::from_str("123").unwrap()));
		assert_eq!(string_to_i256(I256_MAX_STR), Ok(I256::MAX));
		assert_eq!(string_to_i256(I256_MIN_STR), Ok(I256::MIN));
		assert_eq!(string_to_i256("-123"), Ok(I256::from_str("-123").unwrap()));
		assert_eq!(string_to_i256("-0"), Ok(I256::ZERO));

		// --- Valid hex cases ---
		assert_eq!(string_to_i256("0x0"), Ok(I256::ZERO));
		assert_eq!(string_to_i256("0X0"), Ok(I256::ZERO)); // Case insensitive
		assert_eq!(string_to_i256(I256_MAX_HEX_STR), Ok(I256::MAX));
		assert_eq!(string_to_i256(I256_MIN_HEX_STR), Ok(I256::MIN));

		// --- Invalid cases ---
		assert!(string_to_i256("").is_err());
		assert!(string_to_i256("   ").is_err());
		assert!(string_to_i256("0x").is_err());
		assert!(string_to_i256("abc").is_err());
		assert!(string_to_i256("-abc").is_err());
		assert!(string_to_i256(I256_POS_OVERFLOW_STR).is_err());
		assert!(string_to_i256(I256_NEG_OVERFLOW_STR).is_err());
		assert!(string_to_i256(I256_HEX_OVERFLOW_STR).is_err());
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
