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
	/// * `script_path` - The file path to the script
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
