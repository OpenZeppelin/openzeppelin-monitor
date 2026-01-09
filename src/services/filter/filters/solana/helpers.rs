//! Helper functions for Solana-specific operations.
//!
//! This module provides utility functions for working with Solana-specific data types
//! and formatting, including address normalization, signature matching, and
//! instruction data parsing.

use sha2::{Digest, Sha256};

/// Base58 alphabet used by Bitcoin/Solana
const BASE58_ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

/// Base58 reverse lookup table (character -> index)
fn base58_char_to_index(c: u8) -> Option<u8> {
	match c {
		b'1'..=b'9' => Some(c - b'1'),
		b'A'..=b'H' => Some(c - b'A' + 9),
		b'J'..=b'N' => Some(c - b'J' + 17),
		b'P'..=b'Z' => Some(c - b'P' + 22),
		b'a'..=b'k' => Some(c - b'a' + 33),
		b'm'..=b'z' => Some(c - b'm' + 44),
		_ => None,
	}
}

/// Normalizes a Solana address (public key) to a consistent format.
///
/// Solana addresses are base58-encoded 32-byte public keys.
/// This function trims whitespace and validates the format.
///
/// # Arguments
/// * `address` - The address string to normalize
///
/// # Returns
/// The normalized address string, or the original if normalization fails
pub fn normalize_address(address: &str) -> String {
	address.trim().to_string()
}

/// Compares two Solana addresses for equality.
///
/// Handles case-sensitivity (base58 is case-sensitive).
///
/// # Arguments
/// * `addr1` - First address to compare
/// * `addr2` - Second address to compare
///
/// # Returns
/// `true` if addresses are equal, `false` otherwise
pub fn are_same_address(addr1: &str, addr2: &str) -> bool {
	normalize_address(addr1) == normalize_address(addr2)
}

/// Compares two instruction signatures for equality.
///
/// Signatures are case-sensitive and must match exactly.
///
/// # Arguments
/// * `sig1` - First signature to compare
/// * `sig2` - Second signature to compare
///
/// # Returns
/// `true` if signatures match, `false` otherwise
pub fn are_same_signature(sig1: &str, sig2: &str) -> bool {
	sig1.trim() == sig2.trim()
}

/// Calculates the Anchor instruction discriminator from an instruction name.
///
/// Anchor uses the first 8 bytes of SHA256("global:<instruction_name>")
/// as the instruction discriminator.
///
/// # Arguments
/// * `instruction_name` - The name of the instruction
///
/// # Returns
/// The 8-byte discriminator as a vector
pub fn calculate_discriminator(instruction_name: &str) -> Vec<u8> {
	let preimage = format!("global:{}", instruction_name);
	let mut hasher = Sha256::new();
	hasher.update(preimage.as_bytes());
	let result = hasher.finalize();
	result[..8].to_vec()
}

/// Calculates the Anchor event discriminator from an event name.
///
/// Anchor uses the first 8 bytes of SHA256("event:<event_name>")
/// as the event discriminator.
///
/// # Arguments
/// * `event_name` - The name of the event
///
/// # Returns
/// The 8-byte discriminator as a vector
pub fn calculate_event_discriminator(event_name: &str) -> Vec<u8> {
	let preimage = format!("event:{}", event_name);
	let mut hasher = Sha256::new();
	hasher.update(preimage.as_bytes());
	let result = hasher.finalize();
	result[..8].to_vec()
}

/// Decodes base58-encoded data to bytes.
///
/// # Arguments
/// * `data` - The base58-encoded string
///
/// # Returns
/// The decoded bytes, or an error if decoding fails
pub fn decode_base58(data: &str) -> Result<Vec<u8>, String> {
	let data = data.as_bytes();

	// Count leading '1's (they represent leading zero bytes)
	let leading_zeros = data.iter().take_while(|&&c| c == b'1').count();

	// Convert base58 to big integer representation
	let mut result: Vec<u8> = Vec::new();

	for &c in data.iter().skip(leading_zeros) {
		let Some(carry) = base58_char_to_index(c) else {
			return Err(format!("Invalid base58 character: {}", c as char));
		};

		let mut carry = carry as u32;
		for byte in result.iter_mut().rev() {
			carry += (*byte as u32) * 58;
			*byte = (carry & 0xff) as u8;
			carry >>= 8;
		}

		while carry > 0 {
			result.insert(0, (carry & 0xff) as u8);
			carry >>= 8;
		}
	}

	// Prepend leading zero bytes
	let mut final_result = vec![0u8; leading_zeros];
	final_result.extend(result);

	Ok(final_result)
}

/// Encodes bytes to base58.
///
/// # Arguments
/// * `data` - The bytes to encode
///
/// # Returns
/// The base58-encoded string
pub fn encode_base58(data: &[u8]) -> String {
	if data.is_empty() {
		return String::new();
	}

	// Count leading zeros
	let leading_zeros = data.iter().take_while(|&&b| b == 0).count();

	// Convert bytes to base58
	let mut digits: Vec<u8> = Vec::new();

	for &byte in data.iter().skip(leading_zeros) {
		let mut carry = byte as u32;
		for digit in digits.iter_mut() {
			carry += (*digit as u32) << 8;
			*digit = (carry % 58) as u8;
			carry /= 58;
		}

		while carry > 0 {
			digits.push((carry % 58) as u8);
			carry /= 58;
		}
	}

	// Build result string
	let mut result = String::with_capacity(leading_zeros + digits.len());

	// Add leading '1's for each leading zero byte
	for _ in 0..leading_zeros {
		result.push('1');
	}

	// Add the encoded digits in reverse order
	for &digit in digits.iter().rev() {
		result.push(BASE58_ALPHABET[digit as usize] as char);
	}

	result
}

/// Hex encoding alphabet
const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

/// Encodes bytes to hexadecimal string.
///
/// # Arguments
/// * `data` - The bytes to encode
///
/// # Returns
/// The hex-encoded string (lowercase)
pub fn encode_hex(data: &[u8]) -> String {
	let mut result = String::with_capacity(data.len() * 2);
	for &byte in data {
		result.push(HEX_CHARS[(byte >> 4) as usize] as char);
		result.push(HEX_CHARS[(byte & 0x0f) as usize] as char);
	}
	result
}

/// Validates if a string is a valid Solana public key (base58-encoded 32 bytes).
///
/// # Arguments
/// * `address` - The address string to validate
///
/// # Returns
/// `true` if the address is valid, `false` otherwise
pub fn is_valid_pubkey(address: &str) -> bool {
	match decode_base58(address) {
		Ok(bytes) => bytes.len() == 32,
		Err(_) => false,
	}
}

/// Validates if a string is a valid Solana transaction signature (base58-encoded 64 bytes).
///
/// # Arguments
/// * `signature` - The signature string to validate
///
/// # Returns
/// `true` if the signature is valid, `false` otherwise
pub fn is_valid_signature(signature: &str) -> bool {
	match decode_base58(signature) {
		Ok(bytes) => bytes.len() == 64,
		Err(_) => false,
	}
}

/// Extracts the instruction discriminator from instruction data.
///
/// The discriminator is the first 8 bytes of the instruction data.
///
/// # Arguments
/// * `data` - The instruction data (base58 or raw bytes)
///
/// # Returns
/// The 8-byte discriminator if available
pub fn extract_discriminator(data: &[u8]) -> Option<Vec<u8>> {
	if data.len() >= 8 {
		Some(data[..8].to_vec())
	} else {
		None
	}
}

/// Parses a program log message to extract event data.
///
/// Anchor programs emit events with the format:
/// "Program data: <base64_encoded_event_data>"
///
/// # Arguments
/// * `log` - The log message to parse
///
/// # Returns
/// The decoded event data if the log is a program data log
pub fn parse_program_data_log(log: &str) -> Option<Vec<u8>> {
	let prefix = "Program data: ";
	if let Some(base64_data) = log.strip_prefix(prefix) {
		base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_data).ok()
	} else {
		None
	}
}

/// Extracts the program ID from a "Program <id> invoke" log message.
///
/// # Arguments
/// * `log` - The log message to parse
///
/// # Returns
/// The program ID if the log matches the invoke pattern
pub fn extract_program_invoke(log: &str) -> Option<String> {
	if log.starts_with("Program ") && log.contains(" invoke [") {
		let parts: Vec<&str> = log.split_whitespace().collect();
		if parts.len() >= 2 {
			return Some(parts[1].to_string());
		}
	}
	None
}

/// Checks if a log message indicates program success.
///
/// # Arguments
/// * `log` - The log message to check
/// * `program_id` - The program ID to check for
///
/// # Returns
/// `true` if the log indicates the program succeeded
pub fn is_program_success(log: &str, program_id: &str) -> bool {
	log == format!("Program {} success", program_id)
}

/// Checks if a log message indicates program failure.
///
/// # Arguments
/// * `log` - The log message to check
/// * `program_id` - The program ID to check for
///
/// # Returns
/// `true` if the log indicates the program failed
pub fn is_program_failure(log: &str, program_id: &str) -> bool {
	log.starts_with(&format!("Program {} failed", program_id))
}

/// Formats a lamport amount to SOL.
///
/// # Arguments
/// * `lamports` - The amount in lamports
///
/// # Returns
/// The amount in SOL as a string with 9 decimal places
pub fn lamports_to_sol(lamports: u64) -> String {
	let sol = lamports as f64 / 1_000_000_000.0;
	format!("{:.9}", sol)
}

/// Parses a SOL amount string to lamports.
///
/// # Arguments
/// * `sol` - The SOL amount as a string
///
/// # Returns
/// The amount in lamports, or an error if parsing fails
pub fn sol_to_lamports(sol: &str) -> Result<u64, String> {
	let sol_float: f64 = sol
		.parse()
		.map_err(|e| format!("Failed to parse SOL amount: {}", e))?;
	Ok((sol_float * 1_000_000_000.0) as u64)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_normalize_address() {
		let address = "  TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA  ";
		assert_eq!(
			normalize_address(address),
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
		);
	}

	#[test]
	fn test_are_same_address() {
		let addr1 = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
		let addr2 = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
		let addr3 = "11111111111111111111111111111111";

		assert!(are_same_address(addr1, addr2));
		assert!(!are_same_address(addr1, addr3));
	}

	#[test]
	fn test_are_same_signature() {
		let sig1 = "transfer(u64,pubkey)";
		let sig2 = "transfer(u64,pubkey)";
		let sig3 = "transfer(u32,pubkey)";

		assert!(are_same_signature(sig1, sig2));
		assert!(!are_same_signature(sig1, sig3));
	}

	#[test]
	fn test_calculate_discriminator() {
		// Test that discriminator is 8 bytes
		let discriminator = calculate_discriminator("transfer");
		assert_eq!(discriminator.len(), 8);

		// Same instruction should produce same discriminator
		let discriminator2 = calculate_discriminator("transfer");
		assert_eq!(discriminator, discriminator2);

		// Different instructions should produce different discriminators
		let discriminator3 = calculate_discriminator("initialize");
		assert_ne!(discriminator, discriminator3);
	}

	#[test]
	fn test_calculate_event_discriminator() {
		let discriminator = calculate_event_discriminator("TransferEvent");
		assert_eq!(discriminator.len(), 8);

		// Event discriminator should differ from instruction discriminator
		let instruction_discriminator = calculate_discriminator("TransferEvent");
		assert_ne!(discriminator, instruction_discriminator);
	}

	#[test]
	fn test_base58_encode_decode() {
		let original = vec![1, 2, 3, 4, 5, 6, 7, 8];
		let encoded = encode_base58(&original);
		let decoded = decode_base58(&encoded).unwrap();
		assert_eq!(original, decoded);
	}

	#[test]
	fn test_is_valid_pubkey() {
		// Valid Solana public key (Token Program)
		assert!(is_valid_pubkey(
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
		));

		// Invalid - too short
		assert!(!is_valid_pubkey("abc"));

		// Invalid - not base58
		assert!(!is_valid_pubkey("0xabc123"));
	}

	#[test]
	fn test_is_valid_signature() {
		// A valid signature would be 64 bytes base58-encoded
		// This is just a length check
		let valid_sig_bytes = vec![0u8; 64];
		let valid_sig = encode_base58(&valid_sig_bytes);
		assert!(is_valid_signature(&valid_sig));

		// Invalid - wrong length
		assert!(!is_valid_signature("abc"));
	}

	#[test]
	fn test_extract_discriminator() {
		let data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
		let discriminator = extract_discriminator(&data);
		assert_eq!(discriminator, Some(vec![1, 2, 3, 4, 5, 6, 7, 8]));

		// Too short
		let short_data = vec![1, 2, 3];
		assert!(extract_discriminator(&short_data).is_none());
	}

	#[test]
	fn test_parse_program_data_log() {
		// Valid program data log
		let log = "Program data: AQIDBA=="; // base64 for [1, 2, 3, 4]
		let data = parse_program_data_log(log);
		assert_eq!(data, Some(vec![1, 2, 3, 4]));

		// Not a program data log
		let other_log = "Program invoked";
		assert!(parse_program_data_log(other_log).is_none());
	}

	#[test]
	fn test_extract_program_invoke() {
		let log = "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA invoke [1]";
		let program_id = extract_program_invoke(log);
		assert_eq!(
			program_id,
			Some("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string())
		);

		let other_log = "Program success";
		assert!(extract_program_invoke(other_log).is_none());
	}

	#[test]
	fn test_is_program_success() {
		let program_id = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
		let success_log = "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success";
		let failure_log = "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA failed: error";

		assert!(is_program_success(success_log, program_id));
		assert!(!is_program_success(failure_log, program_id));
	}

	#[test]
	fn test_is_program_failure() {
		let program_id = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
		let failure_log = "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA failed: error";
		let success_log = "Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success";

		assert!(is_program_failure(failure_log, program_id));
		assert!(!is_program_failure(success_log, program_id));
	}

	#[test]
	fn test_lamports_to_sol() {
		assert_eq!(lamports_to_sol(1_000_000_000), "1.000000000");
		assert_eq!(lamports_to_sol(500_000_000), "0.500000000");
		assert_eq!(lamports_to_sol(1), "0.000000001");
	}

	#[test]
	fn test_sol_to_lamports() {
		assert_eq!(sol_to_lamports("1.0").unwrap(), 1_000_000_000);
		assert_eq!(sol_to_lamports("0.5").unwrap(), 500_000_000);
		assert!(sol_to_lamports("invalid").is_err());
	}
}
