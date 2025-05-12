//! Mock implementations of repository traits.
//!
//! This module provides mock implementations of the repository interfaces used
//! for testing. It includes:
//! - [`MockTriggerRepository`] - Mock implementation of trigger repository
//! - [`MockNetworkRepository`] - Mock implementation of network repository
//! - [`MockMonitorRepository`] - Mock implementation of monitor repository
//!
//! These mocks allow testing repository-dependent functionality without actual
//! file system operations.

use openzeppelin_monitor::{
	models::{Monitor, Network, Trigger},
	repositories::{
		MonitorRepositoryTrait, NetworkRepositoryTrait, NetworkService, RepositoryError,
		TriggerRepositoryTrait, TriggerService,
	},
};

use std::{collections::HashMap, path::Path};

use async_trait::async_trait;
use mockall::{mock, predicate::*};

mock! {
	/// Mock implementation of the trigger repository.
	///
	/// Provides methods to simulate trigger storage and retrieval operations
	/// for testing purposes.
	pub TriggerRepository {}

	#[async_trait]
	impl TriggerRepositoryTrait for TriggerRepository {
		#[mockall::concretize]
		async fn new(path: Option<&Path>) -> Result<Self, RepositoryError>
		where
			Self: Sized;
		#[mockall::concretize]
		async fn load_all(path: Option<&Path>) -> Result<HashMap<String, Trigger>, RepositoryError>;
		fn get(&self, trigger_id: &str) -> Option<Trigger>;
		fn get_all(&self) -> HashMap<String, Trigger>;
	}

	impl Clone for TriggerRepository {
		fn clone(&self) -> Self {
			Self {}
		}
	}
}

mock! {
	/// Mock implementation of the network repository.
	///
	/// Provides methods to simulate network configuration storage and retrieval
	/// operations for testing purposes.
	pub NetworkRepository {}

	#[async_trait]
	impl NetworkRepositoryTrait for NetworkRepository {
		#[mockall::concretize]
		async fn new(path: Option<&Path>) -> Result<Self, RepositoryError>
		where
			Self: Sized;
		#[mockall::concretize]
		async fn load_all(path: Option<&Path>) -> Result<HashMap<String, Network>, RepositoryError>;
		fn get(&self, network_id: &str) -> Option<Network>;
		fn get_all(&self) -> HashMap<String, Network>;
	}

	impl Clone for NetworkRepository {
		fn clone(&self) -> Self {
			Self {}
		}
	}
}

mock! {
	/// Mock implementation of the monitor repository.
	///
	/// Provides methods to simulate monitor configuration storage and retrieval
	/// operations for testing purposes.
	pub MonitorRepository<N: NetworkRepositoryTrait + Send + Sync + 'static, T: TriggerRepositoryTrait + Send + Sync + 'static> {}

	#[async_trait]
	impl<N: NetworkRepositoryTrait + Send + Sync + 'static, T: TriggerRepositoryTrait + Send + Sync + 'static>
		MonitorRepositoryTrait<N, T> for MonitorRepository<N, T>
	{
		#[mockall::concretize]
		async fn new(
			path: Option<&Path>,
			network_service: Option<NetworkService<N>>,
			trigger_service: Option<TriggerService<T>>,
		) -> Result<Self, RepositoryError>
		where
			Self: Sized;
		#[mockall::concretize]
		async fn load_all(
			path: Option<&Path>,
			network_service: Option<NetworkService<N>>,
			trigger_service: Option<TriggerService<T>>,
		) -> Result<HashMap<String, Monitor>, RepositoryError>;
		#[mockall::concretize]
		async fn load_from_path(
			&self,
			path: Option<&Path>,
			network_service: Option<NetworkService<N>>,
			trigger_service: Option<TriggerService<T>>,
		) -> Result<Monitor, RepositoryError>;
		fn get(&self, monitor_id: &str) -> Option<Monitor>;
		fn get_all(&self) -> HashMap<String, Monitor>;
	}

	impl<N: NetworkRepositoryTrait + Send + Sync + 'static, T: TriggerRepositoryTrait + Send + Sync + 'static> Clone
		for MonitorRepository<N, T>
	{
		fn clone(&self) -> Self {
			Self {}
		}
	}
}
