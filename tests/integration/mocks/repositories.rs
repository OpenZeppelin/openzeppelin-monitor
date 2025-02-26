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

use mockall::{mock, predicate::*};

mock! {
	/// Mock implementation of the trigger repository.
	///
	/// Provides methods to simulate trigger storage and retrieval operations
	/// for testing purposes.
	pub TriggerRepository {}

	impl TriggerRepositoryTrait for TriggerRepository {
		#[mockall::concretize]
		fn new(path: Option<&Path>) -> Result<Self, Box<RepositoryError>>;
		#[mockall::concretize]
		fn load_all(path: Option<&Path>) -> Result<HashMap<String, Trigger>, Box<RepositoryError>>;
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

	impl NetworkRepositoryTrait for NetworkRepository {
		#[mockall::concretize]
		fn new(path: Option<&Path>) -> Result<Self, Box<RepositoryError>>;
		#[mockall::concretize]
		fn load_all(path: Option<&Path>) -> Result<HashMap<String, Network>, Box<RepositoryError>>;
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
	pub MonitorRepository<N: NetworkRepositoryTrait + 'static, T: TriggerRepositoryTrait + 'static> {}

	impl<N: NetworkRepositoryTrait + 'static, T: TriggerRepositoryTrait + 'static>
		MonitorRepositoryTrait<N, T> for MonitorRepository<N, T>
	{
		#[mockall::concretize]
		fn new(
			path: Option<&Path>,
			network_service: Option<NetworkService<N>>,
			trigger_service: Option<TriggerService<T>>,
		) -> Result<Self, Box<RepositoryError>>;
		#[mockall::concretize]
		fn load_all(
			path: Option<&Path>,
			network_service: Option<NetworkService<N>>,
			trigger_service: Option<TriggerService<T>>,
		) -> Result<HashMap<String, Monitor>, Box<RepositoryError>>;
		fn get(&self, monitor_id: &str) -> Option<Monitor>;
		fn get_all(&self) -> HashMap<String, Monitor>;
	}

	impl<N: NetworkRepositoryTrait + 'static, T: TriggerRepositoryTrait + 'static> Clone
		for MonitorRepository<N, T>
	{
		fn clone(&self) -> Self {
			Self {}
		}
	}
}
