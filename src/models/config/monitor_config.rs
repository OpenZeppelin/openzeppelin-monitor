//! Monitor configuration loading and validation.
//!
//! This module implements the ConfigLoader trait for Monitor configurations,
//! allowing monitors to be loaded from JSON files.

use async_trait::async_trait;
use std::{collections::HashMap, fs, path::Path};

use crate::{
	models::{config::error::ConfigError, ConfigLoader, Monitor},
	services::trigger::validate_script_config,
	utils::normalize_string,
};

#[async_trait]
impl ConfigLoader for Monitor {
	/// Resolve all secrets in the monitor configuration
	async fn resolve_secrets(&self) -> Result<Self, ConfigError> {
		dotenvy::dotenv().ok();
		Ok(self.clone())
	}

	/// Load all monitor configurations from a directory
	///
	/// Reads and parses all JSON files in the specified directory (or default
	/// config directory) as monitor configurations.
	async fn load_all<T>(path: Option<&Path>) -> Result<T, ConfigError>
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

			let monitor = Self::load_from_path(&path).await?;

			let existing_monitors: Vec<&Monitor> =
				pairs.iter().map(|(_, monitor)| monitor).collect();
			// Check monitor name uniqueness before pushing
			Self::validate_uniqueness(&existing_monitors, &monitor, &path.display().to_string())?;

			pairs.push((name, monitor));
		}

		Ok(T::from_iter(pairs))
	}

	/// Load a monitor configuration from a specific file
	///
	/// Reads and parses a single JSON file as a monitor configuration.
	async fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
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
		let mut config: Monitor = serde_json::from_reader(file).map_err(|e| {
			ConfigError::parse_error(
				format!("failed to parse monitor config: {}", e),
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

		// Validate trigger conditions (focus on script path, timeout, and language)
		for trigger_condition in &self.trigger_conditions {
			validate_script_config(
				&trigger_condition.script_path,
				&trigger_condition.language,
				&trigger_condition.timeout_ms,
			)?;
		}

		// Log a warning if the monitor uses an insecure protocol
		self.validate_protocol();

		Ok(())
	}

	/// Validate the safety of the protocols used in the monitor
	///
	/// Returns if safe, or logs a warning message if unsafe.
	fn validate_protocol(&self) {
		// Check script file permissions on Unix systems
		#[cfg(unix)]
		for condition in &self.trigger_conditions {
			use std::os::unix::fs::PermissionsExt;
			if let Ok(metadata) = std::fs::metadata(&condition.script_path) {
				let permissions = metadata.permissions();
				let mode = permissions.mode();
				if mode & 0o022 != 0 {
					tracing::warn!(
						"Monitor '{}' trigger conditions script file has overly permissive write permissions: {}. The recommended permissions are `644` (`rw-r--r--`)",
						self.name,
						condition.script_path
					);
				}
			}
		}
	}

	fn validate_uniqueness(
		instances: &[&Self],
		current_instance: &Self,
		file_path: &str,
	) -> Result<(), ConfigError> {
		// Check monitor name uniqueness before pushing
		if instances.iter().any(|existing_monitor| {
			normalize_string(&existing_monitor.name) == normalize_string(&current_instance.name)
		}) {
			Err(ConfigError::validation_error(
				format!("Duplicate monitor name found: '{}'", current_instance.name),
				None,
				Some(HashMap::from([
					(
						"monitor_name".to_string(),
						current_instance.name.to_string(),
					),
					("path".to_string(), file_path.to_string()),
				])),
			))
		} else {
			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		models::core::{ScriptLanguage, TransactionStatus},
		utils::tests::builders::evm::monitor::MonitorBuilder,
	};
	use std::collections::HashMap;
	use tempfile::TempDir;
	use tracing_test::traced_test;

	#[tokio::test]
	async fn test_load_valid_monitor() {
		let temp_dir = TempDir::new().unwrap();
		let file_path = temp_dir.path().join("valid_monitor.json");

		let valid_config = r#"{
            "name": "TestMonitor",
			"networks": ["ethereum_mainnet"],
			"paused": false,
			"addresses": [
				{
					"address": "0x0000000000000000000000000000000000000000",
					"contract_spec": null
				}
			],
            "match_conditions": {
                "functions": [
                    {"signature": "transfer(address,uint256)"}
                ],
                "events": [
                    {"signature": "Transfer(address,address,uint256)"}
                ],
                "transactions": [
					{
						"status": "Success",
						"expression": null
					}
                ]
            },
			"trigger_conditions": [],
			"triggers": ["trigger1", "trigger2"]
        }"#;

		fs::write(&file_path, valid_config).unwrap();

		let result = Monitor::load_from_path(&file_path).await;
		assert!(result.is_ok());

		let monitor = result.unwrap();
		assert_eq!(monitor.name, "TestMonitor");
	}

	#[tokio::test]
	async fn test_load_invalid_monitor() {
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

		let result = Monitor::load_from_path(&file_path).await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_load_all_monitors() {
		let temp_dir = TempDir::new().unwrap();

		let valid_config_1 = r#"{
            "name": "TestMonitor1",
			"networks": ["ethereum_mainnet"],
			"paused": false,
			"addresses": [
				{
					"address": "0x0000000000000000000000000000000000000000",
					"contract_spec": null
				}
			],
            "match_conditions": {
                "functions": [
                    {"signature": "transfer(address,uint256)"}
                ],
                "events": [
                    {"signature": "Transfer(address,address,uint256)"}
                ],
                "transactions": [
					{
						"status": "Success",
						"expression": null
					}
                ]
            },
			"trigger_conditions": [],
			"triggers": ["trigger1", "trigger2"]
        }"#;

		let valid_config_2 = r#"{
            "name": "TestMonitor2",
			"networks": ["ethereum_mainnet"],
			"paused": false,
			"addresses": [
				{
					"address": "0x0000000000000000000000000000000000000000",
					"contract_spec": null
				}
			],
            "match_conditions": {
                "functions": [
                    {"signature": "transfer(address,uint256)"}
                ],
                "events": [
                    {"signature": "Transfer(address,address,uint256)"}
                ],
                "transactions": [
					{
						"status": "Success",
						"expression": null
					}
                ]
            },
			"trigger_conditions": [],
			"triggers": ["trigger1", "trigger2"]
        }"#;

		fs::write(temp_dir.path().join("monitor1.json"), valid_config_1).unwrap();
		fs::write(temp_dir.path().join("monitor2.json"), valid_config_2).unwrap();

		let result: Result<HashMap<String, Monitor>, _> =
			Monitor::load_all(Some(temp_dir.path())).await;
		assert!(result.is_ok());

		let monitors = result.unwrap();
		assert_eq!(monitors.len(), 2);
		assert!(monitors.contains_key("monitor1"));
		assert!(monitors.contains_key("monitor2"));
	}

	#[test]
	fn test_validate_monitor() {
		let valid_monitor = MonitorBuilder::new()
			.name("TestMonitor")
			.networks(vec!["ethereum_mainnet".to_string()])
			.address("0x0000000000000000000000000000000000000000")
			.function("transfer(address,uint256)", None)
			.event("Transfer(address,address,uint256)", None)
			.transaction(TransactionStatus::Success, None)
			.triggers(vec!["trigger1".to_string()])
			.build();

		assert!(valid_monitor.validate().is_ok());

		let invalid_monitor = MonitorBuilder::new().name("").build();

		assert!(invalid_monitor.validate().is_err());
	}

	#[test]
	fn test_validate_monitor_with_trigger_conditions() {
		// Create a temporary directory and script file
		let temp_dir = TempDir::new().unwrap();
		let script_path = temp_dir.path().join("test_script.py");
		fs::write(&script_path, "print('test')").unwrap();

		// Set current directory to temp directory to make relative paths work
		let original_dir = std::env::current_dir().unwrap();
		std::env::set_current_dir(temp_dir.path()).unwrap();

		// Test with valid script path
		let valid_monitor = MonitorBuilder::new()
			.name("TestMonitor")
			.networks(vec!["ethereum_mainnet".to_string()])
			.address("0x0000000000000000000000000000000000000000")
			.function("transfer(address,uint256)", None)
			.event("Transfer(address,address,uint256)", None)
			.transaction(TransactionStatus::Success, None)
			.trigger_condition("test_script.py", 1000, ScriptLanguage::Python, None)
			.build();

		assert!(valid_monitor.validate().is_ok());

		// Restore original directory
		std::env::set_current_dir(original_dir).unwrap();
	}

	#[test]
	fn test_validate_monitor_with_invalid_script_path() {
		let invalid_monitor = MonitorBuilder::new()
			.name("TestMonitor")
			.networks(vec!["ethereum_mainnet".to_string()])
			.trigger_condition("non_existent_script.py", 1000, ScriptLanguage::Python, None)
			.build();

		assert!(invalid_monitor.validate().is_err());
	}

	#[test]
	fn test_validate_monitor_with_timeout_zero() {
		// Create a temporary directory and script file
		let temp_dir = TempDir::new().unwrap();
		let script_path = temp_dir.path().join("test_script.py");
		fs::write(&script_path, "print('test')").unwrap();

		// Set current directory to temp directory to make relative paths work
		let original_dir = std::env::current_dir().unwrap();
		std::env::set_current_dir(temp_dir.path()).unwrap();

		let invalid_monitor = MonitorBuilder::new()
			.name("TestMonitor")
			.networks(vec!["ethereum_mainnet".to_string()])
			.trigger_condition("test_script.py", 0, ScriptLanguage::Python, None)
			.build();

		assert!(invalid_monitor.validate().is_err());

		// Restore original directory
		std::env::set_current_dir(original_dir).unwrap();
		// Clean up temp directory
		temp_dir.close().unwrap();
	}

	#[test]
	fn test_validate_monitor_with_different_script_languages() {
		// Create a temporary directory and script files
		let temp_dir = TempDir::new().unwrap();
		let temp_path = temp_dir.path().to_owned();

		let python_script = temp_path.join("test_script.py");
		let js_script = temp_path.join("test_script.js");
		let bash_script = temp_path.join("test_script.sh");

		fs::write(&python_script, "print('test')").unwrap();
		fs::write(&js_script, "console.log('test')").unwrap();
		fs::write(&bash_script, "echo 'test'").unwrap();

		// Test each script language
		let test_cases = vec![
			(ScriptLanguage::Python, python_script),
			(ScriptLanguage::JavaScript, js_script),
			(ScriptLanguage::Bash, bash_script),
		];

		for (language, script_path) in test_cases {
			let language_clone = language.clone();
			let script_path_clone = script_path.clone();

			let monitor = MonitorBuilder::new()
				.name("TestMonitor")
				.networks(vec!["ethereum_mainnet".to_string()])
				.trigger_condition(
					&script_path_clone.to_string_lossy(),
					1000,
					language_clone,
					None,
				)
				.build();

			assert!(monitor.validate().is_ok());

			// Test with mismatched extension
			let wrong_path = temp_path.join("test_script.wrong");
			fs::write(&wrong_path, "test content").unwrap();

			let monitor_wrong_ext = MonitorBuilder::new()
				.name("TestMonitor")
				.networks(vec!["ethereum_mainnet".to_string()])
				.trigger_condition(
					&wrong_path.to_string_lossy(),
					monitor.trigger_conditions[0].timeout_ms,
					language,
					monitor.trigger_conditions[0].arguments.clone(),
				)
				.build();

			assert!(monitor_wrong_ext.validate().is_err());
		}

		// TempDir will automatically clean up when dropped
	}
	#[tokio::test]
	async fn test_invalid_load_from_path() {
		let path = Path::new("config/monitors/invalid.json");
		assert!(matches!(
			Monitor::load_from_path(path).await,
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
			Monitor::load_from_path(path).await,
			Err(ConfigError::ParseError(_))
		));
	}

	#[tokio::test]
	async fn test_load_all_directory_not_found() {
		let non_existent_path = Path::new("non_existent_directory");

		// Test that loading from this path results in a file error
		let result: Result<HashMap<String, Monitor>, ConfigError> =
			Monitor::load_all(Some(non_existent_path)).await;
		assert!(matches!(result, Err(ConfigError::FileError(_))));

		if let Err(ConfigError::FileError(err)) = result {
			assert!(err.message.contains("monitors directory not found"));
		}
	}

	#[cfg(unix)]
	#[test]
	#[traced_test]
	fn test_validate_protocol_script_permissions() {
		use std::fs::File;
		use std::os::unix::fs::PermissionsExt;
		use tempfile::TempDir;

		use crate::models::{MatchConditions, TriggerConditions};

		let temp_dir = TempDir::new().unwrap();
		let script_path = temp_dir.path().join("test_script.sh");
		File::create(&script_path).unwrap();

		// Set overly permissive permissions (777)
		let metadata = std::fs::metadata(&script_path).unwrap();
		let mut permissions = metadata.permissions();
		permissions.set_mode(0o777);
		std::fs::set_permissions(&script_path, permissions).unwrap();

		let monitor = Monitor {
			name: "TestMonitor".to_string(),
			networks: vec!["ethereum_mainnet".to_string()],
			paused: false,
			addresses: vec![],
			match_conditions: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![],
			},
			trigger_conditions: vec![TriggerConditions {
				script_path: script_path.to_str().unwrap().to_string(),
				timeout_ms: 1000,
				arguments: None,
				language: ScriptLanguage::Bash,
			}],
			triggers: vec![],
		};

		monitor.validate_protocol();
		assert!(logs_contain(
			"script file has overly permissive write permissions"
		));
	}

	#[tokio::test]
	async fn test_load_all_monitors_duplicate_name() {
		let temp_dir = TempDir::new().unwrap();

		let valid_config_1 = r#"{
            "name": "TestMonitor",
			"networks": ["ethereum_mainnet"],
			"paused": false,
			"addresses": [
				{
					"address": "0x0000000000000000000000000000000000000000",
					"contract_spec": null
				}
			],
            "match_conditions": {
                "functions": [
                    {"signature": "transfer(address,uint256)"}
                ],
                "events": [
                    {"signature": "Transfer(address,address,uint256)"}
                ],
                "transactions": [
					{
						"status": "Success",
						"expression": null
					}
                ]
            },
			"trigger_conditions": [],
			"triggers": ["trigger1", "trigger2"]
        }"#;

		let valid_config_2 = r#"{
            "name": "Testmonitor",
			"networks": ["ethereum_mainnet"],
			"paused": false,
			"addresses": [
				{
					"address": "0x0000000000000000000000000000000000000000",
					"contract_spec": null
				}
			],
            "match_conditions": {
                "functions": [
                    {"signature": "transfer(address,uint256)"}
                ],
                "events": [
                    {"signature": "Transfer(address,address,uint256)"}
                ],
                "transactions": [
					{
						"status": "Success",
						"expression": null
					}
                ]
            },
			"trigger_conditions": [],
			"triggers": ["trigger1", "trigger2"]
        }"#;

		fs::write(temp_dir.path().join("monitor1.json"), valid_config_1).unwrap();
		fs::write(temp_dir.path().join("monitor2.json"), valid_config_2).unwrap();

		let result: Result<HashMap<String, Monitor>, _> =
			Monitor::load_all(Some(temp_dir.path())).await;

		assert!(result.is_err());
		if let Err(ConfigError::ValidationError(err)) = result {
			assert!(err.message.contains("Duplicate monitor name found"));
		}
	}
}
