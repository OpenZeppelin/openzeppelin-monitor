//! Trigger script validation implementation.
//!
//! This module provides functionality to validate script configuration parameters.

use crate::models::{ConfigError, ScriptLanguage};
use std::path::Path;

/// Validates script configuration parameters
///
/// # Arguments
/// * `script_path` - Path to the script file
/// * `language` - The supported script language
/// * `timeout_ms` - Timeout in milliseconds
///
/// # Returns
/// * `Ok(())` if validation passes
/// * `Err(ConfigError)` if any validation fails
#[allow(clippy::result_large_err)]
pub fn validate_script_config(
	script_path: &str,
	language: &ScriptLanguage,
	timeout_ms: &u32,
) -> Result<(), ConfigError> {
	// Validate script path exists
	if !Path::new(script_path).exists() {
		return Err(ConfigError::validation_error(
			format!("Script path does not exist: {}", script_path),
			None,
			None,
		));
	}

	let script_path_instance = Path::new(script_path);
	// Validate file extension matches language
	let extension = script_path_instance
		.extension()
		.and_then(|ext| ext.to_str())
		.unwrap_or("");

	let valid_extension = match language {
		ScriptLanguage::Python => extension == "py",
		ScriptLanguage::JavaScript => extension == "js",
		ScriptLanguage::Bash => extension == "sh",
	};

	if !valid_extension {
		return Err(ConfigError::validation_error(
			format!(
				"Script file extension does not match specified language {:?}: {}",
				language, script_path
			),
			None,
			None,
		));
	}

	// Validate timeout
	if *timeout_ms == 0 {
		return Err(ConfigError::validation_error(
			"Timeout must be greater than 0".to_string(),
			None,
			None,
		));
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use tempfile::NamedTempFile;

	#[test]
	fn test_validate_script_config_valid_python() {
		let temp_file = NamedTempFile::new().unwrap();
		let path = temp_file.path().to_str().unwrap().to_string();
		let python_path = path + ".py";
		fs::rename(temp_file.path(), &python_path).unwrap();

		let result = validate_script_config(&python_path, &ScriptLanguage::Python, &1000);

		assert!(result.is_ok());
		fs::remove_file(python_path).unwrap();
	}

	#[test]
	fn test_validate_script_config_invalid_path() {
		let result =
			validate_script_config("nonexistent_script.py", &ScriptLanguage::Python, &1000);

		assert!(result.is_err());
		if let Err(e) = result {
			assert!(e.to_string().contains("Script path does not exist"));
		}
	}

	#[test]
	fn test_validate_script_config_wrong_extension() {
		let temp_file = NamedTempFile::new().unwrap();
		let path = temp_file.path().to_str().unwrap().to_string();
		let wrong_path = path + ".py";
		fs::rename(temp_file.path(), &wrong_path).unwrap();

		let result = validate_script_config(&wrong_path, &ScriptLanguage::JavaScript, &1000);

		assert!(result.is_err());
		if let Err(e) = result {
			assert!(e.to_string().contains("does not match specified language"));
		}
		fs::remove_file(wrong_path).unwrap();
	}

	#[test]
	fn test_validate_script_config_zero_timeout() {
		let temp_file = NamedTempFile::new().unwrap();
		let path = temp_file.path().to_str().unwrap().to_string();
		let python_path = path + ".py";
		fs::rename(temp_file.path(), &python_path).unwrap();

		let result = validate_script_config(&python_path, &ScriptLanguage::Python, &0);

		assert!(result.is_err());
		if let Err(e) = result {
			assert!(e.to_string().contains("Timeout must be greater than 0"));
		}
		fs::remove_file(python_path).unwrap();
	}
}
