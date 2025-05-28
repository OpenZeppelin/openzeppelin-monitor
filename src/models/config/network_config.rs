//! Network configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Network configurations,
//! allowing network definitions to be loaded from JSON files.

use async_trait::async_trait;
use std::{collections::HashMap, path::Path, str::FromStr};

use crate::{
	models::{config::error::ConfigError, BlockChainType, ConfigLoader, Network, SecretValue},
	utils::{get_cron_interval_ms, normalize_string},
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

#[async_trait]
impl ConfigLoader for Network {
	/// Resolve all secrets in the network configuration
	async fn resolve_secrets(&self) -> Result<Self, ConfigError> {
		dotenvy::dotenv().ok();
		let mut network = self.clone();

		for rpc_url in &mut network.rpc_urls {
			let resolved_url = rpc_url.url.resolve().await.map_err(|e| {
				ConfigError::parse_error(
					format!("failed to resolve RPC URL: {}", e),
					Some(Box::new(e)),
					None,
				)
			})?;
			rpc_url.url = SecretValue::Plain(resolved_url);
		}
		Ok(network)
	}

	/// Load all network configurations from a directory
	///
	/// Reads and parses all JSON files in the specified directory (or default
	/// config directory) as network configurations.
	async fn load_all<T>(path: Option<&Path>) -> Result<T, ConfigError>
	where
		T: FromIterator<(String, Self)>,
	{
		let network_dir = path.unwrap_or(Path::new("config/networks"));
		let mut pairs = Vec::new();

		if !network_dir.exists() {
			return Err(ConfigError::file_error(
				"networks directory not found",
				None,
				Some(HashMap::from([(
					"path".to_string(),
					network_dir.display().to_string(),
				)])),
			));
		}

		for entry in std::fs::read_dir(network_dir).map_err(|e| {
			ConfigError::file_error(
				format!("failed to read networks directory: {}", e),
				Some(Box::new(e)),
				Some(HashMap::from([(
					"path".to_string(),
					network_dir.display().to_string(),
				)])),
			)
		})? {
			let entry = entry.map_err(|e| {
				ConfigError::file_error(
					format!("failed to read directory entry: {}", e),
					Some(Box::new(e)),
					Some(HashMap::from([(
						"path".to_string(),
						network_dir.display().to_string(),
					)])),
				)
			})?;
			let path = entry.path();

			if !Self::is_json_file(&path) {
				continue;
			}

			let name = path
				.file_stem()
				.and_then(|s| s.to_str())
				.unwrap_or("unknown")
				.to_string();

			let network = Self::load_from_path(&path).await?;

			let existing_networks: Vec<&Network> =
				pairs.iter().map(|(_, network)| network).collect();
			// Check network name uniqueness before pushing
			Self::validate_uniqueness(&existing_networks, &network, &path.display().to_string())?;

			pairs.push((name, network));
		}

		Ok(T::from_iter(pairs))
	}

	/// Load a network configuration from a specific file
	///
	/// Reads and parses a single JSON file as a network configuration.
	async fn load_from_path(path: &std::path::Path) -> Result<Self, ConfigError> {
		let file = std::fs::File::open(path).map_err(|e| {
			ConfigError::file_error(
				format!("failed to open network config file: {}", e),
				Some(Box::new(e)),
				Some(HashMap::from([(
					"path".to_string(),
					path.display().to_string(),
				)])),
			)
		})?;
		let mut config: Network = serde_json::from_reader(file).map_err(|e| {
			ConfigError::parse_error(
				format!("failed to parse network config: {}", e),
				Some(Box::new(e)),
				Some(HashMap::from([(
					"path".to_string(),
					path.display().to_string(),
				)])),
			)
		})?;

		// Resolve secrets before validating
		config = config.resolve_secrets().await?;

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
			return Err(ConfigError::validation_error(
				"Network name is required",
				None,
				None,
			));
		}

		// Validate network_type
		match self.network_type {
			BlockChainType::EVM | BlockChainType::Stellar => {}
			_ => {
				return Err(ConfigError::validation_error(
					"Invalid network_type",
					None,
					None,
				));
			}
		}

		// Validate slug
		if !self
			.slug
			.chars()
			.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
		{
			return Err(ConfigError::validation_error(
				"Slug must contain only lowercase letters, numbers, and underscores",
				None,
				None,
			));
		}

		// Validate RPC URL types
		let supported_types = ["rpc"];
		if !self
			.rpc_urls
			.iter()
			.all(|rpc_url| supported_types.contains(&rpc_url.type_.as_str()))
		{
			return Err(ConfigError::validation_error(
				format!(
					"RPC URL type must be one of: {}",
					supported_types.join(", ")
				),
				None,
				None,
			));
		}

		// Validate RPC URLs format
		if !self.rpc_urls.iter().all(|rpc_url| {
			rpc_url.url.starts_with("http://") || rpc_url.url.starts_with("https://")
		}) {
			return Err(ConfigError::validation_error(
				"All RPC URLs must start with http:// or https://",
				None,
				None,
			));
		}

		// Validate RPC URL weights
		if !self.rpc_urls.iter().all(|rpc_url| rpc_url.weight <= 100) {
			return Err(ConfigError::validation_error(
				"All RPC URL weights must be between 0 and 100",
				None,
				None,
			));
		}

		// Validate block time
		if self.block_time_ms < 100 {
			return Err(ConfigError::validation_error(
				"Block time must be at least 100ms",
				None,
				None,
			));
		}

		// Validate confirmation blocks
		if self.confirmation_blocks == 0 {
			return Err(ConfigError::validation_error(
				"Confirmation blocks must be greater than 0",
				None,
				None,
			));
		}

		// Validate cron_schedule
		if self.cron_schedule.is_empty() {
			return Err(ConfigError::validation_error(
				"Cron schedule must be provided",
				None,
				None,
			));
		}

		// Add cron schedule format validation
		if let Err(e) = cron::Schedule::from_str(&self.cron_schedule) {
			return Err(ConfigError::validation_error(e.to_string(), None, None));
		}

		// Validate max_past_blocks
		if let Some(max_blocks) = self.max_past_blocks {
			if max_blocks == 0 {
				return Err(ConfigError::validation_error(
					"max_past_blocks must be greater than 0",
					None,
					None,
				));
			}

			let recommended_blocks = self.get_recommended_past_blocks();

			if max_blocks < recommended_blocks {
				tracing::warn!(
					"Network '{}' max_past_blocks ({}) below recommended {} \
					 (cron_interval/block_time + confirmations + 1)",
					self.slug,
					max_blocks,
					recommended_blocks
				);
			}
		}

		// Log a warning if the network uses an insecure protocol
		self.validate_protocol();

		Ok(())
	}

	/// Validate the safety of the protocol used in the network
	///
	/// Returns if safe, or logs a warning message if unsafe.
	fn validate_protocol(&self) {
		for rpc_url in &self.rpc_urls {
			if rpc_url.url.starts_with("http://") {
				tracing::warn!(
					"Network '{}' uses an insecure RPC URL: {}",
					self.slug,
					rpc_url.url.as_str()
				);
			}
			// Additional check for websocket connections
			if rpc_url.url.starts_with("ws://") {
				tracing::warn!(
					"Network '{}' uses an insecure WebSocket URL: {}",
					self.slug,
					rpc_url.url.as_str()
				);
			}
		}
	}

	fn validate_uniqueness(
		instances: &[&Self],
		current_instance: &Self,
		file_path: &str,
	) -> Result<(), ConfigError> {
		let fields = [
			("name", &current_instance.name),
			("slug", &current_instance.slug),
		];

		for (field_name, field_value) in fields {
			if instances.iter().any(|existing_network| {
				let existing_value = match field_name {
					"name" => &existing_network.name,
					"slug" => &existing_network.slug,
					_ => unreachable!(),
				};
				normalize_string(existing_value) == normalize_string(field_value)
			}) {
				return Err(ConfigError::validation_error(
					format!("Duplicate network {} found: '{}'", field_name, field_value),
					None,
					Some(HashMap::from([
						(format!("network_{}", field_name), field_value.to_string()),
						("path".to_string(), file_path.to_string()),
					])),
				));
			}
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::utils::tests::builders::network::NetworkBuilder;
	use std::fs;
	use tempfile::TempDir;
	use tracing_test::traced_test;

	// Replace create_valid_network() with NetworkBuilder usage
	fn create_valid_network() -> Network {
		NetworkBuilder::new()
			.name("Test Network")
			.slug("test_network")
			.network_type(BlockChainType::EVM)
			.chain_id(1)
			.store_blocks(true)
			.rpc_url("https://test.network")
			.block_time_ms(1000)
			.confirmation_blocks(1)
			.cron_schedule("0 */5 * * * *")
			.max_past_blocks(10)
			.build()
	}

	#[test]
	fn test_get_recommended_past_blocks() {
		let network = NetworkBuilder::new()
			.block_time_ms(1000) // 1 second
			.confirmation_blocks(2)
			.cron_schedule("0 */5 * * * *") // every 5 minutes
			.build();

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
		let network = NetworkBuilder::new().name("").build();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_invalid_slug() {
		let network = NetworkBuilder::new().slug("Invalid-Slug").build();
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
		let network = NetworkBuilder::new().rpc_url("invalid-url").build();
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
		let network = NetworkBuilder::new().block_time_ms(50).build();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_zero_confirmation_blocks() {
		let network = NetworkBuilder::new().confirmation_blocks(0).build();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_invalid_cron_schedule() {
		let network = NetworkBuilder::new().cron_schedule("invalid cron").build();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_zero_max_past_blocks() {
		let network = NetworkBuilder::new().max_past_blocks(0).build();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[test]
	fn test_validate_empty_cron_schedule() {
		let network = NetworkBuilder::new().cron_schedule("").build();
		assert!(matches!(
			network.validate(),
			Err(ConfigError::ValidationError(_))
		));
	}

	#[tokio::test]
	async fn test_invalid_load_from_path() {
		let path = Path::new("config/networks/invalid.json");
		assert!(matches!(
			Network::load_from_path(path).await,
			Err(ConfigError::FileError(_))
		));
	}

	#[tokio::test]
	async fn test_invalid_config_from_load_from_path() {
		use std::io::Write;
		use tempfile::NamedTempFile;

		let mut temp_file = NamedTempFile::new().unwrap();
		write!(temp_file, "{{\"invalid\": \"json").unwrap();

		let path = temp_file.path();

		assert!(matches!(
			Network::load_from_path(path).await,
			Err(ConfigError::ParseError(_))
		));
	}

	#[tokio::test]
	async fn test_load_all_directory_not_found() {
		let non_existent_path = Path::new("non_existent_directory");

		let result: Result<HashMap<String, Network>, ConfigError> =
			Network::load_all(Some(non_existent_path)).await;
		assert!(matches!(result, Err(ConfigError::FileError(_))));

		if let Err(ConfigError::FileError(err)) = result {
			assert!(err.message.contains("networks directory not found"));
		}
	}

	#[test]
	#[traced_test]
	fn test_validate_protocol_insecure_rpc() {
		let network = NetworkBuilder::new()
			.name("Test Network")
			.slug("test_network")
			.network_type(BlockChainType::EVM)
			.chain_id(1)
			.store_blocks(true)
			.add_rpc_url("http://test.network", "rpc", 100)
			.add_rpc_url("ws://test.network", "rpc", 100)
			.build();

		network.validate_protocol();
		assert!(logs_contain(
			"uses an insecure RPC URL: http://test.network"
		));
		assert!(logs_contain(
			"uses an insecure WebSocket URL: ws://test.network"
		));
	}

	#[test]
	#[traced_test]
	fn test_validate_protocol_secure_rpc() {
		let network = NetworkBuilder::new()
			.name("Test Network")
			.slug("test_network")
			.network_type(BlockChainType::EVM)
			.chain_id(1)
			.store_blocks(true)
			.add_rpc_url("https://test.network", "rpc", 100)
			.add_rpc_url("wss://test.network", "rpc", 100)
			.build();

		network.validate_protocol();
		assert!(!logs_contain("uses an insecure RPC URL"));
		assert!(!logs_contain("uses an insecure WebSocket URL"));
	}

	#[test]
	#[traced_test]
	fn test_validate_protocol_mixed_security() {
		let network = NetworkBuilder::new()
			.name("Test Network")
			.slug("test_network")
			.network_type(BlockChainType::EVM)
			.chain_id(1)
			.store_blocks(true)
			.add_rpc_url("https://secure.network", "rpc", 100)
			.add_rpc_url("http://insecure.network", "rpc", 50)
			.add_rpc_url("wss://secure.ws.network", "rpc", 25)
			.add_rpc_url("ws://insecure.ws.network", "rpc", 25)
			.build();

		network.validate_protocol();
		assert!(logs_contain(
			"uses an insecure RPC URL: http://insecure.network"
		));
		assert!(logs_contain(
			"uses an insecure WebSocket URL: ws://insecure.ws.network"
		));
		assert!(!logs_contain("https://secure.network"));
		assert!(!logs_contain("wss://secure.ws.network"));
	}

	#[tokio::test]
	async fn test_load_all_duplicate_network_name() {
		let temp_dir = TempDir::new().unwrap();
		let file_path_1 = temp_dir.path().join("duplicate_network.json");
		let file_path_2 = temp_dir.path().join("duplicate_network_2.json");

		let network_config_1 = r#"{
			"name": " Testnetwork",
			"slug": "test_network",
			"network_type": "EVM",
			"rpc_urls": [
				{
					"type_": "rpc",
					"url": {
						"type": "plain",
						"value": "https://eth.drpc.org"
					},
					"weight": 100
				}
			],
			"chain_id": 1,
			"block_time_ms": 1000,
			"confirmation_blocks": 1,
			"cron_schedule": "0 */5 * * * *",
			"max_past_blocks": 10,
			"store_blocks": true
		}"#;

		let network_config_2 = r#"{
			"name": "TestNetwork",
			"slug": "test_network",
			"network_type": "EVM",
			"rpc_urls": [
				{
					"type_": "rpc",
					"url": {
						"type": "plain",
						"value": "https://eth.drpc.org"
					},
					"weight": 100
				}
			],
			"chain_id": 1,
			"block_time_ms": 1000,
			"confirmation_blocks": 1,
			"cron_schedule": "0 */5 * * * *",
			"max_past_blocks": 10,
			"store_blocks": true
		}"#;

		fs::write(&file_path_1, network_config_1).unwrap();
		fs::write(&file_path_2, network_config_2).unwrap();

		let result: Result<HashMap<String, Network>, ConfigError> =
			Network::load_all(Some(temp_dir.path())).await;

		assert!(result.is_err());
		if let Err(ConfigError::ValidationError(err)) = result {
			assert!(err.message.contains("Duplicate network name found"));
		}
	}

	#[tokio::test]
	async fn test_load_all_duplicate_network_slug() {
		let temp_dir = TempDir::new().unwrap();
		let file_path_1 = temp_dir.path().join("duplicate_network.json");
		let file_path_2 = temp_dir.path().join("duplicate_network_2.json");

		let network_config_1 = r#"{
			"name": "Test Network",
			"slug": "test_network",
			"network_type": "EVM",
			"rpc_urls": [
				{
					"type_": "rpc",
					"url": {
						"type": "plain",
						"value": "https://eth.drpc.org"
					},
					"weight": 100
				}
			],
			"chain_id": 1,
			"block_time_ms": 1000,
			"confirmation_blocks": 1,
			"cron_schedule": "0 */5 * * * *",
			"max_past_blocks": 10,
			"store_blocks": true
		}"#;

		let network_config_2 = r#"{
			"name": "Test Network 2",
			"slug": "test_network",
			"network_type": "EVM",
			"rpc_urls": [
				{
					"type_": "rpc",
					"url": {
						"type": "plain",
						"value": "https://eth.drpc.org"
					},
					"weight": 100
				}
			],
			"chain_id": 1,
			"block_time_ms": 1000,
			"confirmation_blocks": 1,
			"cron_schedule": "0 */5 * * * *",
			"max_past_blocks": 10,
			"store_blocks": true
		}"#;

		fs::write(&file_path_1, network_config_1).unwrap();
		fs::write(&file_path_2, network_config_2).unwrap();

		let result: Result<HashMap<String, Network>, ConfigError> =
			Network::load_all(Some(temp_dir.path())).await;

		assert!(result.is_err());
		if let Err(ConfigError::ValidationError(err)) = result {
			assert!(err.message.contains("Duplicate network slug found"));
		}
	}
}
