//! Helper functions for Midnight-specific operations.
//!
//! This module provides utility functions for working with Midnight-specific data types
//! and formatting, including address normalization, value parsing, and
//! operation processing.

use midnight_ledger::{
	base_crypto::hash::{HashOutput, PERSISTENT_HASH_BYTES},
	structure::TransactionHash,
};
use midnight_serialize::NetworkId;

use crate::models::MidnightChainType;

/// Parse a transaction index item
#[allow(dead_code)]
pub fn parse_tx_index_item(
	hash: &str,
	body: &str,
	_network_id: NetworkId,
) -> Result<TransactionHash, anyhow::Error> {
	let (_hex_prefix, hash_str) = hash.split_at(2);
	let (_hex_prefix, _body_str) = body.split_at(2);
	let hash =
		hex::decode(hash_str).map_err(|e| anyhow::anyhow!("TransactionHashDecodeError: {}", e))?;
	if hash.len() != PERSISTENT_HASH_BYTES {
		return Err(anyhow::anyhow!(
			"hash length ({}) != {PERSISTENT_HASH_BYTES}",
			hash.len()
		));
	}
	let hash = TransactionHash(HashOutput(hash.try_into().unwrap()));

	// let body =
	// 	hex::decode(body_str).map_err(|e| anyhow::anyhow!("TransactionBodyDecodeError: {}", e))?;

	// let api = midnight_node_ledger::ledger_v2::api::new(network_id);
	// let decoded_with_api = midnight_node_ledger::ledger_v2::Bridge::get_decoded_transaction(
	// 	network_id,
	// 	body.as_slice(),
	// );
	// let decoded_with_api = api.deserialize::<Tx>(body.as_slice());
	// println!("decoded_with_api: {:#?}", decoded_with_api);

	// let tx: Transaction<P, D> = deserialize(body.as_slice(), network_id)
	// 	.map_err(|e| anyhow::anyhow!("TransactionDeserializeError: {}", e))?;

	Ok(hash)
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
