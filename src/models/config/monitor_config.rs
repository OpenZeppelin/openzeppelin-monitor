//! Monitor configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Monitor configurations,
//! allowing monitors to be loaded from JSON files.

use std::{collections::HashMap, fs, path::Path};

use crate::models::{config::error::ConfigError, ConfigLoader, Monitor};

impl ConfigLoader for Monitor {
	/// Load all monitor configurations from a directory
	///
	/// Reads and parses all JSON files in the specified directory (or default
	/// config directory) as monitor configurations.
	fn load_all<T>(path: Option<&Path>) -> Result<T, ConfigError>
	where
		T: FromIterator<(String, Self)>,
	{
		let monitor_dir = path.unwrap_or(Path::new("config/monitors"));
		let mut pairs = Vec::new();

		if !monitor_dir.exists() {
			return Err(ConfigError::file_error(
				"monitors directory not found",
				None,
				Some(HashMap::from([(
					"path".to_string(),
					monitor_dir.display().to_string(),
				)])),
			));
		}

		for entry in fs::read_dir(monitor_dir).map_err(|e| {
			ConfigError::file_error(
				format!("failed to read monitors directory: {}", e),
				Some(Box::new(e)),
				Some(HashMap::from([(
					"path".to_string(),
					monitor_dir.display().to_string(),
				)])),
			)
		})? {
			let entry = entry.map_err(|e| {
				ConfigError::file_error(
					format!("failed to read directory entry: {}", e),
					Some(Box::new(e)),
					Some(HashMap::from([(
						"path".to_string(),
						monitor_dir.display().to_string(),
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

			let monitor = Self::load_from_path(&path)?;
			pairs.push((name, monitor));
		}

		Ok(T::from_iter(pairs))
	}

	/// Load a monitor configuration from a specific file
	///
	/// Reads and parses a single JSON file as a monitor configuration.
	fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
		let file = std::fs::File::open(path).map_err(|e| {
			ConfigError::file_error(
				format!("failed to open monitor config file: {}", e),
				Some(Box::new(e)),
				Some(HashMap::from([(
					"path".to_string(),
					path.display().to_string(),
				)])),
			)
		})?;
		let config: Monitor = serde_json::from_reader(file).map_err(|e| {
			ConfigError::parse_error(
				format!("failed to parse monitor config: {}", e),
				Some(Box::new(e)),
				Some(HashMap::from([(
					"path".to_string(),
					path.display().to_string(),
				)])),
			)
		})?;

		// Validate the config after loading
		config.validate().map_err(|e| {
			ConfigError::validation_error(
				format!("monitor validation failed: {}", e),
				Some(Box::new(e)),
				Some(HashMap::from([
					("path".to_string(), path.display().to_string()),
					("monitor_name".to_string(), config.name.clone()),
				])),
			)
		})?;

		Ok(config)
	}

	/// Validate the monitor configuration
	fn validate(&self) -> Result<(), ConfigError> {
		// Validate monitor name
		if self.name.is_empty() {
			return Err(ConfigError::validation_error(
				"Monitor name is required",
				None,
				None,
			));
		}

		// Validate networks
		if self.networks.is_empty() {
			return Err(ConfigError::validation_error(
				"At least one network must be specified",
				None,
				None,
			));
		}

		// Validate function signatures
		for func in &self.match_conditions.functions {
			if !func.signature.contains('(') || !func.signature.contains(')') {
				return Err(ConfigError::validation_error(
					format!("Invalid function signature format: {}", func.signature),
					None,
					None,
				));
			}
		}

		// Validate event signatures
		for event in &self.match_conditions.events {
			if !event.signature.contains('(') || !event.signature.contains(')') {
				return Err(ConfigError::validation_error(
					format!("Invalid event signature format: {}", event.signature),
					None,
					None,
				));
			}
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::core::{
		AddressWithABI, EventCondition, FunctionCondition, MatchConditions, TransactionCondition,
		TransactionStatus,
	};
	use std::collections::HashMap;
	use tempfile::TempDir;

	#[test]
	fn test_load_valid_monitor() {
		let temp_dir = TempDir::new().unwrap();
		let file_path = temp_dir.path().join("valid_monitor.json");

		let valid_config = r#"{
            "name": "TestMonitor",
			"networks": ["ethereum_mainnet"],
			"paused": false,
			"addresses": [
				{
					"address": "0x0000000000000000000000000000000000000000",
					"abi": null
				}
			],
            "description": "Test monitor configuration",
            "match_conditions": {
                "functions": [
                    {"signature": "transfer(address,uint256)"}
                ],
                "events": [
                    {"signature": "Transfer(address,address,uint256)"}
                ],
                "transactions": [
					{
						"signature": "Transfer(address,address,uint256)",
						"status": "Success"
					}
                ]
            },
			"triggers": ["trigger1", "trigger2"]
        }"#;

		fs::write(&file_path, valid_config).unwrap();

		let result = Monitor::load_from_path(&file_path);
		assert!(result.is_ok());

		let monitor = result.unwrap();
		assert_eq!(monitor.name, "TestMonitor");
	}

	#[test]
	fn test_load_invalid_monitor() {
		let temp_dir = TempDir::new().unwrap();
		let file_path = temp_dir.path().join("invalid_monitor.json");

		let invalid_config = r#"{
            "name": "",
            "description": "Invalid monitor configuration",
            "match_conditions": {
                "functions": [
                    {"signature": "invalid_signature"}
                ],
                "events": []
            }
        }"#;

		fs::write(&file_path, invalid_config).unwrap();

		let result = Monitor::load_from_path(&file_path);
		assert!(result.is_err());
	}

	#[test]
	fn test_load_all_monitors() {
		let temp_dir = TempDir::new().unwrap();

		let valid_config_1 = r#"{
            "name": "TestMonitor1",
			"networks": ["ethereum_mainnet"],
			"paused": false,
			"addresses": [
				{
					"address": "0x0000000000000000000000000000000000000000",
					"abi": null
				}
			],
            "description": "Test monitor configuration",
            "match_conditions": {
                "functions": [
                    {"signature": "transfer(address,uint256)"}
                ],
                "events": [
                    {"signature": "Transfer(address,address,uint256)"}
                ],
                "transactions": [
					{
						"signature": "Transfer(address,address,uint256)",
						"status": "Success"
					}
                ]
            },
			"triggers": ["trigger1", "trigger2"]
        }"#;

		let valid_config_2 = r#"{
            "name": "TestMonitor2",
			"networks": ["ethereum_mainnet"],
			"paused": false,
			"addresses": [
				{
					"address": "0x0000000000000000000000000000000000000000",
					"abi": null
				}
			],
            "description": "Test monitor configuration",
            "match_conditions": {
                "functions": [
                    {"signature": "transfer(address,uint256)"}
                ],
                "events": [
                    {"signature": "Transfer(address,address,uint256)"}
                ],
                "transactions": [
					{
						"signature": "Transfer(address,address,uint256)",
						"status": "Success"
					}
                ]
            },
			"triggers": ["trigger1", "trigger2"]
        }"#;

		fs::write(temp_dir.path().join("monitor1.json"), valid_config_1).unwrap();
		fs::write(temp_dir.path().join("monitor2.json"), valid_config_2).unwrap();

		let result: Result<HashMap<String, Monitor>, _> = Monitor::load_all(Some(temp_dir.path()));
		assert!(result.is_ok());

		let monitors = result.unwrap();
		assert_eq!(monitors.len(), 2);
		assert!(monitors.contains_key("monitor1"));
		assert!(monitors.contains_key("monitor2"));
	}

	#[test]
	fn test_validate_monitor() {
		let valid_monitor = Monitor {
			name: "TestMonitor".to_string(),
			networks: vec!["ethereum_mainnet".to_string()],
			paused: false,
			addresses: vec![AddressWithABI {
				address: "0x0000000000000000000000000000000000000000".to_string(),
				abi: None,
			}],
			match_conditions: MatchConditions {
				functions: vec![FunctionCondition {
					signature: "transfer(address,uint256)".to_string(),
					expression: None,
				}],
				events: vec![EventCondition {
					signature: "Transfer(address,address,uint256)".to_string(),
					expression: None,
				}],
				transactions: vec![TransactionCondition {
					status: TransactionStatus::Success,
					expression: None,
				}],
			},
			triggers: vec!["trigger1".to_string()],
		};

		assert!(valid_monitor.validate().is_ok());

		let invalid_monitor = Monitor {
			name: "".to_string(),
			networks: vec![],
			paused: false,
			addresses: vec![],
			match_conditions: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![],
			},
			triggers: vec![],
		};

		assert!(invalid_monitor.validate().is_err());
	}
}
