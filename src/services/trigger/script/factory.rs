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
	/// The language selects the interpreter and its eval flag via
	/// [`ScriptLanguage::interpreter`].
	///
	/// # Arguments
	///
	/// * `language` - The programming language of the script
	/// * `script_content` - The content of the script
	/// * `runtime_arguments` - Arguments passed to the interpreter itself (before `-e` / `-c`);
	///   an empty slice means no extra arguments
	///
	/// # Returns
	///
	/// Returns an [`InterpreterExecutor`] that borrows both `script_content` and
	/// `runtime_arguments` for the duration of one execution; nothing is copied
	pub fn create<'a>(
		language: &ScriptLanguage,
		script_content: &'a str,
		runtime_arguments: &'a [String],
	) -> InterpreterExecutor<'a> {
		let (program, eval_flag) = language.interpreter();
		InterpreterExecutor {
			program,
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

	#[test]
	fn test_create_maps_language_to_interpreter() {
		// Compare against the shared mapping, so the factory cannot keep its own table.
		for language in &LANGUAGES {
			let (program, eval_flag) = language.interpreter();
			for script in ["print('Hello')", ""] {
				let executor = ScriptExecutorFactory::create(language, script, &[]);
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
	fn test_create_passes_runtime_arguments() {
		let script = "echo 'Hello'";
		for runtime_arguments in [vec![], vec!["--flag".to_string(), "value".to_string()]] {
			for language in &LANGUAGES {
				let executor = ScriptExecutorFactory::create(language, script, &runtime_arguments);
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
			let executor = ScriptExecutorFactory::create(language, script, &[]);
			let result = executor
				.execute(MonitorMatchBuilder::new().build(), &5000, None, false)
				.await;
			assert!(
				matches!(result, Ok(true)),
				"{:?} should print true, got {:?}",
				language,
				result
			);
		}
	}
}
