//! Network configuration repository implementation.
//!
//! This module provides storage and retrieval of network configurations, which define
//! blockchain connection details and parameters. The repository loads network
//! configurations from JSON files.

use std::{collections::HashMap, path::Path};

use crate::{
	models::{ConfigLoader, Network},
	repositories::error::RepositoryError,
};

/// Repository for storing and retrieving network configurations
#[derive(Clone)]
pub struct NetworkRepository {
	/// Map of network slugs to their configurations
	pub networks: HashMap<String, Network>,
}

impl NetworkRepository {
	/// Create a new network repository from the given path
	///
	/// Loads all network configurations from JSON files in the specified directory
	/// (or default config directory if None is provided).
	pub fn new(path: Option<&Path>) -> Result<Self, RepositoryError> {
		let networks = Self::load_all(path)?;
		Ok(NetworkRepository { networks })
	}
}

/// Interface for network repository implementations
///
/// This trait defines the standard operations that any network repository must support,
/// allowing for different storage backends while maintaining a consistent interface.
pub trait NetworkRepositoryTrait: Clone {
	/// Create a new repository instance
	fn new(path: Option<&Path>) -> Result<Self, RepositoryError>
	where
		Self: Sized;

	/// Load all network configurations from the given path
	///
	/// If no path is provided, uses the default config directory.
	/// This is a static method that doesn't require an instance.
	fn load_all(path: Option<&Path>) -> Result<HashMap<String, Network>, RepositoryError>;

	/// Get a specific network by ID
	///
	/// Returns None if the network doesn't exist.
	fn get(&self, network_id: &str) -> Option<Network>;

	/// Get all networks
	///
	/// Returns a copy of the network map to prevent external mutation.
	fn get_all(&self) -> HashMap<String, Network>;
}

impl NetworkRepositoryTrait for NetworkRepository {
	fn new(path: Option<&Path>) -> Result<Self, RepositoryError> {
		NetworkRepository::new(path)
	}

	fn load_all(path: Option<&Path>) -> Result<HashMap<String, Network>, RepositoryError> {
		Network::load_all(path)
			.map_err(|e| RepositoryError::load_error(format!("Failed to load networks: {}", e)))
	}

	fn get(&self, network_id: &str) -> Option<Network> {
		self.networks.get(network_id).cloned()
	}

	fn get_all(&self) -> HashMap<String, Network> {
		self.networks.clone()
	}
}

/// Service layer for network repository operations
///
/// This type provides a higher-level interface for working with network configurations,
/// handling repository initialization and access through a trait-based interface.

#[derive(Clone)]
pub struct NetworkService<T: NetworkRepositoryTrait> {
	repository: T,
}

impl<T: NetworkRepositoryTrait> NetworkService<T> {
	/// Create a new network service with the default repository implementation
	pub fn new(path: Option<&Path>) -> Result<NetworkService<NetworkRepository>, RepositoryError> {
		let repository = NetworkRepository::new(path)?;
		Ok(NetworkService { repository })
	}

	/// Create a new network service with a custom repository implementation
	pub fn new_with_repository(repository: T) -> Result<Self, RepositoryError> {
		Ok(NetworkService { repository })
	}

	/// Create a new network service with a specific configuration path
	pub fn new_with_path(
		path: Option<&Path>,
	) -> Result<NetworkService<NetworkRepository>, RepositoryError> {
		let repository = NetworkRepository::new(path)?;
		Ok(NetworkService { repository })
	}

	/// Get a specific network by ID
	pub fn get(&self, network_id: &str) -> Option<Network> {
		self.repository.get(network_id)
	}

	/// Get all networks
	pub fn get_all(&self) -> HashMap<String, Network> {
		self.repository.get_all()
	}
}
