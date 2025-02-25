use crate::{models::MonitorMatch, utils::script::error::ScriptError};
use async_trait::async_trait;
use libc::{c_int, getrlimit, RLIMIT_NOFILE};
use log::info;
use std::{mem::MaybeUninit, process::Stdio};
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
	async fn execute(&self, input: MonitorMatch) -> Result<bool, ScriptError>;
}

/// Executes Python scripts using the python3 interpreter.
pub struct PythonScriptExecutor {
	/// Path to the Python script file to be executed
	pub script_content: String,
}

/// Counts the number of open file descriptors for the current process
fn count_open_fds() -> (usize, u64) {
	#[cfg(unix)]
	{
		let mut rlimit = MaybeUninit::uninit();
		let ret = unsafe { getrlimit(RLIMIT_NOFILE, rlimit.as_mut_ptr()) };

		if ret == 0 {
			let rlimit = unsafe { rlimit.assume_init() };
			let mut count = 0;

			// Check each potential file descriptor up to the soft limit
			for fd in 0..rlimit.rlim_cur {
				let ret = unsafe { libc::fcntl(fd as c_int, libc::F_GETFD) };
				if ret != -1 {
					count += 1;
				}
			}
			(count, rlimit.rlim_cur)
		} else {
			info!("Failed to get rlimit");
			(0, 0)
		}
	}
}

#[async_trait]
impl ScriptExecutor for PythonScriptExecutor {
	async fn execute(&self, input: MonitorMatch) -> Result<bool, ScriptError> {
		let input_json =
			serde_json::to_string(&input).map_err(|e| ScriptError::parse_error(e.to_string()))?;

		let (open_fds, max_fds) = count_open_fds();

		// Warning if open file descriptors exceed the maximum limit
		if open_fds > max_fds as usize {
			log::warn!(
				"Critical: Number of open file descriptors ({}) exceeds maximum allowed ({}). \
				 This will cause issues. You should increase the limit for open files by running:  \
				 ulimit -n <number of fds>",
				open_fds,
				max_fds
			);
		}

		let cmd = tokio::process::Command::new("python3")
			.arg("-c")
			.arg(&self.script_content)
			.arg(&input_json)
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.kill_on_drop(true)
			.spawn()
			.map_err(|e| {
				if e.to_string().contains("too many open files") {
					log::error!(
						"Too many open files error detected. Current open FDs: {}/{}",
						open_fds,
						max_fds
					);
				}
				ScriptError::execution_error(e.to_string())
			})?;

		let output = cmd
			.wait_with_output()
			.await
			.map_err(|e| ScriptError::execution_error(e.to_string()))?;

		process_script_output(output)
	}
}

/// Executes JavaScript scripts using the Node.js runtime.
pub struct JavaScriptScriptExecutor {
	/// Path to the JavaScript script file to be executed
	pub script_content: String,
}

#[async_trait]
impl ScriptExecutor for JavaScriptScriptExecutor {
	async fn execute(&self, input: MonitorMatch) -> Result<bool, ScriptError> {
		let input_json =
			serde_json::to_string(&input).map_err(|e| ScriptError::parse_error(e.to_string()))?;
		let (open_fds, max_fds) = count_open_fds();

		// Warning if open file descriptors exceed the maximum limit
		if open_fds > max_fds as usize {
			log::warn!(
				"Critical: Number of open file descriptors ({}) exceeds maximum allowed ({}). \
				 This will cause issues. You should increase the limit for open files.",
				open_fds,
				max_fds
			);
		}

		let cmd = tokio::process::Command::new("node")
			.arg("-e")
			.arg(&self.script_content)
			.arg(&input_json)
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::null())
			.kill_on_drop(true)
			.spawn()
			.map_err(|e| {
				if e.to_string().contains("too many open files") {
					log::error!(
						"Too many open files error detected. Current open FDs: {}/{}",
						open_fds,
						max_fds
					);
				}
				ScriptError::execution_error(e.to_string())
			})?;

		let output = cmd
			.wait_with_output()
			.await
			.map_err(|e| ScriptError::execution_error(e.to_string()))?;
		process_script_output(output)
	}
}

/// Executes Bash shell scripts.
pub struct BashScriptExecutor {
	/// Path to the Bash script file to be executed
	pub script_content: String,
}

#[async_trait]
impl ScriptExecutor for BashScriptExecutor {
	async fn execute(&self, input: MonitorMatch) -> Result<bool, ScriptError> {
		let input_json =
			serde_json::to_string(&input).map_err(|e| ScriptError::parse_error(e.to_string()))?;
		let (open_fds, max_fds) = count_open_fds();

		// Warning if open file descriptors exceed the maximum limit
		if open_fds > max_fds as usize {
			log::warn!(
				"Critical: Number of open file descriptors ({}) exceeds maximum allowed ({}). \
				 This will cause issues. You should increase the limit for open files.",
				open_fds,
				max_fds
			);
		}

		let cmd = tokio::process::Command::new("sh")
			.arg("-c")
			.arg(&self.script_content)
			.arg(&input_json)
			.stdin(Stdio::null())
			.stdout(Stdio::piped())
			.stderr(Stdio::null())
			.kill_on_drop(true)
			.spawn()
			.map_err(|e| {
				if e.to_string().contains("too many open files") {
					log::error!(
						"Too many open files error detected. Current open FDs: {}/{}",
						open_fds,
						max_fds
					);
				}
				ScriptError::execution_error(e.to_string())
			})?;

		let output = cmd
			.wait_with_output()
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
/// * The script produced no output
pub fn process_script_output(output: std::process::Output) -> Result<bool, ScriptError> {
	if !output.status.success() {
		return Err(ScriptError::execution_error(
			String::from_utf8_lossy(&output.stderr).to_string(),
		));
	}

	let stdout = String::from_utf8_lossy(&output.stdout);

	if stdout.trim().is_empty() {
		return Err(ScriptError::parse_error(
			"Script produced no output".to_string(),
		));
	}

	let last_line = stdout
		.lines()
		.last()
		.ok_or_else(|| ScriptError::parse_error("No output from script".to_string()))?
		.trim();

	match last_line.to_lowercase().as_str() {
		"true" => Ok(true),
		"false" => Ok(false),
		_ => Err(ScriptError::parse_error(format!(
			"Last line of output is not a valid boolean: '{}'",
			last_line
		))),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{
		AddressWithABI, EVMMonitorMatch, EVMTransaction, EventCondition, FunctionCondition,
		MatchConditions, Monitor, MonitorMatch, TransactionCondition,
	};
	use web3::types::{TransactionReceipt, H160, U256};

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
print("debugging...")
def test():
    return True
result = test()
print(result)
"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();

		let result = executor.execute(input).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), true);
	}

	#[tokio::test]
	async fn test_python_script_executor_invalid_output() {
		let script_content = r#"
import sys

print("debugging...")
def test():
    return "not a boolean"
result = test()
print(result)
"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();

		let result = executor.execute(input).await;
		assert!(result.is_err());
		match result {
			Err(ScriptError::ParseError(msg)) => {
				assert!(msg.contains("Last line of output is not a valid boolean"));
			}
			_ => panic!("Expected ParseError"),
		}
	}

	#[tokio::test]
	async fn test_python_script_executor_multiple_prints() {
		let script_content = r#"
import sys
import json

input_json = sys.argv[1]
data = json.loads(input_json)
print("Starting script execution...")
print("Processing data...")
print("More debug info")
print("true")
"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();

		let result = executor.execute(input).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), true);
	}

	#[tokio::test]
	async fn test_javascript_script_executor_success() {
		let script_content = r#"
		// Do something with input and return true/false
		console.log("debugging...");
		console.log("finished");
		console.log("true");
		"#;

		let executor = JavaScriptScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();

		let result = executor.execute(input).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), true);
	}

	#[tokio::test]
	async fn test_javascript_script_executor_invalid_output() {
		let script_content = r#"
		console.log("debugging...");
		console.log("finished");
		console.log("not a boolean");
		"#;

		let executor = JavaScriptScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();

		let result = executor.execute(input).await;
		assert!(result.is_err());
		match result {
			Err(ScriptError::ParseError(msg)) => {
				assert!(msg.contains("Last line of output is not a valid boolean"));
			}
			_ => panic!("Expected ParseError"),
		}
	}

	#[tokio::test]
	async fn test_bash_script_executor_success() {
		let script_content = r#"
	#!/bin/bash
	echo "debugging..."
	echo "true"
	"#;
		let executor = BashScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();

		let result = executor.execute(input).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), true);
	}

	#[tokio::test]
	async fn test_bash_script_executor_invalid_output() {
		let script_content = r#"
	#!/bin/bash
	echo "debugging..."
	echo "not a boolean"
	"#;

		let executor = BashScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();

		let result = executor.execute(input).await;
		assert!(result.is_err());
		match result {
			Err(ScriptError::ParseError(msg)) => {
				assert!(msg.contains("Last line of output is not a valid boolean"));
			}
			_ => panic!("Expected ParseError"),
		}
	}

	#[tokio::test]
	async fn test_script_executor_empty_output() {
		let script_content = r#"
	# This script produces no output
	"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();
		let result = executor.execute(input).await;

		match result {
			Err(ScriptError::ParseError(msg)) => {
				assert!(msg.contains("Script produced no output"));
			}
			_ => panic!("Expected ParseError"),
		}
	}

	#[tokio::test]
	async fn test_script_executor_whitespace_output() {
		let script_content = r#"
print("   ")
print("     true    ")  # Should handle whitespace correctly
"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();
		let result = executor.execute(input).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), true);
	}

	#[tokio::test]
	async fn test_script_executor_invalid_json_input() {
		let script_content = r#"
	import sys
	import json

	input_json = sys.argv[1]
	data = json.loads(input_json)
	print("true")
	print("Invalid JSON input")
	exit(1)
	"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		// Create an invalid MonitorMatch that will fail JSON serialization
		let input = create_mock_monitor_match();

		let result = executor.execute(input).await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_script_executor_with_multiple_lines_of_output() {
		let script_content = r#"
import sys
import json

input_json = sys.argv[1]
data = json.loads(input_json)
print("debugging...")
print("false")
print("true")
print("false")
print("true")
"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		// Create an invalid MonitorMatch that will fail JSON serialization
		let input = create_mock_monitor_match();

		let result = executor.execute(input).await;
		println!("result ===>: {:?}", result);
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), true);
	}
}
