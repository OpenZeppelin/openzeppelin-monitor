//! Mock implementations of blockchain clients.
//!
//! This module provides mock implementations of the blockchain client traits
//! used for testing. It includes:
//! - [`MockEvmClientTrait`] - Mock implementation of EVM blockchain client
//! - [`MockStellarClientTrait`] - Mock implementation of Stellar blockchain client
//! - [`MockClientPool`] - Mock implementation of the client pool
//!
//! These mocks allow testing blockchain-related functionality without actual
//! network connections.

use std::{marker::PhantomData, sync::Arc};

use openzeppelin_monitor::{
	models::{
		BlockType, ContractSpec, EVMReceiptLog, EVMTransactionReceipt, Network, StellarEvent,
		StellarTransaction,
	},
	services::{
		blockchain::{
			BlockChainClient, BlockFilterFactory, ClientPoolTrait, EvmClientTrait,
			StellarClientTrait,
		},
		filter::{EVMBlockFilter, StellarBlockFilter},
	},
};

use async_trait::async_trait;
use mockall::{mock, predicate::*};

use super::{MockEVMTransportClient, MockStellarTransportClient};

mock! {
	/// Mock implementation of the EVM client trait.
	///
	/// This mock allows testing EVM-specific functionality by simulating blockchain
	/// responses without actual network calls.
	pub EvmClientTrait<T: Send + Sync + Clone + 'static> {
		pub fn new_with_transport(transport: T, network: &Network) -> Self;
	}

	#[async_trait]
	impl<T: Send + Sync + Clone + 'static> BlockChainClient for EvmClientTrait<T> {
		async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error>;
		async fn get_blocks(
			&self,
			start_block: u64,
			end_block: Option<u64>,
		) -> Result<Vec<BlockType>, anyhow::Error>;
	}

	#[async_trait]
	impl<T: Send + Sync + Clone + 'static> EvmClientTrait for EvmClientTrait<T> {
		async fn get_transaction_receipt(
			&self,
			transaction_hash: String,
		) -> Result<EVMTransactionReceipt,  anyhow::Error>;

		async fn get_logs_for_blocks(
			&self,
			from_block: u64,
			to_block: u64,
			addresses: Option<Vec<String>>,
		) -> Result<Vec<EVMReceiptLog>,  anyhow::Error>;
	}

	impl<T: Send + Sync + Clone + 'static> Clone for EvmClientTrait<T> {
		fn clone(&self) -> Self {
			Self{}
		}
	}
}

mock! {
	/// Mock implementation of the Stellar client trait.
	///
	/// This mock allows testing Stellar-specific functionality by simulating blockchain
	/// responses without actual network calls.
	pub StellarClientTrait<T: Send + Sync + Clone + 'static> {
		pub fn new_with_transport(transport: T, network: &Network) -> Self;
	}

	#[async_trait]
	impl<T: Send + Sync + Clone + 'static> BlockChainClient for StellarClientTrait<T> {
		async fn get_latest_block_number(&self) -> Result<u64, anyhow::Error>;
		async fn get_blocks(
			&self,
			start_block: u64,
			end_block: Option<u64>,
		) -> Result<Vec<BlockType>, anyhow::Error>;
		async fn get_contract_spec(
			&self,
			contract_id: &str,
		) -> Result<ContractSpec, anyhow::Error>;
	}

	#[async_trait]
	impl<T: Send + Sync + Clone + 'static> StellarClientTrait for StellarClientTrait<T> {
		async fn get_transactions(
			&self,
			start_sequence: u32,
			end_sequence: Option<u32>,
		) -> Result<Vec<StellarTransaction>, anyhow::Error>;

		async fn get_events(
			&self,
			start_sequence: u32,
			end_sequence: Option<u32>,
		) -> Result<Vec<StellarEvent>, anyhow::Error>;


	}

	impl<T: Send + Sync + Clone + 'static> Clone for StellarClientTrait<T> {
		fn clone(&self) -> Self {
			Self{}
		}
	}
}

impl<T: Send + Sync + Clone + 'static> BlockFilterFactory<MockStellarClientTrait<T>>
	for MockStellarClientTrait<T>
{
	type Filter = StellarBlockFilter<MockStellarClientTrait<T>>;
	fn filter() -> Self::Filter {
		StellarBlockFilter {
			_client: PhantomData,
		}
	}
}

impl<T: Send + Sync + Clone + 'static> BlockFilterFactory<MockEvmClientTrait<T>>
	for MockEvmClientTrait<T>
{
	type Filter = EVMBlockFilter<MockEvmClientTrait<T>>;
	fn filter() -> Self::Filter {
		EVMBlockFilter {
			_client: PhantomData,
		}
	}
}

mock! {
	#[derive(Debug)]
	pub ClientPool {}

	#[async_trait]
	impl ClientPoolTrait for ClientPool {
		type EvmClient = MockEvmClientTrait<MockEVMTransportClient>;
		type StellarClient = MockStellarClientTrait<MockStellarTransportClient>;
		async fn get_evm_client(&self, network: &Network) -> Result<Arc<MockEvmClientTrait<MockEVMTransportClient>>,  anyhow::Error>;
		async fn get_stellar_client(&self, network: &Network) -> Result<Arc<MockStellarClientTrait<MockStellarTransportClient>>,  anyhow::Error>;
	}

	impl Clone for ClientPool {
		fn clone(&self) -> Self;
	}
}
