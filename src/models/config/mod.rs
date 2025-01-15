//! Configuration loading and validation.
//!
//! This module provides traits and implementations for loading and validating
//! configuration files for networks, monitors, and triggers.

use std::path::Path;

mod error;
mod monitor_config;
mod network_config;
mod trigger_config;

/// Common interface for loading configuration files
pub trait ConfigLoader: Sized {
	/// Load all configuration files from a directory
	///
	/// If no path is provided, uses the default config directory.
	fn load_all<T>(path: Option<&Path>) -> Result<T, error::ConfigError>
	where
		T: FromIterator<(String, Self)>;

	/// Load configuration from a specific file path
	fn load_from_path(path: &Path) -> Result<Self, error::ConfigError>;

	/// Validate the configuration
	///
	/// Returns Ok(()) if valid, or an error message if invalid.
	fn validate(&self) -> Result<(), error::ConfigError>;

	/// Check if a file is a JSON file based on extension
	fn is_json_file(path: &Path) -> bool {
		path.extension()
			.map(|ext| ext.to_string_lossy().to_lowercase() == "json")
			.unwrap_or(false)
	}
}
