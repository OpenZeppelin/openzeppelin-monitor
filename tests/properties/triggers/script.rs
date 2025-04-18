use crate::properties::strategies::process_output_strategy;
use openzeppelin_monitor::services::trigger::process_script_output;
use proptest::{prelude::*, test_runner::Config};
use std::os::unix::process::ExitStatusExt;

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn test_process_script_output(output in process_output_strategy()) {
		let result = process_script_output(output.clone(), false);
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
				let err_msg = err.to_string();
				if err_msg.contains("Last line of output is not a valid boolean") {
					prop_assert!(true);
				} else if output.stderr.is_empty() {
					prop_assert!(!err_msg.is_empty(), "Error should have a message");
				} else {
					prop_assert!(err_msg.contains(&*String::from_utf8_lossy(&output.stderr)));
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

		let result = process_script_output(output, false);

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

		let output_clone = output.clone();

		let result = process_script_output(output, false);
		prop_assert!(result.is_err());

		if let Err(err) = result {
			let err_msg = err.to_string();
			if err_msg.contains("Failed to process script output") {
				prop_assert!(true);
			} else if output_clone.stderr.is_empty() {
				prop_assert!(!err_msg.is_empty(), "Error should have a message");
			} else {
				prop_assert!(err_msg.contains(&*String::from_utf8_lossy(&output_clone.stderr)));
			}
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
			value,
			spaces_after
		);

		let output = std::process::Output {
			status: std::process::ExitStatus::from_raw(0),
			stdout: output_str.into_bytes(),
			stderr: Vec::new(),
		};

		let result = process_script_output(output, false);
		prop_assert!(result.is_ok());
		prop_assert_eq!(result.unwrap(), value);
	}

	#[test]
	fn test_script_executor_with_ignore_output(
		lines in prop::collection::vec(any::<String>(), 0..10),
		exit_code in 0..2i32
	) {
		let output_content = lines.join("\n");

		let output = std::process::Output {
			status: std::process::ExitStatus::from_raw(exit_code),
			stdout: output_content.into_bytes(),
			stderr: Vec::new(),
		};

		let result = process_script_output(output, true);

		// When ignore_output is true, the result should be:
		// - Ok(true) if exit_code is 0
		// - Err(ExecutionError) if exit_code is not 0
		if exit_code == 0 {
			prop_assert!(result.is_ok());
			prop_assert!(result.unwrap());
		} else {
			prop_assert!(result.is_err());
			if let Err(e) = result {
				prop_assert!(e.to_string().contains("Script execution failed"));
			} else {
				prop_assert!(false, "Expected ExecutionError");
			}
		}
	}
}
