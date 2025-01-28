//! Network configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Network configurations,
//! allowing network definitions to be loaded from JSON files.

use log::warn;
use std::{path::Path, str::FromStr};

use crate::{
	models::{config::error::ConfigError, BlockChainType, ConfigLoader, Network},
	utils::get_cron_interval_ms,
};

impl Network {
	/// Calculates the recommended minimum number of past blocks to maintain for this network.
	///
	/// This function computes a safe minimum value based on three factors:
	/// 1. The number of blocks that occur during one cron interval (`blocks_per_cron`)
	/// 2. The required confirmation blocks for the network
	/// 3. An additional buffer block (+1)
	///
	/// The formula used is: `(cron_interval_ms / block_time_ms) + confirmation_blocks + 1`
	///
	/// # Returns
	/// * `u64` - The recommended minimum number of past blocks to maintain
	///
	/// # Note
	/// If the cron schedule parsing fails, the blocks_per_cron component will be 0,
	/// resulting in a minimum recommendation of `confirmation_blocks + 1`
	pub fn get_recommended_past_blocks(&self) -> u64 {
		let cron_interval_ms = get_cron_interval_ms(&self.cron_schedule).unwrap_or(0) as u64;
		let blocks_per_cron = cron_interval_ms / self.block_time_ms;
		blocks_per_cron + self.confirmation_blocks + 1
	}
}

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
		config.validate()?;

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
		// Validate network name
		if self.name.is_empty() {
			return Err(ConfigError::validation_error("Network name is required"));
		}

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

		// Validate cron_schedule
		if self.cron_schedule.is_empty() {
			return Err(ConfigError::validation_error(
				"Cron schedule must be provided",
			));
		}

		// Add cron schedule format validation
		if let Err(e) = cron::Schedule::from_str(&self.cron_schedule) {
			return Err(ConfigError::validation_error(format!(
				"Invalid cron schedule format: {}",
				e
			)));
		}

		// Validate max_past_blocks
		if let Some(max_blocks) = self.max_past_blocks {
			if max_blocks == 0 {
				return Err(ConfigError::validation_error(
					"max_past_blocks must be greater than 0",
				));
			}

			let recommended_blocks = self.get_recommended_past_blocks();

			if max_blocks < recommended_blocks {
				warn!(
					"Network '{}' max_past_blocks ({}) below recommended {} \
					 (cron_interval/block_time + confirmations + 1)",
					self.slug, max_blocks, recommended_blocks
				);
			}
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::RpcUrl;

	fn create_valid_network() -> Network {
		Network {
			name: "Test Network".to_string(),
			slug: "test_network".to_string(),
			network_type: BlockChainType::EVM,
			chain_id: Some(1),
			network_passphrase: None,
			store_blocks: Some(true),
			rpc_urls: vec![RpcUrl {
				type_: "rpc".to_string(),
				url: "https://test.network".to_string(),
				weight: 100,
			}],
			block_time_ms: 1000,
			confirmation_blocks: 1,
			cron_schedule: "0 */5 * * * *".to_string(),
			max_past_blocks: Some(10),
		}
	}

	#[test]
	fn test_get_recommended_past_blocks() {
		let mut network = create_valid_network();
		network.block_time_ms = 1000; // 1 second
		network.confirmation_blocks = 2;
		network.cron_schedule = "0 */5 * * * *".to_string(); // every 5 minutes

		let cron_interval_ms = get_cron_interval_ms(&network.cron_schedule).unwrap() as u64; // 300.000 (5 minutes in ms)
		let blocks_per_cron = cron_interval_ms / network.block_time_ms; // 300.000 / 1000 = 300
		let recommended_past_blocks = blocks_per_cron + network.confirmation_blocks + 1; // 300 + 2 + 1 = 303

		assert_eq!(
			network.get_recommended_past_blocks(),
			recommended_past_blocks
		);
	}

	#[test]
	fn test_validate_valid_network() {
		let network = create_valid_network();
		assert!(network.validate().is_ok());
	}

	#[test]
	fn test_validate_empty_name() {
		let mut network = create_valid_network();
		network.name = "".to_string();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_invalid_slug() {
		let mut network = create_valid_network();
		network.slug = "Invalid-Slug".to_string();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_invalid_rpc_url_type() {
		let mut network = create_valid_network();
		network.rpc_urls[0].type_ = "invalid".to_string();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_invalid_rpc_url_format() {
		let mut network = create_valid_network();
		network.rpc_urls[0].url = "invalid-url".to_string();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_invalid_rpc_weight() {
		let mut network = create_valid_network();
		network.rpc_urls[0].weight = 101;
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_invalid_block_time() {
		let mut network = create_valid_network();
		network.block_time_ms = 50;
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_zero_confirmation_blocks() {
		let mut network = create_valid_network();
		network.confirmation_blocks = 0;
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_invalid_cron_schedule() {
		let mut network = create_valid_network();
		network.cron_schedule = "invalid cron".to_string();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_zero_max_past_blocks() {
		let mut network = create_valid_network();
		network.max_past_blocks = Some(0);
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}
}
