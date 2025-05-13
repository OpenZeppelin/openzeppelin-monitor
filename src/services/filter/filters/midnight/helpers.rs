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
	structure::{Proofish, Transaction as MidnightNodeTransaction, TransactionHash},
};
use midnight_node_ledger_helpers::NetworkId;

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
