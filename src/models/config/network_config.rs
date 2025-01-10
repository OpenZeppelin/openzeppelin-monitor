//! Network configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Network configurations,
//! allowing network definitions to be loaded from JSON files.

use std::path::Path;

use crate::models::config::error::ConfigError;
use crate::models::{BlockChainType, ConfigLoader, Network};

impl ConfigLoader for Network {
    /// Load all network configurations from a directory
    ///
    /// Reads and parses all JSON files in the specified directory (or default
    /// config directory) as network configurations.
    fn load_all<T>(path: Option<&Path>) -> Result<T, ConfigError>
    where
        T: FromIterator<(String, Self)>,
    {
        let network_dir = path.unwrap_or(Path::new("config/networks"));
        let mut pairs = Vec::new();

        if !network_dir.exists() {
            return Err(ConfigError::file_error("networks directory not found"));
        }

        for entry in std::fs::read_dir(network_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !Self::is_json_file(&path) {
                continue;
            }

            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            if let Ok(network) = Self::load_from_path(&path) {
                pairs.push((name, network));
            }
        }

        Ok(T::from_iter(pairs))
    }

    /// Load a network configuration from a specific file
    ///
    /// Reads and parses a single JSON file as a network configuration.
    fn load_from_path(path: &std::path::Path) -> Result<Self, ConfigError> {
        let file = std::fs::File::open(path)?;
        let config: Network = serde_json::from_reader(file)?;

        // Validate the config after loading
        if let Err(validation_error) = config.validate() {
            return Err(ConfigError::validation_error(validation_error.to_string()));
        }

        Ok(config)
    }

    /// Validate the network configuration
    ///
    /// Ensures that:
    /// - The network has a valid name and slug
    /// - At least one RPC URL is specified
    /// - Required chain-specific parameters are present
    /// - Block time and confirmation values are reasonable
    fn validate(&self) -> Result<(), ConfigError> {
        // Validate network_type
        match self.network_type {
            BlockChainType::EVM | BlockChainType::Stellar => {}
            _ => return Err(ConfigError::validation_error("Invalid network_type")),
        }

        // Validate slug
        if !self
            .slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(ConfigError::validation_error(
                "Slug must contain only lowercase letters, numbers, and underscores",
            ));
        }

        // Validate RPC URL types
        let supported_types = ["rpc"];
        if !self
            .rpc_urls
            .iter()
            .all(|rpc_url| supported_types.contains(&rpc_url.type_.as_str()))
        {
            return Err(ConfigError::validation_error(format!(
                "RPC URL type must be one of: {}",
                supported_types.join(", ")
            )));
        }

        // Validate RPC URLs format
        if !self.rpc_urls.iter().all(|rpc_url| {
            rpc_url.url.starts_with("http://") || rpc_url.url.starts_with("https://")
        }) {
            return Err(ConfigError::validation_error(
                "All RPC URLs must start with http:// or https://",
            ));
        }

        // Validate RPC URL weights
        if !self.rpc_urls.iter().all(|rpc_url| rpc_url.weight <= 100) {
            return Err(ConfigError::validation_error(
                "All RPC URL weights must be between 0 and 100",
            ));
        }

        // Validate block time
        if self.block_time_ms < 100 {
            return Err(ConfigError::validation_error(
                "Block time must be at least 100ms",
            ));
        }

        // Validate confirmation blocks
        if self.confirmation_blocks == 0 {
            return Err(ConfigError::validation_error(
                "Confirmation blocks must be greater than 0",
            ));
        }

        // Validate max_past_blocks
        if self.max_past_blocks.unwrap_or(0) == 0 {
            return Err(ConfigError::validation_error(
                "max_past_blocks must be greater than 0",
            ));
        }

        Ok(())
    }
}
