//! Helper functions for Midnight-specific operations.
//!
//! This module provides utility functions for working with Midnight-specific data types
//! and formatting, including address normalization, value parsing, and
//! operation processing.

use midnight_ledger::base_crypto::hash::{HashOutput, PERSISTENT_HASH_BYTES};
use midnight_ledger::serialize::{deserialize, NetworkId};
use midnight_ledger::storage::DefaultDB;
use midnight_ledger::structure::{Proofish, Transaction, TransactionHash};

use subxt::utils::H256;

use crate::models::MidnightChainType;

/// Convert a H256 hash to a string
#[allow(dead_code)]
pub fn hash_to_str(h: H256) -> String {
	format!("0x{}", hex::encode(h.as_bytes()))
}

/// Parse a transaction index item
pub fn parse_tx_index_item<P: Proofish<DefaultDB>>(
	hash: &str,
	body: &str,
	network_id: NetworkId,
) -> Result<(TransactionHash, Transaction<P, DefaultDB>), anyhow::Error> {
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
		MidnightChainType::Live => NetworkId::MainNet,
		MidnightChainType::Local => NetworkId::Undeployed,
		MidnightChainType::Custom(_) => NetworkId::Undeployed,
	}
}
