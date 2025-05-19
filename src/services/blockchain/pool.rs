//! Client pool for managing blockchain clients.
//!
//! This module provides a thread-safe client pooling system that:
//! - Caches blockchain clients by network
//! - Creates clients lazily on first use
//! - Handles EVM, Stellar, and Midnight clients
//! - Provides type-safe access to clients
//! - Manages client lifecycles automatically
//!
//! The pool uses a fast path for existing clients and a slow path for
//! creating new ones, optimizing performance while maintaining safety.

use crate::{
	models::{BlockChainType, Network},
	services::blockchain::{
		BlockChainClient, BlockFilterFactory, EVMTransportClient, EvmClient, EvmClientTrait,
		MidnightClient, MidnightClientTrait, MidnightTransportClient, StellarClient,
		StellarClientTrait, StellarTransportClient, WsTransportClient,
	},
};
use anyhow::Context;
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::{any::Any, collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

/// Trait for the client pool.
#[async_trait]
pub trait ClientPoolTrait: Send + Sync {
	type EvmClient: EvmClientTrait + BlockChainClient + BlockFilterFactory<Self::EvmClient>;
	type StellarClient: StellarClientTrait
		+ BlockChainClient
		+ BlockFilterFactory<Self::StellarClient>;
	type MidnightClient: MidnightClientTrait
		+ BlockChainClient
		+ BlockFilterFactory<Self::MidnightClient>;

	async fn get_evm_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::EvmClient>, anyhow::Error>;
	async fn get_stellar_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::StellarClient>, anyhow::Error>;
	async fn get_midnight_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::MidnightClient>, anyhow::Error>;
}

/// Generic client storage that can hold any type of blockchain client
///
/// Clients are stored in a thread-safe way using a HashMap and an RwLock.
/// The HashMap is indexed by the network slug and the value is an Arc of the client.
pub struct ClientStorage<T> {
	clients: Arc<RwLock<HashMap<String, Arc<T>>>>,
}

impl<T> ClientStorage<T> {
	pub fn new() -> Self {
		Self {
			clients: Arc::new(RwLock::new(HashMap::new())),
		}
	}
}

/// Main client pool manager that handles multiple blockchain types.
///
/// Provides type-safe access to cached blockchain clients. Clients are created
/// on demand when first requested and then cached for future use. Uses RwLock
/// for thread-safe access and Arc for shared ownership.
pub struct ClientPool {
	/// Map of client storages indexed by client type
	pub storages: HashMap<BlockChainType, Box<dyn Any + Send + Sync>>,
}

impl ClientPool {
	/// Creates a new empty client pool.
	///
	/// Initializes empty hashmaps for all supported clients types.
	pub fn new() -> Self {
		let mut pool = Self {
			storages: HashMap::new(),
		};

		// Register client types
		pool.register_client_type::<EvmClient<EVMTransportClient>>(BlockChainType::EVM);
		pool.register_client_type::<StellarClient<StellarTransportClient>>(BlockChainType::Stellar);
		pool.register_client_type::<MidnightClient<MidnightTransportClient, WsTransportClient>>(
			BlockChainType::Midnight,
		);

		pool
	}

	fn register_client_type<T: 'static + Send + Sync>(&mut self, client_type: BlockChainType) {
		self.storages
			.insert(client_type, Box::new(ClientStorage::<T>::new()));
	}

	/// Internal helper method to get or create a client of any type.
	///
	/// Uses a double-checked locking pattern:
	/// 1. Fast path with read lock to check for existing client
	/// 2. Slow path with write lock to create new client if needed
	///
	/// This ensures thread-safety while maintaining good performance
	/// for the common case of accessing existing clients.
	async fn get_or_create_client<T: BlockChainClient + 'static>(
		&self,
		client_type: BlockChainType,
		network: &Network,
		create_fn: impl Fn(&Network) -> BoxFuture<'static, Result<T, anyhow::Error>>,
	) -> Result<Arc<T>, anyhow::Error> {
		let storage = self
			.storages
			.get(&client_type)
			.and_then(|s| s.downcast_ref::<ClientStorage<T>>())
			.with_context(|| "Invalid client type")?;

		// Fast path: check if client exists
		if let Some(client) = storage.clients.read().await.get(&network.slug) {
			return Ok(client.clone());
		}

		// Slow path: create new client
		let mut clients = storage.clients.write().await;
		let client = Arc::new(create_fn(network).await?);
		clients.insert(network.slug.clone(), client.clone());
		Ok(client)
	}

	/// Get the number of clients for a given client type.
	pub async fn get_client_count<T: 'static>(&self, client_type: BlockChainType) -> usize {
		match self
			.storages
			.get(&client_type)
			.and_then(|s| s.downcast_ref::<ClientStorage<T>>())
		{
			Some(storage) => storage.clients.read().await.len(),
			None => 0,
		}
	}
}

#[async_trait]
impl ClientPoolTrait for ClientPool {
	type EvmClient = EvmClient<EVMTransportClient>;
	type StellarClient = StellarClient<StellarTransportClient>;
	type MidnightClient = MidnightClient<MidnightTransportClient, WsTransportClient>;

	/// Gets or creates an EVM client for the given network.
	///
	/// First checks the cache for an existing client. If none exists,
	/// creates a new client under a write lock.
	async fn get_evm_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::EvmClient>, anyhow::Error> {
		self.get_or_create_client(BlockChainType::EVM, network, |n| {
			let network = n.clone();
			Box::pin(async move { Self::EvmClient::new(&network).await })
		})
		.await
		.with_context(|| "Failed to get or create EVM client")
	}

	/// Gets or creates a Stellar client for the given network.
	///
	/// First checks the cache for an existing client. If none exists,
	/// creates a new client under a write lock.
	async fn get_stellar_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::StellarClient>, anyhow::Error> {
		self.get_or_create_client(BlockChainType::Stellar, network, |n| {
			let network = n.clone();
			Box::pin(async move { Self::StellarClient::new(&network).await })
		})
		.await
		.with_context(|| "Failed to get or create Stellar client")
	}

	/// Gets or creates a Midnight client for the given network.
	///
	/// First checks the cache for an existing client. If none exists,
	/// creates a new client under a write lock.
	async fn get_midnight_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::MidnightClient>, anyhow::Error> {
		self.get_or_create_client(BlockChainType::Midnight, network, |n| {
			let network = n.clone();
			Box::pin(async move { Self::MidnightClient::new(&network).await })
		})
		.await
		.with_context(|| "Failed to get or create Midnight client")
	}
}

impl Default for ClientPool {
	fn default() -> Self {
		Self::new()
	}
}
