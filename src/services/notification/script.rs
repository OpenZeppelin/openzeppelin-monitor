use async_trait::async_trait;

use crate::{
	models::{MonitorMatch, ScriptLanguage, TriggerTypeConfig},
	services::notification::{NotificationError, ScriptExecutor},
	services::trigger::ScriptExecutorFactory,
};

/// A notification handler that executes scripts when triggered
///
/// This notifier takes a script configuration and executes the specified script
/// when a monitor match occurs. It supports different script languages and
/// allows passing arguments and setting timeouts for script execution.
#[derive(Debug)]
pub struct ScriptNotifier {
	config: TriggerTypeConfig,
}

impl ScriptNotifier {
	/// Creates a Script notifier from a trigger configuration
	pub fn from_config(config: &TriggerTypeConfig) -> Result<Self, NotificationError> {
		if let TriggerTypeConfig::Script { .. } = config {
			Ok(Self {
				config: config.clone(),
			})
		} else {
			let msg = format!("Invalid script configuration: {:?}", config);
			Err(NotificationError::config_error(msg, None, None))
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
	) -> Result<(), NotificationError> {
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
					Ok(false) => Err(NotificationError::execution_error(
						"Trigger script execution failed",
						None,
						None,
					)),
					Err(e) => {
						return Err(NotificationError::execution_error(
							format!("Trigger script execution error: {}", e),
							Some(e.into()),
							None,
						));
					}
				}
			}
			_ => Err(NotificationError::config_error(
				"Invalid configuration type for ScriptNotifier",
				None,
				None,
			)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		models::{
			EVMMonitorMatch, EVMTransactionReceipt, MatchConditions, Monitor, MonitorMatch,
			NotificationMessage, SecretString, SecretValue, TriggerType,
		},
		services::notification::NotificationService,
		utils::tests::{
			builders::evm::monitor::MonitorBuilder, evm::transaction::TransactionBuilder,
			trigger::TriggerBuilder,
		},
	};
	use std::{collections::HashMap, time::Instant};

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
		assert!(notifier.is_ok());
	}

	#[test]
	fn test_from_config_invalid_type() {
		// Create a config that is not a script type
		let config = TriggerTypeConfig::Slack {
			slack_url: SecretValue::Plain(SecretString::new("random.url".to_string())),
			message: NotificationMessage {
				title: "Test Slack".to_string(),
				body: "This is a test message".to_string(),
			},
			retry_policy: Default::default(),
		};

		let notifier = ScriptNotifier::from_config(&config);
		assert!(notifier.is_err());

		let error = notifier.unwrap_err();
		assert!(matches!(error, NotificationError::ConfigError { .. }));
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

	#[tokio::test]
	async fn test_script_notify_with_invalid_script() {
		let config = create_test_script_config();
		let notifier = ScriptNotifier::from_config(&config).unwrap();
		let monitor_match = create_test_monitor_match();
		let script_content = (ScriptLanguage::Python, "invalid syntax".to_string());

		let result = notifier
			.script_notify(&monitor_match, &script_content)
			.await;
		assert!(result.is_err());

		let error = result.unwrap_err();
		assert!(matches!(error, NotificationError::ExecutionError { .. }));
	}

	#[tokio::test]
	async fn test_script_notification_script_content_not_found() {
		let service = NotificationService::new();
		let script_config = TriggerTypeConfig::Script {
			language: ScriptLanguage::Python,
			script_path: "non_existent_script.py".to_string(), // This path won't be in the map
			arguments: None,
			timeout_ms: 1000,
		};
		let trigger = TriggerBuilder::new()
        .name("test_script_missing")
        .config(script_config) // Use the actual script config
        .trigger_type(TriggerType::Script)
        .build();

		let variables = HashMap::new();
		let monitor_match = create_test_monitor_match();
		let trigger_scripts = HashMap::new(); // Empty map, so script won't be found

		let result = service
			.execute(&trigger, &variables, &monitor_match, &trigger_scripts)
			.await;

		assert!(result.is_err());
		match result.unwrap_err() {
			NotificationError::ConfigError(ctx) => {
				assert!(ctx.message.contains("Script content not found"));
			}
			_ => panic!("Expected ConfigError for missing script content"),
		}
	}
}
