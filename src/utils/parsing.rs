//! Parsing utilities
//!
//! This module provides utilities for parsing various types of data.

use byte_unit::Byte;

/// Parses a string argument into a `u64` value representing a file size.
///
/// Accepts human-readable formats like "1GB", "500MB", "1024KB", etc.
/// Returns an error if the format is invalid.
pub fn parse_string_to_bytes_size(s: &str) -> Result<u64, String> {
	match Byte::from_str(s) {
		Ok(byte) => Ok(byte.get_bytes() as u64),
		Err(e) => Err(format!("Invalid size format: '{}'. Error: {}", s, e)),
	}
}
