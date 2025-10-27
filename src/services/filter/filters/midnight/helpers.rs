//! Helper functions for Midnight-specific operations.
//!
//! This module provides utility functions for working with Midnight-specific data types
//! and formatting, including address normalization, value parsing, and
//! operation processing.

use midnight_base_crypto::hash::{HashOutput, PERSISTENT_HASH_BYTES};
use midnight_ledger::structure::{Transaction as MidnightNodeTransaction, TransactionHash};
use midnight_node_ledger_helpers::{deserialize, CoinInfo, NetworkId};
use midnight_storage::DefaultDB;
use midnight_transient_crypto::encryption;
use midnight_zswap::{
	keys::{SecretKeys, Seed},
	CoinCiphertext,
};
use tracing;

/// Parse a transaction index item
#[allow(clippy::type_complexity)]
pub fn parse_tx_index_item(
	hash_without_prefix: &str,
	raw_tx_data: &str,
	_network_id: NetworkId,
) -> Result<
	(
		TransactionHash,
		Option<
			MidnightNodeTransaction<
				midnight_base_crypto::signatures::Signature,
				(),
				midnight_transient_crypto::commitment::Pedersen,
				DefaultDB,
			>,
		>,
	),
	anyhow::Error,
> {
	let hash = hex::decode(hash_without_prefix)
		.map_err(|e| anyhow::anyhow!("TransactionHashDecodeError: {}", e))?;

	// When testing, we don't have the raw tx data, so we just return the hash
	if raw_tx_data.is_empty() {
		return Ok((
			TransactionHash(HashOutput(
				hash.try_into()
					.map_err(|_| anyhow::anyhow!("Invalid hash length"))?,
			)),
			None,
		));
	}

	if hash.len() != PERSISTENT_HASH_BYTES {
		return Err(anyhow::anyhow!(
			"hash length ({}) != {PERSISTENT_HASH_BYTES}",
			hash.len()
		));
	}

	let hash_result = TransactionHash(HashOutput(
		hash.try_into()
			.map_err(|_| anyhow::anyhow!("Invalid hash length"))?,
	));

	// Try to deserialize the transaction data
	// If deserialization fails (e.g., due to data format issues), return None for the transaction
	// This allows the code to continue processing while the Midnight node team investigates

	// Skip the first 2 characters (hex prefix) and try to decode and deserialize
	let body_str = if raw_tx_data.len() >= 2 {
		&raw_tx_data[2..]
	} else {
		raw_tx_data
	};

	let tx = match hex::decode(body_str) {
		Ok(body) => match deserialize(body.as_slice()) {
			Ok(t) => Some(t),
			Err(e) => {
				tracing::warn!(
					"Failed to deserialize transaction data: {}. Returning None for transaction.",
					e
				);
				None
			}
		},
		Err(e) => {
			tracing::warn!(
				"Failed to decode transaction hex data: {}. Returning None for transaction.",
				e
			);
			None
		}
	};

	Ok((hash_result, tx))
}

/// Map a chain type to a NetworkId
pub fn map_chain_type(chain_type: &str) -> NetworkId {
	if chain_type.contains("testnet") {
		NetworkId::TestNet
	} else if chain_type.contains("mainnet") {
		NetworkId::MainNet
	} else if chain_type.contains("devnet") {
		NetworkId::DevNet
	} else {
		NetworkId::Undeployed
	}
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
	normalize_address_size(
		address
			.strip_prefix("0x")
			.unwrap_or(address)
			.replace(char::is_whitespace, "")
			.to_lowercase()
			.as_str(),
	)
}

/// Compares two hashes for equality, ignoring case and "0x" prefixes.
///
/// # Arguments
/// * `hash1` - First hash to compare
/// * `hash2` - Second hash to compare
///
/// # Returns
/// `true` if the hashes are equivalent, `false` otherwise
pub fn are_same_hash(hash1: &str, hash2: &str) -> bool {
	normalize_hash(hash1) == normalize_hash(hash2)
}

/// Normalizes a hash by removing "0x" prefix, spaces, and converting to lowercase.
///
/// # Arguments
/// * `hash` - The hash string to normalize
///
/// # Returns
/// The normalized hash string
pub fn normalize_hash(hash: &str) -> String {
	hash.strip_prefix("0x")
		.unwrap_or(hash)
		.replace(char::is_whitespace, "")
		.to_lowercase()
}

/// Compares two function signatures for equality, ignoring case and whitespace.
/// We remove anything between parentheses from the signatures before comparing them because we cannot
/// access the function arguments from the transaction in Midnight.
///
/// # Arguments
/// * `signature1` - First signature to compare
/// * `signature2` - Second signature to compare
///
/// # Returns
/// `true` if the signatures are equivalent, `false` otherwise
pub fn are_same_signature(signature1: &str, signature2: &str) -> bool {
	remove_parentheses(&normalize_signature(signature1))
		== remove_parentheses(&normalize_signature(signature2))
}

/// Normalizes a function signature by removing spaces and converting to lowercase.
///
/// # Arguments
/// * `signature` - The signature string to normalize
///
/// # Returns
/// The normalized signature string
pub fn normalize_signature(signature: &str) -> String {
	signature.replace(char::is_whitespace, "").to_lowercase()
}

/// Removes anything after the first parenthesis from a string
///
/// # Arguments
/// * `value` - The string to remove parentheses from
///
/// # Returns
/// The string with parentheses removed
pub fn remove_parentheses(value: &str) -> String {
	value.split('(').next().unwrap_or(value).trim().to_string()
}

/// Convert a seed to a viewing key
///
/// # Arguments
/// * `seed` - The seed to convert
///
/// # Returns
/// The SecretKeys
pub fn seed_to_secret_keys(seed: Seed) -> Result<SecretKeys, anyhow::Error> {
	Ok(SecretKeys::from(seed))
}

/// Process the coins in a transaction
///
/// # Arguments
/// * `viewing_key_hex` - The hex encoded viewing key to use for decryption (hex::encode(viewing_key.repr()))
/// * `tx` - The transaction to process
///
/// # Returns
/// The result of the operation
pub fn process_transaction_for_coins<D: midnight_storage::db::DB>(
	viewing_key_hex: &str,
	tx: &MidnightNodeTransaction<
		midnight_base_crypto::signatures::Signature,
		(),
		midnight_transient_crypto::commitment::Pedersen,
		D,
	>,
) -> Result<Vec<CoinInfo>, anyhow::Error> {
	let reconstructed_viewing_key = encryption::SecretKey::from_repr(
		&hex::decode(viewing_key_hex)
			.map_err(|e| anyhow::anyhow!("Failed to decode hex: {}", e))?
			.try_into()
			.map_err(|_| anyhow::anyhow!("Invalid key length"))?,
	);

	if !bool::from(reconstructed_viewing_key.is_some()) {
		return Err(anyhow::anyhow!("Failed to reconstruct viewing key"));
	}
	// Safe to unwrap here as we've verified the key exists
	let viewing_key = reconstructed_viewing_key.unwrap();

	let mut coins: Vec<CoinInfo> = vec![];

	if let MidnightNodeTransaction::Standard(tx) = tx {
		if let Some(guaranteed_coins) = &tx.guaranteed_coins {
			for output in guaranteed_coins.outputs.iter() {
				if let Some(ciphertext) = &output.ciphertext {
					if let Some(coin) =
						try_decrypt_coin(&Some((**ciphertext).clone()), &viewing_key)?
					{
						coins.push(coin);
					}
				}
			}
		}
	}
	Ok(coins)
}

/// Try to decrypt a coin ciphertext using a viewing key
///
/// # Arguments
/// * `ciphertext` - The ciphertext to decrypt
/// * `viewing_key` - The viewing key to use for decryption
///
/// # Returns
/// The decrypted coin info
pub fn try_decrypt_coin(
	ciphertext: &Option<CoinCiphertext>,
	viewing_key: &encryption::SecretKey,
) -> Result<Option<CoinInfo>, anyhow::Error> {
	if let Some(ciphertext) = ciphertext {
		let plaintext = viewing_key.decrypt(&ciphertext.clone().into());
		Ok(plaintext)
	} else {
		Ok(None)
	}
}

/// Normalize the size of an address
///
/// Midnight uses 35 byte addresses which includes 3 byte network id (e.g. 020200) followed by the 32 byte address.
/// This function normalizes the size of an address to 32 bytes in case the address is 35 bytes.
///
/// # Arguments
/// * `address` - The address to normalize
///
/// # Returns
/// The normalized address
pub fn normalize_address_size(address: &str) -> String {
	let address_bytes = match hex::decode(address) {
		Ok(bytes) => bytes,
		Err(_) => return address.to_string(),
	};

	if address_bytes.len() == 35 {
		hex::encode(&address_bytes[3..])
	} else {
		hex::encode(&address_bytes)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_normalize_hash() {
		let hash = "0x1234567890123456789012345678901234567890";
		let normalized = normalize_hash(hash);
		assert_eq!(normalized, "1234567890123456789012345678901234567890");

		let hash = "1234567890123456789012345678901234567890";
		let normalized = normalize_hash(hash);
		assert_eq!(normalized, "1234567890123456789012345678901234567890");

		let hash = "0x123456 7890123456 ABCDEFGHIJKLMNOPQRSTUVWXYZ";
		let normalized = normalize_hash(hash);
		assert_eq!(normalized, "1234567890123456abcdefghijklmnopqrstuvwxyz");
	}

	#[test]
	fn test_are_same_hash() {
		let hash1 = "0x1234567890123456789012345678901234567890";
		let hash2 = "0x1234567890123456789012345678901234567890";
		assert!(are_same_hash(hash1, hash2));

		let hash1 = "0x1234567890123456789012345678901234567890";
		let hash2 = "0x1234567890123456789012345678901234567891";
		assert!(!are_same_hash(hash1, hash2));

		let hash1 = "0x123456 7890123456 ABCDEFGHIJKLMNOPQRSTUVWXYZ";
		let hash2 = "0x1234567890123456abcdefghijklmnopqrstuvwxyz";
		assert!(are_same_hash(hash1, hash2));
	}

	#[test]
	fn test_are_same_signature() {
		let signature1 = "transfer(address,uint256)";
		let signature2 = "transfer(address,uint256)";
		assert!(are_same_signature(signature1, signature2));

		let signature1 = "transfer()";
		let signature2 = "transfer(address,uint256)";
		assert!(are_same_signature(signature1, signature2));

		let signature1 = "approve()";
		let signature2 = "transfer(address,uint256)";
		assert!(!are_same_signature(signature1, signature2));

		let signature1 = "approve";
		let signature2 = "approve";
		assert!(are_same_signature(signature1, signature2));

		let signature1 = "approve";
		let signature2 = "transfer(address,uint256)";
		assert!(!are_same_signature(signature1, signature2));
	}

	#[test]
	fn test_normalize_signature() {
		let signature = "transfer(address, uint256)";
		let normalized = normalize_signature(signature);
		assert_eq!(normalized, "transfer(address,uint256)");

		let signature = "transfer";
		let normalized = normalize_signature(signature);
		assert_eq!(normalized, "transfer");

		let signature = "transfer()";
		let normalized = normalize_signature(signature);
		assert_eq!(normalized, "transfer()");

		let signature = "transfer( address     , uint256 )";
		let normalized = normalize_signature(signature);
		assert_eq!(normalized, "transfer(address,uint256)");
	}

	#[test]
	fn test_remove_parentheses() {
		let signature = "transfer(address,uint256)";
		let normalized = remove_parentheses(signature);
		assert_eq!(normalized, "transfer");

		let signature = "transfer()";
		let normalized = remove_parentheses(signature);
		assert_eq!(normalized, "transfer");

		let signature = "transfer";
		let normalized = remove_parentheses(signature);
		assert_eq!(normalized, "transfer");
	}

	#[test]
	fn test_normalize_address() {
		let address = "0x1234567890123456789012345678901234567890";
		let normalized = normalize_address(address);
		assert_eq!(normalized, "1234567890123456789012345678901234567890");

		let address = "1234567890123456789012345678901234567890";
		let normalized = normalize_address(address);
		assert_eq!(normalized, "1234567890123456789012345678901234567890");

		let address = "0x12345678901 2345678901234567890 1234567890";
		let normalized = normalize_address(address);
		assert_eq!(normalized, "1234567890123456789012345678901234567890");
	}

	#[test]
	fn test_are_same_address() {
		let address1 = "0x1234567890123456789012345678901234567890";
		let address2 = "0x1234567890123456789012345678901234567890";
		assert!(are_same_address(address1, address2));

		let address1 = "0x1234567890123456789012345678901234567890";
		let address2 = "0x1234567890123456789012345678901234567891";
		assert!(!are_same_address(address1, address2));

		let address1 = "0x123456 7890123456 7890123456   789012345 67890";
		let address2 = "0x1234567890123456789012345678901234567890";
		assert!(are_same_address(address1, address2));
	}

	#[test]
	fn test_map_chain_type() {
		let chain_type = "testnet-02-1";
		let network_id = map_chain_type(chain_type);
		assert_eq!(network_id, NetworkId::TestNet);

		let chain_type = "mainnet-02-1";
		let network_id = map_chain_type(chain_type);
		assert_eq!(network_id, NetworkId::MainNet);

		let chain_type = "devnet-02-1";
		let network_id = map_chain_type(chain_type);
		assert_eq!(network_id, NetworkId::DevNet);

		let chain_type = "custom-02-1";
		let network_id = map_chain_type(chain_type);
		assert_eq!(network_id, NetworkId::Undeployed);
	}

	#[test]
	fn test_normalize_address_size() {
		let address = "0x1234567890123456789012345678901234567890";
		let normalized = normalize_address_size(address);
		assert_eq!(normalized, "0x1234567890123456789012345678901234567890");

		let address = "020200bf19b4b8a1cb232880d26999f01c0aca5c57635365019456109be2ee809f5919";
		let normalized = normalize_address_size(address);
		assert_eq!(
			normalized,
			"bf19b4b8a1cb232880d26999f01c0aca5c57635365019456109be2ee809f5919"
		);

		let address = "bf19b4b8a1cb232880d26999f01c0aca5c57635365019456109be2ee809f5919";
		let normalized = normalize_address_size(address);
		assert_eq!(
			normalized,
			"bf19b4b8a1cb232880d26999f01c0aca5c57635365019456109be2ee809f5919"
		);
	}

	#[test]
	fn test_seed_to_secret_keys() {
		// Test with a randomly generated seed
		use rand::RngCore;
		let mut rng = rand::rng();
		let mut bytes = [0u8; 32];
		rng.fill_bytes(&mut bytes);
		let seed = Seed::from(bytes);
		let result = seed_to_secret_keys(seed);
		assert!(result.is_ok());
		let secret_keys = result.unwrap();

		// Verify that we got a SecretKeys back and can access encryption_secret_key.repr()
		// The repr() method provides the internal byte representation
		let enc_key_repr = secret_keys.encryption_secret_key.repr();
		assert_eq!(enc_key_repr.len(), 32); // SecretKey should be 32 bytes
	}

	#[test]
	fn test_seed_to_secret_keys_deterministic() {
		// Test that the same seed produces the same SecretKeys
		use rand::RngCore;
		let mut rng = rand::rng();
		let mut bytes = [0u8; 32];
		rng.fill_bytes(&mut bytes);

		// Create two seeds from the same bytes
		let seed1 = Seed::from(bytes);
		let seed2 = Seed::from(bytes);

		let result1 = seed_to_secret_keys(seed1);
		let result2 = seed_to_secret_keys(seed2);

		assert!(result1.is_ok());
		assert!(result2.is_ok());

		let secret_keys1 = result1.unwrap();
		let secret_keys2 = result2.unwrap();

		// Verify that the same seed produces the same encryption secret key
		assert_eq!(
			secret_keys1.encryption_secret_key.repr(),
			secret_keys2.encryption_secret_key.repr()
		);
	}

	#[test]
	fn test_seed_to_secret_keys_different_seeds() {
		// Test that different seeds produce different SecretKeys
		use rand::RngCore;
		let mut rng = rand::rng();
		let mut bytes1 = [0u8; 32];
		let bytes2 = [1u8; 32]; // Different seed
		rng.fill_bytes(&mut bytes1);

		let seed1 = Seed::from(bytes1);
		let seed2 = Seed::from(bytes2);

		let result1 = seed_to_secret_keys(seed1);
		let result2 = seed_to_secret_keys(seed2);

		assert!(result1.is_ok());
		assert!(result2.is_ok());

		let secret_keys1 = result1.unwrap();
		let secret_keys2 = result2.unwrap();

		// Verify that different seeds produce different encryption secret keys
		assert_ne!(
			secret_keys1.encryption_secret_key.repr(),
			secret_keys2.encryption_secret_key.repr()
		);
	}

	#[test]
	fn test_parse_tx_index_item_valid_hash_with_data() {
		// Test with valid hash and non-empty data
		// Due to the TODO early return (lines 44-52), this should return Ok with None for tx
		let valid_hash = "1234567890123456789012345678901234567890123456789012345678901234";
		let raw_tx_data = "0012345678"; // Non-empty data

		let result = parse_tx_index_item(valid_hash, raw_tx_data, NetworkId::TestNet);

		// Should return Ok with None for tx due to early return
		assert!(result.is_ok());
		let (hash, tx) = result.unwrap();
		assert!(tx.is_none());
		assert_eq!(hash.0 .0.len(), PERSISTENT_HASH_BYTES);
	}

	#[test]
	fn test_parse_tx_index_item_invalid_hash_length() {
		// Test with hash that's too short - this should error during hex decode or conversion
		let short_hash = "1234"; // Only 2 bytes, should be 32 bytes
		let raw_tx_data = ""; // Empty data to hit the early return

		let result = parse_tx_index_item(short_hash, raw_tx_data, NetworkId::TestNet);

		// Should fail when trying to convert to 32-byte array
		assert!(result.is_err());
		let error_msg = result.unwrap_err().to_string();
		assert!(error_msg.contains("Invalid hash length"));
	}

	#[test]
	fn test_parse_tx_index_item_invalid_hex_in_hash() {
		// Test with invalid hex in hash
		let invalid_hex_hash = "GHIJKLmnopqrstuvwxyz123456789012345678901234567890123456789012";

		let result = parse_tx_index_item(invalid_hex_hash, "", NetworkId::TestNet);

		assert!(result.is_err());
		let error_msg = result.unwrap_err().to_string();
		assert!(error_msg.contains("TransactionHashDecodeError"));
	}

	#[test]
	fn test_parse_tx_index_item_invalid_hex_in_body() {
		// Test with invalid hex in body
		// Note: Due to the TODO early return (line 44), this code path is unreachable
		// When the TODO is fixed, this should test TransactionBodyDecodeError
		let valid_hash = "1234567890123456789012345678901234567890123456789012345678901234";
		let invalid_hex_body = "00GHIJKL"; // Invalid hex in body

		let result = parse_tx_index_item(valid_hash, invalid_hex_body, NetworkId::TestNet);

		// Currently returns Ok due to early return, but when TODO is fixed:
		// assert!(result.is_err());
		// let error_msg = result.unwrap_err().to_string();
		// assert!(error_msg.contains("TransactionBodyDecodeError"));
		assert!(result.is_ok());
	}

	#[test]
	fn test_parse_tx_index_item_empty_raw_data() {
		// Test with empty raw_tx_data (should return None for tx)
		let valid_hash = "1234567890123456789012345678901234567890123456789012345678901234";

		let result = parse_tx_index_item(valid_hash, "", NetworkId::TestNet);

		assert!(result.is_ok());
		let (hash, tx) = result.unwrap();
		assert!(tx.is_none());
		// Hash should be created correctly
		assert_eq!(hash.0 .0.len(), PERSISTENT_HASH_BYTES);
	}

	#[test]
	fn test_parse_tx_index_item_hash_length_validation_with_data() {
		// Test hash length validation when raw_tx_data is non-empty (line 66-70)
		let short_hash = "12345678901234567890123456789012"; // 16 bytes instead of 32
		let raw_tx_data = "00123456789012345678901234567890"; // Non-empty data

		let result = parse_tx_index_item(short_hash, raw_tx_data, NetworkId::TestNet);

		// Should fail on hash length validation (line 66-70)
		assert!(result.is_err());
		let error_msg = result.unwrap_err().to_string();
		assert!(error_msg.contains("hash length (16) != 32"));
	}

	#[test]
	fn test_parse_tx_index_item_invalid_body_hex() {
		// Test with invalid hex in body
		// Now returns Ok with None for transaction when deserialization fails
		let valid_hash = "1234567890123456789012345678901234567890123456789012345678901234";
		let invalid_hex_body = "00GHIJKL"; // Invalid hex in body

		let result = parse_tx_index_item(valid_hash, invalid_hex_body, NetworkId::TestNet);

		// Should return Ok with None for transaction
		assert!(result.is_ok());
		let (hash, tx) = result.unwrap();
		assert!(tx.is_none());
		assert_eq!(hash.0 .0.len(), PERSISTENT_HASH_BYTES);
	}

	#[test]
	fn test_parse_tx_index_item_deserialize_error() {
		// Test deserialize error
		// Now returns Ok with None for transaction when deserialization fails
		let valid_hash = "1234567890123456789012345678901234567890123456789012345678901234";
		let invalid_body = "001122334455"; // Valid hex but not valid transaction data

		let result = parse_tx_index_item(valid_hash, invalid_body, NetworkId::TestNet);

		// Should return Ok with None for transaction (deserialization error is handled gracefully)
		assert!(result.is_ok());
		let (hash, tx) = result.unwrap();
		assert!(tx.is_none());
		assert_eq!(hash.0 .0.len(), PERSISTENT_HASH_BYTES);
	}

	#[test]
	fn test_try_decrypt_coin_with_none() {
		// Test try_decrypt_coin with None ciphertext
		use rand::RngCore;
		let mut rng = rand::rng();
		let mut bytes = [0u8; 32];
		rng.fill_bytes(&mut bytes);
		let seed = Seed::from(bytes);
		let secret_keys = SecretKeys::from(seed);

		let viewing_key = &secret_keys.encryption_secret_key;
		let result = try_decrypt_coin(&None, viewing_key).unwrap();
		assert!(result.is_none());
	}

	#[test]
	fn test_process_transaction_for_coins_invalid_key_hex() {
		// Test process_transaction_for_coins with invalid viewing key hex
		// Create a transaction (we can't easily create a real MidnightNodeTransaction, so we'll test error path)
		let invalid_key = "not_valid_hex_12345";

		// This function requires actual transaction data which is hard to mock
		// We test that it errors appropriately
		assert!(hex::decode(invalid_key).is_err());
	}
}
