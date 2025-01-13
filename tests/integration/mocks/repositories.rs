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

use openzeppelin_monitor::models::{Monitor, Network, Trigger};
use openzeppelin_monitor::repositories::{
    MonitorRepositoryTrait, NetworkRepositoryTrait, RepositoryError, TriggerRepositoryTrait,
};

use std::{collections::HashMap, path::Path};

use mockall::mock;
use mockall::predicate::*;

mock! {
    /// Mock implementation of the trigger repository.
    ///
    /// Provides methods to simulate trigger storage and retrieval operations
    /// for testing purposes.
    pub TriggerRepository {}

    impl TriggerRepositoryTrait for TriggerRepository {
        fn load_all<'a>(&'a self, path: Option<&'a Path>) -> Result<HashMap<String, Trigger>, RepositoryError>;
        fn get(&self, trigger_id: &str) -> Option<Trigger>;
        fn get_all(&self) -> HashMap<String, Trigger>;
    }
}

mock! {
    /// Mock implementation of the network repository.
    ///
    /// Provides methods to simulate network configuration storage and retrieval
    /// operations for testing purposes.
    pub NetworkRepository {}

    impl NetworkRepositoryTrait for NetworkRepository {
        fn load_all<'a>(&'a self, path: Option<&'a Path>) -> Result<HashMap<String,Network> , RepositoryError>;
        fn get(&self, network_id: &str) -> Option<Network>;
        fn get_all(&self) -> HashMap<String, Network>;
    }
}

mock! {
    /// Mock implementation of the monitor repository.
    ///
    /// Provides methods to simulate monitor configuration storage and retrieval
    /// operations for testing purposes.
    pub MonitorRepository {}

    impl MonitorRepositoryTrait for MonitorRepository {
        fn load_all<'a>(&'a self, path: Option<&'a Path>) -> Result<HashMap<String,Monitor> , RepositoryError>;
        fn get(&self, monitor_id: &str) -> Option<Monitor>;
        fn get_all(&self) -> HashMap<String, Monitor>;
    }
}
