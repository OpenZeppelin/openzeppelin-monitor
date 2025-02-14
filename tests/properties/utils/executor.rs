use crate::properties::strategies::process_output_strategy;
use openzeppelin_monitor::utils::script::{process_script_output, ScriptError};
use proptest::{prelude::*, test_runner::Config};

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
					ScriptError::ParseError(msg) => {
						prop_assert!(
							msg == "Script produced no output" ||
							msg == "No output from script" ||
							msg.starts_with("Last line of output is not a valid boolean")
						);
					},
					ScriptError::ExecutionError(msg) => {
						prop_assert_eq!(msg, String::from_utf8_lossy(&output.stderr).to_string());
					},
					_ => {
						prop_assert!(false, "Expected ParseError, got different error type");
					}
				}
			}
		}
	}
}
