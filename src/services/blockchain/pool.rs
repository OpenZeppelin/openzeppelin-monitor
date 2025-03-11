//! Client pool for managing blockchain clients.
//!
//! This module provides a thread-safe client pooling system that:
//! - Caches blockchain clients by network
//! - Creates clients lazily on first use
//! - Handles both EVM and Stellar clients
//! - Provides type-safe access to clients
//! - Manages client lifecycles automatically
//!
//! The pool uses a fast path for existing clients and a slow path for
//! creating new ones, optimizing performance while maintaining safety.

use crate::{
	models::{BlockChainType, Network},
	services::blockchain::{
		AlloyTransportClient, BlockChainClient, BlockChainError, BlockFilterFactory, EvmClient,
		EvmClientTrait, StellarClient, StellarClientTrait, StellarTransportClient,
	},
};
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

	async fn get_evm_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::EvmClient>, BlockChainError>;
	async fn get_stellar_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::StellarClient>, BlockChainError>;
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
	/// Initializes empty hashmaps for both EVM and Stellar clients.
	pub fn new() -> Self {
		let mut pool = Self {
			storages: HashMap::new(),
		};

		// Register client types
		pool.register_client_type::<EvmClient<AlloyTransportClient>>(BlockChainType::EVM);
		pool.register_client_type::<StellarClient<StellarTransportClient>>(BlockChainType::Stellar);

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
		create_fn: impl Fn(&Network) -> BoxFuture<'static, Result<T, BlockChainError>>,
	) -> Result<Arc<T>, BlockChainError> {
		let storage = self
			.storages
			.get(&client_type)
			.and_then(|s| s.downcast_ref::<ClientStorage<T>>())
			.ok_or_else(|| BlockChainError::client_pool_error("Invalid client type".to_string()))?;

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
	type EvmClient = EvmClient<AlloyTransportClient>;
	type StellarClient = StellarClient<StellarTransportClient>;

	/// Gets or creates an EVM client for the given network.
	///
	/// First checks the cache for an existing client. If none exists,
	/// creates a new client under a write lock.
	async fn get_evm_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::EvmClient>, BlockChainError> {
		self.get_or_create_client(BlockChainType::EVM, network, |n| {
			let network = n.clone();
			Box::pin(async move { Self::EvmClient::new(&network).await })
		})
		.await
		.map_err(|e| BlockChainError::client_pool_error(e.to_string()))
	}

	/// Gets or creates a Stellar client for the given network.
	///
	/// First checks the cache for an existing client. If none exists,
	/// creates a new client under a write lock.
	async fn get_stellar_client(
		&self,
		network: &Network,
	) -> Result<Arc<Self::StellarClient>, BlockChainError> {
		self.get_or_create_client(BlockChainType::Stellar, network, |n| {
			let network = n.clone();
			Box::pin(async move { Self::StellarClient::new(&network).await })
		})
		.await
		.map_err(|e| BlockChainError::client_pool_error(e.to_string()))
	}
}

impl Default for ClientPool {
	fn default() -> Self {
		Self::new()
	}
}
