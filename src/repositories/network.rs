use std::{collections::HashMap, path::Path};

use crate::{
    models::{ConfigLoader, Network},
    repositories::error::RepositoryError,
};

pub struct NetworkRepository {
    pub networks: HashMap<String, Network>,
}

impl NetworkRepository {
    pub fn new(path: Option<&Path>) -> Result<Self, RepositoryError> {
        let networks = Network::load_all(path)
            .map_err(|e| RepositoryError::load_error(format!("Failed to load networks: {}", e)))?;
        Ok(NetworkRepository { networks })
    }
}

pub trait NetworkRepositoryTrait {
    fn load_all(&self, path: Option<&Path>) -> Result<HashMap<String, Network>, RepositoryError>;
    fn get(&self, network_id: &str) -> Option<Network>;
    fn get_all(&self) -> HashMap<String, Network>;
}

impl NetworkRepositoryTrait for NetworkRepository {
    fn load_all(&self, path: Option<&Path>) -> Result<HashMap<String, Network>, RepositoryError> {
        Network::load_all(path)
            .map_err(|e| RepositoryError::load_error(format!("Failed  to load networks: {}", e)))
    }

    fn get(&self, network_id: &str) -> Option<Network> {
        self.networks.get(network_id).cloned()
    }

    fn get_all(&self) -> HashMap<String, Network> {
        self.networks.clone()
    }
}

pub struct NetworkService<T: NetworkRepositoryTrait> {
    repository: T,
}

impl<T: NetworkRepositoryTrait> NetworkService<T> {
    pub fn new(path: Option<&Path>) -> Result<NetworkService<NetworkRepository>, RepositoryError> {
        let repository = NetworkRepository::new(path)?;
        Ok(NetworkService { repository })
    }

    pub fn new_with_repository(repository: T) -> Result<Self, RepositoryError> {
        Ok(NetworkService { repository })
    }

    pub fn new_with_path(
        path: Option<&Path>,
    ) -> Result<NetworkService<NetworkRepository>, RepositoryError> {
        let repository = NetworkRepository::new(path)?;
        Ok(NetworkService { repository })
    }

    pub fn get(&self, network_id: &str) -> Option<Network> {
        self.repository.get(network_id)
    }

    pub fn get_all(&self) -> HashMap<String, Network> {
        self.repository.get_all()
    }
}
