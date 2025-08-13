//! Template formatter implementation.
//!
//! This module provides shared functionality for formatting message templates
//! with variable substitution and building match reasons sections for events and functions.
//! It is used by both email notifications and webhook payload builders.

use std::collections::HashMap;

/// Formats a message template by substituting variables and building match reasons sections
/// This function handles both basic variable substitution and special sections like ${events} and ${functions}
///
/// # Arguments
/// * `template` - The message template with variables like ${...}
/// * `variables` - The map of variables to substitute into the template
///
/// # Returns
/// * `String` - Formatted message with variables replaced and match reasons sections built
pub fn format_template(template: &str, variables: &HashMap<String, String>) -> String {
	let mut message = template.to_string();

	// First, substitute basic variables
	for (key, value) in variables {
		message = message.replace(&format!("${{{}}}", key), value);
	}

	// Handle special sections for events and functions
	if template.contains("${functions}") {
		if let Some(functions_section) = build_match_reasons(variables, "functions") {
			message = message.replace("${functions}", &functions_section);
		} else {
			message = message.replace("${functions}", "");
		}
	}

	if template.contains("${events}") {
		if let Some(events_section) = build_match_reasons(variables, "events") {
			message = message.replace("${events}", &events_section);
		} else {
			message = message.replace("${events}", "");
		}
	}

	message
}

/// Builds the "Match reasons" section for events or functions if they are present
/// This function creates formatted sections showing matched events/functions with their signatures and parameters
///
/// # Arguments
/// * `variables` - The map of variables containing event/function data
/// * `prefix` - The prefix to look for ("events" or "functions")
///
/// # Returns
/// * `Option<String>` - Some formatted match reasons section, or None if no matches found
pub fn build_match_reasons(variables: &HashMap<String, String>, prefix: &str) -> Option<String> {
	let mut indexes = Vec::new();

	// Find all signature keys for the given prefix
	for key in variables.keys() {
		if key.starts_with(&format!("{}.", prefix)) && key.ends_with(".signature") {
			if let Some(index_part) = key
				.strip_prefix(&format!("{}.", prefix))
				.and_then(|s| s.strip_suffix(".signature"))
			{
				if let Ok(index) = index_part.parse::<usize>() {
					indexes.push(index);
				}
			}
		}
	}

	if indexes.is_empty() {
		return None;
	}

	indexes.sort();

	let formatted_prefix = prefix[..1].to_uppercase() + &prefix[1..];
	let mut match_reasons = String::from(&format!("\n\n*Matched {}:*\n", formatted_prefix));
	let last_index = *indexes.last().unwrap(); // Safe because we checked indexes.is_empty() above

	for (reason_number, &index) in indexes.iter().enumerate() {
		let signature_key = format!("{}.{}.signature", prefix, index);
		if let Some(signature) = variables.get(&signature_key) {
			// Display uses 1-based indexing for user clarity
			match_reasons.push_str(&format!("\n*Reason {}*\n", reason_number + 1));
			match_reasons.push_str(&format!("\n*Signature:* `{}`\n", signature));

			match_reasons.push_str("\n*Params:*\n");

			let mut params = Vec::new();
			for (key, value) in variables {
				if key.starts_with(&format!("{}.{}.args.", prefix, index)) {
					if let Some(param_name) =
						key.strip_prefix(&format!("{}.{}.args.", prefix, index))
					{
						params.push((param_name.to_string(), value.clone()));
					}
				}
			}

			params.sort_by(|a, b| a.0.cmp(&b.0));

			for (param_name, param_value) in params {
				match_reasons.push_str(&format!("\n{}: `{}`", param_name, param_value));
			}

			if index != last_index {
				match_reasons.push('\n');
			}
		}
	}

	Some(match_reasons)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_format_template_with_events() {
		let template = "Transaction detected: ${transaction.hash}\n\n${events}";
		let variables = HashMap::from([
			(
				"transaction.hash".to_string(),
				"0x1234567890abcdef".to_string(),
			),
			(
				"events.0.signature".to_string(),
				"Transfer(address,address,uint256)".to_string(),
			),
			("events.0.args.from".to_string(), "0x1234".to_string()),
			("events.0.args.to".to_string(), "0x5678".to_string()),
			("events.0.args.value".to_string(), "1000000".to_string()),
		]);

		let result = format_template(template, &variables);
		let expected = "Transaction detected: 0x1234567890abcdef\n\n\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`\nvalue: `1000000`";
		assert_eq!(result, expected);
	}

	#[test]
	fn test_format_template_with_functions() {
		let template = "Transaction detected: ${transaction.hash}\n\n${functions}";
		let variables = HashMap::from([
			(
				"transaction.hash".to_string(),
				"0x1234567890abcdef".to_string(),
			),
			(
				"functions.0.signature".to_string(),
				"transfer(address,uint256)".to_string(),
			),
			("functions.0.args.to".to_string(), "0x1234".to_string()),
			("functions.0.args.amount".to_string(), "1000000".to_string()),
		]);

		let result = format_template(template, &variables);
		let expected = "Transaction detected: 0x1234567890abcdef\n\n\n\n*Matched Functions:*\n\n*Reason 1*\n\n*Signature:* `transfer(address,uint256)`\n\n*Params:*\n\namount: `1000000`\nto: `0x1234`";
		assert_eq!(result, expected);
	}

	#[test]
	fn test_format_template_with_both_events_and_functions() {
		let template = "Transaction detected: ${transaction.hash}\n\n${functions}\n\n${events}";
		let variables = HashMap::from([
			(
				"transaction.hash".to_string(),
				"0x1234567890abcdef".to_string(),
			),
			(
				"events.0.signature".to_string(),
				"Transfer(address,address,uint256)".to_string(),
			),
			("events.0.args.from".to_string(), "0x1234".to_string()),
			("events.0.args.to".to_string(), "0x5678".to_string()),
			(
				"functions.0.signature".to_string(),
				"transfer(address,uint256)".to_string(),
			),
			("functions.0.args.to".to_string(), "0x9abc".to_string()),
			("functions.0.args.amount".to_string(), "750000".to_string()),
		]);

		let result = format_template(template, &variables);
		let expected = "Transaction detected: 0x1234567890abcdef\n\n\n\n*Matched Functions:*\n\n*Reason 1*\n\n*Signature:* `transfer(address,uint256)`\n\n*Params:*\n\namount: `750000`\nto: `0x9abc`\n\n\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`";
		assert_eq!(result, expected);
	}

	#[test]
	fn test_format_template_with_no_events_removes_variable() {
		let template = "Transaction detected: ${transaction.hash}\n\n${events}";
		let variables = HashMap::from([
			(
				"transaction.hash".to_string(),
				"0x1234567890abcdef".to_string(),
			),
			// No events variables present
		]);

		let result = format_template(template, &variables);
		let expected = "Transaction detected: 0x1234567890abcdef\n\n";
		assert_eq!(result, expected);
	}

	#[test]
	fn test_format_template_with_no_functions_removes_variable() {
		let template = "Transaction detected: ${transaction.hash}\n\n${functions}";
		let variables = HashMap::from([
			(
				"transaction.hash".to_string(),
				"0x1234567890abcdef".to_string(),
			),
			// No functions variables present
		]);

		let result = format_template(template, &variables);
		let expected = "Transaction detected: 0x1234567890abcdef\n\n";
		assert_eq!(result, expected);
	}

	#[test]
	fn test_build_match_reasons_single_event() {
		let variables = HashMap::from([
			(
				"events.0.signature".to_string(),
				"Transfer(address,address,uint256)".to_string(),
			),
			("events.0.args.from".to_string(), "0x1234".to_string()),
			("events.0.args.to".to_string(), "0x5678".to_string()),
		]);

		let result = build_match_reasons(&variables, "events");
		assert!(result.is_some());
		let expected = "\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_build_match_reasons_multiple_events() {
		let variables = HashMap::from([
			(
				"events.0.signature".to_string(),
				"Transfer(address,address,uint256)".to_string(),
			),
			("events.0.args.from".to_string(), "0x1234".to_string()),
			("events.0.args.to".to_string(), "0x5678".to_string()),
			(
				"events.1.signature".to_string(),
				"Approval(address,address,uint256)".to_string(),
			),
			(
				"events.1.args.owner".to_string(),
				"0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6".to_string(),
			),
			("events.1.args.spender".to_string(), "0x1234".to_string()),
			("events.1.args.value".to_string(), "1000000000".to_string()),
		]);

		let result = build_match_reasons(&variables, "events");
		assert!(result.is_some());
		let expected = "\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`\n\n*Reason 2*\n\n*Signature:* `Approval(address,address,uint256)`\n\n*Params:*\n\nowner: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\nspender: `0x1234`\nvalue: `1000000000`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_build_match_reasons_single_function() {
		let variables = HashMap::from([
			(
				"functions.0.signature".to_string(),
				"transfer(address,uint256)".to_string(),
			),
			("functions.0.args.to".to_string(), "0x1234".to_string()),
			("functions.0.args.amount".to_string(), "1000000".to_string()),
		]);

		let result = build_match_reasons(&variables, "functions");
		assert!(result.is_some());
		let expected = "\n\n*Matched Functions:*\n\n*Reason 1*\n\n*Signature:* `transfer(address,uint256)`\n\n*Params:*\n\namount: `1000000`\nto: `0x1234`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_build_match_reasons_multiple_functions() {
		let variables = HashMap::from([
			(
				"functions.0.signature".to_string(),
				"transfer(address,uint256)".to_string(),
			),
			("functions.0.args.to".to_string(), "0x1234".to_string()),
			("functions.0.args.amount".to_string(), "1000000".to_string()),
			(
				"functions.1.signature".to_string(),
				"approve(address,uint256)".to_string(),
			),
			(
				"functions.1.args.spender".to_string(),
				"0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6".to_string(),
			),
			("functions.1.args.amount".to_string(), "500000".to_string()),
		]);

		let result = build_match_reasons(&variables, "functions");
		assert!(result.is_some());
		let expected = "\n\n*Matched Functions:*\n\n*Reason 1*\n\n*Signature:* `transfer(address,uint256)`\n\n*Params:*\n\namount: `1000000`\nto: `0x1234`\n\n*Reason 2*\n\n*Signature:* `approve(address,uint256)`\n\n*Params:*\n\namount: `500000`\nspender: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_build_match_reasons_no_events() {
		let variables = HashMap::from([
			("transaction.hash".to_string(), "0x1234".to_string()),
			("monitor.name".to_string(), "Test Monitor".to_string()),
		]);

		let result = build_match_reasons(&variables, "events");
		assert!(result.is_none());
	}

	#[test]
	fn test_build_match_reasons_no_functions() {
		let variables = HashMap::from([
			("transaction.hash".to_string(), "0x1234".to_string()),
			("monitor.name".to_string(), "Test Monitor".to_string()),
		]);

		let result = build_match_reasons(&variables, "functions");
		assert!(result.is_none());
	}

	#[test]
	fn test_build_match_reasons_out_of_order() {
		let variables = HashMap::from([
			(
				"events.2.signature".to_string(),
				"ValueChanged(uint256)".to_string(),
			),
			("events.2.args.value".to_string(), "1000000".to_string()),
			(
				"events.0.signature".to_string(),
				"Transfer(address,address,uint256)".to_string(),
			),
			("events.0.args.from".to_string(), "0x1234".to_string()),
			("events.0.args.to".to_string(), "0x5678".to_string()),
		]);

		let result = build_match_reasons(&variables, "events");
		assert!(result.is_some());
		let expected = "\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`\n\n*Reason 2*\n\n*Signature:* `ValueChanged(uint256)`\n\n*Params:*\n\nvalue: `1000000`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_build_match_reasons_invalid_index_format() {
		let variables = HashMap::from([
			// Invalid index format - not a number
			("events.abc.signature".to_string(), "Transfer".to_string()),
			// Invalid index format - negative number
			("events.-1.signature".to_string(), "Transfer".to_string()),
			// Valid index format
			(
				"events.0.signature".to_string(),
				"Transfer(address,address,uint256)".to_string(),
			),
			("events.0.args.from".to_string(), "0x1234".to_string()),
		]);

		let result = build_match_reasons(&variables, "events");
		// Should only include the valid event 0, skipping invalid formats
		assert!(result.is_some());
		let result_str = result.unwrap();
		assert!(result_str.contains("Transfer(address,address,uint256)"));
		assert!(!result_str.contains("abc")); // Should not contain invalid index
		assert!(!result_str.contains("-1")); // Should not contain negative index
	}
}
