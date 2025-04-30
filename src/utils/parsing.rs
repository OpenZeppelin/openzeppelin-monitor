//! Parsing utilities
//!
//! This module provides utilities for parsing various types of data.

use byte_unit::Byte;
use std::str::FromStr;

/// Parses a string argument into a `u64` value representing a file size.
///
/// Accepts human-readable formats like "1GB", "500MB", "1024KB", etc.
/// Returns an error if the format is invalid.
pub fn parse_string_to_bytes_size(s: &str) -> Result<u64, String> {
	match Byte::from_str(s) {
		Ok(byte) => Ok(byte.as_u64()),
		Err(e) => Err(format!("Invalid size format: '{}'. Error: {}", s, e)),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_valid_size_formats() {
		let test_cases = vec![
			("1B", 1),
			("1KB", 1000),
			("1KiB", 1024),
			("1MB", 1000 * 1000),
			("1MiB", 1024 * 1024),
			("1GB", 1000 * 1000 * 1000),
			("1GiB", 1024 * 1024 * 1024),
			("1.5GB", (1.5 * 1000.0 * 1000.0 * 1000.0) as u64),
			("500MB", 500 * 1000 * 1000),
			("0B", 0),
		];

		for (input, expected) in test_cases {
			let result = parse_string_to_bytes_size(input);
			assert!(result.is_ok(), "Failed to parse valid input: {}", input);
			assert_eq!(
				result.unwrap(),
				expected,
				"Incorrect parsing for input: {}",
				input
			);
		}
	}

	#[test]
	fn test_invalid_size_formats() {
		let invalid_inputs = vec!["", "invalid", "GB", "-1GB", "1.5.5GB", "1GB2"];

		for input in invalid_inputs {
			let result = parse_string_to_bytes_size(input);
			assert!(
				result.is_err(),
				"Expected error for invalid input: {}",
				input
			);
		}
	}
}
