use openzeppelin_monitor::{
	models::{EVMMonitorMatch, MatchConditions, Monitor, MonitorMatch, ScriptLanguage},
	services::notification::{NotificationError, NotificationService},
	utils::tests::{
		evm::{monitor::MonitorBuilder, transaction::TransactionBuilder},
		trigger::TriggerBuilder,
	},
};
use std::collections::HashMap;

use crate::integration::mocks::{create_test_evm_logs, create_test_evm_transaction_receipt};

fn create_test_monitor(name: &str) -> Monitor {
	MonitorBuilder::new()
		.name(name)
		.networks(vec!["ethereum_mainnet".to_string()])
		.paused(false)
		.triggers(vec!["test_trigger".to_string()])
		.build()
}

fn create_test_evm_match(monitor: Monitor) -> MonitorMatch {
	let transaction = TransactionBuilder::new().build();

	MonitorMatch::EVM(Box::new(EVMMonitorMatch {
		monitor,
		transaction,
		receipt: Some(create_test_evm_transaction_receipt()),
		logs: Some(create_test_evm_logs()),
		network_slug: "ethereum_mainnet".to_string(),
		matched_on: MatchConditions::default(),
		matched_on_args: None,
	}))
}

fn create_test_trigger_scripts(
	monitor_name: Option<&str>,
) -> HashMap<String, (ScriptLanguage, String)> {
	let mut scripts = HashMap::new();
	scripts.insert(
		format!("{}|test_script.py", monitor_name.unwrap_or("test_monitor")),
		(ScriptLanguage::Python, "print(True)".to_string()),
	);
	scripts
}

#[tokio::test]
async fn test_notification_service_script_execution() {
	let notification_service = NotificationService::new();

	// Create a script trigger
	let trigger = TriggerBuilder::new()
		.name("test_trigger")
		.script("test_script.py", ScriptLanguage::Python)
		.script_arguments(vec!["arg1".to_string()])
		.script_timeout_ms(1000)
		.build();

	// Create monitor match and trigger scripts
	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));
	let trigger_scripts = create_test_trigger_scripts(None);

	// Execute the notification
	let result = notification_service
		.execute(&trigger, &HashMap::new(), &monitor_match, &trigger_scripts)
		.await;
	assert!(result.is_ok());
}

#[tokio::test]
async fn test_notification_service_script_execution_failure() {
	let notification_service = NotificationService::new();

	// Create a script trigger with a non-existent script
	let trigger = TriggerBuilder::new()
		.name("test_trigger")
		.script("nonexistent.py", ScriptLanguage::Python)
		.script_arguments(vec!["arg1".to_string()])
		.script_timeout_ms(1000)
		.build();

	let monitor_match = create_test_evm_match(create_test_monitor("test_monitor"));
	let trigger_scripts = create_test_trigger_scripts(None);

	let result = notification_service
		.execute(&trigger, &HashMap::new(), &monitor_match, &trigger_scripts)
		.await;

	assert!(result.is_err());

	let error = result.unwrap_err();

	println!("Error: {:?}", error);

	if let NotificationError::ConfigError(ctx) = error {
		assert!(ctx.to_string().contains("Script content not found"));
	} else {
		panic!("Expected NotificationError::ConfigError variant");
	}
}

#[tokio::test]
async fn test_notification_service_script_execution_normalized_monitor_name() {
	let notification_service = NotificationService::new();

	// Create a script trigger
	let trigger = TriggerBuilder::new()
		.name("test_trigger")
		.script("test_script.py", ScriptLanguage::Python)
		.script_arguments(vec!["arg1".to_string()])
		.script_timeout_ms(1000)
		.build();

	// Create monitor match and trigger scripts
	let monitor_match = create_test_evm_match(create_test_monitor("Test Monitor"));
	let trigger_scripts = create_test_trigger_scripts(Some("test monitor"));

	// Execute the notification
	let result = notification_service
		.execute(&trigger, &HashMap::new(), &monitor_match, &trigger_scripts)
		.await;
	assert!(result.is_ok());
}
