use crate::{
	models::ScriptLanguage,
	services::script::executor::{
		BashScriptExecutor, JavaScriptScriptExecutor, PythonScriptExecutor, ScriptExecutor,
	},
};

pub struct ScriptExecutorFactory;

impl ScriptExecutorFactory {
	pub fn create(language: &ScriptLanguage, script_path: &str) -> Box<dyn ScriptExecutor> {
		match language {
			ScriptLanguage::Python => Box::new(PythonScriptExecutor {
				script_path: script_path.to_string(),
			}),
			ScriptLanguage::JavaScript => Box::new(JavaScriptScriptExecutor {
				script_path: script_path.to_string(),
			}),
			ScriptLanguage::Bash => Box::new(BashScriptExecutor {
				script_path: script_path.to_string(),
			}),
		}
	}
}
