use crate::models::MonitorMatch;
use async_trait::async_trait;

#[async_trait]
pub trait ScriptExecutor: Send + Sync {
	async fn execute(&self, input: MonitorMatch) -> Result<bool, CustomScriptError>;
}

#[derive(Debug)]
pub enum CustomScriptError {
	IoError(std::io::Error),
	ProcessError { code: Option<i32>, stderr: String },
	SerdeJsonError(serde_json::Error),
	ExecutionError(String),
	ParseError(String),
}

impl From<std::io::Error> for CustomScriptError {
	fn from(error: std::io::Error) -> Self {
		CustomScriptError::IoError(error)
	}
}

impl From<serde_json::Error> for CustomScriptError {
	fn from(error: serde_json::Error) -> Self {
		CustomScriptError::SerdeJsonError(error)
	}
}

impl std::fmt::Display for CustomScriptError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			CustomScriptError::IoError(e) => write!(f, "IO error: {}", e),
			CustomScriptError::ProcessError { code, stderr } => {
				write!(f, "Process error (code: {:?}): {}", code, stderr)
			}
			CustomScriptError::SerdeJsonError(e) => write!(f, "JSON serialization error: {}", e),
			CustomScriptError::ExecutionError(e) => write!(f, "Execution error: {}", e),
			CustomScriptError::ParseError(e) => write!(f, "Parse error: {}", e),
		}
	}
}

impl std::error::Error for CustomScriptError {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		match self {
			CustomScriptError::IoError(e) => Some(e),
			CustomScriptError::SerdeJsonError(e) => Some(e),
			_ => None,
		}
	}
}

impl CustomScriptError {
	pub fn process_error(code: Option<i32>, stderr: String) -> Self {
		CustomScriptError::ProcessError { code, stderr }
	}
}

pub struct PythonScriptExecutor {
	pub script_path: String,
}

#[async_trait]
impl ScriptExecutor for PythonScriptExecutor {
	async fn execute(&self, input: MonitorMatch) -> Result<bool, CustomScriptError> {
		let input_json = serde_json::to_string(&input)?;

		let output = tokio::process::Command::new("python3")
			.arg(&self.script_path)
			.arg(input_json)
			.output()
			.await?;

		process_script_output(output)
	}
}

pub struct JavaScriptScriptExecutor {
	pub script_path: String,
}

#[async_trait]
impl ScriptExecutor for JavaScriptScriptExecutor {
	async fn execute(&self, input: MonitorMatch) -> Result<bool, CustomScriptError> {
		let input_json = serde_json::to_string(&input)?;

		let output = tokio::process::Command::new("node")
			.arg(&self.script_path)
			.arg(input_json)
			.output()
			.await?;

		process_script_output(output)
	}
}

pub struct BashScriptExecutor {
	pub script_path: String,
}

#[async_trait]
impl ScriptExecutor for BashScriptExecutor {
	async fn execute(&self, input: MonitorMatch) -> Result<bool, CustomScriptError> {
		let input_json = serde_json::to_string(&input)?;

		let output = tokio::process::Command::new("bash")
			.arg(&self.script_path)
			.arg(input_json)
			.output()
			.await?;

		process_script_output(output)
	}
}

fn process_script_output(output: std::process::Output) -> Result<bool, CustomScriptError> {
	if !output.status.success() {
		return Err(CustomScriptError::ExecutionError(
			String::from_utf8_lossy(&output.stderr).to_string(),
		));
	}

	String::from_utf8_lossy(&output.stdout)
		.trim()
		.parse::<bool>()
		.map_err(|e| CustomScriptError::ParseError(e.to_string()))
}
