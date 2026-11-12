//! Trigger script validation implementation.
//!
//! This module provides functionality to validate script configuration parameters.

use crate::models::{ConfigError, ScriptLanguage};
use std::{
	ffi::OsStr,
	path::{Path, PathBuf},
};

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

	if extension != language.extension() {
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

/// Hint appended to "not found" errors when the configured value contains whitespace
const WHITESPACE_HINT: &str = " (the value contains whitespace and is used verbatim as the \
	program to run; interpreter flags belong in `runtime_arguments`)";

/// Validates the optional interpreter override of a script
///
/// For trigger conditions a spawn failure does not filter the match out, so the match would
/// be notified as if the condition had not been configured. An interpreter that cannot be
/// resolved is therefore rejected when the configuration is loaded, for both kinds of script
/// (trigger conditions and script notifications), instead of when a match is processed.
///
/// The value is checked exactly as given, without trimming or normalization, because that is
/// how it is handed to the operating system when the script runs:
/// * a value containing a path separator is an explicit path. It must point to a regular file
///   (symlinks are followed) that is executable;
/// * any other value is a command name and must resolve through the `PATH` of this process,
///   which is the `PATH` the script process inherits.
///
/// # Arguments
/// * `interpreter` - The configured interpreter, `None` to use the default for the language
///
/// # Returns
/// * `Ok(())` if no interpreter is configured or the configured one can be resolved
/// * `Err(ConfigError)` if the interpreter is empty or cannot be resolved
#[allow(clippy::result_large_err)]
pub fn validate_script_interpreter(interpreter: Option<&str>) -> Result<(), ConfigError> {
	// Nothing to resolve; the defaults are not checked
	if interpreter.is_none() {
		return Ok(());
	}

	let path = std::env::var_os("PATH");
	validate_script_interpreter_with_path(interpreter, path.as_deref())
}

/// Implementation of [`validate_script_interpreter`] with the `PATH` value passed in
///
/// If `path` is `None` (`PATH` is not set), a command name does not resolve. This is
/// intentionally stricter than `execvp`, which falls back to a default search path in that
/// case: the default differs between libc implementations (glibc, musl, macOS), and guessing
/// it wrong would accept a configuration that fails when the script runs.
#[allow(clippy::result_large_err)]
fn validate_script_interpreter_with_path(
	interpreter: Option<&str>,
	path: Option<&OsStr>,
) -> Result<(), ConfigError> {
	let Some(interpreter) = interpreter else {
		return Ok(());
	};

	if interpreter.trim().is_empty() {
		return Err(ConfigError::validation_error(
			"Script interpreter must not be empty".to_string(),
			None,
			None,
		));
	}

	let whitespace_hint = if interpreter.contains(char::is_whitespace) {
		WHITESPACE_HINT
	} else {
		""
	};

	if is_explicit_interpreter_path(interpreter) {
		let metadata = std::fs::metadata(interpreter).map_err(|e| {
			ConfigError::validation_error(
				format!(
					"Script interpreter path '{}' does not exist or is not accessible: {}{}",
					interpreter, e, whitespace_hint
				),
				None,
				None,
			)
		})?;

		if !metadata.is_file() {
			return Err(ConfigError::validation_error(
				format!(
					"Script interpreter path '{}' is not a regular file",
					interpreter
				),
				None,
				None,
			));
		}

		if !is_executable_by_current_user(Path::new(interpreter)) {
			return Err(ConfigError::validation_error(
				format!(
					"Script interpreter path '{}' is not executable by the current user",
					interpreter
				),
				None,
				None,
			));
		}

		return Ok(());
	}

	if path.is_none() {
		return Err(ConfigError::validation_error(
			format!(
				"Script interpreter '{}' cannot be resolved because the PATH environment \
				 variable is not set; set PATH or use an explicit path",
				interpreter
			),
			None,
			None,
		));
	}

	if find_executable_in_path(interpreter, path).is_none() {
		return Err(ConfigError::validation_error(
			format!(
				"Script interpreter '{}' was not found as an executable file in PATH{}",
				interpreter, whitespace_hint
			),
			None,
			None,
		));
	}

	Ok(())
}

/// Whether an interpreter value is an explicit path, as opposed to a command name
///
/// Same rule as `execvp`: a value that contains a path separator is used as a path and is
/// never searched in `PATH`.
fn is_explicit_interpreter_path(interpreter: &str) -> bool {
	interpreter.contains(std::path::is_separator)
}

/// Whether a configured interpreter is an explicit path to a file that is writable by its
/// group or by others (`mode & 0o022 != 0`, the same mask as the script file check)
///
/// Whoever can replace the interpreter can run code with the privileges of the Monitor, so
/// the configuration validators log a warning for such a file. This is advisory only and is
/// not part of [`validate_script_interpreter`].
///
/// Returns `false` when no interpreter is configured, when it is a command name (programs
/// resolved through `PATH` are not checked), and when the metadata cannot be read (a missing
/// file is reported by the validation). Symlinks are followed, so the permissions of the
/// real binary are checked; a symlink itself always reports `777`.
#[cfg(unix)]
pub(crate) fn interpreter_is_writable_by_others(interpreter: Option<&str>) -> bool {
	use std::os::unix::fs::PermissionsExt;

	let Some(interpreter) = interpreter else {
		return false;
	};
	if !is_explicit_interpreter_path(interpreter) {
		return false;
	}
	std::fs::metadata(interpreter)
		.map(|metadata| metadata.permissions().mode() & 0o022 != 0)
		.unwrap_or(false)
}

/// Searches the directories of a `PATH` value for an executable file named `command`
///
/// Returns the first candidate that is a regular file (symlinks are followed) which the
/// current user may execute. A candidate that exists but is not executable is skipped and the
/// search continues, as `execvp` does. An empty `PATH` entry stands for the current directory,
/// also as in `execvp`.
///
/// If `path` is `None` (`PATH` is not set) nothing is found; see
/// [`validate_script_interpreter_with_path`] for why no default search path is assumed.
fn find_executable_in_path(command: &str, path: Option<&OsStr>) -> Option<PathBuf> {
	std::env::split_paths(path?)
		.map(|dir| dir.join(command))
		.find(|candidate| {
			std::fs::metadata(candidate)
				.map(|metadata| metadata.is_file() && is_executable_by_current_user(candidate))
				.unwrap_or(false)
		})
}

/// Whether the current user is allowed to execute `path`, according to `access(2)` with `X_OK`
///
/// `access` checks with the real user and group ids, which matches what `execve` will decide
/// for a process that is not setuid. It also succeeds on a searchable directory, so callers
/// must check separately that `path` is a regular file.
///
/// The result is advisory: permissions can change between this check and the spawn, and
/// `execve` can still fail for other reasons, so the spawn error remains the final authority.
///
/// A path with an interior NUL byte cannot be passed to the operating system and is reported
/// as not executable.
#[cfg(unix)]
fn is_executable_by_current_user(path: &Path) -> bool {
	use std::os::unix::ffi::OsStrExt;

	let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
		return false;
	};
	// SAFETY: `c_path` is a valid NUL-terminated string that outlives the call; `access` only
	// reads it and has no other memory-safety preconditions.
	unsafe { libc::access(c_path.as_ptr(), libc::X_OK) == 0 }
}

/// Non-Unix targets have no execute permission bits; any regular file is accepted
#[cfg(not(unix))]
fn is_executable_by_current_user(_path: &Path) -> bool {
	true
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
	fn test_validate_script_config_valid_javascript() {
		let temp_file = NamedTempFile::new().unwrap();
		let path = temp_file.path().to_str().unwrap().to_string();
		let javascript_path = path + ".js";
		fs::rename(temp_file.path(), &javascript_path).unwrap();

		let result = validate_script_config(&javascript_path, &ScriptLanguage::JavaScript, &1000);

		assert!(result.is_ok());
		fs::remove_file(javascript_path).unwrap();
	}

	#[test]
	fn test_validate_script_config_valid_bash() {
		let temp_file = NamedTempFile::new().unwrap();
		let path = temp_file.path().to_str().unwrap().to_string();
		let bash_path = path + ".sh";
		fs::rename(temp_file.path(), &bash_path).unwrap();

		let result = validate_script_config(&bash_path, &ScriptLanguage::Bash, &1000);

		assert!(result.is_ok());
		fs::remove_file(bash_path).unwrap();
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

	/// Creates a file named `name` in `dir` with the given permission bits
	#[cfg(unix)]
	fn create_file_with_mode(dir: &Path, name: &str, mode: u32) -> PathBuf {
		use std::os::unix::fs::PermissionsExt;
		let file_path = dir.join(name);
		fs::write(&file_path, "#!/bin/sh\n").unwrap();
		fs::set_permissions(&file_path, fs::Permissions::from_mode(mode)).unwrap();
		file_path
	}

	#[test]
	fn test_validate_script_interpreter_none() {
		assert!(validate_script_interpreter(None).is_ok());
		// No lookup happens, so a missing PATH does not matter either
		assert!(validate_script_interpreter_with_path(None, None).is_ok());
	}

	#[test]
	fn test_validate_script_interpreter_empty() {
		let error = validate_script_interpreter(Some(""))
			.unwrap_err()
			.to_string();
		assert!(
			error.contains("Script interpreter must not be empty"),
			"unexpected error: {error}"
		);
	}

	#[test]
	fn test_validate_script_interpreter_whitespace_only() {
		let error = validate_script_interpreter(Some(" \t "))
			.unwrap_err()
			.to_string();
		assert!(
			error.contains("Script interpreter must not be empty"),
			"unexpected error: {error}"
		);
	}

	#[test]
	fn test_validate_script_interpreter_path_missing() {
		let temp_dir = tempfile::tempdir().unwrap();
		let missing = temp_dir.path().join("missing-interpreter");
		let missing = missing.to_str().unwrap();

		let error = validate_script_interpreter(Some(missing))
			.unwrap_err()
			.to_string();
		assert!(
			error.contains("does not exist or is not accessible"),
			"unexpected error: {error}"
		);
		assert!(error.contains(missing), "unexpected error: {error}");
		assert!(
			!error.contains("runtime_arguments"),
			"unexpected error: {error}"
		);
	}

	#[test]
	fn test_validate_script_interpreter_path_missing_with_flags() {
		let temp_dir = tempfile::tempdir().unwrap();
		let with_flag = format!("{} -u", temp_dir.path().join("python3").to_str().unwrap());

		let error = validate_script_interpreter(Some(&with_flag))
			.unwrap_err()
			.to_string();
		assert!(
			error.contains("does not exist or is not accessible"),
			"unexpected error: {error}"
		);
		assert!(
			error.contains("runtime_arguments"),
			"unexpected error: {error}"
		);
	}

	#[test]
	fn test_validate_script_interpreter_path_is_directory() {
		let temp_dir = tempfile::tempdir().unwrap();
		let directory = temp_dir.path().to_str().unwrap();

		let error = validate_script_interpreter(Some(directory))
			.unwrap_err()
			.to_string();
		assert!(
			error.contains("is not a regular file"),
			"unexpected error: {error}"
		);
	}

	#[cfg(unix)]
	#[test]
	fn test_validate_script_interpreter_path_not_executable() {
		let temp_dir = tempfile::tempdir().unwrap();
		let file_path = create_file_with_mode(temp_dir.path(), "interpreter", 0o644);

		let error = validate_script_interpreter(Some(file_path.to_str().unwrap()))
			.unwrap_err()
			.to_string();
		assert!(
			error.contains("is not executable by the current user"),
			"unexpected error: {error}"
		);
	}

	#[cfg(unix)]
	#[test]
	fn test_validate_script_interpreter_path_executable() {
		let temp_dir = tempfile::tempdir().unwrap();
		let file_path = create_file_with_mode(temp_dir.path(), "interpreter", 0o755);

		let result = validate_script_interpreter(Some(file_path.to_str().unwrap()));
		assert!(result.is_ok(), "unexpected error: {result:?}");
	}

	#[cfg(unix)]
	#[test]
	fn test_validate_script_interpreter_path_symlink_to_executable() {
		let temp_dir = tempfile::tempdir().unwrap();
		let target = create_file_with_mode(temp_dir.path(), "interpreter3.11", 0o755);
		let link = temp_dir.path().join("interpreter3");
		std::os::unix::fs::symlink(&target, &link).unwrap();

		let result = validate_script_interpreter(Some(link.to_str().unwrap()));
		assert!(result.is_ok(), "unexpected error: {result:?}");
	}

	#[cfg(unix)]
	#[test]
	fn test_validate_script_interpreter_path_with_spaces_is_accepted() {
		let temp_dir = tempfile::tempdir().unwrap();
		let spaced_dir = temp_dir.path().join("my tools");
		fs::create_dir(&spaced_dir).unwrap();
		let file_path = create_file_with_mode(&spaced_dir, "interpreter", 0o755);

		let result = validate_script_interpreter(Some(file_path.to_str().unwrap()));
		assert!(result.is_ok(), "unexpected error: {result:?}");
	}

	#[cfg(unix)]
	#[test]
	fn test_validate_script_interpreter_command_found() {
		let temp_dir = tempfile::tempdir().unwrap();
		create_file_with_mode(temp_dir.path(), "my-interpreter", 0o755);

		let result = validate_script_interpreter_with_path(
			Some("my-interpreter"),
			Some(temp_dir.path().as_os_str()),
		);
		assert!(result.is_ok(), "unexpected error: {result:?}");
	}

	#[test]
	fn test_validate_script_interpreter_command_not_found() {
		let temp_dir = tempfile::tempdir().unwrap();

		let error = validate_script_interpreter_with_path(
			Some("definitely-not-a-program"),
			Some(temp_dir.path().as_os_str()),
		)
		.unwrap_err()
		.to_string();
		assert!(
			error.contains(
				"Script interpreter 'definitely-not-a-program' was not found as an executable \
				 file in PATH"
			),
			"unexpected error: {error}"
		);
		assert!(
			!error.contains("runtime_arguments"),
			"unexpected error: {error}"
		);
	}

	#[cfg(unix)]
	#[test]
	fn test_validate_script_interpreter_command_with_flags() {
		// `python3` itself resolves, but the value is used verbatim as the program name
		let temp_dir = tempfile::tempdir().unwrap();
		create_file_with_mode(temp_dir.path(), "python3", 0o755);

		let error = validate_script_interpreter_with_path(
			Some("python3 -u"),
			Some(temp_dir.path().as_os_str()),
		)
		.unwrap_err()
		.to_string();
		assert!(
			error.contains("'python3 -u' was not found"),
			"unexpected error: {error}"
		);
		assert!(
			error.contains("runtime_arguments"),
			"unexpected error: {error}"
		);
	}

	#[test]
	fn test_validate_script_interpreter_command_without_path_variable() {
		let error = validate_script_interpreter_with_path(Some("sh"), None)
			.unwrap_err()
			.to_string();
		assert!(
			error.contains("PATH environment variable is not set"),
			"unexpected error: {error}"
		);
	}

	#[cfg(unix)]
	#[test]
	fn test_find_executable_in_path_finds_executable() {
		let temp_dir = tempfile::tempdir().unwrap();
		let expected = create_file_with_mode(temp_dir.path(), "my-interpreter", 0o755);

		let found = find_executable_in_path("my-interpreter", Some(temp_dir.path().as_os_str()));
		assert_eq!(found, Some(expected));
	}

	#[cfg(unix)]
	#[test]
	fn test_find_executable_in_path_skips_non_executable() {
		let temp_dir = tempfile::tempdir().unwrap();
		create_file_with_mode(temp_dir.path(), "my-interpreter", 0o644);

		let found = find_executable_in_path("my-interpreter", Some(temp_dir.path().as_os_str()));
		assert_eq!(found, None);
	}

	#[cfg(unix)]
	#[test]
	fn test_find_executable_in_path_continues_past_non_executable() {
		let first_dir = tempfile::tempdir().unwrap();
		let second_dir = tempfile::tempdir().unwrap();
		create_file_with_mode(first_dir.path(), "my-interpreter", 0o644);
		let expected = create_file_with_mode(second_dir.path(), "my-interpreter", 0o755);
		let path = std::env::join_paths([first_dir.path(), second_dir.path()]).unwrap();

		let found = find_executable_in_path("my-interpreter", Some(path.as_os_str()));
		assert_eq!(found, Some(expected));
	}

	#[cfg(unix)]
	#[test]
	fn test_interpreter_not_executable_by_current_user_is_rejected() {
		// SAFETY: `getuid` only queries the real user id.
		if unsafe { libc::getuid() } == 0 {
			// `access` checks with the real user id, and root may execute any file that
			// has at least one execute bit set, so the scenario below cannot be reproduced
			// when the tests run as root.
			return;
		}

		// Execute permission for "others" only: the owner (the current user) is denied,
		// although an execute bit is set.
		let temp_dir = tempfile::tempdir().unwrap();
		let file_path = create_file_with_mode(temp_dir.path(), "my-interpreter", 0o001);

		let error = validate_script_interpreter(Some(file_path.to_str().unwrap()))
			.unwrap_err()
			.to_string();
		assert!(
			error.contains("is not executable by the current user"),
			"unexpected error: {error}"
		);

		let found = find_executable_in_path("my-interpreter", Some(temp_dir.path().as_os_str()));
		assert_eq!(found, None);
	}

	#[test]
	fn test_is_explicit_interpreter_path() {
		for path in [
			"/usr/bin/python3",
			"./venv/bin/python",
			"venv/bin/python",
			"bin/",
		] {
			assert!(is_explicit_interpreter_path(path), "{path:?} is a path");
		}
		for command in ["python3", "python3.11", "python3 -u", ""] {
			assert!(
				!is_explicit_interpreter_path(command),
				"{command:?} is a command name"
			);
		}
	}

	#[cfg(unix)]
	#[test]
	fn test_interpreter_is_writable_by_others() {
		let temp_dir = tempfile::tempdir().unwrap();

		// Group-writable, world-writable or both
		for mode in [0o777, 0o775, 0o757, 0o722] {
			let file_path =
				create_file_with_mode(temp_dir.path(), &format!("interpreter-{mode:o}"), mode);
			assert!(
				interpreter_is_writable_by_others(file_path.to_str()),
				"mode {mode:o} should be reported"
			);
		}

		// Writable by the owner only
		for mode in [0o755, 0o700, 0o555] {
			let file_path =
				create_file_with_mode(temp_dir.path(), &format!("interpreter-{mode:o}"), mode);
			assert!(
				!interpreter_is_writable_by_others(file_path.to_str()),
				"mode {mode:o} should not be reported"
			);
		}
	}

	#[cfg(unix)]
	#[test]
	fn test_interpreter_is_writable_by_others_follows_symlinks() {
		let temp_dir = tempfile::tempdir().unwrap();
		let link = temp_dir.path().join("interpreter3");

		// A symlink itself reports `777`; only the target counts
		let safe_target = create_file_with_mode(temp_dir.path(), "safe", 0o755);
		std::os::unix::fs::symlink(&safe_target, &link).unwrap();
		assert!(!interpreter_is_writable_by_others(link.to_str()));

		fs::remove_file(&link).unwrap();
		let writable_target = create_file_with_mode(temp_dir.path(), "writable", 0o777);
		std::os::unix::fs::symlink(&writable_target, &link).unwrap();
		assert!(interpreter_is_writable_by_others(link.to_str()));
	}

	#[cfg(unix)]
	#[test]
	fn test_interpreter_is_writable_by_others_ignores_unchecked_values() {
		// No interpreter configured
		assert!(!interpreter_is_writable_by_others(None));

		// A command name is never looked up, even if a writable file of that name exists in
		// a `PATH` directory or in the current directory
		assert!(!interpreter_is_writable_by_others(Some("sh")));

		// Unreadable metadata is skipped; validation reports the missing file
		let temp_dir = tempfile::tempdir().unwrap();
		let missing = temp_dir.path().join("missing-interpreter");
		assert!(!interpreter_is_writable_by_others(missing.to_str()));
	}

	#[test]
	fn test_find_executable_in_path_without_path_variable() {
		assert_eq!(find_executable_in_path("sh", None), None);
	}
}
