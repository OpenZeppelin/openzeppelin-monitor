use openzeppelin_monitor::models::{Monitor, Network, Trigger};
use openzeppelin_monitor::repositories::{
    MonitorRepositoryTrait, NetworkRepositoryTrait, RepositoryError, TriggerRepositoryTrait,
};

use std::{collections::HashMap, path::Path};

use mockall::mock;
use mockall::predicate::*;

mock! {
    pub TriggerRepository {}

    impl TriggerRepositoryTrait for TriggerRepository {
        fn load_all<'a>(&'a self, path: Option<&'a Path>) -> Result<HashMap<String, Trigger>, RepositoryError>;
        fn get(&self, trigger_id: &str) -> Option<Trigger>;
        fn get_all(&self) -> HashMap<String, Trigger>;
    }
}

mock! {
    pub NetworkRepository {}

    impl NetworkRepositoryTrait for NetworkRepository {
        fn load_all<'a>(&'a self, path: Option<&'a Path>) -> Result<HashMap<String,Network> , RepositoryError>;
        fn get(&self, network_id: &str) -> Option<Network>;
        fn get_all(&self) -> HashMap<String, Network>;
    }
}

mock! {
    pub MonitorRepository {}

    impl MonitorRepositoryTrait for MonitorRepository {
        fn load_all<'a>(&'a self, path: Option<&'a Path>) -> Result<HashMap<String,Monitor> , RepositoryError>;
        fn get(&self, monitor_id: &str) -> Option<Monitor>;
        fn get_all(&self) -> HashMap<String, Monitor>;
    }
}
