//! Helper functions for Midnight-specific operations.
//!
//! This module provides utility functions for working with Midnight-specific data types
//! and formatting, including address normalization, value parsing, and
//! operation processing.

use midnight_ledger::{
	base_crypto::hash::{HashOutput, PERSISTENT_HASH_BYTES},
	serialize::deserialize,
	storage::DefaultDB,
	structure::{Proof, Proofish, Transaction as MidnightNodeTransaction, TransactionHash},
	transient_crypto::encryption,
	zswap::{
		keys::{SecretKeys, Seed},
		CoinCiphertext,
	},
};
use midnight_node_ledger_helpers::{CoinInfo, NetworkId, DB};

/// Parse a transaction index item
pub fn parse_tx_index_item<P: Proofish<DefaultDB>>(
	hash_without_prefix: &str,
	raw_tx_data: &str,
	network_id: NetworkId,
) -> Result<
	(
		TransactionHash,
		Option<MidnightNodeTransaction<P, DefaultDB>>,
	),
	anyhow::Error,
> {
	let hash = hex::decode(hash_without_prefix)
		.map_err(|e| anyhow::anyhow!("TransactionHashDecodeError: {}", e))?;

	// TODO: For now we always return early because there is an issue with deserialising tx data from raw_tx_data while the Midnight node team investigates
	// Remove this once the issue is fixed
	// TransactionDeserializeError: Invalid input data for core::option::Option<midnight_ledger::structure::Transaction<midnight_ledger::structure::Proof, midnight_storage::db::InMemoryDB>>,
	//                              received version: None, maximum supported version is None. Invalid discriminant: 4
	if !raw_tx_data.is_empty() {
		return Ok((
			TransactionHash(HashOutput(
				hash.try_into()
					.map_err(|_| anyhow::anyhow!("Invalid hash length"))?,
			)),
			None,
		));
	}

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

	let (_hex_prefix, body_str) = raw_tx_data.split_at(2);
	if hash.len() != PERSISTENT_HASH_BYTES {
		return Err(anyhow::anyhow!(
			"hash length ({}) != {PERSISTENT_HASH_BYTES}",
			hash.len()
		));
	}
	let hash = TransactionHash(HashOutput(
		hash.try_into()
			.map_err(|_| anyhow::anyhow!("Invalid hash length"))?,
	));

	// NOTE: Alternative way to decode the transaction if we want the 35 byte addresses as opposed to the 32 byte addresses
	// 		 This method uses api.serialize() to serialize the addresses
	// let tx = midnight_node_ledger::host_api::ledger_bridge::get_decoded_transaction(
	// 	&[network_id as u8],
	// 	body_str.as_bytes(),
	// );

	let body =
		hex::decode(body_str).map_err(|e| anyhow::anyhow!("TransactionBodyDecodeError: {}", e))?;

	let tx = deserialize(body.as_slice(), network_id)
		.map_err(|e| anyhow::anyhow!("TransactionDeserializeError: {}", e))?;

	Ok((hash, tx))
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
pub fn process_transaction_for_coins<D: DB>(
	viewing_key_hex: &str,
	tx: &MidnightNodeTransaction<Proof, D>,
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
		for output in tx.guaranteed_coins.outputs.iter() {
			if let Some(coin) = try_decrypt_coin(&output.ciphertext, &viewing_key)? {
				coins.push(coin);
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
}
