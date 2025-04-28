use openzeppelin_monitor::{
	models::{
		BlockChainType, EVMMonitorMatch, MatchConditions, Monitor, MonitorMatch, ScriptLanguage,
		TransactionType,
	},
	services::notification::NotificationService,
	utils::tests::{evm::monitor::MonitorBuilder, trigger::TriggerBuilder},
};
use std::collections::HashMap;

use crate::integration::mocks::{create_test_evm_transaction_receipt, create_test_transaction};

fn create_test_monitor(name: &str) -> Monitor {
	MonitorBuilder::new()
		.name(name)
		.networks(vec!["ethereum_mainnet".to_string()])
		.paused(false)
		.triggers(vec!["test_trigger".to_string()])
		.build()
}

fn create_test_evm_match(monitor: Monitor) -> MonitorMatch {
	let transaction = match create_test_transaction(BlockChainType::EVM) {
		TransactionType::EVM(transaction) => transaction,
		_ => panic!("Failed to create test transaction"),
	};

	MonitorMatch::EVM(Box::new(EVMMonitorMatch {
		monitor,
		transaction,
		receipt: create_test_evm_transaction_receipt(),
		network_slug: "ethereum_mainnet".to_string(),
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
	let trigger_scripts = create_test_trigger_scripts();

	// Execute the notification
	let result = notification_service
		.execute(&trigger, HashMap::new(), &monitor_match, &trigger_scripts)
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
	let trigger_scripts = create_test_trigger_scripts();

	let result = notification_service
		.execute(&trigger, HashMap::new(), &monitor_match, &trigger_scripts)
		.await;

	assert!(result.is_err());
	assert!(result
		.unwrap_err()
		.to_string()
		.contains("Script content not found"));
}
