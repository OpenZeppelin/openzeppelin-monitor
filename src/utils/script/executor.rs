use crate::{
	models::{MonitorMatch, ProcessedBlock},
	utils::script::error::ScriptError,
};
use async_trait::async_trait;

/// A trait that defines the interface for executing custom scripts in different languages.
/// Implementors must be both Send and Sync to ensure thread safety.
#[async_trait]
pub trait ScriptExecutor: Send + Sync {
	/// Executes the script with the given MonitorMatch input.
	///
	/// # Arguments
	/// * `input` - A MonitorMatch instance containing the data to be processed by the script
	///
	/// # Returns
	/// * `Result<bool, ScriptError>` - Returns true/false based on script execution or an error
	async fn execute(&self, input: ProcessedBlock) -> Result<bool, ScriptError>;
}

/// Executes Python scripts using the python3 interpreter.
pub struct PythonScriptExecutor {
	/// Path to the Python script file to be executed
	pub script_path: String,
}

#[async_trait]
impl ScriptExecutor for PythonScriptExecutor {
	async fn execute(&self, input: ProcessedBlock) -> Result<bool, ScriptError> {
		let input_json =
			serde_json::to_string(&input).map_err(|e| ScriptError::parse_error(e.to_string()))?;

		let output = tokio::process::Command::new("python3")
			.arg(&self.script_path)
			.arg(input_json)
			.output()
			.await
			.map_err(|e| ScriptError::execution_error(e.to_string()))?;

		process_script_output(output)
	}
}

/// Executes JavaScript scripts using the Node.js runtime.
pub struct JavaScriptScriptExecutor {
	/// Path to the JavaScript script file to be executed
	pub script_path: String,
}

#[async_trait]
impl ScriptExecutor for JavaScriptScriptExecutor {
	async fn execute(&self, input: ProcessedBlock) -> Result<bool, ScriptError> {
		let input_json =
			serde_json::to_string(&input).map_err(|e| ScriptError::parse_error(e.to_string()))?;

		let output = tokio::process::Command::new("node")
			.arg(&self.script_path)
			.arg(input_json)
			.output()
			.await
			.map_err(|e| ScriptError::execution_error(e.to_string()))?;

		process_script_output(output)
	}
}

/// Executes Bash shell scripts.
pub struct BashScriptExecutor {
	/// Path to the Bash script file to be executed
	pub script_path: String,
}

#[async_trait]
impl ScriptExecutor for BashScriptExecutor {
	async fn execute(&self, input: ProcessedBlock) -> Result<bool, ScriptError> {
		let input_json =
			serde_json::to_string(&input).map_err(|e| ScriptError::parse_error(e.to_string()))?;

		let output = tokio::process::Command::new("bash")
			.arg(&self.script_path)
			.arg(input_json)
			.output()
			.await
			.map_err(|e| ScriptError::execution_error(e.to_string()))?;

		process_script_output(output)
	}
}

/// Processes the output from script execution.
///
/// # Arguments
/// * `output` - The process output containing stdout, stderr, and status
///
/// # Returns
/// * `Result<bool, ScriptError>` - Returns parsed boolean result or error
///
/// # Errors
/// Returns an error if:
/// * The script execution was not successful (non-zero exit code)
/// * The output cannot be parsed as a boolean
fn process_script_output(output: std::process::Output) -> Result<bool, ScriptError> {
	if !output.status.success() {
		return Err(ScriptError::execution_error(
			String::from_utf8_lossy(&output.stderr).to_string(),
		));
	}

	String::from_utf8_lossy(&output.stdout)
		.trim()
		.parse::<bool>()
		.map_err(|e| ScriptError::execution_error(e.to_string()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{
		AddressWithABI, EVMMonitorMatch, EVMTransaction, EventCondition, FunctionCondition,
		MatchConditions, Monitor, MonitorMatch, ProcessedBlock, TransactionCondition,
	};
	use std::io::Write;
	use tempfile::NamedTempFile;
	use web3::types::{TransactionReceipt, H160, U256};

	// Helper function to create a temporary script file
	fn create_temp_script(content: &str) -> NamedTempFile {
		let mut file = NamedTempFile::new().unwrap();
		file.write_all(content.as_bytes()).unwrap();
		file
	}
	/// Creates a test monitor with customizable parameters
	fn create_test_monitor(
		event_conditions: Vec<EventCondition>,
		function_conditions: Vec<FunctionCondition>,
		transaction_conditions: Vec<TransactionCondition>,
		addresses: Vec<AddressWithABI>,
	) -> Monitor {
		Monitor {
			match_conditions: MatchConditions {
				events: event_conditions,
				functions: function_conditions,
				transactions: transaction_conditions,
			},
			addresses,
			name: "test".to_string(),
			networks: vec!["evm_mainnet".to_string()],
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

	fn create_mock_monitor_match() -> MonitorMatch {
		MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor(vec![], vec![], vec![], vec![]),
			transaction: create_test_evm_transaction(),
			receipt: TransactionReceipt::default(),
			matched_on: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![],
			},
			matched_on_args: None,
		}))
	}

	#[tokio::test]
	async fn test_python_script_executor_success() {
		let script_content = r#"
import sys
import json

input_json = sys.argv[1]
data = json.loads(input_json)
print("Nico")
def test():
    return False
result =test()
sys.exit(0 if result else 1)
"#;
		let temp_file = create_temp_script(script_content);

		let executor = PythonScriptExecutor {
			script_path: temp_file.path().to_str().unwrap().to_string(),
		};

		let input = ProcessedBlock {
			block_number: 1_u64,
			network_slug: "test".to_string(),
			processing_results: vec![create_mock_monitor_match()],
		};

		let result = executor.execute(input).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_javascript_script_executor_success() {
		let script_content = r#"
const input = JSON.parse(process.argv[2]);
// Do something with input and return true/false
console.log('true');
"#;
		let temp_file = create_temp_script(script_content);

		let executor = JavaScriptScriptExecutor {
			script_path: temp_file.path().to_str().unwrap().to_string(),
		};

		let input = ProcessedBlock {
			block_number: 1_u64,
			network_slug: "test".to_string(),
			processing_results: vec![create_mock_monitor_match()],
		};

		let result = executor.execute(input).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_bash_script_executor_success() {
		let script_content = r#"#!/bin/bash
input_json="$1"
# Do something with input_json and return true/false
echo "true"
"#;
		let temp_file = create_temp_script(script_content);

		let executor = BashScriptExecutor {
			script_path: temp_file.path().to_str().unwrap().to_string(),
		};

		let input = ProcessedBlock {
			block_number: 1_u64,
			network_slug: "test".to_string(),
			processing_results: vec![create_mock_monitor_match()],
		};

		let result = executor.execute(input).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_script_execution_error() {
		let script_content = "invalid_command"; // This will cause an error
		let temp_file = create_temp_script(script_content);

		let executor = BashScriptExecutor {
			script_path: temp_file.path().to_str().unwrap().to_string(),
		};

		let input = ProcessedBlock {
			block_number: 1_u64,
			network_slug: "test".to_string(),
			processing_results: vec![create_mock_monitor_match()],
		};

		let result = executor.execute(input).await;
		assert!(matches!(result, Err(CustomScriptError::ExecutionError(_))));
	}

	#[tokio::test]
	async fn test_script_invalid_output() {
		let script_content = "echo 'not a boolean'";
		let temp_file = create_temp_script(script_content);

		let executor = BashScriptExecutor {
			script_path: temp_file.path().to_str().unwrap().to_string(),
		};

		let input = ProcessedBlock {
			block_number: 1_u64,
			network_slug: "test".to_string(),
			processing_results: vec![create_mock_monitor_match()],
		};

		let result = executor.execute(input).await;
		assert!(matches!(result, Err(CustomScriptError::ParseError(_))));
	}

	#[test]
	fn test_custom_script_error_display() {
		let io_error = CustomScriptError::IoError(std::io::Error::new(
			std::io::ErrorKind::Other,
			"test error",
		));
		assert!(format!("{}", io_error).contains("IO error"));

		let process_error = CustomScriptError::process_error(Some(1), "command failed".to_string());
		assert!(format!("{}", process_error).contains("Process error"));
	}
}
