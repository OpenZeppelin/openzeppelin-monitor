//! Telegram notification implementation.
//!
//! Provides functionality to send formatted messages to Telegram channels
//! via incoming webhooks, supporting message templates with variable substitution.

use async_trait::async_trait;
use regex::Regex;
use std::collections::HashMap;

use crate::{
	models::TriggerTypeConfig,
	services::notification::{NotificationError, Notifier, WebhookConfig, WebhookNotifier},
};

/// Implementation of Telegram notifications via webhooks
#[derive(Debug)]
pub struct TelegramNotifier {
	inner: WebhookNotifier,
	/// Disable web preview
	disable_web_preview: bool,
}

impl TelegramNotifier {
	/// Creates a new Telegram notifier instance
	///
	/// # Arguments
	/// * `token` - Telegram bot token
	/// * `chat_id` - Telegram chat ID
	/// * `disable_web_preview` - Disable web preview
	/// * `title` - Title to display in the message
	/// * `body_template` - Message template with variables
	pub fn new(
		base_url: Option<String>,
		token: String,
		chat_id: String,
		disable_web_preview: Option<bool>,
		title: String,
		body_template: String,
	) -> Result<Self, NotificationError> {
		let url = format!(
			"{}/bot{}/sendMessage",
			base_url.unwrap_or("https://api.telegram.org".to_string()),
			token
		);

		// Set up initial URL parameters
		let mut url_params = HashMap::new();
		url_params.insert("chat_id".to_string(), chat_id);
		url_params.insert("parse_mode".to_string(), "MarkdownV2".to_string());

		Ok(Self {
			inner: WebhookNotifier::new(WebhookConfig {
				url,
				url_params: Some(url_params),
				title,
				body_template,
				method: Some("GET".to_string()),
				secret: None,
				headers: None,
				payload_fields: None,
			})?,
			disable_web_preview: disable_web_preview.unwrap_or(false),
		})
	}

	/// Formats a message by substituting variables in the template
	///
	/// # Arguments
	/// * `variables` - Map of variable names to values
	///
	/// # Returns
	/// * `String` - Formatted message with variables replaced
	pub fn format_message(&self, variables: &HashMap<String, String>) -> String {
		let message = self.inner.format_message(variables);
		let escaped_message = Self::escape_markdown_v2(&message);
		let escaped_title = Self::escape_markdown_v2(&self.inner.title);
		format!("*{}* \n\n{}", escaped_title, escaped_message)
	}

	/// Escape a full MarkdownV2 message, preserving entities and
	/// escaping *all* special chars inside link URLs too.
	///
	/// # Arguments
	/// * `text` - The text to escape
	///
	/// # Returns
	/// * `String` - The escaped text
	pub fn escape_markdown_v2(text: &str) -> String {
		// Full set of Telegram MDV2 metacharacters (including backslash)
		const SPECIAL: &[char] = &[
			'_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.',
			'!', '\\',
		];

		// Regex that captures either:
		//  - any MD entity: ```…```, `…`, *…*, _…_, ~…~
		//  - or an inline link, capturing label & URL separately
		let re =
			Regex::new(r"(?s)```.*?```|`[^`]*`|\*[^*]*\*|_[^_]*_|~[^~]*~|\[([^\]]+)\]\(([^)]+)\)")
				.unwrap();

		let mut out = String::with_capacity(text.len());
		let mut last = 0;

		for caps in re.captures_iter(text) {
			let mat = caps.get(0).unwrap();

			// 1) escape everything before this match
			for c in text[last..mat.start()].chars() {
				if SPECIAL.contains(&c) {
					out.push('\\');
				}
				out.push(c);
			}

			// 2) if this is an inline link (has two capture groups)
			if let (Some(lbl), Some(url)) = (caps.get(1), caps.get(2)) {
				// fully escape the label
				let mut esc_label = String::with_capacity(lbl.as_str().len() * 2);
				for c in lbl.as_str().chars() {
					if SPECIAL.contains(&c) {
						esc_label.push('\\');
					}
					esc_label.push(c);
				}
				// fully escape the URL (dots, hyphens, slashes, etc.)
				let mut esc_url = String::with_capacity(url.as_str().len() * 2);
				for c in url.as_str().chars() {
					if SPECIAL.contains(&c) {
						esc_url.push('\\');
					}
					esc_url.push(c);
				}
				// emit the link markers unescaped
				out.push('[');
				out.push_str(&esc_label);
				out.push(']');
				out.push('(');
				out.push_str(&esc_url);
				out.push(')');
			} else {
				// 3) otherwise just copy the entire MD entity verbatim
				out.push_str(mat.as_str());
			}

			last = mat.end();
		}

		// 4) escape the trailing text after the last match
		for c in text[last..].chars() {
			if SPECIAL.contains(&c) {
				out.push('\\');
			}
			out.push(c);
		}

		out
	}

	/// Creates a Telegram notifier from a trigger configuration
	///
	/// # Arguments
	/// * `config` - Trigger configuration containing Telegram parameters
	///
	/// # Returns
	/// * `Result<Self, NotificationError>` - Notifier instance if config is Telegram type
	pub fn from_config(config: &TriggerTypeConfig) -> Result<Self, NotificationError> {
		if let TriggerTypeConfig::Telegram {
			token,
			chat_id,
			disable_web_preview,
			message,
		} = config
		{
			let mut url_params = HashMap::new();
			url_params.insert("chat_id".to_string(), chat_id.clone());
			url_params.insert("parse_mode".to_string(), "MarkdownV2".to_string());

			let webhook_config = WebhookConfig {
				url: format!("https://api.telegram.org/bot{}/sendMessage", token),
				url_params: Some(url_params),
				title: message.title.clone(),
				body_template: message.body.clone(),
				method: Some("GET".to_string()),
				secret: None,
				headers: None,
				payload_fields: None,
			};

			Ok(Self {
				inner: WebhookNotifier::new(webhook_config)?,
				disable_web_preview: disable_web_preview.unwrap_or(false),
			})
		} else {
			Err(NotificationError::config_error(
				format!("Invalid telegram configuration: {:?}", config),
				None,
				None,
			))
		}
	}
}

#[async_trait]
impl Notifier for TelegramNotifier {
	/// Sends a formatted message to Telegram
	///
	/// # Arguments
	/// * `message` - The formatted message to send
	///
	/// # Returns
	/// * `Result<(), NotificationError>` - Success or error
	async fn notify(&self, message: &str) -> Result<(), NotificationError> {
		// Add message and disable_web_preview to URL parameters
		let mut url_params = self.inner.url_params.clone().unwrap_or_default();
		url_params.insert("text".to_string(), message.to_string());
		url_params.insert(
			"disable_web_page_preview".to_string(),
			self.disable_web_preview.to_string(),
		);

		// Create a new WebhookNotifier with updated URL parameters
		let notifier = WebhookNotifier::new(WebhookConfig {
			url: self.inner.url.clone(),
			url_params: Some(url_params),
			title: self.inner.title.clone(),
			body_template: self.inner.body_template.clone(),
			method: Some("GET".to_string()),
			secret: None,
			headers: self.inner.headers.clone(),
			payload_fields: None,
		})?;

		notifier.notify_with_payload(message, HashMap::new()).await
	}
}

#[cfg(test)]
mod tests {
	use crate::models::{NotificationMessage, SecretString, SecretValue};

	use super::*;

	fn create_test_notifier(body_template: &str) -> TelegramNotifier {
		TelegramNotifier::new(
			None,
			"test-token".to_string(),
			"test-chat-id".to_string(),
			Some(true),
			"Alert".to_string(),
			body_template.to_string(),
		)
		.unwrap()
	}

	fn create_test_telegram_config() -> TriggerTypeConfig {
		TriggerTypeConfig::Telegram {
			token: SecretValue::Plain(SecretString::new("test-token".to_string())),
			chat_id: "test-chat-id".to_string(),
			disable_web_preview: Some(true),
			message: NotificationMessage {
				title: "Alert".to_string(),
				body: "Test message ${value}".to_string(),
			},
		}
	}

	////////////////////////////////////////////////////////////
	// format_message tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_format_message() {
		let notifier = create_test_notifier("Value is ${value} and status is ${status}");

		let mut variables = HashMap::new();
		variables.insert("value".to_string(), "100".to_string());
		variables.insert("status".to_string(), "critical".to_string());

		let result = notifier.format_message(&variables);
		assert_eq!(result, "*Alert* \n\nValue is 100 and status is critical");
	}

	#[test]
	fn test_format_message_with_missing_variables() {
		let notifier = create_test_notifier("Value is ${value} and status is ${status}");

		let mut variables = HashMap::new();
		variables.insert("value".to_string(), "100".to_string());
		// status variable is not provided

		let result = notifier.format_message(&variables);
		assert_eq!(
			result,
			"*Alert* \n\nValue is 100 and status is $\\{status\\}"
		);
	}

	#[test]
	fn test_format_message_with_empty_template() {
		let notifier = create_test_notifier("");

		let variables = HashMap::new();
		let result = notifier.format_message(&variables);
		assert_eq!(result, "*Alert* \n\n");
	}

	////////////////////////////////////////////////////////////
	// from_config tests
	////////////////////////////////////////////////////////////

	#[test]
	fn test_from_config_with_telegram_config() {
		let config = create_test_telegram_config();

		let notifier = TelegramNotifier::from_config(&config);
		assert!(notifier.is_ok());

		let notifier = notifier.unwrap();
		assert_eq!(
			notifier.inner.url,
			"https://api.telegram.org/bottest-token/sendMessage"
		);
		assert!(notifier.disable_web_preview);
		assert_eq!(notifier.inner.body_template, "Test message ${value}");
	}

	#[test]
	fn test_from_config_invalid_type() {
		// Create a config that is not a Telegram type
		let config = TriggerTypeConfig::Slack {
			slack_url: SecretValue::Plain(SecretString::new(
				"https://slack.example.com".to_string(),
			)),
			message: NotificationMessage {
				title: "Test Alert".to_string(),
				body: "Test message ${value}".to_string(),
			},
		};

		let notifier = TelegramNotifier::from_config(&config);
		assert!(notifier.is_err());

		let error = notifier.unwrap_err();
		assert!(matches!(error, NotificationError::ConfigError { .. }));
	}

	#[test]
	fn test_from_config_disable_web_preview_default_in_config() {
		let config = TriggerTypeConfig::Telegram {
			token: SecretValue::Plain(SecretString::new("test-token".to_string())),
			chat_id: "test-chat-id".to_string(),
			disable_web_preview: None, // Test default within TriggerTypeConfig
			message: NotificationMessage {
				title: "Alert".to_string(),
				body: "Test message ${value}".to_string(),
			},
		};
		let notifier = TelegramNotifier::from_config(&config).unwrap();
		assert!(!notifier.disable_web_preview);
	}

	////////////////////////////////////////////////////////////
	// notify tests
	////////////////////////////////////////////////////////////

	#[tokio::test]
	async fn test_notify_failure() {
		let notifier = create_test_notifier("Test message");
		let result = notifier.notify("Test message").await;
		assert!(result.is_err());

		let error = result.unwrap_err();
		assert!(matches!(error, NotificationError::NotifyFailed { .. }));
	}

	#[tokio::test]
	async fn test_notify_with_payload_failure() {
		let notifier = create_test_notifier("Test message");
		let result = notifier
			.notify_with_payload("Test message", HashMap::new())
			.await;
		assert!(result.is_err());

		let error = result.unwrap_err();
		assert!(matches!(error, NotificationError::NotifyFailed { .. }));
	}

	#[test]
	fn test_escape_markdown_v2() {
		// Test for real life examples
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("*Transaction Alert*\n*Network:* Base Sepolia\n*From:* 0x00001\n*To:* 0x00002\n*Transaction:* [View on Blockscout](https://base-sepolia.blockscout.com/tx/0x00003)"),
			"*Transaction Alert*\n*Network:* Base Sepolia\n*From:* 0x00001\n*To:* 0x00002\n*Transaction:* [View on Blockscout](https://base\\-sepolia\\.blockscout\\.com/tx/0x00003)"
		);

		// Test basic special character escaping
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("Hello *world*!"),
			"Hello *world*\\!"
		);

		// Test multiple special characters
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("(test) [test] {test} <test>"),
			"\\(test\\) \\[test\\] \\{test\\} <test\\>"
		);

		// Test markdown code blocks (should be preserved)
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("```code block```"),
			"```code block```"
		);

		// Test inline code (should be preserved)
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("`inline code`"),
			"`inline code`"
		);

		// Test bold text (should be preserved)
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("*bold text*"),
			"*bold text*"
		);

		// Test italic text (should be preserved)
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("_italic text_"),
			"_italic text_"
		);

		// Test strikethrough (should be preserved)
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("~strikethrough~"),
			"~strikethrough~"
		);

		// Test links with special characters
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("[link](https://example.com/test.html)"),
			"[link](https://example\\.com/test\\.html)"
		);

		// Test complex link with special characters in both label and URL
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("[test!*_]{link}](https://test.com/path[1])"),
			"\\[test\\!\\*\\_\\]\\{link\\}\\]\\(https://test\\.com/path\\[1\\]\\)"
		);

		// Test mixed content
		assert_eq!(
			TelegramNotifier::escape_markdown_v2(
				"Hello *bold* and [link](http://test.com) and `code`"
			),
			"Hello *bold* and [link](http://test\\.com) and `code`"
		);

		// Test escaping backslashes
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("test\\test"),
			"test\\\\test"
		);

		// Test all special characters
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("_*[]()~`>#+-=|{}.!\\"),
			"\\_\\*\\[\\]\\(\\)\\~\\`\\>\\#\\+\\-\\=\\|\\{\\}\\.\\!\\\\",
		);

		// Test nested markdown (outer should be preserved, inner escaped)
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("*bold with [link](http://test.com)*"),
			"*bold with [link](http://test.com)*"
		);

		// Test empty string
		assert_eq!(TelegramNotifier::escape_markdown_v2(""), "");

		// Test string with only special characters
		assert_eq!(
			TelegramNotifier::escape_markdown_v2("***"),
			"**\\*" // First * is preserved as markdown, others escaped
		);
	}
}
