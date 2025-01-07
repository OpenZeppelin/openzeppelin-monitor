use std::collections::HashMap;
use std::path::Path;

use crate::models::{ConfigLoader, Monitor, Network, Trigger};
use crate::repositories::error::RepositoryError;
use crate::repositories::network::NetworkRepository;
use crate::repositories::trigger::TriggerRepository;

pub struct MonitorRepository {
    pub monitors: HashMap<String, Monitor>,
}

impl MonitorRepository {
    pub fn new(path: Option<&Path>) -> Result<Self, RepositoryError> {
        let monitors = Monitor::load_all(path)
            .map_err(|e| RepositoryError::load_error(format!("Failed to load monitors: {}", e)))?;
        Ok(MonitorRepository { monitors })
    }

    pub fn validate_monitor_references(
        monitors: &HashMap<String, Monitor>,
        triggers: &HashMap<String, Trigger>,
        networks: &HashMap<String, Network>,
    ) -> Result<(), RepositoryError> {
        let mut validation_errors = Vec::new();

        for (monitor_name, monitor) in monitors {
            // Validate trigger references
            for trigger_id in &monitor.triggers {
                if !triggers.contains_key(trigger_id) {
                    validation_errors.push(format!(
                        "Monitor '{}' references non-existent trigger '{}'",
                        monitor_name, trigger_id
                    ));
                }
            }

            // Validate network references
            for network_slug in &monitor.networks {
                if !networks.contains_key(network_slug) {
                    validation_errors.push(format!(
                        "Monitor '{}' references non-existent network '{}'",
                        monitor_name, network_slug
                    ));
                }
            }
        }

        if !validation_errors.is_empty() {
            return Err(RepositoryError::validation_error(format!(
                "Configuration validation failed:\n{}",
                validation_errors.join("\n")
            )));
        }

        Ok(())
    }
}

pub trait MonitorRepositoryTrait {
    fn load_all(&self, path: Option<&Path>) -> Result<HashMap<String, Monitor>, RepositoryError>;
    fn get(&self, monitor_id: &str) -> Option<Monitor>;
    fn get_all(&self) -> HashMap<String, Monitor>;
}

impl MonitorRepositoryTrait for MonitorRepository {
    fn load_all(&self, path: Option<&Path>) -> Result<HashMap<String, Monitor>, RepositoryError> {
        let monitors =
            Monitor::load_all(path).map_err(|e| RepositoryError::load_error(e.to_string()))?;
        let triggers = TriggerRepository::new(None).unwrap().triggers;
        let networks = NetworkRepository::new(None).unwrap().networks;

        Self::validate_monitor_references(&monitors, &triggers, &networks)?;

        Ok(monitors)
    }

    fn get(&self, monitor_id: &str) -> Option<Monitor> {
        self.monitors.get(monitor_id).cloned()
    }

    fn get_all(&self) -> HashMap<String, Monitor> {
        self.monitors.clone()
    }
}

pub struct MonitorService<T: MonitorRepositoryTrait> {
    repository: T,
}

impl<T: MonitorRepositoryTrait> MonitorService<T> {
    pub fn new(path: Option<&Path>) -> Result<MonitorService<MonitorRepository>, RepositoryError> {
        let repository = MonitorRepository::new(path)?;
        Ok(MonitorService { repository })
    }

    pub fn new_with_repository(repository: T) -> Result<Self, RepositoryError> {
        Ok(MonitorService { repository })
    }

    pub fn new_with_path(
        path: Option<&Path>,
    ) -> Result<MonitorService<MonitorRepository>, RepositoryError> {
        let repository = MonitorRepository::new(path)?;
        Ok(MonitorService { repository })
    }

    pub fn get(&self, monitor_id: &str) -> Option<Monitor> {
        self.repository.get(monitor_id)
    }

    pub fn get_all(&self) -> HashMap<String, Monitor> {
        self.repository.get_all()
    }
}
