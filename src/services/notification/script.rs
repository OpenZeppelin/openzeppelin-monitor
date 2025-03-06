use async_trait::async_trait;
use std::{collections::HashMap, time::Duration};

use crate::{
	models::{MonitorMatch, ScriptLanguage, TriggerTypeConfig},
	services::notification::{NotificationError, Notifier},
	utils::ScriptExecutorFactory,
};

pub struct ScriptNotifier {
	config: TriggerTypeConfig,
}

impl ScriptNotifier {
	/// Creates a Script notifier from a trigger configuration
	pub fn from_config(config: &TriggerTypeConfig) -> Option<Self> {
		match config {
			TriggerTypeConfig::Script { .. } => Some(Self {
				config: config.clone(),
			}),
			_ => None,
		}
	}
}

#[async_trait]
impl Notifier for ScriptNotifier {
	async fn notify(&self, _message: &str) -> Result<(), NotificationError> {
		Err(NotificationError::config_error(
			"ScriptNotifier does not support regular notifications".to_string(),
		))
	}

	/// Implement the actual script notification logic
	async fn script_notify(
		&self,
		monitor_match: &MonitorMatch,
		trigger_scripts: &HashMap<String, (ScriptLanguage, String)>,
	) -> Result<(), NotificationError> {
		match &self.config {
			TriggerTypeConfig::Script {
				script_path,
				language,
				arguments,
				timeout_ms,
			} => {
				println!("ScriptNotifier config: {:?}", self.config);

				let monitor_name = match monitor_match {
					MonitorMatch::EVM(evm_match) => &evm_match.monitor.name,
					MonitorMatch::Stellar(stellar_match) => &stellar_match.monitor.name,
				};
				let script = trigger_scripts
					.get(&format!("{}|{}", monitor_name, script_path))
					.ok_or_else(|| {
						NotificationError::execution_error("Script content not found".to_string())
					});
				let script_content = match &script {
					Ok(content) => content,
					Err(e) => return Err(NotificationError::execution_error(e.to_string())),
				};

				let executor = ScriptExecutorFactory::create(language, &script_content.1);

				let result = tokio::time::timeout(
					Duration::from_millis(u64::from(*timeout_ms)),
					executor.execute(monitor_match.clone(), &arguments),
				)
				.await;

				match result {
					Ok(Ok(true)) => Ok(()),
					Err(e) => {
						return Err(NotificationError::execution_error(e.to_string()));
					}
					_ => {
						return Err(NotificationError::execution_error(
							"Trigger script execution error",
						))
					}
				}
			}
			_ => Err(NotificationError::config_error(
				"Invalid configuration type for ScriptNotifier".to_string(),
			)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{EVMMonitorMatch, EVMTransaction, MatchConditions, Monitor, MonitorMatch};
	use web3::types::{H160, U256};

	fn create_test_script_config() -> TriggerTypeConfig {
		TriggerTypeConfig::Script {
			language: ScriptLanguage::Python,
			script_path: "test_script.py".to_string(),
			arguments: vec!["arg1".to_string(), "arg2".to_string()],
			timeout_ms: 1000,
		}
	}

	fn create_test_monitor(
		name: &str,
		networks: Vec<&str>,
		paused: bool,
		triggers: Vec<&str>,
	) -> Monitor {
		Monitor {
			name: name.to_string(),
			networks: networks.into_iter().map(|s| s.to_string()).collect(),
			paused,
			triggers: triggers.into_iter().map(|s| s.to_string()).collect(),
			..Default::default()
		}
	}

	fn create_test_evm_transaction() -> EVMTransaction {
		EVMTransaction::from({
			web3::types::Transaction {
				from: Some(H160::default()),
				to: Some(H160::default()),
				value: U256::default(),
				..Default::default()
			}
		})
	}

	fn create_test_monitor_match() -> MonitorMatch {
		MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor("test_monitor", vec!["ethereum_mainnet"], false, vec![]),
			transaction: create_test_evm_transaction(),
			receipt: web3::types::TransactionReceipt::default(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		}))
	}

	fn create_test_trigger_scripts() -> HashMap<String, (ScriptLanguage, String)> {
		let mut scripts = HashMap::new();
		scripts.insert(
			"test_monitor|test_script.py".to_string(),
			(ScriptLanguage::Python, "print(True)".to_string()),
		);
		scripts
	}

	#[test]
	fn test_from_config_with_script_config() {
		let config = create_test_script_config();
		let notifier = ScriptNotifier::from_config(&config);
		assert!(notifier.is_some());
	}

	#[tokio::test]
	async fn test_notify_returns_error() {
		let config = create_test_script_config();
		let notifier = ScriptNotifier::from_config(&config).unwrap();
		let result = notifier.notify("test message").await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("does not support regular notifications"));
	}

	#[tokio::test]
	async fn test_script_notify_with_valid_script() {
		let config = create_test_script_config();
		let notifier = ScriptNotifier::from_config(&config).unwrap();
		let monitor_match = create_test_monitor_match();
		let trigger_scripts = create_test_trigger_scripts();

		let result = notifier
			.script_notify(&monitor_match, &trigger_scripts)
			.await;
		println!("Result: {:?}", result);
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_script_notify_with_missing_script() {
		let config = create_test_script_config();
		let notifier = ScriptNotifier::from_config(&config).unwrap();
		let monitor_match = create_test_monitor_match();
		let trigger_scripts = HashMap::new();

		let result = notifier
			.script_notify(&monitor_match, &trigger_scripts)
			.await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Script content not found"));
	}
}
