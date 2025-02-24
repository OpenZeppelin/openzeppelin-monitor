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
	models::Network,
	services::blockchain::{BlockChainClient, BlockChainError, EvmClient, StellarClient},
};
use futures::future::BoxFuture;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

/// Main client pool manager that handles multiple blockchain types.
///
/// Provides type-safe access to cached blockchain clients. Clients are created
/// on demand when first requested and then cached for future use. Uses RwLock
/// for thread-safe access and Arc for shared ownership.
pub struct ClientPool {
	/// Thread-safe map of EVM clients indexed by network slug
	evm_clients: Arc<RwLock<HashMap<String, Arc<EvmClient>>>>,
	/// Thread-safe map of Stellar clients indexed by network slug
	stellar_clients: Arc<RwLock<HashMap<String, Arc<StellarClient>>>>,
}

impl ClientPool {
	/// Creates a new empty client pool.
	///
	/// Initializes empty hashmaps for both EVM and Stellar clients.
	pub fn new() -> Self {
		Self {
			evm_clients: Arc::new(RwLock::new(HashMap::new())),
			stellar_clients: Arc::new(RwLock::new(HashMap::new())),
		}
	}

	/// Gets or creates an EVM client for the given network.
	///
	/// First checks the cache for an existing client. If none exists,
	/// creates a new client under a write lock.
	pub async fn get_evm_client(
		&self,
		network: &Network,
	) -> Result<Arc<EvmClient>, BlockChainError> {
		self.get_or_create_client(&self.evm_clients, network, |n| {
			let network = n.clone();
			Box::pin(async move { EvmClient::new(&network).await })
		})
		.await
	}

	/// Gets or creates a Stellar client for the given network.
	///
	/// First checks the cache for an existing client. If none exists,
	/// creates a new client under a write lock.
	pub async fn get_stellar_client(
		&self,
		network: &Network,
	) -> Result<Arc<StellarClient>, BlockChainError> {
		self.get_or_create_client(&self.stellar_clients, network, |n| {
			let network = n.clone();
			Box::pin(async move { StellarClient::new(&network).await })
		})
		.await
	}

	/// Internal helper method to get or create a client of any type.
	///
	/// Uses a double-checked locking pattern:
	/// 1. Fast path with read lock to check for existing client
	/// 2. Slow path with write lock to create new client if needed
	///
	/// This ensures thread-safety while maintaining good performance
	/// for the common case of accessing existing clients.
	async fn get_or_create_client<T: BlockChainClient>(
		&self,
		clients: &Arc<RwLock<HashMap<String, Arc<T>>>>,
		network: &Network,
		create_fn: impl Fn(&Network) -> BoxFuture<'static, Result<T, BlockChainError>>,
	) -> Result<Arc<T>, BlockChainError> {
		// Fast path: check if client exists
		if let Some(client) = clients.read().await.get(&network.slug) {
			return Ok(client.clone());
		}

		// Slow path: create new client
		let mut clients = clients.write().await;
		let client = Arc::new(create_fn(network).await?);
		clients.insert(network.slug.clone(), client.clone());
		Ok(client)
	}
}

impl Default for ClientPool {
	fn default() -> Self {
		Self::new()
	}
}
