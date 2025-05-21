use async_trait::async_trait;

use crate::{
	models::{MonitorMatch, ScriptLanguage, TriggerTypeConfig},
	services::notification::ScriptExecutor,
	services::trigger::ScriptExecutorFactory,
};

/// A notification handler that executes scripts when triggered
///
/// This notifier takes a script configuration and executes the specified script
/// when a monitor match occurs. It supports different script languages and
/// allows passing arguments and setting timeouts for script execution.
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
impl ScriptExecutor for ScriptNotifier {
	/// Implement the actual script notification logic
	async fn script_notify(
		&self,
		monitor_match: &MonitorMatch,
		script_content: &(ScriptLanguage, String),
	) -> Result<(), anyhow::Error> {
		match &self.config {
			TriggerTypeConfig::Script {
				script_path: _,
				language,
				arguments,
				timeout_ms,
			} => {
				let executor = ScriptExecutorFactory::create(language, &script_content.1);

				let result = executor
					.execute(
						monitor_match.clone(),
						timeout_ms,
						arguments.as_deref(),
						true,
					)
					.await;

				match result {
					Ok(true) => Ok(()),
					Ok(false) => Err(anyhow::anyhow!("Trigger script execution failed")),
					Err(e) => {
						return Err(anyhow::anyhow!("Trigger script execution error: {}", e));
					}
				}
			}
			_ => Err(anyhow::anyhow!(
				"Invalid configuration type for ScriptNotifier"
			)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		models::{EVMMonitorMatch, EVMTransactionReceipt, MatchConditions, Monitor, MonitorMatch},
		utils::tests::{
			builders::evm::monitor::MonitorBuilder, evm::transaction::TransactionBuilder,
		},
	};
	use std::time::Instant;

	fn create_test_script_config() -> TriggerTypeConfig {
		TriggerTypeConfig::Script {
			language: ScriptLanguage::Python,
			script_path: "test_script.py".to_string(),
			arguments: Some(vec!["arg1".to_string(), "arg2".to_string()]),
			timeout_ms: 1000,
		}
	}

	fn create_test_monitor(
		name: &str,
		networks: Vec<&str>,
		paused: bool,
		triggers: Vec<&str>,
	) -> Monitor {
		MonitorBuilder::new()
			.name(name)
			.networks(networks.into_iter().map(|s| s.to_string()).collect())
			.paused(paused)
			.triggers(triggers.into_iter().map(|s| s.to_string()).collect())
			.build()
	}

	fn create_test_monitor_match() -> MonitorMatch {
		MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor("test_monitor", vec!["ethereum_mainnet"], false, vec![]),
			transaction: TransactionBuilder::new().build(),
			receipt: Some(EVMTransactionReceipt::default()),
			logs: Some(vec![]),
			network_slug: "ethereum_mainnet".to_string(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		}))
	}

	#[test]
	fn test_from_config_with_script_config() {
		let config = create_test_script_config();
		let notifier = ScriptNotifier::from_config(&config);
		assert!(notifier.is_some());
	}

	#[tokio::test]
	async fn test_script_notify_with_valid_script() {
		let config = create_test_script_config();
		let notifier = ScriptNotifier::from_config(&config).unwrap();
		let monitor_match = create_test_monitor_match();
		let script_content = (ScriptLanguage::Python, "print(True)".to_string());

		let result = notifier
			.script_notify(&monitor_match, &script_content)
			.await;
		assert!(result.is_ok());
	}

	#[tokio::test]
	async fn test_script_notify_succeeds_within_timeout() {
		let config = TriggerTypeConfig::Script {
			language: ScriptLanguage::Python,
			script_path: "test_script.py".to_string(),
			arguments: None,
			timeout_ms: 1000, // Timeout longer than sleep time
		};
		let notifier = ScriptNotifier::from_config(&config).unwrap();
		let monitor_match = create_test_monitor_match();

		let script_content = (
			ScriptLanguage::Python,
			"import time\ntime.sleep(0.3)\nprint(True)".to_string(),
		);

		let start_time = Instant::now();
		let result = notifier
			.script_notify(&monitor_match, &script_content)
			.await;
		let elapsed = start_time.elapsed();

		assert!(result.is_ok());
		// Verify that execution took at least 300ms (the sleep time)
		assert!(elapsed.as_millis() >= 300);
		// Verify that execution took less than the timeout
		assert!(elapsed.as_millis() < 1000);
	}

	#[tokio::test]
	async fn test_script_notify_completes_before_timeout() {
		let config = TriggerTypeConfig::Script {
			language: ScriptLanguage::Python,
			script_path: "test_script.py".to_string(),
			arguments: None,
			timeout_ms: 400, // Set timeout lower than the sleep time
		};
		let notifier = ScriptNotifier::from_config(&config).unwrap();
		let monitor_match = create_test_monitor_match();

		let script_content = (
			ScriptLanguage::Python,
			"import time\ntime.sleep(0.5)\nprint(True)".to_string(),
		);
		let start_time = Instant::now();
		let result = notifier
			.script_notify(&monitor_match, &script_content)
			.await;
		let elapsed = start_time.elapsed();

		// The script should fail because it takes 500ms but timeout is 400ms
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Script execution timed out"));
		// Verify that it failed around the timeout time
		assert!(elapsed.as_millis() >= 400 && elapsed.as_millis() < 600);
	}
}
