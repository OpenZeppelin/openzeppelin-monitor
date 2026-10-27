//! Regression tests for graceful shutdown on Unix termination signals.
//!
//! See <https://github.com/OpenZeppelin/openzeppelin-monitor/issues/522>.
//!
//! Each test spawns a real `openzeppelin-monitor` subprocess, waits until it
//! reports readiness, sends it exactly one signal and then asserts that the
//! process went through the common graceful shutdown path in `main` (exit code
//! 0 and `"Shutdown complete"` logged). `SIGINT` acts as the positive control,
//! while `SIGTERM` and `SIGHUP` exercise the bug reported in #522: only
//! `tokio::signal::ctrl_c()` is registered, so those two signals terminate the
//! process through the default signal action instead.
//!
//! The metrics server is deliberately disabled (`METRICS_ENABLED=false`, no
//! `--metrics` flag): Actix installs its own `SIGINT`/`SIGTERM`/`SIGQUIT`
//! handlers and would resolve the metrics future on `SIGTERM`, which enters the
//! graceful path through the other `select!` arm and masks the bug.
//!
//! Scope: these tests verify only that a termination signal reaches the
//! graceful shutdown path. They do not spawn trigger scripts and make no
//! assertions about script processes. Script cancellation is covered
//! separately by the PR #520 tests in `src/services/trigger/script/executor.rs`,
//! which check that `ProcessGroupGuard` kills a script's whole process group
//! when the script future times out, fails on I/O or is cancelled (for example
//! when the runtime is torn down after `main` returns). The two are related
//! because death by the default signal action runs no destructors at all, so
//! without the fix for #522 that guard is never dropped on `SIGTERM`/`SIGHUP`.
#![cfg(unix)]

use std::{
	os::unix::process::ExitStatusExt,
	process::{ExitStatus, Stdio},
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};

use mockito::Server;
use tokio::{
	io::{AsyncBufReadExt, AsyncRead, BufReader},
	process::{Child, Command},
	sync::mpsc,
	task::JoinHandle,
};

use crate::integration::mocks::create_evm_valid_server_mock_network_response;

/// Prefix of the log line that marks the service as ready to receive signals.
const READINESS_MARKER: &str = "Service started";
/// Log line emitted at the end of the common graceful shutdown path.
const SHUTDOWN_COMPLETE_MARKER: &str = "Shutdown complete";

const READINESS_TIMEOUT: Duration = Duration::from_secs(30);
/// The graceful path contains a fixed 1 s sleep, so give it ample headroom.
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
const PRE_SIGNAL_GRACE: Duration = Duration::from_millis(250);

type OutputBuffer = Arc<Mutex<Vec<String>>>;

/// Everything captured from the child process after it has exited.
struct ChildOutcome {
	status: ExitStatus,
	stdout: Vec<String>,
	stderr: Vec<String>,
	wall_time: Duration,
}

impl ChildOutcome {
	fn diagnostics(&self) -> String {
		format!(
			"status={:?} code={:?} signal={:?} wall_time={:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
			self.status,
			self.status.code(),
			self.status.signal(),
			self.wall_time,
			self.stdout.join("\n"),
			self.stderr.join("\n"),
		)
	}
}

fn snapshot(buffer: &OutputBuffer) -> String {
	buffer.lock().expect("output buffer poisoned").join("\n")
}

fn write_minimal_config(root: &std::path::Path, rpc_url: &str) {
	let networks_dir = root.join("config").join("networks");
	let monitors_dir = root.join("config").join("monitors");
	let triggers_dir = root.join("config").join("triggers");
	for dir in [&networks_dir, &monitors_dir, &triggers_dir] {
		std::fs::create_dir_all(dir).expect("failed to create config directory");
	}

	// Follows examples/config/networks/anvil_local.json. The cron schedule never
	// fires during the test, so the block watcher starts but makes no RPC calls
	// beyond the `net_version` probe answered by the mockito server.
	let network = format!(
		r#"{{
  "network_type": "EVM",
  "slug": "shutdown_test",
  "name": "Shutdown Test (Mock)",
  "rpc_urls": [
    {{
      "type_": "rpc",
      "url": {{ "type": "plain", "value": "{rpc_url}" }},
      "weight": 100
    }}
  ],
  "chain_id": 31337,
  "block_time_ms": 1000,
  "confirmation_blocks": 1,
  "cron_schedule": "0 0 0 1 1 *",
  "max_past_blocks": 10,
  "store_blocks": false
}}
"#
	);
	std::fs::write(networks_dir.join("shutdown_test.json"), network)
		.expect("failed to write network config");

	let monitor = r#"{
  "name": "Shutdown Test Monitor",
  "paused": false,
  "networks": ["shutdown_test"],
  "addresses": [],
  "match_conditions": {
    "functions": [],
    "events": [],
    "transactions": []
  },
  "trigger_conditions": [],
  "triggers": []
}
"#;
	std::fs::write(monitors_dir.join("shutdown_test.json"), monitor)
		.expect("failed to write monitor config");
}

/// Reads `pipe` line by line until EOF, appending every line to `buffer` and
/// forwarding a copy to `lines` (if the receiver is still alive).
fn spawn_reader<R>(
	pipe: R,
	buffer: OutputBuffer,
	lines: Option<mpsc::UnboundedSender<String>>,
) -> JoinHandle<()>
where
	R: AsyncRead + Unpin + Send + 'static,
{
	tokio::spawn(async move {
		let mut reader = BufReader::new(pipe).lines();
		while let Ok(Some(line)) = reader.next_line().await {
			buffer
				.lock()
				.expect("output buffer poisoned")
				.push(line.clone());
			if let Some(tx) = &lines {
				// The receiver may already have been dropped after readiness.
				let _ = tx.send(line);
			}
		}
	})
}

async fn kill_and_reap(child: &mut Child) {
	let _ = child.start_kill();
	let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
}

/// Best-effort wait for the reader tasks after the child has been killed, so
/// that panic diagnostics include output that was still in flight. Errors and
/// timeouts are ignored: the caller is about to panic anyway.
async fn drain_readers(stdout_task: &mut JoinHandle<()>, stderr_task: &mut JoinHandle<()>) {
	let _ = tokio::time::timeout(DRAIN_TIMEOUT, async {
		let _ = tokio::join!(stdout_task, stderr_task);
	})
	.await;
}

fn send_signal(child: &Child, signal: libc::c_int) {
	let raw_pid = child
		.id()
		.expect("child has no pid; it must still be running when signalled");
	let pid = libc::pid_t::try_from(raw_pid).expect("pid does not fit in pid_t");
	// Never signal a process group (negative or zero pid): the test runner shares
	// the child's process group and would be killed as well.
	assert!(pid > 0, "refusing to signal non-positive pid {pid}");
	// SAFETY: `kill` has no memory-safety preconditions; `pid` is the positive id
	// of a child process we own and have not yet reaped.
	let rc = unsafe { libc::kill(pid, signal) };
	if rc == -1 {
		panic!(
			"kill({pid}, {signal}) failed: {}",
			std::io::Error::last_os_error()
		);
	}
}

/// Spawns the monitor binary against a mocked RPC endpoint, waits until it is
/// ready, sends `signal` once and returns the exit status and captured output.
async fn run_monitor_and_signal(signal: libc::c_int) -> ChildOutcome {
	let started = Instant::now();

	let mut server = Server::new_async().await;
	let _net_version_mock = create_evm_valid_server_mock_network_response(&mut server);

	// Kept alive until the end of the test so the config tree outlives the child.
	let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
	write_minimal_config(temp_dir.path(), &server.url());

	let mut child = Command::new(env!("CARGO_BIN_EXE_openzeppelin-monitor"))
		.current_dir(temp_dir.path())
		.env("LOG_MODE", "stdout")
		.env("LOG_LEVEL", "info")
		.env("METRICS_ENABLED", "false")
		.env_remove("RUST_LOG")
		.env_remove("IN_DOCKER")
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.kill_on_drop(true)
		.spawn()
		.expect("failed to spawn openzeppelin-monitor");

	let stdout_buf: OutputBuffer = Arc::default();
	let stderr_buf: OutputBuffer = Arc::default();
	let (stdout_tx, mut stdout_rx) = mpsc::unbounded_channel::<String>();

	let mut stdout_task = spawn_reader(
		child.stdout.take().expect("stdout not piped"),
		Arc::clone(&stdout_buf),
		Some(stdout_tx),
	);
	let mut stderr_task = spawn_reader(
		child.stderr.take().expect("stderr not piped"),
		Arc::clone(&stderr_buf),
		None,
	);

	// Wait for readiness: the first stdout line containing the marker prefix.
	let readiness = tokio::time::timeout(READINESS_TIMEOUT, async {
		while let Some(line) = stdout_rx.recv().await {
			if line.contains(READINESS_MARKER) {
				return true;
			}
		}
		false
	})
	.await;
	drop(stdout_rx);

	match readiness {
		Ok(true) => {}
		Ok(false) => {
			kill_and_reap(&mut child).await;
			drain_readers(&mut stdout_task, &mut stderr_task).await;
			panic!(
				"child stdout closed before readiness marker {READINESS_MARKER:?} appeared\n--- stdout ---\n{}\n--- stderr ---\n{}",
				snapshot(&stdout_buf),
				snapshot(&stderr_buf),
			);
		}
		Err(_) => {
			kill_and_reap(&mut child).await;
			drain_readers(&mut stdout_task, &mut stderr_task).await;
			panic!(
				"timed out after {READINESS_TIMEOUT:?} waiting for readiness marker {READINESS_MARKER:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
				snapshot(&stdout_buf),
				snapshot(&stderr_buf),
			);
		}
	}

	// Give the child a moment after the readiness line before signalling. Before
	// #522 the SIGINT handler was registered lazily, after that line was logged;
	// the fix registers eagerly, so this grace period is now belt and braces.
	tokio::time::sleep(PRE_SIGNAL_GRACE).await;

	send_signal(&child, signal);

	let status = match tokio::time::timeout(EXIT_TIMEOUT, child.wait()).await {
		Ok(Ok(status)) => status,
		Ok(Err(e)) => {
			kill_and_reap(&mut child).await;
			drain_readers(&mut stdout_task, &mut stderr_task).await;
			panic!(
				"failed to wait for child after signal {signal}: {e}\n--- stdout ---\n{}\n--- stderr ---\n{}",
				snapshot(&stdout_buf),
				snapshot(&stderr_buf),
			);
		}
		Err(_) => {
			kill_and_reap(&mut child).await;
			drain_readers(&mut stdout_task, &mut stderr_task).await;
			panic!(
				"child did not exit within {EXIT_TIMEOUT:?} after signal {signal}\n--- stdout ---\n{}\n--- stderr ---\n{}",
				snapshot(&stdout_buf),
				snapshot(&stderr_buf),
			);
		}
	};

	// The child is gone, so both pipes reach EOF and the readers finish promptly.
	// Await them before inspecting the buffers, otherwise trailing output (such
	// as the final "Shutdown complete" line) could still be in flight.
	let drained = tokio::time::timeout(DRAIN_TIMEOUT, async {
		let (a, b) = tokio::join!(stdout_task, stderr_task);
		a.and(b)
	})
	.await;
	match drained {
		Ok(Ok(())) => {}
		Ok(Err(e)) => panic!(
			"output reader task panicked: {e}\n--- stdout ---\n{}\n--- stderr ---\n{}",
			snapshot(&stdout_buf),
			snapshot(&stderr_buf),
		),
		Err(_) => panic!(
			"output reader tasks did not finish within {DRAIN_TIMEOUT:?} after child exit\n--- stdout ---\n{}\n--- stderr ---\n{}",
			snapshot(&stdout_buf),
			snapshot(&stderr_buf),
		),
	}

	let stdout = stdout_buf.lock().expect("output buffer poisoned").clone();
	let stderr = stderr_buf.lock().expect("output buffer poisoned").clone();

	drop(temp_dir);

	ChildOutcome {
		status,
		stdout,
		stderr,
		wall_time: started.elapsed(),
	}
}

fn assert_graceful_shutdown(signal_name: &str, outcome: &ChildOutcome) {
	assert_eq!(
		outcome.status.code(),
		Some(0),
		"{signal_name}: expected a normal exit with code 0, not termination by a signal\n{}",
		outcome.diagnostics(),
	);
	assert!(
		outcome
			.stdout
			.iter()
			.any(|line| line.contains(SHUTDOWN_COMPLETE_MARKER)),
		"{signal_name}: expected {SHUTDOWN_COMPLETE_MARKER:?} in stdout\n{}",
		outcome.diagnostics(),
	);
}

#[tokio::test]
async fn test_sigterm_triggers_graceful_shutdown() {
	let outcome = run_monitor_and_signal(libc::SIGTERM).await;
	assert_graceful_shutdown("SIGTERM", &outcome);
}

#[tokio::test]
async fn test_sighup_triggers_graceful_shutdown() {
	let outcome = run_monitor_and_signal(libc::SIGHUP).await;
	assert_graceful_shutdown("SIGHUP", &outcome);
}

#[tokio::test]
async fn test_sigint_triggers_graceful_shutdown() {
	let outcome = run_monitor_and_signal(libc::SIGINT).await;
	assert_graceful_shutdown("SIGINT", &outcome);
}
