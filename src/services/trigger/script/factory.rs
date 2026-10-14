//! Trigger script factory implementation.
//!
//! This module provides functionality to create script executors based on the script language.

use crate::{
	models::ScriptLanguage,
	services::trigger::script::executor::{
		BashScriptExecutor, JavaScriptScriptExecutor, PythonScriptExecutor, ScriptExecutor,
	},
};

/// Factory for creating script executors based on the script language.
pub struct ScriptExecutorFactory;

impl ScriptExecutorFactory {
	/// Creates a new script executor for the specified language and script path.
	///
	/// # Arguments
	///
	/// * `language` - The programming language of the script
	/// * `script_content` - The content of the script
	/// * `runtime_arguments` - Optional arguments passed to the interpreter itself (before
	///   `-e` / `-c`); `None` means no extra arguments
	///
	/// # Returns
	///
	/// Returns a boxed (Rust will allocate on the heap) trait object implementing the
	/// `ScriptExecutor` trait
	pub fn create(
		language: &ScriptLanguage,
		script_content: &str,
		runtime_arguments: Option<&[String]>,
	) -> Box<dyn ScriptExecutor> {
		let runtime_arguments = runtime_arguments.unwrap_or_default().to_vec();
		match language {
			ScriptLanguage::Python => Box::new(PythonScriptExecutor {
				script_content: script_content.to_string(),
				runtime_arguments,
			}),
			ScriptLanguage::JavaScript => Box::new(JavaScriptScriptExecutor {
				script_content: script_content.to_string(),
				runtime_arguments,
			}),
			ScriptLanguage::Bash => Box::new(BashScriptExecutor {
				script_content: script_content.to_string(),
				runtime_arguments,
			}),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::ScriptLanguage;

	#[test]
	fn test_create_python_executor() {
		let script = "print('Hello')";
		let executor = ScriptExecutorFactory::create(&ScriptLanguage::Python, script, None);
		assert!(
			executor
				.as_any()
				.downcast_ref::<PythonScriptExecutor>()
				.unwrap()
				.script_content
				== script
		);

		// Test with empty script
		let empty_script = "";
		let executor = ScriptExecutorFactory::create(&ScriptLanguage::Python, empty_script, None);
		assert!(executor
			.as_any()
			.downcast_ref::<PythonScriptExecutor>()
			.unwrap()
			.script_content
			.is_empty());
	}

	#[test]
	fn test_create_javascript_executor() {
		let script = "console.log('Hello')";
		let executor = ScriptExecutorFactory::create(&ScriptLanguage::JavaScript, script, None);
		assert!(
			executor
				.as_any()
				.downcast_ref::<JavaScriptScriptExecutor>()
				.unwrap()
				.script_content
				== script
		);

		// Test with empty script
		let empty_script = "";
		let executor =
			ScriptExecutorFactory::create(&ScriptLanguage::JavaScript, empty_script, None);
		assert!(executor
			.as_any()
			.downcast_ref::<JavaScriptScriptExecutor>()
			.unwrap()
			.script_content
			.is_empty());
	}

	#[test]
	fn test_create_bash_executor() {
		let script = "echo 'Hello'";
		let executor = ScriptExecutorFactory::create(&ScriptLanguage::Bash, script, None);
		assert!(
			executor
				.as_any()
				.downcast_ref::<BashScriptExecutor>()
				.unwrap()
				.script_content
				== script
		);

		// Test with empty script
		let empty_script = "";
		let executor = ScriptExecutorFactory::create(&ScriptLanguage::Bash, empty_script, None);
		assert!(executor
			.as_any()
			.downcast_ref::<BashScriptExecutor>()
			.unwrap()
			.script_content
			.is_empty());
	}

	#[test]
	fn test_create_executor_with_runtime_arguments() {
		let script = "echo 'Hello'";
		let runtime_arguments = vec!["--flag".to_string(), "value".to_string()];

		let executor = ScriptExecutorFactory::create(
			&ScriptLanguage::Python,
			script,
			Some(&runtime_arguments),
		);
		assert_eq!(
			executor
				.as_any()
				.downcast_ref::<PythonScriptExecutor>()
				.unwrap()
				.runtime_arguments,
			runtime_arguments
		);

		let executor = ScriptExecutorFactory::create(
			&ScriptLanguage::JavaScript,
			script,
			Some(&runtime_arguments),
		);
		assert_eq!(
			executor
				.as_any()
				.downcast_ref::<JavaScriptScriptExecutor>()
				.unwrap()
				.runtime_arguments,
			runtime_arguments
		);

		let executor =
			ScriptExecutorFactory::create(&ScriptLanguage::Bash, script, Some(&runtime_arguments));
		assert_eq!(
			executor
				.as_any()
				.downcast_ref::<BashScriptExecutor>()
				.unwrap()
				.runtime_arguments,
			runtime_arguments
		);
	}

	#[test]
	fn test_create_executor_without_runtime_arguments() {
		let script = "echo 'Hello'";

		let executor = ScriptExecutorFactory::create(&ScriptLanguage::Python, script, None);
		assert!(executor
			.as_any()
			.downcast_ref::<PythonScriptExecutor>()
			.unwrap()
			.runtime_arguments
			.is_empty());

		let executor = ScriptExecutorFactory::create(&ScriptLanguage::JavaScript, script, None);
		assert!(executor
			.as_any()
			.downcast_ref::<JavaScriptScriptExecutor>()
			.unwrap()
			.runtime_arguments
			.is_empty());

		let executor = ScriptExecutorFactory::create(&ScriptLanguage::Bash, script, None);
		assert!(executor
			.as_any()
			.downcast_ref::<BashScriptExecutor>()
			.unwrap()
			.runtime_arguments
			.is_empty());
	}
}
