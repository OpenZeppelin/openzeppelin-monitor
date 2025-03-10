use openzeppelin_monitor::{
	models::{
		EVMMonitorMatch, EVMTransaction, MatchConditions, Monitor, MonitorMatch, ScriptLanguage,
		Trigger, TriggerType, TriggerTypeConfig,
	},
	services::notification::NotificationService,
};
use std::collections::HashMap;
use web3::types::{H160, U256};

fn create_test_monitor(name: &str) -> Monitor {
	Monitor {
		name: name.to_string(),
		networks: vec!["ethereum_mainnet".to_string()],
		paused: false,
		triggers: vec!["test_trigger".to_string()],
		..Default::default()
	}
}

fn create_test_evm_match(monitor: Monitor) -> MonitorMatch {
	let transaction = EVMTransaction::from(web3::types::Transaction {
		from: Some(H160::default()),
		to: Some(H160::default()),
		value: U256::default(),
		..Default::default()
	});

	MonitorMatch::EVM(Box::new(EVMMonitorMatch {
		monitor,
		transaction,
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

#[tokio::test]
async fn test_notification_service_script_execution() {
	let notification_service = NotificationService::new();

	// Create a script trigger
	let trigger = Trigger {
		name: "test_trigger".to_string(),
		trigger_type: TriggerType::Script,
		config: TriggerTypeConfig::Script {
			language: ScriptLanguage::Python,
			script_path: "test_script.py".to_string(),
			arguments: Some(vec!["arg1".to_string()]),
			timeout_ms: 1000,
		},
	};

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
	let trigger = Trigger {
		name: "test_trigger".to_string(),
		trigger_type: TriggerType::Script,
		config: TriggerTypeConfig::Script {
			language: ScriptLanguage::Python,
			script_path: "nonexistent.py".to_string(),
			arguments: Some(vec!["arg1".to_string()]),
			timeout_ms: 1000,
		},
	};

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
