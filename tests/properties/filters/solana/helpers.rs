//! Property-based tests for Solana helper functions.

use openzeppelin_monitor::services::filter::solana_helpers::{
	are_same_address, are_same_signature, calculate_discriminator, calculate_event_discriminator,
	decode_base58, encode_base58, encode_hex, extract_discriminator, extract_program_invoke,
	is_program_failure, is_program_success, is_valid_pubkey, is_valid_signature, lamports_to_sol,
	normalize_address, parse_program_data_log, sol_to_lamports,
};
use proptest::{prelude::*, test_runner::Config};

// Strategy to generate valid Solana public keys (base58-encoded 32 bytes)
fn arb_solana_pubkey() -> impl Strategy<Value = String> {
	prop::collection::vec(any::<u8>(), 32).prop_map(|bytes| encode_base58(&bytes))
}

// Strategy to generate valid Solana transaction signatures (base58-encoded 64 bytes)
fn arb_solana_signature() -> impl Strategy<Value = String> {
	prop::collection::vec(any::<u8>(), 64).prop_map(|bytes| encode_base58(&bytes))
}

// Strategy to generate random bytes
fn arb_bytes(min_len: usize, max_len: usize) -> impl Strategy<Value = Vec<u8>> {
	prop::collection::vec(any::<u8>(), min_len..=max_len)
}

// Strategy to generate instruction names
fn arb_instruction_name() -> impl Strategy<Value = String> {
	"[a-z_][a-z0-9_]*".prop_filter("non-empty", |s| !s.is_empty())
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn prop_normalize_address_is_idempotent(
		address in arb_solana_pubkey()
	) {
		let normalized = normalize_address(&address);
		let double_normalized = normalize_address(&normalized);
		prop_assert_eq!(normalized, double_normalized);
	}

	#[test]
	fn prop_same_address_is_reflexive(
		address in arb_solana_pubkey()
	) {
		prop_assert!(are_same_address(&address, &address));
	}

	#[test]
	fn prop_same_signature_is_reflexive(
		sig in arb_instruction_name()
	) {
		prop_assert!(are_same_signature(&sig, &sig));
	}

	#[test]
	fn prop_base58_roundtrip(
		bytes in arb_bytes(1, 64)
	) {
		let encoded = encode_base58(&bytes);
		let decoded = decode_base58(&encoded);
		prop_assert!(decoded.is_ok());
		prop_assert_eq!(decoded.unwrap(), bytes);
	}

	#[test]
	fn prop_hex_encoding_length(
		bytes in arb_bytes(0, 100)
	) {
		let hex = encode_hex(&bytes);
		prop_assert_eq!(hex.len(), bytes.len() * 2);
	}

	#[test]
	fn prop_valid_pubkey_is_32_bytes(
		pubkey in arb_solana_pubkey()
	) {
		prop_assert!(is_valid_pubkey(&pubkey));
	}

	#[test]
	fn prop_valid_signature_is_64_bytes(
		sig in arb_solana_signature()
	) {
		prop_assert!(is_valid_signature(&sig));
	}

	#[test]
	fn prop_discriminator_is_8_bytes(
		name in arb_instruction_name()
	) {
		let disc = calculate_discriminator(&name);
		prop_assert_eq!(disc.len(), 8);
	}

	#[test]
	fn prop_event_discriminator_is_8_bytes(
		name in arb_instruction_name()
	) {
		let disc = calculate_event_discriminator(&name);
		prop_assert_eq!(disc.len(), 8);
	}

	#[test]
	fn prop_discriminator_is_deterministic(
		name in arb_instruction_name()
	) {
		let disc1 = calculate_discriminator(&name);
		let disc2 = calculate_discriminator(&name);
		prop_assert_eq!(disc1, disc2);
	}

	#[test]
	fn prop_extract_discriminator_needs_8_bytes(
		bytes in arb_bytes(0, 20)
	) {
		let result = extract_discriminator(&bytes);
		if bytes.len() >= 8 {
			prop_assert!(result.is_some());
			prop_assert_eq!(result.unwrap().len(), 8);
		} else {
			prop_assert!(result.is_none());
		}
	}

	#[test]
	fn prop_program_success_format(
		program_id in arb_solana_pubkey()
	) {
		let log = format!("Program {} success", program_id);
		prop_assert!(is_program_success(&log, &program_id));
	}

	#[test]
	fn prop_program_failure_format(
		program_id in arb_solana_pubkey()
	) {
		let log = format!("Program {} failed: some error", program_id);
		prop_assert!(is_program_failure(&log, &program_id));
	}

	#[test]
	fn prop_program_invoke_extraction(
		program_id in arb_solana_pubkey(),
		depth in 1u32..5u32
	) {
		let log = format!("Program {} invoke [{}]", program_id, depth);
		let extracted = extract_program_invoke(&log);
		prop_assert!(extracted.is_some());
		prop_assert_eq!(extracted.unwrap(), program_id);
	}

	#[test]
	fn prop_program_data_log_parsing(
		data in arb_bytes(1, 100)
	) {
		use base64::Engine;
		let base64_data = base64::engine::general_purpose::STANDARD.encode(&data);
		let log = format!("Program data: {}", base64_data);
		let parsed = parse_program_data_log(&log);
		prop_assert!(parsed.is_some());
		prop_assert_eq!(parsed.unwrap(), data);
	}

	#[test]
	fn prop_lamports_to_sol_format(
		lamports in 0u64..1_000_000_000_000_000u64
	) {
		let sol_str = lamports_to_sol(lamports);
		let parsed: Result<f64, _> = sol_str.parse();
		prop_assert!(parsed.is_ok());
	}

	#[test]
	fn prop_lamports_conversion_roundtrip(
		lamports in 0u64..1_000_000_000_000u64
	) {
		let sol_str = lamports_to_sol(lamports);
		let back_to_lamports = sol_to_lamports(&sol_str);
		prop_assert!(back_to_lamports.is_ok());
		let diff = (back_to_lamports.unwrap() as i64 - lamports as i64).abs();
		prop_assert!(diff <= 1);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_known_discriminator() {
		let disc = calculate_discriminator("transfer");
		assert_eq!(disc.len(), 8);
		assert_eq!(disc, calculate_discriminator("transfer"));
	}

	#[test]
	fn test_base58_known_values() {
		let bytes = vec![0, 0, 0, 1];
		let encoded = encode_base58(&bytes);
		let decoded = decode_base58(&encoded).unwrap();
		assert_eq!(decoded, bytes);
	}

	#[test]
	fn test_hex_encoding() {
		let bytes = vec![0xde, 0xad, 0xbe, 0xef];
		let hex = encode_hex(&bytes);
		assert_eq!(hex, "deadbeef");
	}

	#[test]
	fn test_lamports_to_sol_specific() {
		assert_eq!(lamports_to_sol(1_000_000_000), "1.000000000");
		assert_eq!(lamports_to_sol(500_000_000), "0.500000000");
		assert_eq!(lamports_to_sol(1), "0.000000001");
	}

	#[test]
	fn test_sol_to_lamports_specific() {
		assert_eq!(sol_to_lamports("1.0").unwrap(), 1_000_000_000);
		assert_eq!(sol_to_lamports("0.5").unwrap(), 500_000_000);
	}
}
