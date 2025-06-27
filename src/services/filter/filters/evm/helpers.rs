//! Helper functions for EVM-specific operations.
//!
//! This module provides utility functions for working with EVM-specific data types
//! and formatting, including address and hash conversions, signature normalization,
//! and token value formatting.

use alloy::core::dyn_abi::DynSolValue;
use alloy::primitives::{Address, B256, I256, U256};
use std::str::FromStr;

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

/// Formats a DynSolValue into a consistent string representation.
///
/// # Arguments
/// * `token` - The DynSolValue to format
///
/// # Returns
/// A string representation of the token value, with appropriate formatting
/// based on the token type
pub fn format_token_value(token: &DynSolValue) -> String {
	match token {
		DynSolValue::Address(addr) => format!("0x{:x}", addr),
		DynSolValue::FixedBytes(bytes, _) => format!("0x{}", hex::encode(bytes)),
		DynSolValue::Bytes(bytes) => format!("0x{}", hex::encode(bytes)),
		DynSolValue::Int(num, _) => num.to_string(),
		DynSolValue::Uint(num, _) => num.to_string(),
		DynSolValue::Bool(b) => b.to_string(),
		DynSolValue::String(s) => s.clone(),
		DynSolValue::Array(arr) => {
			format!(
				"[{}]",
				arr.iter()
					.map(format_token_value)
					.collect::<Vec<String>>()
					.join(",")
			)
		}
		DynSolValue::FixedArray(arr) => {
			format!(
				"[{}]",
				arr.iter()
					.map(format_token_value)
					.collect::<Vec<String>>()
					.join(",")
			)
		}
		DynSolValue::Tuple(tuple) => {
			format!(
				"({})",
				tuple
					.iter()
					.map(dyn_value_to_string)
					.collect::<Vec<String>>()
					.join(",")
			)
		}
		DynSolValue::Function(selector) => format!("0x{}", hex::encode(selector)),
	}
}

/// Convert a DynSolValue into serde_json::Value for structured JSON output
///
/// # Arguments
/// * `val` - The DynSolValue to convert
///
/// # Returns
/// A String representing the DynSolValue
pub fn dyn_value_to_string(val: &DynSolValue) -> String {
	match val {
		DynSolValue::Bool(b) => b.to_string(),
		DynSolValue::String(s) => format!("\"{}\"", s),
		DynSolValue::Address(addr) => format!("\"0x{:x}\"", addr),
		DynSolValue::Uint(u, _) => u.to_string(),
		DynSolValue::Int(i, _) => i.to_string(),
		DynSolValue::FixedBytes(bytes, _) => format!("\"0x{}\"", hex::encode(bytes)),
		DynSolValue::Bytes(bytes) => format!("\"0x{}\"", hex::encode(bytes)),
		DynSolValue::Array(arr) => format!(
			"[{}]",
			arr.iter()
				.map(dyn_value_to_string)
				.collect::<Vec<String>>()
				.join(",")
		),
		DynSolValue::FixedArray(arr) => format!(
			"[{}]",
			arr.iter()
				.map(dyn_value_to_string)
				.collect::<Vec<String>>()
				.join(",")
		),
		DynSolValue::Tuple(fields) => format!(
			"({})",
			fields
				.iter()
				.map(dyn_value_to_string)
				.collect::<Vec<String>>()
				.join(",")
		),
		DynSolValue::Function(selector) => format!("\"0x{}\"", hex::encode(selector)),
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
		assert_eq!(string_to_i256("-123"), Ok(I256::try_from(-123).unwrap()));
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
		let address =
			Address::from_slice(&hex::decode("0123456789abcdef0123456789abcdef01234567").unwrap());
		assert_eq!(
			format_token_value(&DynSolValue::Address(address)),
			format!("0x{:x}", address)
		);

		// Test Bytes
		let bytes = hex::decode("0123456789").unwrap();
		assert_eq!(
			format_token_value(&DynSolValue::Bytes(bytes.clone())),
			format!("0x{}", hex::encode(bytes.clone()))
		);

		// Test FixedBytes with 32-byte hash
		let hash_bytes =
			hex::decode("abcdef0123456789abcdef0123456789abcdef0123456789abcdef012345678a")
				.unwrap();
		let mut fixed_bytes = [0u8; 32];
		fixed_bytes[..hash_bytes.len()].copy_from_slice(&hash_bytes);
		assert_eq!(
			format_token_value(&DynSolValue::FixedBytes(
				alloy::primitives::FixedBytes::<32>::from(fixed_bytes),
				32
			)),
			format!("0x{}", hex::encode(hash_bytes))
		);

		// Test Numbers
		assert_eq!(
			format_token_value(&DynSolValue::Int(I256::try_from(-123).unwrap(), 256)),
			"-123"
		);
		assert_eq!(
			format_token_value(&DynSolValue::Uint(U256::from(456), 256)),
			"456"
		);

		// Test formatting unsigned int with Int type
		assert_eq!(
			format_token_value(&DynSolValue::Int(I256::try_from(456).unwrap(), 256)),
			"456"
		);

		// Test formatting -1 as Uint (should show U256::MAX)
		let negative_one_as_uint = DynSolValue::Uint(U256::MAX, 256);
		assert_eq!(
			format_token_value(&negative_one_as_uint),
			"115792089237316195423570985008687907853269984665640564039457584007913129639935" // U256::MAX instead of -1
		);

		// Test formatting large unsigned value (>INT256_MAX) as Int type
		// This should appear negative due to sign bit interpretation
		let large_uint_as_int = DynSolValue::Int(
			I256::from_raw(U256::from(2).pow(U256::from(255))), // 2^255 (just over INT256_MAX)
			256,
		);
		assert_eq!(
			format_token_value(&large_uint_as_int),
			"-57896044618658097711785492504343953926634992332820282019728792003956564819968" // Shows as negative!
		);

		// Test Bool
		assert_eq!(format_token_value(&DynSolValue::Bool(true)), "true");
		assert_eq!(format_token_value(&DynSolValue::Bool(false)), "false");

		// Test String
		assert_eq!(
			format_token_value(&DynSolValue::String("hello world".to_string())),
			"hello world"
		);

		// Test Array (empty and non-empty)
		assert_eq!(format_token_value(&DynSolValue::Array(vec![])), "[]");
		let arr = vec![
			DynSolValue::Uint(U256::from(1), 256),
			DynSolValue::Uint(U256::from(2), 256),
		];
		assert_eq!(
			format_token_value(&DynSolValue::Array(arr.clone())),
			"[1,2]"
		);
		assert_eq!(format_token_value(&DynSolValue::FixedArray(arr)), "[1,2]");

		// Test nested structures
		let nested_tuple = vec![
			DynSolValue::String("transfer".to_string()),
			DynSolValue::Address(Address::from_slice(
				&hex::decode("0123456789abcdef0123456789abcdef01234567").unwrap(),
			)),
			DynSolValue::Uint(U256::from(1000), 256),
		];
		assert_eq!(
			format_token_value(&DynSolValue::Tuple(nested_tuple)),
			"(\"transfer\",\"0x0123456789abcdef0123456789abcdef01234567\",1000)"
		);

		// Test Function - represents function selector (4 bytes) + address (20 bytes)
		// This is a more realistic function pointer with actual function selector and address
		let transfer_selector = [0xa9, 0x05, 0x9c, 0xbb]; // transfer(address,uint256) selector
		let contract_address = hex::decode("0123456789abcdef0123456789abcdef01234567").unwrap();
		let mut function_bytes = [0u8; 24];
		function_bytes[..4].copy_from_slice(&transfer_selector);
		function_bytes[4..24].copy_from_slice(&contract_address);

		assert_eq!(
			format_token_value(&DynSolValue::Function(alloy::primitives::Function::from(
				function_bytes
			))),
			format!("0x{}", hex::encode(function_bytes))
		);
	}

	#[test]
	fn test_dyn_value_to_string() {
		// Test Bool values
		assert_eq!(dyn_value_to_string(&DynSolValue::Bool(true)), "true");
		assert_eq!(dyn_value_to_string(&DynSolValue::Bool(false)), "false");

		// Test String values
		assert_eq!(
			dyn_value_to_string(&DynSolValue::String("hello world".to_string())),
			"\"hello world\""
		);
		assert_eq!(
			dyn_value_to_string(&DynSolValue::String("".to_string())),
			"\"\""
		);

		// Test Address values
		let address =
			Address::from_slice(&hex::decode("0123456789abcdef0123456789abcdef01234567").unwrap());
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Address(address)),
			"\"0x0123456789abcdef0123456789abcdef01234567\""
		);

		// Test Uint values
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Uint(U256::from(0), 256)),
			"0"
		);
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Uint(U256::from(123), 256)),
			"123"
		);
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Uint(U256::from(u64::MAX), 256)),
			u64::MAX.to_string()
		);

		// Test large Uint values
		let large_uint = U256::MAX;
		let result = dyn_value_to_string(&DynSolValue::Uint(large_uint, 256));
		assert_eq!(result, large_uint.to_string());

		// Test Int values
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Int(I256::try_from(0).unwrap(), 256)),
			"0"
		);
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Int(I256::try_from(-123).unwrap(), 256)),
			"-123"
		);
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Int(I256::try_from(456).unwrap(), 256)),
			"456"
		);

		// Test Array values
		assert_eq!(dyn_value_to_string(&DynSolValue::Array(vec![])), "[]");

		let simple_array = vec![
			DynSolValue::Bool(true),
			DynSolValue::Uint(U256::from(42), 256),
			DynSolValue::String("test".to_string()),
		];
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Array(simple_array)),
			"[true,42,\"test\"]"
		);

		// Test Tuple values
		let simple_tuple = vec![
			DynSolValue::Address(address),
			DynSolValue::Uint(U256::from(1000), 256),
			DynSolValue::Bool(false),
		];
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Tuple(simple_tuple)),
			"(\"0x0123456789abcdef0123456789abcdef01234567\",1000,false)"
		);

		// Test nested structures
		let nested_array = vec![
			DynSolValue::Array(vec![
				DynSolValue::Uint(U256::from(1), 256),
				DynSolValue::Uint(U256::from(2), 256),
			]),
			DynSolValue::Tuple(vec![
				DynSolValue::String("nested".to_string()),
				DynSolValue::Bool(true),
			]),
		];
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Array(nested_array)),
			"[[1,2],(\"nested\",true)]"
		);

		// Test that FixedArray behaves identically to Array
		let test_data = vec![
			DynSolValue::Address(address),
			DynSolValue::Uint(U256::from(999), 256),
		];
		let array_result = dyn_value_to_string(&DynSolValue::Array(test_data.clone()));
		let fixed_array_result = dyn_value_to_string(&DynSolValue::FixedArray(test_data));
		assert_eq!(array_result, fixed_array_result);
		assert_eq!(
			array_result,
			"[\"0x0123456789abcdef0123456789abcdef01234567\",999]"
		);

		// Test Bytes values
		let empty_bytes = vec![];
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Bytes(empty_bytes)),
			"\"0x\""
		);

		let some_bytes = vec![0xde, 0xad, 0xbe, 0xef];
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Bytes(some_bytes)),
			"\"0xdeadbeef\""
		);

		let longer_bytes = vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
		assert_eq!(
			dyn_value_to_string(&DynSolValue::Bytes(longer_bytes)),
			"\"0x0123456789abcdef\""
		);

		// Test FixedBytes
		let mut fixed_bytes = [0u8; 32];
		fixed_bytes[0..4].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
		let fixed_bytes_val =
			DynSolValue::FixedBytes(alloy::primitives::FixedBytes::<32>::from(fixed_bytes), 4);
		let fixed_bytes_result = dyn_value_to_string(&fixed_bytes_val);
		assert!(fixed_bytes_result.starts_with("\"0x"));
		assert!(fixed_bytes_result.ends_with("\""));

		// Test Function
		let function_bytes = [
			0xa9, 0x05, 0x9c, 0xbb, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23,
			0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
		];
		let function_val = DynSolValue::Function(alloy::primitives::Function::from(function_bytes));
		let function_result = dyn_value_to_string(&function_val);
		assert!(function_result.starts_with("\"0x"));
		assert!(function_result.ends_with("\""));
		assert_eq!(function_result.len(), 52); // "0x" + 48 hex chars + 2 quotes
	}
}
