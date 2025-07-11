//! Property-based tests for email notifications.
//!
//! These tests verify the behavior of the email notification system using property testing,
//! focusing on template variable substitution, message formatting consistency, and edge cases.
//! The tests ensure that the email notification system handles template variables correctly
//! and produces consistent, well-formed output across various input combinations.

use openzeppelin_monitor::services::notification::EmailNotifier;
use proptest::{prelude::*, test_runner::Config};
use std::collections::HashMap;

/// Generates a strategy for creating HashMaps containing template variable key-value pairs.
/// Keys are alphanumeric strings of length 1-10, values are alphanumeric strings (with spaces) of
/// length 1-20.
fn template_variables_strategy() -> impl Strategy<Value = HashMap<String, String>> {
	prop::collection::hash_map("[a-zA-Z0-9_]{1,10}", "[a-zA-Z0-9 ]{1,20}", 1..5)
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	/// Tests that template formatting is idempotent - applying the same variables multiple times
	/// should produce identical results.
	///
	/// # Properties tested
	/// - Multiple calls to format_message with the same variables should return identical results
	/// - Template can contain alphanumeric characters, spaces, $, {, }, and _
	#[test]
	fn test_notification_template_idempotency(
		template in "[a-zA-Z0-9 ${}_]{1,100}",
		vars in template_variables_strategy()
	) {
		let first_pass = EmailNotifier::format_message(&template, &vars);
		let second_pass = EmailNotifier::format_message(&template, &vars);

		prop_assert_eq!(first_pass, second_pass);
	}

	/// Tests that variable substitution handles variable boundaries correctly and doesn't result
	/// in partial or malformed substitutions.
	///
	/// # Properties tested
	/// - Templates containing ${variable} patterns are processed correctly
	/// - No partial substitution artifacts (${, }) remain in the output
	#[test]
	fn test_notification_variable_boundaries(
		template in "[a-zA-Z0-9 ]{0,50}\\$\\{[a-z_]+\\}[a-zA-Z0-9 ]{0,50}",
		vars in template_variables_strategy()
	) {
		let formatted = EmailNotifier::format_message(&template, &vars);

		// Verify no partial variable substitutions occurred
		prop_assert!(!formatted.contains("${{"));
		prop_assert!(!formatted.contains("}}"));
	}

	/// Tests that templates with no matching variables remain unchanged.
	///
	// # Properties tested
	/// - Template remains identical when processed with an empty variables map
	/// - No unexpected substitutions occur with empty variable set
	#[test]
	fn test_notification_empty_variables(
		template in "[a-zA-Z0-9 ${}_]{1,100}"
	) {
		let empty_vars = HashMap::new();
		let formatted = EmailNotifier::format_message(&template, &empty_vars);
		let html_template = EmailNotifier::markdown_to_html(&template);
		// Template should remain unchanged when no variables are provided
		prop_assert_eq!(formatted, html_template);
	}
}
