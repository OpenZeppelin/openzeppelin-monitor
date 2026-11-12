//! Trigger script factory implementation.
//!
//! This module maps a script language to the interpreter that runs it and creates
//! the corresponding script executor.

use crate::{models::ScriptLanguage, services::trigger::script::executor::InterpreterExecutor};

/// Factory for creating script executors based on the script language.
pub struct ScriptExecutorFactory;

impl ScriptExecutorFactory {
	/// Creates a new script executor for the specified language and script content.
	///
	/// The language selects the default interpreter and the eval flag via
	/// [`ScriptLanguage::interpreter`]. An `interpreter` override replaces only the program:
	/// the eval flag always comes from the language, so the override must accept the
	/// language's eval flag (`-c` for Python and Bash, `-e` for JavaScript).
	///
	/// The override is used as given and is not validated here; that is done when the
	/// configuration is loaded.
	///
	/// # Arguments
	///
	/// * `language` - The programming language of the script
	/// * `interpreter` - Interpreter binary to run instead of the default for the language
	///   (a command name or a path); `None` uses the default
	/// * `script_content` - The content of the script
	/// * `runtime_arguments` - Arguments passed to the interpreter itself (before `-e` / `-c`);
	///   an empty slice means no extra arguments
	///
	/// # Returns
	///
	/// Returns an [`InterpreterExecutor`] that borrows `interpreter`, `script_content` and
	/// `runtime_arguments` for the duration of one execution; nothing is copied
	pub fn create<'a>(
		language: &ScriptLanguage,
		interpreter: Option<&'a str>,
		script_content: &'a str,
		runtime_arguments: &'a [String],
	) -> InterpreterExecutor<'a> {
		let (default_program, eval_flag) = language.interpreter();
		InterpreterExecutor {
			program: interpreter.unwrap_or(default_program),
			eval_flag,
			script_content,
			runtime_arguments,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::services::trigger::ScriptExecutor as _;
	use crate::{
		models::ScriptLanguage, utils::tests::builders::evm::monitor_match::MonitorMatchBuilder,
	};

	/// Every supported language
	const LANGUAGES: [ScriptLanguage; 3] = [
		ScriptLanguage::Python,
		ScriptLanguage::JavaScript,
		ScriptLanguage::Bash,
	];

	/// Generous timeout for scripts that are expected to finish (ms)
	const GENEROUS_TIMEOUT_MS: u32 = 5000;

	#[test]
	fn test_create_maps_language_to_interpreter() {
		// Compare against the shared mapping, so the factory cannot keep its own table.
		for language in &LANGUAGES {
			let (program, eval_flag) = language.interpreter();
			for script in ["print('Hello')", ""] {
				let executor = ScriptExecutorFactory::create(language, None, script, &[]);
				assert_eq!(executor.program, program, "program for {:?}", language);
				assert_eq!(
					executor.eval_flag, eval_flag,
					"eval flag for {:?}",
					language
				);
				assert_eq!(executor.script_content, script, "script for {:?}", language);
			}
		}
	}

	#[test]
	fn test_create_uses_interpreter_override_as_program() {
		// The override replaces the program only; the eval flag still comes from the language.
		for language in &LANGUAGES {
			let (_, eval_flag) = language.interpreter();
			for interpreter in ["custom-interpreter", "/opt/venv/bin/python"] {
				let executor =
					ScriptExecutorFactory::create(language, Some(interpreter), "script", &[]);
				assert_eq!(executor.program, interpreter, "program for {:?}", language);
				assert_eq!(
					executor.eval_flag, eval_flag,
					"eval flag for {:?}",
					language
				);
			}
		}
	}

	#[test]
	fn test_create_borrows_interpreter_override() {
		// A locally owned value: the override does not have to be `'static`
		let interpreter = String::from("/opt/venv/bin/python");

		let executor = ScriptExecutorFactory::create(
			&ScriptLanguage::Python,
			Some(interpreter.as_str()),
			"print('true')",
			&[],
		);

		// Same backing storage, so the string contents were neither copied nor cloned
		assert_eq!(executor.program.as_ptr(), interpreter.as_ptr());
		assert_eq!(executor.program.len(), interpreter.len());
	}

	#[test]
	fn test_create_passes_runtime_arguments() {
		let script = "echo 'Hello'";
		for runtime_arguments in [vec![], vec!["--flag".to_string(), "value".to_string()]] {
			for language in &LANGUAGES {
				let executor =
					ScriptExecutorFactory::create(language, None, script, &runtime_arguments);
				assert_eq!(
					executor.runtime_arguments,
					runtime_arguments.as_slice(),
					"runtime arguments for {:?}",
					language
				);
			}
		}
	}

	#[tokio::test]
	async fn test_create_executor_runs_script_for_every_language() {
		// Each script drains stdin before printing so the child never exits while
		// `process_command` is still writing to its stdin pipe.
		let scripts = [
			(
				ScriptLanguage::Python,
				"import sys; sys.stdin.read(); print('true')",
			),
			(
				ScriptLanguage::JavaScript,
				"require('fs').readFileSync(0, 'utf8'); console.log('true')",
			),
			(ScriptLanguage::Bash, "cat >/dev/null; echo true"),
		];
		for (language, script) in &scripts {
			let executor = ScriptExecutorFactory::create(language, None, script, &[]);
			let result = executor
				.execute(
					MonitorMatchBuilder::new().build(),
					&GENEROUS_TIMEOUT_MS,
					None,
					false,
				)
				.await;
			assert!(
				matches!(result, Ok(true)),
				"{:?} should print true, got {:?}",
				language,
				result
			);
		}
	}

	/// Valid shell, but a syntax error in Python
	#[cfg(unix)]
	const SHELL_ONLY_SCRIPT: &str = "cat >/dev/null; echo true";

	#[cfg(unix)]
	#[tokio::test]
	async fn test_create_executor_runs_interpreter_override() {
		// Premise: the default interpreter for the language cannot run this script
		let executor =
			ScriptExecutorFactory::create(&ScriptLanguage::Python, None, SHELL_ONLY_SCRIPT, &[]);
		let result = executor
			.execute(
				MonitorMatchBuilder::new().build(),
				&GENEROUS_TIMEOUT_MS,
				None,
				false,
			)
			.await;
		match result {
			Err(e) => assert!(
				e.to_string().contains("Script execution failed"),
				"python3 should fail on the shell script itself, got: {e}"
			),
			Ok(v) => panic!("python3 should reject a shell script, got Ok({v})"),
		}

		// With the override, `sh` runs it
		let executor = ScriptExecutorFactory::create(
			&ScriptLanguage::Python,
			Some("sh"),
			SHELL_ONLY_SCRIPT,
			&[],
		);
		let result = executor
			.execute(
				MonitorMatchBuilder::new().build(),
				&GENEROUS_TIMEOUT_MS,
				None,
				false,
			)
			.await;
		assert!(
			matches!(result, Ok(true)),
			"sh override should print true, got {:?}",
			result
		);
	}

	#[cfg(unix)]
	#[tokio::test]
	async fn test_create_executor_runs_interpreter_override_given_as_path() {
		// A symlink to an existing binary, not a freshly written executable: executing a file
		// this process has just written can fail with "Text file busy" when another test
		// thread forks at the wrong moment. The link keeps the name `sh` because multi-call
		// shells select their behaviour from the name they are invoked with.
		let temp_dir = tempfile::tempdir().unwrap();
		let interpreter = temp_dir.path().join("sh");
		std::os::unix::fs::symlink("/bin/sh", &interpreter).unwrap();

		let executor = ScriptExecutorFactory::create(
			&ScriptLanguage::Python,
			Some(interpreter.to_str().unwrap()),
			SHELL_ONLY_SCRIPT,
			&[],
		);
		let result = executor
			.execute(
				MonitorMatchBuilder::new().build(),
				&GENEROUS_TIMEOUT_MS,
				None,
				false,
			)
			.await;
		assert!(
			matches!(result, Ok(true)),
			"override {:?} should print true, got {:?}",
			interpreter,
			result
		);
	}
}
