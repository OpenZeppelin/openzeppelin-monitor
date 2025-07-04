//! Test helper utilities for Trigger configuration
//!
//! - `TriggerBuilder`: Builder for creating test Trigger instances

use crate::{
	models::{
		NotificationMessage, ScriptLanguage, SecretString, SecretValue, Trigger, TriggerType,
		TriggerTypeConfig,
	},
	utils::RetryConfig,
};
use email_address::EmailAddress;

/// Builder for creating test Trigger instances
pub struct TriggerBuilder {
	name: String,
	trigger_type: TriggerType,
	config: TriggerTypeConfig,
}

impl Default for TriggerBuilder {
	fn default() -> Self {
		Self {
			name: "test_trigger".to_string(),
			trigger_type: TriggerType::Webhook,
			config: TriggerTypeConfig::Webhook {
				url: SecretValue::Plain(SecretString::new(
					"https://api.example.com/webhook".to_string(),
				)),
				secret: None,
				method: Some("POST".to_string()),
				headers: None,
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
				retry_policy: RetryConfig::default(),
			},
		}
	}
}

impl TriggerBuilder {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn name(mut self, name: &str) -> Self {
		self.name = name.to_string();
		self
	}

	pub fn config(mut self, config: TriggerTypeConfig) -> Self {
		self.config = config;
		self
	}

	pub fn webhook(mut self, url: &str) -> Self {
		self.trigger_type = TriggerType::Webhook;
		self.config = TriggerTypeConfig::Webhook {
			url: SecretValue::Plain(SecretString::new(url.to_string())),
			secret: None,
			method: Some("POST".to_string()),
			headers: None,
			message: NotificationMessage {
				title: "Alert".to_string(),
				body: "Test message".to_string(),
			},
			retry_policy: RetryConfig::default(),
		};
		self
	}

	pub fn slack(mut self, webhook_url: &str) -> Self {
		self.trigger_type = TriggerType::Slack;
		self.config = TriggerTypeConfig::Slack {
			slack_url: SecretValue::Plain(SecretString::new(webhook_url.to_string())),
			message: NotificationMessage {
				title: "Alert".to_string(),
				body: "Test message".to_string(),
			},
			retry_policy: RetryConfig::default(),
		};
		self
	}

	pub fn discord(mut self, webhook_url: &str) -> Self {
		self.trigger_type = TriggerType::Discord;
		self.config = TriggerTypeConfig::Discord {
			discord_url: SecretValue::Plain(SecretString::new(webhook_url.to_string())),
			message: NotificationMessage {
				title: "Alert".to_string(),
				body: "Test message".to_string(),
			},
			retry_policy: RetryConfig::default(),
		};
		self
	}

	pub fn telegram(mut self, token: &str, chat_id: &str, disable_web_preview: bool) -> Self {
		self.trigger_type = TriggerType::Telegram;
		self.config = TriggerTypeConfig::Telegram {
			token: SecretValue::Plain(SecretString::new(token.to_string())),
			chat_id: chat_id.to_string(),
			disable_web_preview: Some(disable_web_preview),
			message: NotificationMessage {
				title: "Test title".to_string(),
				body: "Test message".to_string(),
			},
			retry_policy: RetryConfig::default(),
		};
		self
	}

	pub fn telegram_token(mut self, token: SecretValue) -> Self {
		if let TriggerTypeConfig::Telegram { token: t, .. } = &mut self.config {
			*t = token;
		}
		self
	}

	pub fn script(mut self, script_path: &str, language: ScriptLanguage) -> Self {
		self.trigger_type = TriggerType::Script;
		self.config = TriggerTypeConfig::Script {
			script_path: script_path.to_string(),
			arguments: None,
			language,
			timeout_ms: 1000,
		};
		self
	}

	pub fn script_arguments(mut self, arguments: Vec<String>) -> Self {
		if let TriggerTypeConfig::Script { arguments: a, .. } = &mut self.config {
			*a = Some(arguments);
		}
		self
	}

	pub fn script_timeout_ms(mut self, timeout_ms: u32) -> Self {
		if let TriggerTypeConfig::Script { timeout_ms: t, .. } = &mut self.config {
			*t = timeout_ms;
		}
		self
	}

	pub fn message(mut self, title: &str, body: &str) -> Self {
		match &mut self.config {
			TriggerTypeConfig::Webhook { message, .. }
			| TriggerTypeConfig::Slack { message, .. }
			| TriggerTypeConfig::Discord { message, .. }
			| TriggerTypeConfig::Telegram { message, .. }
			| TriggerTypeConfig::Email { message, .. } => {
				message.title = title.to_string();
				message.body = body.to_string();
			}
			_ => {}
		}
		self
	}

	pub fn trigger_type(mut self, trigger_type: TriggerType) -> Self {
		self.trigger_type = trigger_type;
		self
	}

	pub fn email(
		mut self,
		host: &str,
		username: &str,
		password: &str,
		sender: &str,
		recipients: Vec<&str>,
	) -> Self {
		self.trigger_type = TriggerType::Email;
		self.config = TriggerTypeConfig::Email {
			host: host.to_string(),
			port: Some(587),
			username: SecretValue::Plain(SecretString::new(username.to_string())),
			password: SecretValue::Plain(SecretString::new(password.to_string())),
			message: NotificationMessage {
				title: "Test Subject".to_string(),
				body: "Test Body".to_string(),
			},
			sender: EmailAddress::new_unchecked(sender),
			recipients: recipients
				.into_iter()
				.map(EmailAddress::new_unchecked)
				.collect(),
			retry_policy: RetryConfig::default(),
		};
		self
	}

	pub fn email_port(mut self, port: u16) -> Self {
		if let TriggerTypeConfig::Email { port: p, .. } = &mut self.config {
			*p = Some(port);
		}
		self
	}

	pub fn email_subject(mut self, subject: &str) -> Self {
		if let TriggerTypeConfig::Email { message, .. } = &mut self.config {
			message.title = subject.to_string();
		}
		self
	}

	pub fn email_username(mut self, username: SecretValue) -> Self {
		if let TriggerTypeConfig::Email { username: u, .. } = &mut self.config {
			*u = username;
		}
		self
	}

	pub fn email_password(mut self, password: SecretValue) -> Self {
		if let TriggerTypeConfig::Email { password: p, .. } = &mut self.config {
			*p = password;
		}
		self
	}

	pub fn webhook_method(mut self, method: &str) -> Self {
		if let TriggerTypeConfig::Webhook { method: m, .. } = &mut self.config {
			*m = Some(method.to_string());
		}
		self
	}

	pub fn webhook_secret(mut self, secret: SecretValue) -> Self {
		if let TriggerTypeConfig::Webhook { secret: s, .. } = &mut self.config {
			*s = Some(secret);
		}
		self
	}

	pub fn webhook_headers(mut self, headers: std::collections::HashMap<String, String>) -> Self {
		if let TriggerTypeConfig::Webhook { headers: h, .. } = &mut self.config {
			*h = Some(headers);
		}
		self
	}

	pub fn url(mut self, url: SecretValue) -> Self {
		self.config = match self.config {
			TriggerTypeConfig::Webhook {
				url: _,
				method,
				headers,
				secret,
				message,
				retry_policy,
			} => TriggerTypeConfig::Webhook {
				url,
				method,
				headers,
				secret,
				message,
				retry_policy,
			},
			TriggerTypeConfig::Discord {
				discord_url: _,
				message,
				retry_policy,
			} => TriggerTypeConfig::Discord {
				discord_url: url,
				message,
				retry_policy,
			},
			TriggerTypeConfig::Slack {
				slack_url: _,
				message,
				retry_policy,
			} => TriggerTypeConfig::Slack {
				slack_url: url,
				message,
				retry_policy,
			},
			config => config,
		};
		self
	}

	pub fn build(self) -> Trigger {
		Trigger {
			name: self.name,
			trigger_type: self.trigger_type,
			config: self.config,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_trigger() {
		let trigger = TriggerBuilder::new().build();

		assert_eq!(trigger.name, "test_trigger");
		assert_eq!(trigger.trigger_type, TriggerType::Webhook);

		match trigger.config {
			TriggerTypeConfig::Webhook { url, method, .. } => {
				assert_eq!(url.as_ref().to_string(), "https://api.example.com/webhook");
				assert_eq!(method, Some("POST".to_string()));
			}
			_ => panic!("Expected webhook config"),
		}
	}

	#[test]
	fn test_trigger_with_config() {
		let trigger = TriggerBuilder::new()
			.name("my_trigger")
			.config(TriggerTypeConfig::Webhook {
				url: SecretValue::Plain(SecretString::new(
					"https://api.example.com/webhook".to_string(),
				)),
				secret: Some(SecretValue::Plain(SecretString::new("secret".to_string()))),
				method: Some("POST".to_string()),
				headers: None,
				message: NotificationMessage {
					title: "Alert".to_string(),
					body: "Test message".to_string(),
				},
				retry_policy: RetryConfig::default(),
			})
			.build();

		assert_eq!(trigger.name, "my_trigger");
		assert_eq!(trigger.trigger_type, TriggerType::Webhook);

		match trigger.config {
			TriggerTypeConfig::Webhook { url, method, .. } => {
				assert_eq!(url.as_ref().to_string(), "https://api.example.com/webhook");
				assert_eq!(method, Some("POST".to_string()));
			}
			_ => panic!("Expected webhook config"),
		}
	}

	#[test]
	fn test_webhook_trigger() {
		let trigger = TriggerBuilder::new()
			.name("my_webhook")
			.webhook("https://webhook.example.com")
			.message("Custom Alert", "Something happened!")
			.build();

		assert_eq!(trigger.name, "my_webhook");
		assert_eq!(trigger.trigger_type, TriggerType::Webhook);

		match trigger.config {
			TriggerTypeConfig::Webhook { url, message, .. } => {
				assert_eq!(url.as_ref().to_string(), "https://webhook.example.com");
				assert_eq!(message.title, "Custom Alert");
				assert_eq!(message.body, "Something happened!");
			}
			_ => panic!("Expected webhook config"),
		}
	}

	#[test]
	fn test_webhook_trigger_with_config() {
		let mut headers = std::collections::HashMap::new();
		headers.insert("Content-Type".to_string(), "application/json".to_string());

		let trigger = TriggerBuilder::new()
			.name("my_webhook")
			.webhook("https://webhook.example.com")
			.webhook_method("POST")
			.webhook_secret(SecretValue::Plain(SecretString::new(
				"secret123".to_string(),
			)))
			.webhook_headers(headers.clone())
			.message("Custom Alert", "Something happened!")
			.build();

		assert_eq!(trigger.name, "my_webhook");
		assert_eq!(trigger.trigger_type, TriggerType::Webhook);

		match trigger.config {
			TriggerTypeConfig::Webhook {
				url,
				method,
				secret,
				headers: h,
				message,
				retry_policy: _,
			} => {
				assert_eq!(url.as_ref().to_string(), "https://webhook.example.com");
				assert_eq!(method, Some("POST".to_string()));
				assert_eq!(
					secret.as_ref().map(|s| s.as_ref().to_string()),
					Some("secret123".to_string())
				);
				assert_eq!(h, Some(headers));
				assert_eq!(message.title, "Custom Alert");
				assert_eq!(message.body, "Something happened!");
			}
			_ => panic!("Expected webhook config"),
		}
	}

	#[test]
	fn test_slack_trigger() {
		let trigger = TriggerBuilder::new()
			.name("slack_alert")
			.slack("https://slack.webhook.com")
			.message("Alert", "Test message")
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Slack);
		match trigger.config {
			TriggerTypeConfig::Slack {
				slack_url,
				message,
				retry_policy: _,
			} => {
				assert_eq!(slack_url.as_ref().to_string(), "https://slack.webhook.com");
				assert_eq!(message.title, "Alert");
				assert_eq!(message.body, "Test message");
			}
			_ => panic!("Expected slack config"),
		}
	}

	#[test]
	fn test_discord_trigger() {
		let trigger = TriggerBuilder::new()
			.name("discord_alert")
			.discord("https://discord.webhook.com")
			.message("Alert", "Test message")
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Discord);
		match trigger.config {
			TriggerTypeConfig::Discord {
				discord_url,
				message,
				retry_policy: _,
			} => {
				assert_eq!(
					discord_url.as_ref().to_string(),
					"https://discord.webhook.com"
				);
				assert_eq!(message.title, "Alert");
				assert_eq!(message.body, "Test message");
			}
			_ => panic!("Expected discord config"),
		}
	}

	#[test]
	fn test_script_trigger() {
		let trigger = TriggerBuilder::new()
			.name("script_trigger")
			.script("test.py", ScriptLanguage::Python)
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Script);
		match trigger.config {
			TriggerTypeConfig::Script {
				script_path,
				language,
				timeout_ms,
				..
			} => {
				assert_eq!(script_path, "test.py");
				assert_eq!(language, ScriptLanguage::Python);
				assert_eq!(timeout_ms, 1000);
			}
			_ => panic!("Expected script config"),
		}
	}

	#[test]
	fn test_script_trigger_with_arguments() {
		let trigger = TriggerBuilder::new()
			.name("script_trigger")
			.script("test.py", ScriptLanguage::Python)
			.script_arguments(vec!["arg1".to_string()])
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Script);
		match trigger.config {
			TriggerTypeConfig::Script { arguments, .. } => {
				assert_eq!(arguments, Some(vec!["arg1".to_string()]));
			}
			_ => panic!("Expected script config"),
		}
	}

	#[test]
	fn test_script_trigger_with_timeout() {
		let trigger = TriggerBuilder::new()
			.name("script_trigger")
			.script("test.py", ScriptLanguage::Python)
			.script_timeout_ms(2000)
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Script);
		match trigger.config {
			TriggerTypeConfig::Script { timeout_ms, .. } => {
				assert_eq!(timeout_ms, 2000);
			}
			_ => panic!("Expected script config"),
		}
	}

	#[test]
	fn test_telegram_trigger() {
		let trigger = TriggerBuilder::new()
			.name("telegram_alert")
			.telegram(
				"1234567890:ABCdefGHIjklMNOpqrSTUvwxYZ123456789", // noboost
				"1234567890",
				false,
			)
			.message("Alert", "Test message")
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Telegram);
		match trigger.config {
			TriggerTypeConfig::Telegram {
				token,
				chat_id,
				message,
				..
			} => {
				assert_eq!(
					token.as_ref().to_string(),
					"1234567890:ABCdefGHIjklMNOpqrSTUvwxYZ123456789".to_string() // noboost
				);
				assert_eq!(chat_id, "1234567890");
				assert_eq!(message.title, "Alert");
				assert_eq!(message.body, "Test message");
			}
			_ => panic!("Expected telegram config"),
		}
	}

	#[test]
	fn test_email_trigger() {
		let trigger = TriggerBuilder::new()
			.name("email_alert")
			.email(
				"smtp.example.com",
				"user",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.email_port(465)
			.email_subject("Custom Subject")
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Email);
		match trigger.config {
			TriggerTypeConfig::Email {
				host,
				port,
				username,
				password,
				message,
				sender,
				recipients,
				..
			} => {
				assert_eq!(host, "smtp.example.com");
				assert_eq!(port, Some(465));
				assert_eq!(username.as_ref().to_string(), "user");
				assert_eq!(password.as_ref().to_string(), "pass");
				assert_eq!(message.title, "Custom Subject");
				assert_eq!(sender.as_str(), "sender@example.com");
				assert_eq!(recipients.len(), 1);
				assert_eq!(recipients[0].as_str(), "recipient@example.com");
			}
			_ => panic!("Expected email config"),
		}
	}

	#[test]
	fn test_telegram_token() {
		let token = SecretValue::Environment("TELEGRAM_TOKEN".to_string());
		let trigger = TriggerBuilder::new()
			.name("telegram_alert")
			.telegram("dummy_token", "1234567890", false)
			.telegram_token(token.clone())
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Telegram);
		match trigger.config {
			TriggerTypeConfig::Telegram { token: t, .. } => {
				assert_eq!(t, token);
			}
			_ => panic!("Expected telegram config"),
		}
	}

	#[test]
	fn test_email_username() {
		let username = SecretValue::Environment("SMTP_USERNAME".to_string());
		let trigger = TriggerBuilder::new()
			.name("email_alert")
			.email(
				"smtp.example.com",
				"dummy_user",
				"pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.email_username(username.clone())
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Email);
		match trigger.config {
			TriggerTypeConfig::Email { username: u, .. } => {
				assert_eq!(u, username);
			}
			_ => panic!("Expected email config"),
		}
	}

	#[test]
	fn test_email_password() {
		let password = SecretValue::Environment("SMTP_PASSWORD".to_string());
		let trigger = TriggerBuilder::new()
			.name("email_alert")
			.email(
				"smtp.example.com",
				"user",
				"dummy_pass",
				"sender@example.com",
				vec!["recipient@example.com"],
			)
			.email_password(password.clone())
			.build();

		assert_eq!(trigger.trigger_type, TriggerType::Email);
		match trigger.config {
			TriggerTypeConfig::Email { password: p, .. } => {
				assert_eq!(p, password);
			}
			_ => panic!("Expected email config"),
		}
	}

	#[test]
	fn test_url() {
		let url = SecretValue::Environment("WEBHOOK_URL".to_string());

		// Test with webhook
		let webhook_trigger = TriggerBuilder::new()
			.name("webhook_alert")
			.webhook("dummy_url")
			.url(url.clone())
			.build();

		assert_eq!(webhook_trigger.trigger_type, TriggerType::Webhook);
		match webhook_trigger.config {
			TriggerTypeConfig::Webhook { url: u, .. } => {
				assert_eq!(u, url);
			}
			_ => panic!("Expected webhook config"),
		}

		// Test with discord
		let discord_trigger = TriggerBuilder::new()
			.name("discord_alert")
			.discord("dummy_url")
			.url(url.clone())
			.build();

		assert_eq!(discord_trigger.trigger_type, TriggerType::Discord);
		match discord_trigger.config {
			TriggerTypeConfig::Discord { discord_url: u, .. } => {
				assert_eq!(u, url);
			}
			_ => panic!("Expected discord config"),
		}

		// Test with slack
		let slack_trigger = TriggerBuilder::new()
			.name("slack_alert")
			.slack("dummy_url")
			.url(url.clone())
			.build();

		assert_eq!(slack_trigger.trigger_type, TriggerType::Slack);
		match slack_trigger.config {
			TriggerTypeConfig::Slack { slack_url: u, .. } => {
				assert_eq!(u, url);
			}
			_ => panic!("Expected slack config"),
		}
	}
}
