use crate::properties::strategies::trigger_strategy;

use openzeppelin_monitor::{
	models::{
		ConfigLoader, NotificationMessage, SecretString, SecretValue, TriggerType,
		TriggerTypeConfig,
	},
	repositories::{TriggerRepository, TriggerRepositoryTrait},
};
use proptest::{prelude::*, test_runner::Config};

const MIN_TEST_CASES: usize = 1;
const MAX_TEST_CASES: usize = 10;

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	// Data Consistency & Round-trip Tests
	#[test]
	fn test_roundtrip(
		triggers in proptest::collection::hash_map(
			"[a-z0-9_]{1,10}",
			trigger_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	){
		// Simulate saving and reloading from a repository
		let repo = TriggerRepository { triggers: triggers.clone() };
		let reloaded_triggers = repo.get_all();

		prop_assert_eq!(triggers, reloaded_triggers); // Ensure roundtrip consistency
	}

	// Query Operations Tests
	#[test]
	fn test_query_operations(
		triggers in proptest::collection::hash_map(
			"[a-z0-9_]{1,10}",
			trigger_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		let repo = TriggerRepository { triggers: triggers.clone() };

		// Test get by slug
		for (slug, trigger) in &triggers {
			let retrieved = repo.get(slug);
			prop_assert_eq!(Some(trigger), retrieved.as_ref());
		}

		// Test get_all consistency
		let all_triggers = repo.get_all();
		prop_assert_eq!(triggers, all_triggers);

		// Test non-existent name
		prop_assert_eq!(None, repo.get("non_existent_name"));
	}

	// Empty/Null Handling Tests
	#[test]
	fn test_empty_repository(
		_triggers in proptest::collection::hash_map(
			"[a-zA-Z0-9_]{1,10}",
			trigger_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		let empty_repo = TriggerRepository { triggers: std::collections::HashMap::new() };
		// Test empty repository operations
		prop_assert!(empty_repo.get_all().is_empty());
		prop_assert_eq!(None, empty_repo.get("any_id"));
	}

	#[test]
	fn test_config_validation(
		triggers in proptest::collection::vec(
			trigger_strategy(),
			MIN_TEST_CASES..MAX_TEST_CASES
		)
	) {
		for trigger in triggers {
			// Valid trigger should pass validation
			prop_assert!(trigger.validate().is_ok());

			// Test invalid trigger name
			let mut invalid_trigger = trigger.clone();
			invalid_trigger.name = "".to_string();
			prop_assert!(invalid_trigger.validate().is_err());

			// Test invalid cases
			match &trigger.trigger_type {
				TriggerType::Slack => {
					if let TriggerTypeConfig::Slack { slack_url: _, message: _, retry_policy: _ } = &trigger.config {
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Slack { slack_url, .. } = &mut invalid_trigger.config {
							*slack_url = SecretValue::Plain(SecretString::new("not-a-url".to_string())); // Invalid URL format
						}
						prop_assert!(invalid_trigger.validate().is_err());


						// Test empty title
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Slack { message: m, .. } = &mut invalid_trigger.config {
							*m = NotificationMessage {
								title: "".to_string(),
								body: "test".to_string(),
							};
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test empty body
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Slack { message: m, .. } = &mut invalid_trigger.config {
							*m = NotificationMessage {
								title: "Alert".to_string(),
								body: "".to_string(),
							};
						}
						prop_assert!(invalid_trigger.validate().is_err());
					}
				}
				TriggerType::Email => {
					if let TriggerTypeConfig::Email { host: _, port: _, username: _, password: _, message: _, sender: _, recipients: _, retry_policy: _ } = &trigger.config {
						// Test empty recipients
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Email { recipients: r, .. } = &mut invalid_trigger.config {
							r.clear();
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test invalid host
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Email { host: h, .. } = &mut invalid_trigger.config {
							*h = "not-a-host".to_string();
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test whitespace-only subject
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Email { message: m, .. } = &mut invalid_trigger.config {
							*m = NotificationMessage {
								title: "   ".to_string(),
								body: "".to_string(),
							};
						}
						prop_assert!(invalid_trigger.validate().is_err());
					}
				}
				TriggerType::Webhook => {
					if let TriggerTypeConfig::Webhook { url: _, method: _, headers: _, secret: _, message: _, retry_policy: _ } = &trigger.config {
						// Test invalid method
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Webhook { method: m, .. } = &mut invalid_trigger.config {
							*m = Some("INVALID_METHOD".to_string());
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test invalid URL
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Webhook { url: u, .. } = &mut invalid_trigger.config {
							*u = SecretValue::Plain(SecretString::new("not-a-url".to_string()));
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test empty title
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Webhook { message: m, .. } = &mut invalid_trigger.config {
							*m = NotificationMessage {
								title: "".to_string(),
								body: "test".to_string(),
							};
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test empty body
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Webhook { message: m, .. } = &mut invalid_trigger.config {
							*m = NotificationMessage {
								title: "Alert".to_string(),
								body: "".to_string(),
							};
						}
						prop_assert!(invalid_trigger.validate().is_err());
					}
				}
				TriggerType::Discord => {
					if let TriggerTypeConfig::Discord { discord_url: _, message: _, retry_policy: _ } = &trigger.config {
						// Test invalid URL
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Discord { discord_url: u, .. } = &mut invalid_trigger.config {
							*u = SecretValue::Plain(SecretString::new("not-a-url".to_string()));
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test empty title
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Discord { message: m, .. } = &mut invalid_trigger.config {
							*m = NotificationMessage {
								title: "".to_string(),
								body: "test".to_string(),
							};
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test empty body
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Discord { message: m, .. } = &mut invalid_trigger.config {
							*m = NotificationMessage {
								title: "Alert".to_string(),
								body: "".to_string(),
							};
						}
						prop_assert!(invalid_trigger.validate().is_err());
					}
				}
				TriggerType::Telegram => {
					if let TriggerTypeConfig::Telegram { token: _, chat_id: _, disable_web_preview: _, message: _, retry_policy: _ } = &trigger.config {
						// Test invalid token
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Telegram { token: t, .. } = &mut invalid_trigger.config {
							*t = SecretValue::Plain(SecretString::new("INVALID_TOKEN".to_string()));
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test invalid chat id
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Telegram { chat_id: c, .. } = &mut invalid_trigger.config {
							*c = "   ".to_string();
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test empty title
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Telegram { message: m, .. } = &mut invalid_trigger.config {
							*m = NotificationMessage {
								title: "".to_string(),
								body: "test".to_string(),
							};
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test empty body
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Telegram { message: m, .. } = &mut invalid_trigger.config {
							*m = NotificationMessage {
								title: "Alert".to_string(),
								body: "".to_string(),
							};
						}
						prop_assert!(invalid_trigger.validate().is_err());
					}
				}
				TriggerType::Script => {
					if let TriggerTypeConfig::Script { script_path: _, arguments: _, language: _, timeout_ms: _ } = &trigger.config {
						// Test invalid path
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Script { script_path: p, .. } = &mut invalid_trigger.config {
							*p = "invalid/path/no-extension".to_string();
						}
						prop_assert!(invalid_trigger.validate().is_err());

						// Test empty path
						invalid_trigger = trigger.clone();
						if let TriggerTypeConfig::Script { script_path: p, .. } = &mut invalid_trigger.config {
							*p = "".to_string();
						}
						prop_assert!(invalid_trigger.validate().is_err());
					}
				}
			}
		}
	}
}
