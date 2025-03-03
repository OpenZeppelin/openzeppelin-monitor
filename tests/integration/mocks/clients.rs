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
	models::{BlockType, Network, StellarEvent, StellarTransaction},
	services::{
		blockchain::{
			BlockChainClient, BlockChainError, BlockFilterFactory, ClientPoolTrait, EvmClientTrait,
			StellarClientTrait,
		},
		filter::{EVMBlockFilter, StellarBlockFilter},
	},
};

use async_trait::async_trait;
use mockall::{mock, predicate::*};

use super::{MockStellarTransportClient, MockWeb3TransportClient};

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
		async fn get_latest_block_number(&self) -> Result<u64, BlockChainError>;
		async fn get_blocks(
			&self,
			start_block: u64,
			end_block: Option<u64>,
		) -> Result<Vec<BlockType>, BlockChainError>;
	}

	#[async_trait]
	impl<T: Send + Sync + Clone + 'static> EvmClientTrait for EvmClientTrait<T> {
		async fn get_transaction_receipt(
			&self,
			transaction_hash: String,
		) -> Result<web3::types::TransactionReceipt, BlockChainError>;

		async fn get_logs_for_blocks(
			&self,
			from_block: u64,
			to_block: u64,
		) -> Result<Vec<web3::types::Log>, BlockChainError>;
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
		async fn get_latest_block_number(&self) -> Result<u64, BlockChainError>;
		async fn get_blocks(
			&self,
			start_block: u64,
			end_block: Option<u64>,
		) -> Result<Vec<BlockType>, BlockChainError>;
	}

	#[async_trait]
	impl<T: Send + Sync + Clone + 'static> StellarClientTrait for StellarClientTrait<T> {
		async fn get_transactions(
			&self,
			start_sequence: u32,
			end_sequence: Option<u32>,
		) -> Result<Vec<StellarTransaction>, BlockChainError>;

		async fn get_events(
			&self,
			start_sequence: u32,
			end_sequence: Option<u32>,
		) -> Result<Vec<StellarEvent>, BlockChainError>;
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
		type EvmClient = MockEvmClientTrait<MockWeb3TransportClient>;
		type StellarClient = MockStellarClientTrait<MockStellarTransportClient>;
		async fn get_evm_client(&self, network: &Network) -> Result<Arc<MockEvmClientTrait<MockWeb3TransportClient>>, BlockChainError>;
		async fn get_stellar_client(&self, network: &Network) -> Result<Arc<MockStellarClientTrait<MockStellarTransportClient>>, BlockChainError>;
	}

	impl Clone for ClientPool {
		fn clone(&self) -> Self;
	}
}
