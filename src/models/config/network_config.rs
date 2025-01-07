use std::path::Path;

use crate::models::{BlockChainType, ConfigLoader, Network};

use super::error::ConfigError;

impl ConfigLoader for Network {
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

    fn load_from_path(path: &std::path::Path) -> Result<Self, ConfigError> {
        let file = std::fs::File::open(path)?;
        let config: Network = serde_json::from_reader(file)?;

        // Validate the config after loading
        if let Err(validation_error) = config.validate() {
            return Err(ConfigError::validation_error(validation_error));
        }

        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        // Validate network_type
        match self.network_type {
            BlockChainType::EVM | BlockChainType::Stellar => {}
            _ => return Err("Invalid network_type".to_string()),
        }

        // Validate slug
        if !self
            .slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(
                "Slug must contain only lowercase letters, numbers, and underscores".to_string(),
            );
        }

        // Validate RPC URL types
        let supported_types = ["rpc"];
        if !self
            .rpc_urls
            .iter()
            .all(|rpc_url| supported_types.contains(&rpc_url.type_.as_str()))
        {
            return Err(format!(
                "RPC URL type must be one of: {}",
                supported_types.join(", ")
            ));
        }

        // Validate RPC URLs format
        if !self.rpc_urls.iter().all(|rpc_url| {
            rpc_url.url.starts_with("http://") || rpc_url.url.starts_with("https://")
        }) {
            return Err("All RPC URLs must start with http:// or https://".to_string());
        }

        // Validate RPC URL weights
        if !self.rpc_urls.iter().all(|rpc_url| rpc_url.weight <= 100) {
            return Err("All RPC URL weights must be between 0 and 100".to_string());
        }

        // Validate block time
        if self.block_time_ms < 100 {
            return Err("Block time must be at least 100ms".to_string());
        }

        // Validate confirmation blocks
        if self.confirmation_blocks == 0 {
            return Err("Confirmation blocks must be greater than 0".to_string());
        }

        // Validate max_past_blocks
        if self.max_past_blocks.unwrap_or(0) == 0 {
            return Err("max_past_blocks must be greater than 0".to_string());
        }

        Ok(())
    }
}
