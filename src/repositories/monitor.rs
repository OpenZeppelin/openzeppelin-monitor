//! Monitor configuration repository implementation.
//!
//! This module provides storage and retrieval of monitor configurations, including
//! validation of references to networks and triggers. The repository loads monitor
//! configurations from JSON files and ensures all referenced components exist.

use std::collections::HashMap;
use std::path::Path;

use crate::models::{ConfigLoader, Monitor, Network, Trigger};
use crate::repositories::error::RepositoryError;
use crate::repositories::network::NetworkRepository;
use crate::repositories::trigger::TriggerRepository;

/// Repository for storing and retrieving monitor configurations
pub struct MonitorRepository {
    /// Map of monitor names to their configurations
    pub monitors: HashMap<String, Monitor>,
}

impl MonitorRepository {
    /// Create a new monitor repository from the given path
    ///
    /// Loads all monitor configurations from JSON files in the specified directory
    /// (or default config directory if None is provided).
    pub fn new(path: Option<&Path>) -> Result<Self, RepositoryError> {
        let monitors = Self::load_all(path)?;
        Ok(MonitorRepository { monitors })
    }

    /// Validate that all networks and triggers referenced by monitors exist
    ///
    /// Returns an error if any monitor references a non-existent network or trigger.
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

/// Interface for monitor repository implementations
///
/// This trait defines the standard operations that any monitor repository must support,
/// allowing for different storage backends while maintaining a consistent interface.
pub trait MonitorRepositoryTrait {
    /// Load all monitor configurations from the given path
    ///
    /// If no path is provided, uses the default config directory.
    /// Also validates references to networks and triggers.
    /// This is a static method that doesn't require an instance.
    fn load_all(path: Option<&Path>) -> Result<HashMap<String, Monitor>, RepositoryError>;

    /// Get a specific monitor by ID
    ///
    /// Returns None if the monitor doesn't exist.
    fn get(&self, monitor_id: &str) -> Option<Monitor>;

    /// Get all monitors
    ///
    /// Returns a copy of the monitor map to prevent external mutation.
    fn get_all(&self) -> HashMap<String, Monitor>;
}

impl MonitorRepositoryTrait for MonitorRepository {
    fn load_all(path: Option<&Path>) -> Result<HashMap<String, Monitor>, RepositoryError> {
        let monitors =
            Monitor::load_all(path).map_err(|e| RepositoryError::load_error(e.to_string()))?;
        let triggers = TriggerRepository::new(None)?.triggers;
        let networks = NetworkRepository::new(None)?.networks;

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

/// Service layer for monitor repository operations
///
/// This type provides a higher-level interface for working with monitor configurations,
/// handling repository initialization and access through a trait-based interface.
/// It also ensures that all monitor references to networks and triggers are valid.
pub struct MonitorService<T: MonitorRepositoryTrait> {
    repository: T,
}

impl<T: MonitorRepositoryTrait> MonitorService<T> {
    /// Create a new monitor service with the default repository implementation
    ///
    /// Loads monitor configurations from the specified path (or default config directory)
    /// and validates all network and trigger references.
    pub fn new(path: Option<&Path>) -> Result<MonitorService<MonitorRepository>, RepositoryError> {
        let repository = MonitorRepository::new(path)?;
        Ok(MonitorService { repository })
    }

    /// Create a new monitor service with a custom repository implementation
    ///
    /// Allows for using alternative storage backends that implement the MonitorRepositoryTrait.
    pub fn new_with_repository(repository: T) -> Result<Self, RepositoryError> {
        Ok(MonitorService { repository })
    }

    /// Create a new monitor service with a specific configuration path
    ///
    /// Similar to `new()` but makes the path parameter more explicit.
    pub fn new_with_path(
        path: Option<&Path>,
    ) -> Result<MonitorService<MonitorRepository>, RepositoryError> {
        let repository = MonitorRepository::new(path)?;
        Ok(MonitorService { repository })
    }

    /// Get a specific monitor by ID
    ///
    /// Returns None if the monitor doesn't exist.
    pub fn get(&self, monitor_id: &str) -> Option<Monitor> {
        self.repository.get(monitor_id)
    }

    /// Get all monitors
    ///
    /// Returns a copy of the monitor map to prevent external mutation.
    pub fn get_all(&self) -> HashMap<String, Monitor> {
        self.repository.get_all()
    }
}
