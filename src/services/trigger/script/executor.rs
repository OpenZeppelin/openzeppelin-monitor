//! Trigger script executor implementation.
//!
//! This module provides functionality to execute scripts in different languages.

use crate::models::MonitorMatch;
use anyhow::Context;
use async_trait::async_trait;
use std::{any::Any, process::Stdio, time::Duration};
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
	/// * `timeout_ms` - The timeout for the script execution in milliseconds
	/// * `args` - Additional arguments passed to the script
	/// * `from_custom_notification` - Whether the script is from a custom notification
	///
	/// # Returns
	/// * `Result<bool, anyhow::Error>` - Returns true/false based on script execution or an error
	async fn execute(
		&self,
		input: MonitorMatch,
		timeout_ms: &u32,
		args: Option<&[String]>,
		from_custom_notification: bool,
	) -> Result<bool, anyhow::Error>;
}

/// Executes a script by handing its content to an interpreter via an eval flag.
pub struct InterpreterExecutor {
	/// Interpreter binary, e.g. `python3`, `node`, `sh`
	pub program: &'static str,
	/// Flag that makes the interpreter evaluate the script given on the command line (`-c` / `-e`)
	pub eval_flag: &'static str,
	/// Content of the script to be executed
	pub script_content: String,
	/// Arguments passed to the interpreter itself, before the eval flag
	pub runtime_arguments: Vec<String>,
}

#[async_trait]
impl ScriptExecutor for InterpreterExecutor {
	fn as_any(&self) -> &dyn Any {
		self
	}
	async fn execute(
		&self,
		input: MonitorMatch,
		timeout_ms: &u32,
		args: Option<&[String]>,
		from_custom_notification: bool,
	) -> Result<bool, anyhow::Error> {
		// Create a combined input with both the monitor match and arguments
		let combined_input = serde_json::json!({
			"monitor_match": input,
			"args": args
		});
		let input_json = serde_json::to_string(&combined_input)
			.with_context(|| "Failed to serialize monitor match and arguments")?;

		let mut command = tokio::process::Command::new(self.program);
		command
			.args(&self.runtime_arguments)
			.arg(self.eval_flag)
			.arg(&self.script_content)
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			// Kill the direct child if the handle is dropped; the process-group guard in
			// `process_command` covers the descendants on timeout and cancellation.
			.kill_on_drop(true);
		// The child leads its own process group (pgid == pid), so timeout and cancellation
		// cleanup can signal the processes remaining in that group (descendants that moved to
		// another session or group are out of reach).
		#[cfg(unix)]
		command.process_group(0);
		let cmd = command
			.spawn()
			.with_context(|| format!("Failed to spawn {} process", self.program))?;

		process_command(cmd, &input_json, timeout_ms, from_custom_notification).await
	}
}

/// Processes the output from script execution.
///
/// # Arguments
/// * `output` - The process output containing stdout, stderr, and status
/// * `from_custom_notification` - Whether the script is from a custom notification
/// # Returns
/// * `Result<bool, anyhow::Error>` - Returns parsed boolean result or error
///
/// # Errors
/// Returns an error if:
/// * The script execution was not successful (non-zero exit code)
/// * The output cannot be parsed as a boolean
/// * The script produced no output
#[allow(clippy::result_large_err)]
pub fn process_script_output(
	output: std::process::Output,
	from_custom_notification: bool,
) -> Result<bool, anyhow::Error> {
	if !output.status.success() {
		let error_message = String::from_utf8_lossy(&output.stderr).to_string();
		return Err(anyhow::anyhow!(
			"Script execution failed: {}",
			error_message
		));
	}

	// If the script is from a custom notification and the status is success, we don't need to check
	// the output
	if from_custom_notification {
		return Ok(true);
	}

	let stdout = String::from_utf8_lossy(&output.stdout);

	if stdout.trim().is_empty() {
		return Err(anyhow::anyhow!("Script produced no output"));
	}

	let last_line = stdout
		.lines()
		.last()
		.ok_or_else(|| anyhow::anyhow!("No output from script"))?
		.trim();

	match last_line.to_lowercase().as_str() {
		"true" => Ok(true),
		"false" => Ok(false),
		_ => Err(anyhow::anyhow!(
			"Last line of output is not a valid boolean: {}",
			last_line
		)),
	}
}

/// Kills the script's process group on drop unless disarmed.
///
/// The script is spawned as the leader of its own process group (see
/// [`InterpreterExecutor::execute`]), so `killpg` reaches the descendants that
/// `kill_on_drop` cannot. The guard holds a numeric pgid, not the kernel group: a late kill
/// after the whole group has exited and been reaped could in theory target a reused id, and
/// descendants that move to another session or group are out of reach (documented
/// limitations).
#[cfg(unix)]
struct ProcessGroupGuard {
	pgid: libc::pid_t,
	armed: bool,
}

#[cfg(unix)]
impl ProcessGroupGuard {
	/// Arms a guard for the process group led by `pid`.
	///
	/// `pid` comes from `Child::id()` right after the spawn, so it is always a positive
	/// `pid_t`. Anything else is a caller bug and panics: `killpg(0)` would signal the
	/// Monitor's own process group.
	fn new(pid: u32) -> Self {
		let pgid = libc::pid_t::try_from(pid).expect("pid does not fit pid_t");
		assert!(
			pgid > 0,
			"process group id must be positive: killpg(0) would signal the Monitor's own group"
		);
		Self { pgid, armed: true }
	}

	/// Sends `SIGKILL` to the process group once; later calls are no-ops.
	fn kill(&mut self) {
		if !self.armed {
			return;
		}
		self.armed = false;
		// SAFETY: `pgid` is a positive id obtained from a child this process spawned as a
		// group leader; `killpg` has no memory-safety preconditions.
		let rc = unsafe { libc::killpg(self.pgid, libc::SIGKILL) };
		if rc == -1 {
			let err = std::io::Error::last_os_error();
			if err.raw_os_error() == Some(libc::ESRCH) {
				tracing::debug!("script process group {} already exited", self.pgid);
			} else {
				tracing::warn!("failed to kill script process group {}: {}", self.pgid, err);
			}
		}
	}

	/// Leaves the process group alive when the guard is dropped.
	fn disarm(&mut self) {
		self.armed = false;
	}
}

#[cfg(unix)]
impl Drop for ProcessGroupGuard {
	fn drop(&mut self) {
		self.kill();
	}
}

/// No-op counterpart of the Unix guard so `process_command` reads the same everywhere.
#[cfg(not(unix))]
struct ProcessGroupGuard;

#[cfg(not(unix))]
impl ProcessGroupGuard {
	fn new(_pid: u32) -> Self {
		Self
	}

	fn kill(&mut self) {}

	fn disarm(&mut self) {}
}

/// Feeds `input_json` to the script and waits for it to exit, bounded by `timeout_ms`.
///
/// The stdin write and the stdout/stderr drain run concurrently so neither can block the
/// other on a full pipe. A `BrokenPipe` while writing stdin is not an error: the child's
/// exit status and output decide. On Unix, a timeout, an I/O failure or cancellation
/// of this future kills the script's process group. After stdin delivery and output collection
/// complete successfully, the process group is left running regardless of the script's exit
/// status or parsed result, so background processes a script leaves behind keep running.
async fn process_command(
	mut cmd: tokio::process::Child,
	input_json: &str,
	timeout_ms: &u32,
	from_custom_notification: bool,
) -> Result<bool, anyhow::Error> {
	let pid = cmd
		.id()
		.expect("child was just spawned and has not been waited for");
	let mut stdin = cmd
		.stdin
		.take()
		.ok_or_else(|| anyhow::anyhow!("Failed to get stdin handle"))?;

	let feed = async move {
		match stdin.write_all(input_json.as_bytes()).await {
			Ok(()) => {}
			Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
				tracing::debug!("script closed stdin before reading its input")
			}
			Err(e) => return Err(anyhow::anyhow!("Failed to write input to script: {}", e)),
		}
		// Closing the handle is what delivers EOF to the child (`shutdown` is a no-op on a
		// pipe), so do it explicitly instead of relying on the completed future being dropped.
		drop(stdin);
		Ok(())
	};
	let wait = async {
		cmd.wait_with_output()
			.await
			.map_err(|e| anyhow::anyhow!("Failed to wait for script output: {}", e))
	};
	tokio::pin!(feed);
	tokio::pin!(wait);
	// Invariant: the `Child` (inside `wait`) and the stdin handle (inside `feed`) are owned by
	// locals declared before the guard below, and `run` only borrows them. Locals drop in
	// reverse order, so on timeout (explicit kill), on a stdin write error and on cancellation
	// (both via `Drop`) the group is signalled before the `Child` is dropped and
	// `kill_on_drop` reaches the already-signalled leader. `try_join!` stops at the first
	// error and drops the branches it was given, but those are `&mut` borrows, so that cannot
	// release the `Child` early. A failure inside `wait_with_output` is the one exception:
	// that future owns the `Child` and drops it as it completes, so there `kill_on_drop` runs
	// first and the guard signals whatever group members remain.
	let run = async { tokio::try_join!(&mut feed, &mut wait) };
	tokio::pin!(run);
	// Nothing above awaits, and the one early return before this point (`stdin.take()`, which
	// cannot fail with a piped stdin) still kills the direct child through `kill_on_drop`.
	let mut guard = ProcessGroupGuard::new(pid);

	let timeout_duration = Duration::from_millis(u64::from(*timeout_ms));
	// Polling `&mut run` keeps `run` alive past an `Elapsed`, so the explicit kill below runs
	// while the `Child` is still held. It does not guarantee the OS process is still alive or
	// unreaped: `wait_with_output` may already have waited for it while another I/O operation
	// was pending.
	match timeout(timeout_duration, &mut run).await {
		Ok(Ok(((), output))) => {
			// Only once both the stdin feed and the wait have completed: a descendant that
			// still holds stdin open after the interpreter exited must remain killable.
			guard.disarm();
			process_script_output(output, from_custom_notification)
		}
		Ok(Err(e)) => Err(e),
		Err(_) => {
			guard.kill();
			Err(anyhow::anyhow!("Script execution timed out"))
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		models::{
			AddressWithSpec, EVMMonitorMatch, EVMReceiptLog, EventCondition, FunctionCondition,
			MatchConditions, Monitor, MonitorMatch, ScriptLanguage, TransactionCondition,
		},
		utils::tests::evm::{
			monitor::MonitorBuilder, receipt::ReceiptBuilder, transaction::TransactionBuilder,
		},
	};
	use std::{fs, path::Path, time::Instant};

	fn read_fixture(filename: &str) -> String {
		let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("tests/integration/fixtures/filters")
			.join(filename);
		fs::read_to_string(fixture_path)
			.unwrap_or_else(|_| panic!("Failed to read fixture file: {}", filename))
	}

	fn interpreter(
		program: &'static str,
		eval_flag: &'static str,
		script: &str,
		runtime_arguments: Vec<String>,
	) -> InterpreterExecutor {
		InterpreterExecutor {
			program,
			eval_flag,
			script_content: script.to_string(),
			runtime_arguments,
		}
	}

	fn python(script: &str, runtime_arguments: Vec<String>) -> InterpreterExecutor {
		let (program, eval_flag) = ScriptLanguage::Python.interpreter();
		interpreter(program, eval_flag, script, runtime_arguments)
	}

	fn node(script: &str, runtime_arguments: Vec<String>) -> InterpreterExecutor {
		let (program, eval_flag) = ScriptLanguage::JavaScript.interpreter();
		interpreter(program, eval_flag, script, runtime_arguments)
	}

	fn sh(script: &str, runtime_arguments: Vec<String>) -> InterpreterExecutor {
		let (program, eval_flag) = ScriptLanguage::Bash.interpreter();
		interpreter(program, eval_flag, script, runtime_arguments)
	}

	/// Creates a test monitor with customizable parameters
	fn create_test_monitor(
		event_conditions: Vec<EventCondition>,
		function_conditions: Vec<FunctionCondition>,
		transaction_conditions: Vec<TransactionCondition>,
		addresses: Vec<AddressWithSpec>,
	) -> Monitor {
		let mut builder = MonitorBuilder::new()
			.name("test")
			.networks(vec!["evm_mainnet".to_string()]);

		for event in event_conditions {
			builder = builder.event(&event.signature, event.expression);
		}
		for function in function_conditions {
			builder = builder.function(&function.signature, function.expression);
		}
		for transaction in transaction_conditions {
			builder = builder.transaction(transaction.status, transaction.expression);
		}

		builder = builder.addresses_with_spec(
			addresses
				.into_iter()
				.map(|a| (a.address, a.contract_spec))
				.collect(),
		);

		builder.build()
	}

	fn create_test_evm_logs() -> Vec<EVMReceiptLog> {
		ReceiptBuilder::new().build().logs.clone()
	}

	fn create_mock_monitor_match() -> MonitorMatch {
		MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: create_test_monitor(vec![], vec![], vec![], vec![]),
			transaction: TransactionBuilder::new().build(),
			receipt: Some(ReceiptBuilder::new().build()),
			logs: Some(create_test_evm_logs()),
			network_slug: "evm_mainnet".to_string(),
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

		let executor = python(script_content, vec![]);

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
input_json = sys.stdin.read()
print("debugging...")
def test():
    return "not a boolean"
result = test()
print(result)
"#;

		let executor = python(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_err());
		match result {
			Err(err) => {
				let err_msg = err.to_string();
				assert!(
					err_msg.contains("Last line of output is not a valid boolean: not a boolean")
				);
			}
			_ => panic!("Expected error"),
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

		let executor = python(script_content, vec![]);

		let input = create_mock_monitor_match();

		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_javascript_script_executor_success() {
		let script_content = r#"
		// Read input from stdin
		(async () => {
			let input = '';

			await new Promise((resolve, reject) => {
				process.stdin.on('data', (chunk) => {
					input += chunk;
				});

				process.stdin.on('end', resolve);

				process.stdin.on('error', reject);
			});

			try {
				const data = JSON.parse(input);
				console.log("debugging...");
				console.log("finished");
				console.log("true");
			} catch (err) {
				console.error(err);
			}
		})();
		"#;

		let executor = node(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &5000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_javascript_script_executor_invalid_output() {
		let script_content = r#"
		// Read input from stdin
		(async () => {
			let input = '';
			await new Promise((resolve, reject) => {
				process.stdin.on('data', chunk => input += chunk);
				process.stdin.on('end', resolve);
				process.stdin.on('error', reject);
			});

			try {
				JSON.parse(input);
				console.log("debugging...");
				console.log("finished");
				console.log("not a boolean");
			} catch (err) {
				console.log(err);
			}
		})();
		"#;

		let executor = node(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &5000, None, false).await;
		assert!(result.is_err());
		match result {
			Err(err) => {
				let err_msg = err.to_string();
				assert!(err_msg.contains("Last line of output is not a valid boolean"));
			}
			_ => panic!("Expected error"),
		}
	}

	#[tokio::test]
	async fn test_bash_script_executor_success() {
		let script_content = r#"
#!/bin/bash
set -e  # Exit on any error
input_json=$(cat)
sleep 0.1  # Small delay to ensure process startup
echo "debugging..."
echo "true"
"#;
		let executor = sh(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_bash_script_executor_invalid_output() {
		let script_content = r#"
#!/bin/bash
set -e  # Exit on any error
input_json=$(cat)
sleep 0.1  # Small delay to ensure process startup
echo "debugging..."
echo "not a boolean"
"#;

		let executor = sh(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_err());
		match result {
			Err(e) => {
				assert!(e
					.to_string()
					.contains("Last line of output is not a valid boolean"));
			}
			Ok(_) => {
				panic!("Expected ParseError, got success");
			}
		}
	}

	#[tokio::test]
	async fn test_script_executor_empty_output() {
		let script_content = r#"
import sys
input_json = sys.stdin.read()
# This script produces no output
"#;

		let executor = python(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;

		match result {
			Err(e) => {
				assert!(e.to_string().contains("Script produced no output"));
			}
			_ => panic!("Expected error"),
		}
	}

	#[tokio::test]
	async fn test_script_executor_whitespace_output() {
		let script_content = r#"
import sys
input_json = sys.stdin.read()
print("   ")
print("     true    ")  # Should handle whitespace correctly
"#;

		let executor = python(script_content, vec![]);

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

		let executor = python(script_content, vec![]);

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

		let executor = python(script_content, vec![]);

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

		let executor = python(script_content, vec![]);

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

		let executor = python(script_content, vec![]);

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

		let executor = python(script_content, vec![]);

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
		let executor = python(&script_content, vec![]);
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
		let executor = python(&script_content, vec![]);

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
import sys
input_json = sys.stdin.read()
"#;

		let executor = python(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, true).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_script_executor_with_non_zero_exit() {
		let script_content = r#"
import sys
input_json = sys.stdin.read()
sys.stderr.write("Error: something went wrong\n")
sys.exit(1)
		"#;

		let executor = python(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, true).await;

		assert!(result.is_err());
		match result {
			Err(e) => {
				assert!(e
					.to_string()
					.contains("Script execution failed: Error: something went wrong"));
			}
			_ => panic!("Expected ExecutionError"),
		}
	}

	#[tokio::test]
	async fn test_script_notify_succeeds_within_timeout() {
		let script_content = r#"
import sys
import time
input_json = sys.stdin.read()
time.sleep(0.3)
		"#;

		let executor = python(script_content, vec![]);

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
input_json = sys.stdin.read()
time.sleep(0.5)
		"#;

		let executor = python(script_content, vec![]);

		let input = create_mock_monitor_match();
		let start_time = Instant::now();
		let result = executor.execute(input, &400, None, true).await;
		let elapsed = start_time.elapsed();

		assert!(result.is_err());
		// Verify that execution took at least 300ms (the sleep time)
		assert!(elapsed.as_millis() >= 400 && elapsed.as_millis() < 600);
	}

	/// Proves that a script which outlives its timeout is actually killed, not just abandoned.
	/// Without `kill_on_drop(true)` the timed-out child keeps running after `execute` returns.
	/// The shell `exec`s `sleep`, so the direct child (the pid it wrote) is the `sleep` itself
	/// and the kill leaves no orphan. Its death is observed with `kill(pid, 0)`, which also
	/// succeeds for a zombie, so the poll waits for `ESRCH`: that arrives once tokio has reaped
	/// the killed child on `SIGCHLD`.
	#[cfg(unix)]
	#[tokio::test]
	async fn test_script_timeout_kills_child_process() {
		let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
		let pid_path = temp_dir.path().join("pid");
		let script = format!("echo $$ > '{}'; exec sleep 30", pid_path.display());

		let executor = sh(&script, vec![]);
		let input = create_mock_monitor_match();
		let handle = tokio::spawn(async move { executor.execute(input, &2000, None, false).await });

		// Wait for the shell to report its pid.
		let poll_deadline = Instant::now() + Duration::from_secs(5);
		let pid = loop {
			if let Ok(contents) = fs::read_to_string(&pid_path) {
				if !contents.trim().is_empty() {
					break contents
						.trim()
						.parse::<libc::pid_t>()
						.expect("pid file is not a number");
				}
			}
			assert!(
				Instant::now() < poll_deadline,
				"timed out waiting for the script to write its pid file"
			);
			tokio::time::sleep(Duration::from_millis(10)).await;
		};

		let result = handle.await.expect("execute task panicked");
		match result {
			Err(e) => assert!(e.to_string().contains("timed out"), "unexpected error: {e}"),
			Ok(v) => panic!("expected a timeout error, got Ok({v})"),
		}

		// Wait for the child to be gone: `kill(pid, 0)` fails with ESRCH once it is reaped.
		let poll_deadline = Instant::now() + Duration::from_secs(5);
		loop {
			// SAFETY: signal 0 performs no action; it only checks whether `pid` exists.
			let rc = unsafe { libc::kill(pid, 0) };
			let errno = (rc == -1)
				.then(|| std::io::Error::last_os_error().raw_os_error())
				.flatten();
			if errno == Some(libc::ESRCH) {
				break;
			}
			assert!(
				Instant::now() < poll_deadline,
				"timed-out script process {pid} still exists (kill returned {rc}, errno {errno:?})"
			);
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	}

	// ---------------------------------------------------------------------------
	// Regression tests for stdin/stdout/stderr handling and process lifecycle.
	//
	// Every scenario drives `execute()` with a stdin payload (via `args`) that is larger than
	// typical Linux pipe capacities (64 KiB by default), so a parent that writes stdin before
	// draining the child's output blocks deterministically. Each `execute()` call that can hang is wrapped in an
	// independent outer timeout so a reproduced deadlock fails the test instead of hanging the
	// suite; dropping the timed-out future kills the direct child through `kill_on_drop`.
	// ---------------------------------------------------------------------------

	/// Size of the stdin payload: well above typical pipe capacities (64 KiB on Linux by
	/// default, up to 1 MiB with `fs.pipe-max-size`).
	const LARGE_INPUT_BYTES: usize = 1024 * 1024;
	/// Size of the stdout/stderr flood a child produces before reading stdin.
	const OUTPUT_FLOOD_BYTES: usize = 256 * 1024;
	/// Script timeout handed to `execute()`.
	const SCRIPT_TIMEOUT_MS: u32 = 500;
	/// Independent safety net around `execute()` so a reproduced hang fails instead of blocking.
	const OUTER_TIMEOUT: Duration = Duration::from_secs(10);

	/// Builds the `args` for `execute()` so the serialized stdin payload is at least
	/// `LARGE_INPUT_BYTES` long.
	fn large_args() -> Vec<String> {
		vec!["x".repeat(LARGE_INPUT_BYTES)]
	}

	/// Runs `execute()` with the large payload under the outer timeout. Returns `None` when the
	/// outer timeout fired, i.e. `execute()` neither returned nor honored its own timeout.
	async fn execute_large_input(
		executor: InterpreterExecutor,
		timeout_ms: u32,
	) -> Option<Result<bool, anyhow::Error>> {
		let input = create_mock_monitor_match();
		let args = large_args();
		tokio::time::timeout(
			OUTER_TIMEOUT,
			executor.execute(input, &timeout_ms, Some(&args), false),
		)
		.await
		.ok()
	}

	/// Kills a process by pid on drop so a leaked descendant is cleaned up even when an
	/// assertion fails first. Signals the pid individually, never a process group, so the
	/// cleanup does not depend on the process-group behaviour under test.
	#[cfg(unix)]
	struct KillOnDrop(libc::pid_t);

	#[cfg(unix)]
	impl Drop for KillOnDrop {
		fn drop(&mut self) {
			// SAFETY: sending SIGKILL to a pid we started ourselves; a stale pid fails with ESRCH.
			unsafe {
				libc::kill(self.0, libc::SIGKILL);
			}
		}
	}

	/// Polls until the pid file written by a script is non-empty and returns the pid.
	#[cfg(unix)]
	async fn wait_for_pid_file(pid_path: &Path) -> libc::pid_t {
		let poll_deadline = Instant::now() + Duration::from_secs(5);
		loop {
			if let Ok(contents) = fs::read_to_string(pid_path) {
				if !contents.trim().is_empty() {
					return contents
						.trim()
						.parse::<libc::pid_t>()
						.expect("pid file is not a number");
				}
			}
			assert!(
				Instant::now() < poll_deadline,
				"timed out waiting for the script to write its pid file"
			);
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	}

	/// Aborts a spawned `execute()` task on drop. Dropping a `JoinHandle` detaches the task
	/// instead of cancelling it, so without this a panic before the handle is awaited (e.g. in
	/// `wait_for_pid_file`) or an outer-timeout expiry would leave `execute()` running.
	#[cfg(unix)]
	struct AbortOnDrop<T>(Option<tokio::task::JoinHandle<T>>);

	#[cfg(unix)]
	impl<T> AbortOnDrop<T> {
		fn new(handle: tokio::task::JoinHandle<T>) -> Self {
			Self(Some(handle))
		}

		/// Awaits the task under `OUTER_TIMEOUT`; the guard still aborts it if the timeout fires.
		async fn join(&mut self) -> Result<T, tokio::task::JoinError> {
			let handle = self.0.as_mut().expect("task already joined");
			tokio::time::timeout(OUTER_TIMEOUT, handle)
				.await
				.expect("execute() hung past the outer timeout")
		}
	}

	#[cfg(unix)]
	impl<T> Drop for AbortOnDrop<T> {
		fn drop(&mut self) {
			if let Some(handle) = self.0.take() {
				handle.abort();
			}
		}
	}

	/// Returns `errno` of `kill(pid, 0)`, or `None` when the process exists. `kill(pid, 0)`
	/// also succeeds for a zombie, so callers poll for `ESRCH` the way
	/// `test_script_timeout_kills_child_process` does, or combine it with [`is_zombie`].
	#[cfg(unix)]
	fn process_probe_errno(pid: libc::pid_t) -> Option<i32> {
		// SAFETY: signal 0 performs no action; it only checks whether `pid` exists.
		let rc = unsafe { libc::kill(pid, 0) };
		(rc == -1)
			.then(|| std::io::Error::last_os_error().raw_os_error())
			.flatten()
	}

	/// Returns `errno` of `killpg(pgid, 0)`, or `None` while the process group still has members.
	#[cfg(unix)]
	fn process_group_probe_errno(pgid: libc::pid_t) -> Option<i32> {
		// SAFETY: signal 0 performs no action; it only checks whether the group exists.
		let rc = unsafe { libc::killpg(pgid, 0) };
		(rc == -1)
			.then(|| std::io::Error::last_os_error().raw_os_error())
			.flatten()
	}

	/// Whether `pid` has terminated but not yet been reaped by its parent. Read from the state
	/// field of `/proc/<pid>/stat`: the single character after the closing parenthesis of the
	/// command name, which may itself contain spaces or parentheses.
	#[cfg(target_os = "linux")]
	fn is_zombie(pid: libc::pid_t) -> bool {
		fs::read_to_string(format!("/proc/{pid}/stat"))
			.ok()
			.and_then(|stat| {
				let rest = &stat[stat.rfind(')')? + 1..];
				rest.split_whitespace().next()?.chars().next()
			}) == Some('Z')
	}

	/// Without procfs a terminated process can only be observed once it is reaped (`ESRCH`).
	#[cfg(all(unix, not(target_os = "linux")))]
	fn is_zombie(_pid: libc::pid_t) -> bool {
		false
	}

	/// Test A: the script timeout must also cover the stdin write. The child stays alive
	/// without reading stdin, so the pipe fills and `write_all` blocks. With the write outside
	/// the timeout, `execute()` only returns once the child exits on its own.
	#[tokio::test]
	async fn test_large_stdin_child_never_reads_times_out() {
		let script_content = r#"
import time
time.sleep(3)
print(True)
"#;
		let executor = python(script_content, vec![]);

		let start = Instant::now();
		let result = execute_large_input(executor, SCRIPT_TIMEOUT_MS).await;
		let elapsed = start.elapsed();

		match result {
			None => panic!("execute() hung past the outer timeout: the stdin write is not bounded"),
			Some(Err(e)) => assert!(e.to_string().contains("timed out"), "unexpected error: {e}"),
			Some(Ok(v)) => panic!("expected a timeout error, got Ok({v})"),
		}
		// The child sleeps for 3 s; a timeout honored while writing stdin returns well before.
		assert!(
			elapsed < Duration::from_secs(2),
			"timeout of {SCRIPT_TIMEOUT_MS} ms was not applied to the stdin write (took {elapsed:?})"
		);
	}

	/// Test B: the child floods stdout before reading stdin. The parent must write stdin and
	/// drain stdout concurrently, otherwise both sides block on a full pipe.
	#[tokio::test]
	async fn test_large_stdin_child_floods_stdout_first() {
		let script_content = format!(
			r#"
import sys
sys.stdout.write("y" * {OUTPUT_FLOOD_BYTES})
sys.stdout.write("\n")
sys.stdout.flush()
sys.stdin.read()
print(True)
"#
		);
		let executor = python(&script_content, vec![]);

		let result = execute_large_input(executor, 5000).await;
		match result {
			None => panic!("execute() deadlocked: stdin write and stdout drain are sequential"),
			Some(Ok(true)) => {}
			Some(other) => panic!("expected Ok(true), got {other:?}"),
		}
	}

	/// Test C: same as Test B with the flood on stderr, so both output pipes must be drained.
	#[tokio::test]
	async fn test_large_stdin_child_floods_stderr_first() {
		let script_content = format!(
			r#"
import sys
sys.stderr.write("e" * {OUTPUT_FLOOD_BYTES})
sys.stderr.write("\n")
sys.stderr.flush()
sys.stdin.read()
print(True)
"#
		);
		let executor = python(&script_content, vec![]);

		let result = execute_large_input(executor, 5000).await;
		match result {
			None => panic!("execute() deadlocked: stdin write and stderr drain are sequential"),
			Some(Ok(true)) => {}
			Some(other) => panic!("expected Ok(true), got {other:?}"),
		}
	}

	/// Test D: the child exits successfully without reading stdin. The payload exceeds the
	/// pipe capacity, so the parent's blocked write fails with `BrokenPipe` once the child is
	/// gone. That must not mask the child's real result.
	#[tokio::test]
	async fn test_large_stdin_early_exit_success_is_reported() {
		let executor = sh("echo true", vec![]);

		let result = execute_large_input(executor, 5000).await;
		match result {
			None => panic!("execute() hung past the outer timeout"),
			Some(Ok(true)) => {}
			Some(other) => panic!("expected Ok(true), got {other:?}"),
		}
	}

	/// Test G: counterpart of Test D. The child exits with a non-zero status without reading
	/// stdin. Tolerating `BrokenPipe` must not turn a failing script into a success: the
	/// error has to come from `process_script_output`, not from the stdin write.
	#[tokio::test]
	async fn test_large_stdin_early_exit_failure_is_reported() {
		let executor = sh("echo oops >&2; exit 3", vec![]);

		let result = execute_large_input(executor, 5000).await;
		match result {
			None => panic!("execute() hung past the outer timeout"),
			Some(Ok(v)) => panic!("expected a script execution error, got Ok({v})"),
			Some(Err(e)) => {
				let msg = e.to_string();
				assert!(
					msg.contains("Script execution failed") && msg.contains("oops"),
					"expected the child's non-zero status to be reported, got: {msg}"
				);
			}
		}
	}

	/// Test F: positive control. A large payload on its own is fine when the child reads
	/// stdin to EOF before producing output; the deadlocks above depend on the child's I/O
	/// order, not on payload size.
	#[tokio::test]
	async fn test_large_stdin_child_reads_all_input() {
		let script_content = r#"
import sys
sys.stdin.read()
print(True)
"#;
		let executor = python(script_content, vec![]);

		let result = execute_large_input(executor, 5000).await;
		match result {
			None => panic!("execute() hung past the outer timeout"),
			Some(Ok(true)) => {}
			Some(other) => panic!("expected Ok(true), got {other:?}"),
		}
	}

	/// Test E: `kill_on_drop(true)` only kills the direct child. The shell starts a `sleep`
	/// in the background and waits for it; on timeout the shell dies but the `sleep` is
	/// orphaned. After the fix the whole process group is terminated, so the descendant must be
	/// gone too. It is reaped by its new parent (init or a subreaper), not by us, and in a
	/// container without an init process it can stay a zombie; the descendant is therefore
	/// accepted when it is either gone (`ESRCH`) or a zombie. The descendant's pid is
	/// signalled individually for cleanup; the test never signals a process group (see
	/// [`KillOnDrop`]).
	#[cfg(unix)]
	#[tokio::test]
	async fn test_script_timeout_kills_descendant_process() {
		let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
		let pid_path = temp_dir.path().join("pid");
		let script = format!("sleep 30 & echo $! > '{}'; wait", pid_path.display());

		// Generous so the pid file exists before the timeout fires on a loaded CI runner.
		const DESCENDANT_TIMEOUT_MS: u32 = 2000;
		let executor = sh(&script, vec![]);
		let input = create_mock_monitor_match();
		let mut task = AbortOnDrop::new(tokio::spawn(async move {
			executor
				.execute(input, &DESCENDANT_TIMEOUT_MS, None, false)
				.await
		}));

		let descendant_pid = wait_for_pid_file(&pid_path).await;
		let _cleanup = KillOnDrop(descendant_pid);

		let result = task.join().await.expect("execute task panicked");
		match result {
			Err(e) => assert!(e.to_string().contains("timed out"), "unexpected error: {e}"),
			Ok(v) => panic!("expected a timeout error, got Ok({v})"),
		}

		// Poll briefly to allow the kill to land: the descendant is gone once `kill(pid, 0)`
		// fails with `ESRCH`, or once it has terminated and only awaits reaping (zombie).
		let poll_deadline = Instant::now() + Duration::from_secs(5);
		loop {
			let errno = process_probe_errno(descendant_pid);
			if errno == Some(libc::ESRCH) || (errno.is_none() && is_zombie(descendant_pid)) {
				break;
			}
			assert!(
				Instant::now() < poll_deadline,
				"descendant process {descendant_pid} survived the script timeout (errno {errno:?}): \
				 only the direct child was killed"
			);
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	}

	/// Counterpart of `test_script_timeout_kills_descendant_process` for external cancellation.
	/// In `create_trigger_handler` a shutdown signal drops the trigger-processing future, and
	/// the cancellation reaches `process_command` while the script is still running. Dropping
	/// the future releases the `Child`, and `kill_on_drop(true)` kills the direct interpreter
	/// process, but not the `sleep` it started. After the fix the whole process group is
	/// terminated on cancellation as well, so both processes must be gone or zombies.
	///
	/// The script timeout is long enough that it cannot fire first; the cancellation comes from
	/// `JoinHandle::abort`. No large stdin payload is used so this isolates cancellation from
	/// the stdin deadlock scenarios. Tokio attempts to reap the direct child asynchronously; the
	/// test also accepts a terminated but unreaped (zombie) process. The descendant is reaped
	/// by init or a subreaper, so it may briefly be a zombie too.
	/// Pids are signalled individually for cleanup; never a process group.
	#[cfg(unix)]
	#[tokio::test]
	async fn test_script_cancellation_kills_descendant_process() {
		let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
		let shell_pid_path = temp_dir.path().join("shell_pid");
		let descendant_pid_path = temp_dir.path().join("descendant_pid");
		let script = format!(
			"echo $$ > '{}'; sleep 30 & echo $! > '{}'; wait",
			shell_pid_path.display(),
			descendant_pid_path.display()
		);

		let executor = sh(&script, vec![]);
		let input = create_mock_monitor_match();
		let mut task = AbortOnDrop::new(tokio::spawn(async move {
			executor.execute(input, &30_000, None, false).await
		}));

		// The shell writes its own pid before starting the `sleep`, so guard it first; a
		// failure while waiting for the descendant's pid then still cleans up the shell.
		let shell_pid = wait_for_pid_file(&shell_pid_path).await;
		let _shell_cleanup = KillOnDrop(shell_pid);
		let descendant_pid = wait_for_pid_file(&descendant_pid_path).await;
		let _descendant_cleanup = KillOnDrop(descendant_pid);

		// Both processes must be running (not just present as zombies) before cancelling.
		for (name, pid) in [("direct child", shell_pid), ("descendant", descendant_pid)] {
			assert_eq!(
				process_probe_errno(pid),
				None,
				"{name} {pid} was not running before cancellation"
			);
			assert!(
				!is_zombie(pid),
				"{name} {pid} was already a zombie before cancellation"
			);
		}

		// Cancel the execution from the outside; the guard keeps the handle while waiting.
		task.0.as_ref().expect("task already joined").abort();
		let join_result = task.join().await;
		match join_result {
			Err(e) if e.is_cancelled() => {}
			Err(e) => panic!("expected the task to be cancelled, got join error: {e}"),
			Ok(result) => panic!("expected the task to be cancelled, got {result:?}"),
		}

		// A process counts as terminated once `kill(pid, 0)` fails with `ESRCH` or it has
		// become a zombie that only awaits reaping.
		let terminated = |pid: libc::pid_t| {
			let errno = process_probe_errno(pid);
			errno == Some(libc::ESRCH) || (errno.is_none() && is_zombie(pid))
		};
		let poll_deadline = Instant::now() + Duration::from_secs(5);
		loop {
			let shell_terminated = terminated(shell_pid);
			let descendant_terminated = terminated(descendant_pid);
			if shell_terminated && descendant_terminated {
				break;
			}
			if Instant::now() >= poll_deadline {
				assert!(
					shell_terminated,
					"direct child {shell_pid} survived external cancellation (errno {:?})",
					process_probe_errno(shell_pid)
				);
				assert!(
					descendant_terminated,
					"descendant process {descendant_pid} survived external cancellation (errno \
					 {:?}): only the direct child was killed",
					process_probe_errno(descendant_pid)
				);
			}
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	}

	/// The interpreter exits successfully, but a descendant it started inherits the stdin read
	/// end and never reads it, so the parent's large stdin write stays blocked. The direct
	/// child is gone and both output pipes have reached EOF (the descendant's go to
	/// `/dev/null`), yet the execution cannot complete: the timeout must still fire and must
	/// still terminate the descendant. This distinguishes cleanup that stays armed until stdin
	/// delivery has completed from cleanup that disarms as soon as the direct child has been
	/// waited for, which would leak the descendant here.
	///
	/// The timeout covers stdin delivery even in this state: the write cannot block until the
	/// descendant exits, and `wait_with_output` reaps the interpreter before the timeout fires.
	/// Pids are signalled individually for cleanup; never a process group.
	#[cfg(unix)]
	#[tokio::test]
	async fn test_timeout_after_interpreter_exit_kills_descendant() {
		let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
		let interpreter_pid_path = temp_dir.path().join("interpreter_pid");
		let descendant_pid_path = temp_dir.path().join("descendant_pid");
		let script_content = format!(
			r#"
import os
import subprocess
import sys
with open("{}", "w") as f:
    f.write(str(os.getpid()))
p = subprocess.Popen(["sleep", "30"], stdin=sys.stdin, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
with open("{}", "w") as f:
    f.write(str(p.pid))
print("true", flush=True)
"#,
			interpreter_pid_path.display(),
			descendant_pid_path.display()
		);

		const SCRIPT_TIMEOUT: u32 = 3000;
		let executor = python(&script_content, vec![]);
		let input = create_mock_monitor_match();
		let args = large_args();
		let spawned_at = Instant::now();
		let mut task = AbortOnDrop::new(tokio::spawn(async move {
			executor
				.execute(input, &SCRIPT_TIMEOUT, Some(&args), false)
				.await
		}));

		let interpreter_pid = wait_for_pid_file(&interpreter_pid_path).await;
		let descendant_pid = wait_for_pid_file(&descendant_pid_path).await;
		let _descendant_cleanup = KillOnDrop(descendant_pid);

		// A process counts as terminated once `kill(pid, 0)` fails with `ESRCH` or it has
		// become a zombie that only awaits reaping.
		let terminated = |pid: libc::pid_t| {
			let errno = process_probe_errno(pid);
			errno == Some(libc::ESRCH) || (errno.is_none() && is_zombie(pid))
		};

		// Precondition 1: the interpreter has exited. `wait_with_output` runs concurrently with
		// the stdin write, so it reaps the interpreter even though that write is still blocked
		// on the descendant.
		let poll_deadline = Instant::now() + Duration::from_secs(5);
		while !terminated(interpreter_pid) {
			assert!(
				Instant::now() < poll_deadline,
				"interpreter {interpreter_pid} did not exit after printing its result (errno {:?})",
				process_probe_errno(interpreter_pid)
			);
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
		// Precondition 2: the descendant is running and holds the stdin read end.
		assert_eq!(
			process_probe_errno(descendant_pid),
			None,
			"descendant {descendant_pid} is not running"
		);
		assert!(
			!is_zombie(descendant_pid),
			"descendant {descendant_pid} is already a zombie"
		);
		// Precondition 3: the execution is still pending although the interpreter is gone.
		assert!(
			!task.0.as_ref().expect("task already joined").is_finished(),
			"execute() completed although the descendant still holds stdin open"
		);

		let result = task.join().await.expect("execute task panicked");
		let elapsed = spawned_at.elapsed();
		match result {
			Err(e) => assert!(e.to_string().contains("timed out"), "unexpected error: {e}"),
			Ok(v) => panic!("expected a timeout error, got Ok({v})"),
		}
		assert!(
			elapsed < Duration::from_secs(5),
			"timeout of {SCRIPT_TIMEOUT} ms was not honored while the descendant held stdin (took {elapsed:?})"
		);

		// The descendant must be terminated by the timeout even though the direct child had
		// already exited.
		let poll_deadline = Instant::now() + Duration::from_secs(5);
		while !terminated(descendant_pid) {
			assert!(
				Instant::now() < poll_deadline,
				"descendant process {descendant_pid} survived the script timeout (errno {:?}): \
				 cleanup stopped once the direct child had exited",
				process_probe_errno(descendant_pid)
			);
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	}

	/// Test D on the custom-notification path, which skips stdout parsing: a child that exits
	/// successfully without reading the large payload is still reported as `Ok(true)`.
	#[tokio::test]
	async fn test_large_stdin_early_exit_success_notification_path() {
		let executor = sh("echo true", vec![]);
		let input = create_mock_monitor_match();
		let args = large_args();

		let result = tokio::time::timeout(
			OUTER_TIMEOUT,
			executor.execute(input, &5000, Some(&args), true),
		)
		.await
		.ok();
		match result {
			None => panic!("execute() hung past the outer timeout"),
			Some(Ok(true)) => {}
			Some(other) => panic!("expected Ok(true), got {other:?}"),
		}
	}

	/// The script runs as the leader of its own process group, distinct from the Monitor's,
	/// so killing that group on timeout or cancellation never reaches the Monitor itself.
	#[cfg(unix)]
	#[tokio::test]
	async fn test_script_process_group_is_isolated() {
		let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
		let pid_path = temp_dir.path().join("pid");
		let script = format!("echo $$ > '{}'; exec sleep 30", pid_path.display());

		// Generous so the `getpgid` check below runs before the timeout fires on a loaded CI
		// runner.
		const SCRIPT_TIMEOUT: u32 = 2000;
		let executor = sh(&script, vec![]);
		let input = create_mock_monitor_match();
		let mut task = AbortOnDrop::new(tokio::spawn(async move {
			executor.execute(input, &SCRIPT_TIMEOUT, None, false).await
		}));

		let pid = wait_for_pid_file(&pid_path).await;
		let _cleanup = KillOnDrop(pid);

		// SAFETY: `getpgid` and `getpgrp` only query process-group ids.
		let (script_pgid, own_pgid) = unsafe { (libc::getpgid(pid), libc::getpgrp()) };
		assert_eq!(
			script_pgid, pid,
			"script process {pid} is not the leader of its own process group"
		);
		assert_ne!(
			script_pgid, own_pgid,
			"script process {pid} shares the test runner's process group {own_pgid}"
		);

		let result = task.join().await.expect("execute task panicked");
		match result {
			Err(e) => assert!(e.to_string().contains("timed out"), "unexpected error: {e}"),
			Ok(v) => panic!("expected a timeout error, got Ok({v})"),
		}
	}

	/// Documents the deliberately kept semantics after a successful execution: nothing is
	/// killed, so a background process the script leaves behind keeps running. Whether the
	/// Monitor should tear down the group after a normal exit is raised in the PR.
	#[cfg(unix)]
	#[tokio::test]
	async fn test_script_background_process_outlives_successful_exit() {
		let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
		let pid_path = temp_dir.path().join("pid");
		let script = format!(
			"sleep 30 >/dev/null 2>&1 & echo $! > '{}'; echo true",
			pid_path.display()
		);

		let executor = sh(&script, vec![]);
		let input = create_mock_monitor_match();
		let mut task = AbortOnDrop::new(tokio::spawn(async move {
			executor.execute(input, &5000, None, false).await
		}));

		// Register the cleanup before asserting on the result, so a failed assertion does not
		// leave the `sleep` running for 30 s.
		let background_pid = wait_for_pid_file(&pid_path).await;
		let _cleanup = KillOnDrop(background_pid);

		let result = task.join().await.expect("execute task panicked");
		match result {
			Ok(true) => {}
			other => panic!("expected Ok(true), got {other:?}"),
		}

		assert_eq!(
			process_probe_errno(background_pid),
			None,
			"background process {background_pid} did not survive the script's successful exit"
		);
		assert!(
			!is_zombie(background_pid),
			"background process {background_pid} is a zombie after the script's successful exit"
		);
	}

	/// Spawns `sh -c <script>` as the leader of its own process group, the way
	/// [`InterpreterExecutor::execute`] spawns scripts, so a guard built from its pid targets
	/// only that group.
	#[cfg(unix)]
	fn spawn_group_leader(script: &str) -> std::process::Child {
		use std::os::unix::process::CommandExt;

		std::process::Command::new("sh")
			.args(["-c", script])
			.process_group(0)
			.stdin(std::process::Stdio::null())
			.stdout(std::process::Stdio::null())
			.stderr(std::process::Stdio::null())
			.spawn()
			.expect("failed to spawn sh")
	}

	/// `killpg(0)` would signal the Monitor's own process group, so `new` must reject a zero
	/// pid instead of arming a guard for it.
	#[cfg(unix)]
	#[test]
	#[should_panic(expected = "killpg(0) would signal the Monitor's own group")]
	fn test_process_group_guard_rejects_zero_pid() {
		let _guard = ProcessGroupGuard::new(0);
	}

	/// `kill()` signals the group once and disarms the guard; a second call, after the group
	/// has been killed, is a no-op and must not fail on the stale pgid.
	#[cfg(unix)]
	#[test]
	fn test_process_group_guard_kill_is_idempotent() {
		use std::os::unix::process::ExitStatusExt;

		let mut child = spawn_group_leader("exec sleep 30");
		let pid = libc::pid_t::try_from(child.id()).expect("pid does not fit pid_t");
		let _cleanup = KillOnDrop(pid);

		let mut guard = ProcessGroupGuard::new(child.id());
		assert!(guard.armed, "a valid pid did not arm the guard");
		guard.kill();
		assert!(!guard.armed, "kill() left the guard armed");

		// The child is our own, so `try_wait` both observes its death and reaps it.
		let poll_deadline = Instant::now() + Duration::from_secs(5);
		let status = loop {
			if let Some(status) = child.try_wait().expect("failed to poll child") {
				break status;
			}
			assert!(
				Instant::now() < poll_deadline,
				"process {pid} survived the process-group kill"
			);
			std::thread::sleep(Duration::from_millis(10));
		};
		assert_eq!(
			status.signal(),
			Some(libc::SIGKILL),
			"process {pid} did not die from the guard's signal: {status}"
		);

		// The group is gone; a second kill() must be a no-op on the stale pgid.
		guard.kill();
		assert!(!guard.armed, "a second kill() re-armed the guard");
	}

	/// A guard whose process group has already exited takes the `ESRCH` branch of `kill()`
	/// and must not panic. Pid reuse between the `wait()` and the `kill()` is practically
	/// impossible within the microseconds the test takes.
	#[cfg(unix)]
	#[test]
	fn test_process_group_guard_tolerates_exited_group() {
		let mut child = spawn_group_leader("true");
		let pid = child.id();
		child.wait().expect("failed to reap child");
		let pgid = libc::pid_t::try_from(pid).expect("pid does not fit pid_t");
		assert_eq!(
			process_group_probe_errno(pgid),
			Some(libc::ESRCH),
			"process group {pgid} still exists after its leader was reaped"
		);

		let mut guard = ProcessGroupGuard::new(pid);
		assert!(guard.armed, "a valid pid did not arm the guard");
		guard.kill();
		assert!(!guard.armed, "kill() left the guard armed");
	}

	#[tokio::test]
	async fn test_script_python_fails_with_non_zero_exit() {
		let script_content = r#"
import sys
import time
input_json = sys.stdin.read()
print("This is a python test error message!", file=sys.stderr)
sys.exit(1)
		"#;

		let executor = python(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;

		assert!(result.is_err());
		match result {
			Err(e) => {
				assert!(e
					.to_string()
					.contains("Script execution failed: This is a python test error message!"));
			}
			_ => panic!("Expected ExecutionError"),
		}
	}

	#[tokio::test]
	async fn test_script_javascript_fails_with_non_zero_exit() {
		let script_content = r#"
		// Read input from stdin
		let input = '';
		process.stdin.on('data', (chunk) => {
			input += chunk;
		});

		process.stdin.on('end', () => {
			// Parse and validate input
			try {
				const data = JSON.parse(input);
				console.error("This is a JS test error message!");
				process.exit(1);
			} catch (err) {
				console.error(err);
				process.exit(1);
			}
		});
		"#;

		let executor = node(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;

		assert!(result.is_err());
		match result {
			Err(e) => {
				assert!(e
					.to_string()
					.contains("Script execution failed: This is a JS test error message!"));
			}
			_ => panic!("Expected ExecutionError"),
		}
	}

	#[tokio::test]
	async fn test_script_bash_fails_with_non_zero_exit() {
		let script_content = r#"
#!/bin/bash
# Read input from stdin
input_json=$(cat)
echo "This is a bash test error message!" >&2
exit 1
"#;

		let executor = sh(script_content, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_err());
		match result {
			Err(e) => {
				assert!(e
					.to_string()
					.contains("Script execution failed: This is a bash test error message!"));
			}
			_ => panic!("Expected ExecutionError"),
		}
	}

	// The test observes `process.execArgv` instead of relying on behaviour such as ES module
	// syntax detection, which changes between Node.js versions (enabled by default from 22.7).
	const JS_EXEC_ARGV_SCRIPT: &str = r#"
let input = '';
process.stdin.on('data', (chunk) => {
	input += chunk;
});
process.stdin.on('end', () => {
	JSON.parse(input);
	console.log(process.execArgv.includes('--max-old-space-size=4096') ? "true" : "false");
});
"#;

	#[tokio::test]
	async fn test_javascript_script_executor_with_runtime_arguments() {
		let executor = node(
			JS_EXEC_ARGV_SCRIPT,
			vec!["--max-old-space-size=4096".to_string()],
		);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &5000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_javascript_script_executor_without_runtime_arguments() {
		let executor = node(JS_EXEC_ARGV_SCRIPT, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &5000, None, false).await;
		assert!(result.is_ok());
		assert!(!result.unwrap());
	}

	const PYTHON_WARNING_SCRIPT: &str = r#"
import sys
import warnings
sys.stdin.read()
warnings.warn("deprecated")
print("true")
"#;

	#[tokio::test]
	async fn test_python_script_executor_with_runtime_arguments() {
		// `-W error` turns the warning into an exception, so the script fails
		let executor = python(
			PYTHON_WARNING_SCRIPT,
			vec!["-W".to_string(), "error".to_string()],
		);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Script execution failed"));
	}

	#[tokio::test]
	async fn test_python_script_executor_without_runtime_arguments_ignores_warning() {
		let executor = python(PYTHON_WARNING_SCRIPT, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	const BASH_FAILING_COMMAND_SCRIPT: &str = r#"
input_json=$(cat)
false
echo "true"
"#;

	#[tokio::test]
	async fn test_bash_script_executor_with_runtime_arguments() {
		// `-e` makes the shell exit on the first failing command
		let executor = sh(BASH_FAILING_COMMAND_SCRIPT, vec!["-e".to_string()]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Script execution failed"));
	}

	#[tokio::test]
	async fn test_bash_script_executor_without_runtime_arguments_continues_after_failure() {
		let executor = sh(BASH_FAILING_COMMAND_SCRIPT, vec![]);

		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[tokio::test]
	async fn test_interpreter_executor_unknown_program_reports_program_name() {
		let executor = interpreter("definitely-not-a-program", "-c", "true", vec![]);
		let input = create_mock_monitor_match();
		let result = executor.execute(input, &1000, None, false).await;
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert!(
			err.contains("Failed to spawn definitely-not-a-program process"),
			"unexpected error: {}",
			err
		);
	}
}
