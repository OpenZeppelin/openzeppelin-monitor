//! Configuration loading and validation.
//!
//! This module provides traits and implementations for loading and validating
//! configuration files for networks, monitors, and triggers.

#![allow(clippy::result_large_err)]

use async_trait::async_trait;
use std::path::Path;

mod error;
mod monitor_config;
mod network_config;
mod trigger_config;

pub use error::ConfigError;

/// Common interface for loading configuration files
#[async_trait]
pub trait ConfigLoader: Sized {
	/// Load all configuration files from a directory
	///
	/// If no path is provided, uses the default config directory.
	async fn load_all<T>(path: Option<&Path>) -> Result<T, error::ConfigError>
	where
		T: FromIterator<(String, Self)>;

	/// Load configuration from a specific file path
	async fn load_from_path(path: &Path) -> Result<Self, error::ConfigError>;

	/// Validate the configuration
	///
	/// Returns Ok(()) if valid, or an error message if invalid.
	fn validate(&self) -> Result<(), error::ConfigError>;

	/// Validate safety of the protocol
	///
	/// Returns if safe, or logs a warning message if unsafe.
	fn validate_protocol(&self);

	/// Check if a file is a JSON file based on extension
	fn is_json_file(path: &Path) -> bool {
		path.extension()
			.map(|ext| ext.to_string_lossy().to_lowercase() == "json")
			.unwrap_or(false)
	}

	/// Resolve all secrets in the configuration
	async fn resolve_secrets(&self) -> Result<Self, ConfigError>;

	/// Validate uniqueness of the configuration
	/// # Arguments
	/// * `instances` - The instances to validate uniqueness against
	/// * `current_instance` - The current instance to validate uniqueness for
	/// * `file_path` - The path to the file containing the current instance (for logging purposes)
	///
	/// Returns Ok(()) if valid, or an error message if found duplicate names.
	fn validate_uniqueness(
		instances: &[&Self],
		current_instance: &Self,
		file_path: &str,
	) -> Result<(), ConfigError>;
}
