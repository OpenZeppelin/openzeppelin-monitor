use crate::{
	models::ScriptLanguage,
	utils::script::executor::{
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
	///
	/// # Returns
	///
	/// Returns a boxed (Rust will allocate on the heap) trait object implementing the
	/// `ScriptExecutor` trait
	pub fn create(language: &ScriptLanguage, script_content: &str) -> Box<dyn ScriptExecutor> {
		match language {
			ScriptLanguage::Python => Box::new(PythonScriptExecutor {
				script_content: script_content.to_string(),
			}),
			ScriptLanguage::JavaScript => Box::new(JavaScriptScriptExecutor {
				script_content: script_content.to_string(),
			}),
			ScriptLanguage::Bash => Box::new(BashScriptExecutor {
				script_content: script_content.to_string(),
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
		let _executor = ScriptExecutorFactory::create(&ScriptLanguage::Python, script);

		// Test with empty script
		let empty_script = "";
		let _executor = ScriptExecutorFactory::create(&ScriptLanguage::Python, empty_script);
	}

	#[test]
	fn test_create_javascript_executor() {
		let script = "console.log('Hello')";
		let _executor = ScriptExecutorFactory::create(&ScriptLanguage::JavaScript, script);

		// Test with empty script
		let empty_script = "";
		let _executor = ScriptExecutorFactory::create(&ScriptLanguage::JavaScript, empty_script);
	}

	#[test]
	fn test_create_bash_executor() {
		let script = "echo 'Hello'";
		let _executor = ScriptExecutorFactory::create(&ScriptLanguage::Bash, script);

		// Test with empty script
		let empty_script = "";
		let _executor = ScriptExecutorFactory::create(&ScriptLanguage::Bash, empty_script);
	}
}
