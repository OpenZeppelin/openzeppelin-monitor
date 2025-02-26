use crate::properties::strategies::process_output_strategy;
use openzeppelin_monitor::utils::{process_script_output, ScriptError};
use proptest::{prelude::*, test_runner::Config};
use std::os::unix::process::ExitStatusExt;

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn test_process_script_output(output in process_output_strategy()) {
		let result = process_script_output(output.clone());
		if let Ok(parse_result) = result {
			match parse_result {
				true => {
					prop_assert!(result.is_ok());
					prop_assert!(result.unwrap());
				},
				false => {
					prop_assert!(result.is_ok());
					prop_assert!(!result.unwrap());
				},
			}
		} else {
			prop_assert!(result.is_err());
			if let Err(err) = result {
				match err {
					ScriptError::ExecutionError(msg) => {
						prop_assert_eq!(msg, String::from_utf8_lossy(&output.stderr).to_string());
					},
					ScriptError::ParseError(msg) => {
						prop_assert!(msg == "Last line of output is not a valid boolean");
					},
					ScriptError::NotFound(msg) => {
						prop_assert_eq!(msg, String::from_utf8_lossy(&output.stderr).to_string());
					},
					ScriptError::SystemError(msg) => {
						prop_assert_eq!(msg, String::from_utf8_lossy(&output.stderr).to_string());
					},
				}
			}
		}
	}

	#[test]
	fn test_script_executor_with_varying_outputs(
		lines in prop::collection::vec(any::<String>(), 0..10),
		append_bool in prop::bool::ANY
	) {
		let output_content = lines.join("\n");
		let final_output = if append_bool {
			format!("{}\n{}", output_content, "true")
		} else {
			output_content
		};

		let output = std::process::Output {
			status: std::process::ExitStatus::from_raw(0),
			stdout: final_output.into_bytes(),
			stderr: Vec::new(),
		};

		let result = process_script_output(output);

		if append_bool {
			prop_assert!(result.is_ok());
			prop_assert!(result.unwrap());
		} else {
			prop_assert!(result.is_err());
		}
	}

	#[test]
	fn test_script_executor_with_error_outputs(
		error_msg in ".*",
		exit_code in 1..255i32
	) {
		let output = std::process::Output {
			status: std::process::ExitStatus::from_raw(exit_code),
			stdout: Vec::new(),
			stderr: error_msg.clone().into_bytes(),
		};

		let result = process_script_output(output);
		prop_assert!(result.is_err());

		if let Err(ScriptError::ExecutionError(msg)) = result {
			prop_assert_eq!(msg, error_msg);
		} else {
			prop_assert!(false, "Expected ExecutionError");
		}
	}

	#[test]
	fn test_script_executor_whitespace_handling(
		spaces_before in " *",
		spaces_after in " *",
		value in prop::bool::ANY
	) {
		let output_str = format!("{}{}{}",
			spaces_before,
			value.to_string(),
			spaces_after
		);

		let output = std::process::Output {
			status: std::process::ExitStatus::from_raw(0),
			stdout: output_str.into_bytes(),
			stderr: Vec::new(),
		};

		let result = process_script_output(output);
		prop_assert!(result.is_ok());
		prop_assert_eq!(result.unwrap(), value);
	}
}
