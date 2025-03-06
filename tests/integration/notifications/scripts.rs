// use openzeppelin_monitor::{
// 	models::{
// 		EVMMonitorMatch, EVMTransaction, MatchConditions, Monitor, MonitorMatch, ScriptLanguage,
// 		StellarTransaction, StellarTransactionInfo, Trigger, TriggerType, TriggerTypeConfig,
// 	},
// 	services::notification::NotificationError,
// 	utils::ScriptExecutorFactory,
// };
// use std::{collections::HashMap, time::Duration};
// use web3::types::{H160, U256};

// fn create_test_monitor(
// 	name: &str,
// 	networks: Vec<&str>,
// 	paused: bool,
// 	triggers: Vec<&str>,
// ) -> Monitor {
// 	Monitor {
// 		name: name.to_string(),
// 		networks: networks.into_iter().map(|s| s.to_string()).collect(),
// 		paused,
// 		triggers: triggers.into_iter().map(|s| s.to_string()).collect(),
// 		..Default::default()
// 	}
// }

// fn create_test_evm_transaction() -> EVMTransaction {
// 	EVMTransaction::from({
// 		web3::types::Transaction {
// 			from: Some(H160::default()),
// 			to: Some(H160::default()),
// 			value: U256::default(),
// 			..Default::default()
// 		}
// 	})
// }

// fn create_test_stellar_transaction() -> StellarTransaction {
// 	StellarTransaction::from({
// 		StellarTransactionInfo {
// 			..Default::default()
// 		}
// 	})
// }

// #[tokio::test]
// async fn test_script_trigger_execution() {
// 	let monitor = create_test_monitor("test_monitor", vec![], false, vec![]);
// 	let monitor_match = MonitorMatch::EVM(Box::new(EVMMonitorMatch {
// 		monitor: monitor.clone(),
// 		transaction: create_test_evm_transaction(),
// 		receipt: web3::types::TransactionReceipt::default(),
// 		matched_on: MatchConditions::default(),
// 		matched_on_args: None,
// 	}));

// 	// Create a mock script content
// 	let mut trigger_scripts = HashMap::new();
// 	let script_path = "test_script.py";
// 	let script_content = ("test_script.py".to_string(), "print(True)".to_string());
// 	trigger_scripts.insert(format!("{}|{}", monitor.name, script_path), script_content);

// 	// Create trigger configuration
// 	let trigger = Trigger {
// 		name: "test_trigger".to_string(),
// 		trigger_type: TriggerType::Script,
// 		config: TriggerTypeConfig::Script {
// 			language: ScriptLanguage::Python,
// 			script_path: script_path.to_string(),
// 			arguments: vec!["arg1".to_string()],
// 			timeout_ms: 5000,
// 		},
// 	};

// 	// Test successful execution
// 	let result = execute_script_trigger(&trigger, &monitor_match, &trigger_scripts).await;
// 	println!("result: {:?}", result);
// 	assert!(result.is_ok());
// }

// // Helper function to execute script trigger
// async fn execute_script_trigger(
// 	trigger: &Trigger,
// 	monitor_match: &MonitorMatch,
// 	trigger_scripts: &HashMap<String, (String, String)>,
// ) -> Result<(), NotificationError> {
// 	if let TriggerTypeConfig::Script {
// 		language,
// 		script_path,
// 		arguments,
// 		timeout_ms: _,
// 	} = &trigger.config
// 	{
// 		let monitor_name = match monitor_match {
// 			MonitorMatch::EVM(evm_match) => &evm_match.monitor.name,
// 			MonitorMatch::Stellar(stellar_match) => &stellar_match.monitor.name,
// 		};

// 		let script = trigger_scripts
// 			.get(&format!("{}|{}", monitor_name, script_path))
// 			.ok_or_else(|| {
// 				NotificationError::execution_error("Script content not found".to_string())
// 			})?;

// 		let executor = ScriptExecutorFactory::create(language, &script.1);
// 		let result = executor.execute(monitor_match.clone(), &arguments).await;

// 		match result {
// 			Ok(true) => Ok(()),
// 			Ok(false) => Err(NotificationError::execution_error("Script returned false")),
// 			Err(e) => Err(NotificationError::execution_error(e.to_string())),
// 		}
// 	} else {
// 		Err(NotificationError::config_error(
// 			"Invalid trigger configuration",
// 		))
// 	}
// }
