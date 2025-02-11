use crate::models::MonitorMatch;
use async_trait::async_trait;

/// A trait that defines the interface for executing custom scripts in different languages.
/// Implementors must be both Send and Sync to ensure thread safety.
#[async_trait]
pub trait ScriptExecutor: Send + Sync {
	/// Executes the script with the given MonitorMatch input.
	///
	/// # Arguments
	/// * `input` - A MonitorMatch instance containing the data to be processed by the script
	///
	/// # Returns
	/// * `Result<bool, CustomScriptError>` - Returns true/false based on script execution or an
	///   error
	async fn execute(&self, input: MonitorMatch) -> Result<bool, CustomScriptError>;
}

/// Represents various error cases that can occur during script execution.
#[derive(Debug)]
pub enum CustomScriptError {
	/// Represents standard IO errors
	IoError(std::io::Error),
	/// Represents errors from script process execution
	ProcessError { code: Option<i32>, stderr: String },
	/// Represents JSON serialization/deserialization errors
	SerdeJsonError(serde_json::Error),
	/// Represents general script execution errors
	ExecutionError(String),
	/// Represents errors in parsing script output
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

/// Executes Python scripts using the python3 interpreter.
pub struct PythonScriptExecutor {
	/// Path to the Python script file to be executed
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

/// Executes JavaScript scripts using the Node.js runtime.
pub struct JavaScriptScriptExecutor {
	/// Path to the JavaScript script file to be executed
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

/// Executes Bash shell scripts.
pub struct BashScriptExecutor {
	/// Path to the Bash script file to be executed
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

/// Processes the output from script execution.
///
/// # Arguments
/// * `output` - The process output containing stdout, stderr, and status
///
/// # Returns
/// * `Result<bool, CustomScriptError>` - Returns parsed boolean result or error
///
/// # Errors
/// Returns an error if:
/// * The script execution was not successful (non-zero exit code)
/// * The output cannot be parsed as a boolean
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
