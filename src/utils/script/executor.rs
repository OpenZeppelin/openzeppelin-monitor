use crate::{models::MonitorMatch, utils::script::error::ScriptError};
use async_trait::async_trait;
use libc::{c_int, getrlimit, RLIMIT_NOFILE};
use log::debug;
use std::{any::Any, mem::MaybeUninit, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, time::timeout};

/// A trait that defines the interface for executing custom scripts in different languages.
/// Implementors must be both Send and Sync to ensure thread safety.
#[async_trait]
pub trait ScriptExecutor: Send + Sync + Any {
	/// Enables downcasting by returning a reference to `Any`
	fn as_any(&self) -> &dyn Any;
	/// Executes the script with the given MonitorMatch input.
	///
	/// # Arguments
	/// * `input` - A MonitorMatch instance containing the data to be processed by the script
	/// * `args` - Additional arguments passed to the script
	/// * `from_custom_notification` - Whether the script is from a custom notification
	///
	/// # Returns
	/// * `Result<bool, ScriptError>` - Returns true/false based on script execution or an error
	async fn execute(
		&self,
		input: MonitorMatch,
		timeout_ms: &u32,
		args: Option<&[String]>,
		from_custom_notification: bool,
	) -> Result<bool, ScriptError>;
}

/// Executes Python scripts using the python3 interpreter.
pub struct PythonScriptExecutor {
	/// Content of the Python script file to be executed
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
			debug!("Failed to get rlimit");
			(0, 0)
		}
	}
}

#[async_trait]
impl ScriptExecutor for PythonScriptExecutor {
	fn as_any(&self) -> &dyn Any {
		self
	}

	async fn execute(
		&self,
		input: MonitorMatch,
		timeout_ms: &u32,
		args: Option<&[String]>,
		from_custom_notification: bool,
	) -> Result<bool, ScriptError> {
		let (open_fds, max_fds) = count_open_fds();
		let combined_input = serde_json::json!({
			"monitor_match": input,
			"args": args
		});
		let input_json = serde_json::to_string(&combined_input)
			.map_err(|e| ScriptError::parse_error(e.to_string()))?;

		// Warning if open file descriptors exceed the maximum limit
		if open_fds > max_fds as usize {
			log::warn!(
				"Critical: Number of open file descriptors ({}) exceeds maximum allowed ({}). \
				 This may cause unexpected runtime issues. You should increase the limit for open \
				 files by running:  ulimit -n <number of fds>",
				open_fds,
				max_fds
			);
		}

		let mut cmd = tokio::process::Command::new("python3")
			.arg("-c")
			.arg(&self.script_content)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.kill_on_drop(true)
			.spawn()
			.map_err(|e| ScriptError::execution_error(e.to_string()))?;

		// Write the input_json to stdin
		if let Some(mut stdin) = cmd.stdin.take() {
			stdin
				.write_all(input_json.as_bytes())
				.await
				.map_err(|e| ScriptError::execution_error(e.to_string()))?;
		} else {
			return Err(ScriptError::parse_error(
				"Failed to get stdin handle".to_string(),
			));
		}

		// Define a timeout duration
		let timeout_duration = Duration::from_millis(u64::from(*timeout_ms));

		// Apply timeout to script execution
		match timeout(timeout_duration, cmd.wait_with_output()).await {
			Ok(result) => {
				let output = result.map_err(|e| ScriptError::execution_error(e.to_string()))?;
				process_script_output(output, from_custom_notification)
			}
			Err(_) => Err(ScriptError::execution_error(
				"Script execution timed out".to_string(),
			)),
		}
	}
}

/// Executes JavaScript scripts using the Node.js runtime.
pub struct JavaScriptScriptExecutor {
	/// Content of the JavaScript script file to be executed
	pub script_content: String,
}

#[async_trait]
impl ScriptExecutor for JavaScriptScriptExecutor {
	fn as_any(&self) -> &dyn Any {
		self
	}
	async fn execute(
		&self,
		input: MonitorMatch,
		timeout_ms: &u32,
		args: Option<&[String]>,
		from_custom_notification: bool,
	) -> Result<bool, ScriptError> {
		let (open_fds, max_fds) = count_open_fds();
		// Create a combined input with both the monitor match and arguments
		let combined_input = serde_json::json!({
			"monitor_match": input,
			"args": args
		});
		let input_json = serde_json::to_string(&combined_input)
			.map_err(|e| ScriptError::parse_error(e.to_string()))?;

		// Warning if open file descriptors exceed the maximum limit
		if open_fds > max_fds as usize {
			log::warn!(
				"Critical: Number of open file descriptors ({}) exceeds maximum allowed ({}). \
				 This will cause issues. You should increase the limit for open files.",
				open_fds,
				max_fds
			);
		}

		let mut cmd = tokio::process::Command::new("node")
			.arg("-e")
			.arg(&self.script_content)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::null())
			.kill_on_drop(true)
			.spawn()
			.map_err(|e| ScriptError::execution_error(e.to_string()))?;

		// Write the input_json to stdin
		if let Some(mut stdin) = cmd.stdin.take() {
			stdin
				.write_all(input_json.as_bytes())
				.await
				.map_err(|e| ScriptError::execution_error(e.to_string()))?;
		} else {
			return Err(ScriptError::parse_error(
				"Failed to get stdin handle".to_string(),
			));
		}

		// Define a timeout duration
		let timeout_duration = Duration::from_millis(u64::from(*timeout_ms));

		// Apply timeout to script execution
		match timeout(timeout_duration, cmd.wait_with_output()).await {
			Ok(result) => {
				let output = result.map_err(|e| ScriptError::execution_error(e.to_string()))?;
				process_script_output(output, from_custom_notification)
			}
			Err(_) => Err(ScriptError::execution_error(
				"Script execution timed out".to_string(),
			)),
		}
	}
}

/// Executes Bash shell scripts.
pub struct BashScriptExecutor {
	/// Content of the Bash script file to be executed
	pub script_content: String,
}

#[async_trait]
impl ScriptExecutor for BashScriptExecutor {
	fn as_any(&self) -> &dyn Any {
		self
	}
	async fn execute(
		&self,
		input: MonitorMatch,
		timeout_ms: &u32,
		args: Option<&[String]>,
		from_custom_notification: bool,
	) -> Result<bool, ScriptError> {
		// Create a combined input with both the monitor match and arguments
		let combined_input = serde_json::json!({
			"monitor_match": input,
			"args": args
		});

		let input_json = serde_json::to_string(&combined_input)
			.map_err(|e| ScriptError::parse_error(e.to_string()))?;

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

		let mut cmd = tokio::process::Command::new("sh")
			.arg("-c")
			.arg(&self.script_content)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::null())
			.kill_on_drop(true)
			.spawn()
			.map_err(|e| ScriptError::execution_error(e.to_string()))?;

		// Write the input_json to stdin
		if let Some(mut stdin) = cmd.stdin.take() {
			stdin
				.write_all(input_json.as_bytes())
				.await
				.map_err(|e| ScriptError::execution_error(e.to_string()))?;
		} else {
			return Err(ScriptError::parse_error(
				"Failed to get stdin handle".to_string(),
			));
		}

		// Define a timeout duration
		let timeout_duration = Duration::from_millis(u64::from(*timeout_ms));

		// Apply timeout to script execution
		match timeout(timeout_duration, cmd.wait_with_output()).await {
			Ok(result) => {
				let output = result.map_err(|e| ScriptError::execution_error(e.to_string()))?;
				process_script_output(output, from_custom_notification)
			}
			Err(_) => Err(ScriptError::execution_error(
				"Script execution timed out".to_string(),
			)),
		}
	}
}

/// Processes the output from script execution.
///
/// # Arguments
/// * `output` - The process output containing stdout, stderr, and status
/// * `from_custom_notification` - Whether the script is from a custom notification
/// # Returns
/// * `Result<bool, ScriptError>` - Returns parsed boolean result or error
///
/// # Errors
/// Returns an error if:
/// * The script execution was not successful (non-zero exit code)
/// * The output cannot be parsed as a boolean
/// * The script produced no output
pub fn process_script_output(
	output: std::process::Output,
	from_custom_notification: bool,
) -> Result<bool, ScriptError> {
	if !output.status.success() {
		return Err(ScriptError::execution_error(
			String::from_utf8_lossy(&output.stderr).to_string(),
		));
	}

	// If the script is from a custom notification and the status is success, we don't need to check
	// the output
	if from_custom_notification {
		return Ok(true);
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
		_ => Err(ScriptError::parse_error(
			"Last line of output is not a valid boolean".to_string(),
		)),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::{
		AddressWithABI, EVMMonitorMatch, EVMTransaction, EventCondition, FunctionCondition,
		MatchConditions, Monitor, MonitorMatch, TransactionCondition,
	};
	use std::{fs, path::Path, time::Instant};
	use web3::types::{TransactionReceipt, H160, U256};
	fn read_fixture(filename: &str) -> String {
		let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("tests/integration/fixtures/filters")
			.join(filename);
		fs::read_to_string(fixture_path)
			.unwrap_or_else(|_| panic!("Failed to read fixture file: {}", filename))
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

# Read from stdin instead of command line arguments
input_json = sys.stdin.read()
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

		let timeout = 1000;
		let result = executor.execute(input, &timeout, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
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
		let result = executor.execute(input, &1000, None, false).await;
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

# Read from stdin instead of command line arguments
input_json = sys.stdin.read()
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

		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
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

		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
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
		let result = executor.execute(input, &1000, None, false).await;
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
set -e  # Exit on any error
sleep 0.1  # Small delay to ensure process startup
echo "debugging..."
echo "true"
"#;
		let executor = BashScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();
		for _ in 0..3 {
			// Retry logic for flaky tests
			match executor.execute(input.clone(), &1000, None, false).await {
				Ok(result) => {
					assert!(result);
					return;
				}
				Err(e) => {
					eprintln!("Test attempt failed: {}", e);
					tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
				}
			}
		}
		panic!("Test failed after 3 retries");
	}

	#[tokio::test]
	async fn test_bash_script_executor_invalid_output() {
		let script_content = r#"
#!/bin/bash
set -e  # Exit on any error
sleep 0.1  # Small delay to ensure process startup
echo "debugging..."
echo "not a boolean"
"#;

		let executor = BashScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();
		for _ in 0..3 {
			// Retry logic for flaky tests
			match executor.execute(input.clone(), &1000, None, false).await {
				Err(ScriptError::ParseError(msg)) => {
					assert!(msg.contains("Last line of output is not a valid boolean"));
					return;
				}
				Ok(_) => {
					panic!("Expected ParseError, got success");
				}
				Err(e) => {
					eprintln!("Test attempt failed with unexpected error: {}", e);
					tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
				}
			}
		}
		panic!("Test failed after 3 retries");
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
		let result = executor.execute(input, &1000, None, false).await;

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
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
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

		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_script_executor_with_multiple_lines_of_output() {
		let script_content = r#"
import sys
import json

# Read from stdin instead of command line arguments
input_json = sys.stdin.read()
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

		let input = create_mock_monitor_match();

		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_python_script_executor_monitor_match_fields() {
		let script_content = r#"
import sys
import json

input_json = sys.stdin.read()
data = json.loads(input_json)

monitor_match = data['monitor_match']
# Verify it's an EVM match type
if monitor_match['EVM']:
	block_number = monitor_match['EVM']['transaction']['blockNumber']
	if block_number:
		print("true")
	else:
		print("false")
else:
    print("false")
"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(!result.unwrap());
	}

	#[tokio::test]
	async fn test_python_script_executor_with_args() {
		let script_content = r#"
import sys
import json

input_json = sys.stdin.read()
data = json.loads(input_json)

# Verify both fields exist
if 'monitor_match' not in data or 'args' not in data:
    print("false")
    exit(1)

# Test args parsing
args = data['args']
if "--verbose" in args:
    print("true")
else:
    print("false")
"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();

		// Test with matching argument
		let args = vec![String::from("test_argument")];
		let result = executor
			.execute(input.clone(), &1000, Some(&args), false)
			.await;
		assert!(result.is_ok());
		assert!(!result.unwrap());

		// Test with non-matching argument
		let args = vec![String::from("--verbose"), String::from("--other-arg")];
		let result = executor
			.execute(input.clone(), &1000, Some(&args), false)
			.await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_python_script_executor_combined_fields() {
		let script_content = r#"
import sys
import json

input_json = sys.stdin.read()
data = json.loads(input_json)

monitor_match = data['monitor_match']
args = data['args']

# Test both monitor_match and args together
expected_args = ["--verbose", "--specific_arg", "--test"]
if (monitor_match['EVM'] and
    args == expected_args):
    print("true")
else:
    print("false")
"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();

		// Test with correct combination
		let args = vec![
			String::from("--verbose"),
			String::from("--specific_arg"),
			String::from("--test"),
		];
		let result = executor
			.execute(input.clone(), &1000, Some(&args), false)
			.await;
		assert!(result.is_ok());
		assert!(result.unwrap());

		// Test with wrong argument
		let args = vec![String::from("wrong_arg")];
		let result = executor
			.execute(input.clone(), &1000, Some(&args), false)
			.await;
		assert!(result.is_ok());
		assert!(!result.unwrap());
	}

	#[tokio::test]
	async fn test_python_script_executor_with_verbose_arg() {
		let script_content = read_fixture("evm_filter_by_arguments.py");
		let executor = PythonScriptExecutor { script_content };
		let input = create_mock_monitor_match();
		let args = vec![String::from("--verbose")];
		let result = executor
			.execute(input.clone(), &1000, Some(&args), false)
			.await;

		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_python_script_executor_with_wrong_arg() {
		let script_content = read_fixture("evm_filter_by_arguments.py");
		let executor = PythonScriptExecutor { script_content };

		let input = create_mock_monitor_match();
		let args = vec![String::from("--wrong_arg"), String::from("--test")];
		let result = executor
			.execute(input.clone(), &1000, Some(&args), false)
			.await;

		assert!(result.is_ok());
		assert!(!result.unwrap());
	}

	#[tokio::test]
	async fn test_script_executor_with_ignore_output() {
		let script_content = r#"
		# This script produces no output
		"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, true).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_script_executor_with_non_zero_exit() {
		let script_content = r#"
import sys
sys.stderr.write("Error: something went wrong\n")
sys.exit(1)
		"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, true).await;

		assert!(result.is_err());
		match result {
			Err(ScriptError::ExecutionError(msg)) => {
				assert!(msg.contains("Error: something went wrong"));
			}
			_ => panic!("Expected ExecutionError"),
		}
	}

	#[tokio::test]
	async fn test_script_notify_succeeds_within_timeout() {
		let script_content = r#"
import sys
import time
time.sleep(0.3)
		"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();
		let start_time = Instant::now();
		let result = executor.execute(input, &1000, None, true).await;
		let elapsed = start_time.elapsed();

		assert!(result.is_ok());
		// Verify that execution took at least 300ms (the sleep time)
		assert!(elapsed.as_millis() >= 300);
		// Verify that execution took less than the timeout
		assert!(elapsed.as_millis() < 1000);
	}

	#[tokio::test]
	async fn test_script_notify_fails_within_timeout() {
		let script_content = r#"
import sys
import time
time.sleep(0.5)
		"#;

		let executor = PythonScriptExecutor {
			script_content: script_content.to_string(),
		};

		let input = create_mock_monitor_match();
		let start_time = Instant::now();
		let result = executor.execute(input, &400, None, true).await;
		let elapsed = start_time.elapsed();

		assert!(result.is_err());
		// Verify that execution took at least 300ms (the sleep time)
		assert!(elapsed.as_millis() >= 400 && elapsed.as_millis() < 600);
	}
}
