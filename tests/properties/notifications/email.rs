use openzeppelin_monitor::services::notification::{EmailContent, EmailNotifier, SmtpConfig};
use proptest::{prelude::*, test_runner::Config};
use std::collections::HashMap;

fn template_variables_strategy() -> impl Strategy<Value = HashMap<String, String>> {
	prop::collection::hash_map("[a-zA-Z0-9_]{1,10}", "[a-zA-Z0-9 ]{1,20}", 1..5)
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn test_notification_template_idempotency(
		template in "[a-zA-Z0-9 ${}_]{1,100}",
		vars in template_variables_strategy()
	) {
		let notifier = EmailNotifier::new(
			SmtpConfig {
				host: "smtp.test.com".to_string(),
				port: 465,
				username: "test".to_string(),
				password: "test".to_string(),
			},
			EmailContent {
				subject: "Test".to_string(),
				body_template: template.clone(),
				sender: "test@test.com".parse().unwrap(),
				recipients: vec!["recipient@test.com".parse().unwrap()],
			}
		);

		let first_pass = notifier.format_message(&vars);
		let second_pass = notifier.format_message(&vars);

		prop_assert_eq!(first_pass, second_pass);
	}

	#[test]
	fn test_notification_variable_boundaries(
		template in "[a-zA-Z0-9 ]{0,50}\\$\\{[a-z_]+\\}[a-zA-Z0-9 ]{0,50}",
		vars in template_variables_strategy()
	) {
		let notifier = EmailNotifier::new(
			SmtpConfig {
				host: "smtp.test.com".to_string(),
				port: 465,
				username: "test".to_string(),
				password: "test".to_string(),
			},
			EmailContent {
				subject: "Test".to_string(),
				body_template: template.clone(),
				sender: "test@test.com".parse().unwrap(),
				recipients: vec!["recipient@test.com".parse().unwrap()],
			}
		);

		let formatted = notifier.format_message(&vars);

		// Verify no partial variable substitutions occurred
		prop_assert!(!formatted.contains("${{"));
		prop_assert!(!formatted.contains("}}"));
	}

	#[test]
	fn test_notification_empty_variables(
		template in "[a-zA-Z0-9 ${}_]{1,100}"
	) {
		let notifier = EmailNotifier::new(
			SmtpConfig {
				host: "smtp.test.com".to_string(),
				port: 465,
				username: "test".to_string(),
				password: "test".to_string(),
			},
			EmailContent {
				subject: "Test".to_string(),
				body_template: template.clone(),
				sender: "test@test.com".parse().unwrap(),
				recipients: vec!["recipient@test.com".parse().unwrap()],
			}
		);

		let empty_vars = HashMap::new();
		let formatted = notifier.format_message(&empty_vars);

		// Template should remain unchanged when no variables are provided
		prop_assert_eq!(formatted, template);
	}
}
