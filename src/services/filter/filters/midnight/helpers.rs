//! Helper functions for Midnight-specific operations.
//!
//! This module provides utility functions for working with Midnight-specific data types
//! and formatting, including address normalization, value parsing, and
//! operation processing.

use crate::models::MidnightChainType;

use midnight_ledger::{
	base_crypto::hash::{HashOutput, PERSISTENT_HASH_BYTES},
	serialize::deserialize,
	structure::{Proofish, Transaction as MidnightNodeTransaction, TransactionHash},
};
use midnight_node_ledger_helpers::{NetworkId, DB};

/// Parse a transaction index item
#[allow(dead_code)]
pub fn parse_tx_index_item<P: Proofish<D>, D: DB>(
	hash: &str,
	body: &str,
	network_id: NetworkId,
) -> Result<(TransactionHash, MidnightNodeTransaction<P, D>), anyhow::Error> {
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

	let decoded_with_api = midnight_node_ledger::host_api::ledger_bridge::get_decoded_transaction(
		&[network_id as u8],
		body_str.as_bytes(),
	);
	println!("decoded_with_api: {:#?}", decoded_with_api);

	let body =
		hex::decode(body_str).map_err(|e| anyhow::anyhow!("TransactionBodyDecodeError: {}", e))?;

	// let decoded_with_api = midnight_node_ledger::ledger_v2::Bridge::get_decoded_transaction(
	// 	network_id,
	// 	body.as_slice(),
	// );
	// let decoded_with_api = api.deserialize::<Tx>(body.as_slice());
	// println!("decoded_with_api: {:#?}", decoded_with_api);

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
