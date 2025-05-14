//! Helper functions for Midnight-specific operations.
//!
//! This module provides utility functions for working with Midnight-specific data types
//! and formatting, including address normalization, value parsing, and
//! operation processing.

use crate::models::MidnightChainType;

use midnight_ledger::{
	base_crypto::hash::{HashOutput, PERSISTENT_HASH_BYTES},
	serialize::deserialize,
	storage::DefaultDB,
	structure::{
		Proof, Proofish, StandardTransaction, Transaction as MidnightNodeTransaction,
		TransactionHash,
	},
	zswap::{
		keys::{SecretKeys, Seed},
		CoinCiphertext,
	},
};
use midnight_node_ledger_helpers::{CoinInfo, NetworkId, DB};

/// Parse a transaction index item
pub fn parse_tx_index_item<P: Proofish<DefaultDB>>(
	hash: &str,
	body: &str,
	network_id: NetworkId,
) -> Result<(TransactionHash, MidnightNodeTransaction<P, DefaultDB>), anyhow::Error> {
	let (_hex_prefix, hash_str) = hash.split_at(2);
	let (_hex_prefix, body_str) = body.split_at(2);
	let hash =
		hex::decode(hash_str).map_err(|e| anyhow::anyhow!("TransactionHashDecodeError: {}", e))?;
	if hash.len() != PERSISTENT_HASH_BYTES {
		return Err(anyhow::anyhow!(
			"hash length ({}) != {PERSISTENT_HASH_BYTES}",
			hash.len()
		));
	}
	let hash = TransactionHash(HashOutput(hash.try_into().unwrap()));

	let body =
		hex::decode(body_str).map_err(|e| anyhow::anyhow!("TransactionBodyDecodeError: {}", e))?;

	let tx = deserialize(body.as_slice(), network_id)
		.map_err(|e| anyhow::anyhow!("TransactionDeserializeError: {}", e))?;

	Ok((hash, tx))
}

/// Map a MidnightChainType to a NetworkId
pub fn map_chain_type(chain_type: &MidnightChainType) -> NetworkId {
	match chain_type {
		MidnightChainType::Development => NetworkId::TestNet,
		MidnightChainType::Live => NetworkId::TestNet, // TODO: Change to MainNet once testnet-02 `system_chainType` returns `Development`
		MidnightChainType::Local => NetworkId::Undeployed,
		MidnightChainType::Custom(_) => NetworkId::Undeployed,
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
	address
		.strip_prefix("0x")
		.unwrap_or(address)
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
/// * `stx` - The transaction to process
///
/// # Returns
/// The result of the operation
pub fn process_coins<D: DB>(
	seed: &str,
	stx: &StandardTransaction<Proof, D>,
) -> Result<(), anyhow::Error> {
	let seed_bytes: [u8; 32] = hex::decode(seed)
		.map_err(|e| anyhow::anyhow!("Invalid hex string: {}", e))?
		.try_into()
		.map_err(|_| anyhow::anyhow!("Seed must be exactly 32 bytes"))?;
	let seed = Seed::from(seed_bytes);

	let keys = seed_to_secret_keys(seed)
		.map_err(|e| anyhow::anyhow!("Failed to convert seed to secret keys: {}", e))?;

	for output in stx.guaranteed_coins.outputs.iter() {
		if let Some(coin) = try_decrypt_coin(&output.ciphertext, &keys)? {
			println!("coin: {:#?}", coin);
		} else {
			println!("unable to decrypt coin");
		}
	}
	Ok(())
}

/// Try to decrypt a coin ciphertext using a secret key
///
/// # Arguments
/// * `ciphertext` - The ciphertext to decrypt
/// * `secret_keys` - The secret keys to use for decryption
///
/// # Returns
/// The decrypted coin info
pub fn try_decrypt_coin(
	ciphertext: &Option<CoinCiphertext>,
	secret_keys: &SecretKeys,
) -> Result<Option<CoinInfo>, anyhow::Error> {
	if let Some(ciphertext) = ciphertext {
		let plaintext = secret_keys.try_decrypt(ciphertext);
		Ok(plaintext)
	} else {
		Ok(None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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
		let chain_type = MidnightChainType::Development;
		let network_id = map_chain_type(&chain_type);
		assert_eq!(network_id, NetworkId::TestNet);

		// TODO: Change to MainNet once testnet-02 `system_chainType` returns `Development`
		let chain_type = MidnightChainType::Live;
		let network_id = map_chain_type(&chain_type);
		assert_eq!(network_id, NetworkId::TestNet);

		let chain_type = MidnightChainType::Local;
		let network_id = map_chain_type(&chain_type);
		assert_eq!(network_id, NetworkId::Undeployed);

		let chain_type = MidnightChainType::Custom(String::from("custom"));
		let network_id = map_chain_type(&chain_type);
		assert_eq!(network_id, NetworkId::Undeployed);
	}
}
