//! Repository implementations for configuration management.
//!
//! This module provides traits and implementations for loading and managing
//! configuration data from the filesystem. Each repository type handles a specific
//! configuration type and provides:
//!
//! - Loading configurations from JSON files
//! - Validating configuration references between different types
//! - Accessing configurations through a service layer
//!
//! Currently supported repositories:
//! - Monitor: Loads and validates monitor configurations, ensuring referenced networks and triggers
//!   exist
//! - Network: Loads network configurations defining blockchain connection details
//! - Trigger: Loads trigger configurations defining actions to take when conditions match

mod error;
mod monitor;
mod network;
mod trigger;

pub use error::RepositoryError;
pub use monitor::{MonitorRepository, MonitorRepositoryTrait, MonitorService};
pub use network::{NetworkRepository, NetworkRepositoryTrait, NetworkService};
pub use trigger::{TriggerRepository, TriggerRepositoryTrait, TriggerService};
