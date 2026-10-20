//! Trigger script factory implementation.
//!
//! This module maps a script language to the interpreter that runs it and creates
//! the corresponding script executor.

use crate::{
	models::ScriptLanguage,
	services::trigger::script::executor::{InterpreterExecutor, ScriptExecutor},
};

/// Factory for creating script executors based on the script language.
pub struct ScriptExecutorFactory;

impl ScriptExecutorFactory {
	/// Creates a new script executor for the specified language and script content.
	///
	/// The language selects the interpreter and its eval flag:
	/// Python → `python3 -c`, JavaScript → `node -e`, Bash → `sh -c`.
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
	/// Returns a boxed (Rust will allocate on the heap) trait object implementing the
	/// `ScriptExecutor` trait
	pub fn create(
		language: &ScriptLanguage,
		script_content: &str,
		runtime_arguments: &[String],
	) -> Box<dyn ScriptExecutor> {
		let (program, eval_flag) = match language {
			ScriptLanguage::Python => ("python3", "-c"),
			ScriptLanguage::JavaScript => ("node", "-e"),
			ScriptLanguage::Bash => ("sh", "-c"),
		};
		Box::new(InterpreterExecutor {
			program,
			eval_flag,
			script_content: script_content.to_string(),
			runtime_arguments: runtime_arguments.to_vec(),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		models::{
			EVMMonitorMatch, EVMTransactionReceipt, MatchConditions, MonitorMatch, ScriptLanguage,
		},
		utils::tests::{
			builders::evm::monitor::MonitorBuilder, evm::transaction::TransactionBuilder,
		},
	};

	/// `(language, expected interpreter, expected eval flag)` for every supported language
	const LANGUAGES: [(ScriptLanguage, &str, &str); 3] = [
		(ScriptLanguage::Python, "python3", "-c"),
		(ScriptLanguage::JavaScript, "node", "-e"),
		(ScriptLanguage::Bash, "sh", "-c"),
	];

	fn downcast<'a>(
		executor: &'a dyn ScriptExecutor,
		language: &ScriptLanguage,
	) -> &'a InterpreterExecutor {
		executor
			.as_any()
			.downcast_ref::<InterpreterExecutor>()
			.unwrap_or_else(|| panic!("{:?} should map to InterpreterExecutor", language))
	}

	fn create_test_monitor_match() -> MonitorMatch {
		MonitorMatch::EVM(Box::new(EVMMonitorMatch {
			monitor: MonitorBuilder::new()
				.name("test_monitor")
				.networks(vec!["ethereum_mainnet".to_string()])
				.build(),
			transaction: TransactionBuilder::new().build(),
			receipt: Some(EVMTransactionReceipt::default()),
			logs: Some(vec![]),
			network_slug: "ethereum_mainnet".to_string(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		}))
	}

	#[test]
	fn test_create_maps_language_to_interpreter() {
		let script = "print('Hello')";
		for (language, program, eval_flag) in &LANGUAGES {
			let executor = ScriptExecutorFactory::create(language, script, &[]);
			let executor = downcast(executor.as_ref(), language);
			assert_eq!(executor.program, *program, "program for {:?}", language);
			assert_eq!(
				executor.eval_flag, *eval_flag,
				"eval flag for {:?}",
				language
			);
			assert_eq!(executor.script_content, script, "script for {:?}", language);
		}
	}

	#[test]
	fn test_create_with_empty_script() {
		for (language, program, eval_flag) in &LANGUAGES {
			let executor = ScriptExecutorFactory::create(language, "", &[]);
			let executor = downcast(executor.as_ref(), language);
			assert_eq!(executor.program, *program, "program for {:?}", language);
			assert_eq!(
				executor.eval_flag, *eval_flag,
				"eval flag for {:?}",
				language
			);
			assert!(
				executor.script_content.is_empty(),
				"script for {:?}",
				language
			);
		}
	}

	#[test]
	fn test_create_executor_with_runtime_arguments() {
		let script = "echo 'Hello'";
		let runtime_arguments = vec!["--flag".to_string(), "value".to_string()];
		for (language, _, _) in &LANGUAGES {
			let executor = ScriptExecutorFactory::create(language, script, &runtime_arguments);
			let executor = downcast(executor.as_ref(), language);
			assert_eq!(
				executor.runtime_arguments, runtime_arguments,
				"runtime arguments for {:?}",
				language
			);
		}
	}

	#[test]
	fn test_create_executor_without_runtime_arguments() {
		let script = "echo 'Hello'";
		for (language, _, _) in &LANGUAGES {
			let executor = ScriptExecutorFactory::create(language, script, &[]);
			let executor = downcast(executor.as_ref(), language);
			assert!(
				executor.runtime_arguments.is_empty(),
				"runtime arguments for {:?}",
				language
			);
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
				.execute(create_test_monitor_match(), &5000, None, false)
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
