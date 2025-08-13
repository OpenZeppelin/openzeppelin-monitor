//! Webhook payload builder implementation.
//!
//! This module provides functionality to build webhook payloads for different notification services (Telegram, Slack, Discord, etc.).

use regex::Regex;
use serde_json::json;
use std::collections::HashMap;

use super::template_formatter;

/// Trait for building webhook payloads.
pub trait WebhookPayloadBuilder: Send + Sync {
	/// Builds a webhook payload by formatting the template and applying channel-specific rules.
	///
	/// # Arguments
	///
	/// * `title` - The raw title of the message.
	/// * `body_template` - The message body template with variables like `${...}`.
	/// * `variables` - The map of variables to substitute into the template.
	///
	/// # Returns
	///
	/// A `serde_json::Value` representing the payload.
	fn build_payload(
		&self,
		title: &str,
		body_template: &str,
		variables: &HashMap<String, String>,
	) -> serde_json::Value;
}

/// Formats a message by substituting variables in the template.
pub fn format_template(template: &str, variables: &HashMap<String, String>) -> String {
	template_formatter::format_template(template, variables)
}

/// A payload builder for Slack.
pub struct SlackPayloadBuilder;

impl WebhookPayloadBuilder for SlackPayloadBuilder {
	fn build_payload(
		&self,
		title: &str,
		body_template: &str,
		variables: &HashMap<String, String>,
	) -> serde_json::Value {
		let formatted_title = format_template(title, variables);
		let formatted_message = format_template(body_template, variables);
		let full_message = format!("*{}*\n\n{}", formatted_title, formatted_message);
		json!({
			"blocks": [
				{
					"type": "section",
					"text": {
						"type": "mrkdwn",
						"text": full_message
					}
				}
			]
		})
	}
}

/// A payload builder for Discord.
pub struct DiscordPayloadBuilder;

impl WebhookPayloadBuilder for DiscordPayloadBuilder {
	fn build_payload(
		&self,
		title: &str,
		body_template: &str,
		variables: &HashMap<String, String>,
	) -> serde_json::Value {
		let formatted_title = format_template(title, variables);
		let formatted_message = format_template(body_template, variables);
		let full_message = format!("*{}*\n\n{}", formatted_title, formatted_message);
		json!({
			"content": full_message
		})
	}
}

/// A payload builder for Telegram.
pub struct TelegramPayloadBuilder {
	pub chat_id: String,
	pub disable_web_preview: bool,
}

impl TelegramPayloadBuilder {
	/// Escape a full MarkdownV2 message, preserving entities and
	/// escaping *all* special chars inside link URLs too.
	fn escape_markdown_v2(text: &str) -> String {
		const SPECIAL: &[char] = &[
			'_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.',
			'!', '\\',
		];

		let re =
			Regex::new(r"(?s)```.*?```|`[^`]*`|\*[^*]*\*|_[^_]*_|~[^~]*~|\[([^\]]+)\]\(([^)]+)\)")
				.unwrap();

		let mut out = String::with_capacity(text.len());
		let mut last = 0;

		for caps in re.captures_iter(text) {
			let mat = caps.get(0).unwrap();

			for c in text[last..mat.start()].chars() {
				if SPECIAL.contains(&c) {
					out.push('\\');
				}
				out.push(c);
			}

			if let (Some(lbl), Some(url)) = (caps.get(1), caps.get(2)) {
				let mut esc_label = String::with_capacity(lbl.as_str().len() * 2);
				for c in lbl.as_str().chars() {
					if SPECIAL.contains(&c) {
						esc_label.push('\\');
					}
					esc_label.push(c);
				}
				let mut esc_url = String::with_capacity(url.as_str().len() * 2);
				for c in url.as_str().chars() {
					if SPECIAL.contains(&c) {
						esc_url.push('\\');
					}
					esc_url.push(c);
				}
				out.push('[');
				out.push_str(&esc_label);
				out.push(']');
				out.push('(');
				out.push_str(&esc_url);
				out.push(')');
			} else {
				out.push_str(mat.as_str());
			}

			last = mat.end();
		}

		for c in text[last..].chars() {
			if SPECIAL.contains(&c) {
				out.push('\\');
			}
			out.push(c);
		}

		out
	}
}

impl WebhookPayloadBuilder for TelegramPayloadBuilder {
	fn build_payload(
		&self,
		title: &str,
		body_template: &str,
		variables: &HashMap<String, String>,
	) -> serde_json::Value {
		// First, substitute variables.
		let formatted_title = format_template(title, variables);
		let formatted_message = format_template(body_template, variables);

		// Then, escape both the title and the formatted message for Telegram MarkdownV2.
		let escaped_title = Self::escape_markdown_v2(&formatted_title);
		let escaped_message = Self::escape_markdown_v2(&formatted_message);

		let full_message = format!("*{}* \n\n{}", escaped_title, escaped_message);
		json!({
			"chat_id": self.chat_id,
			"text": full_message,
			"parse_mode": "MarkdownV2",
			"disable_web_page_preview": self.disable_web_preview
		})
	}
}

/// A payload builder for generic webhooks.
pub struct GenericWebhookPayloadBuilder;

impl WebhookPayloadBuilder for GenericWebhookPayloadBuilder {
	fn build_payload(
		&self,
		title: &str,
		body_template: &str,
		variables: &HashMap<String, String>,
	) -> serde_json::Value {
		let formatted_title = format_template(title, variables);
		let formatted_message = format_template(body_template, variables);
		json!({
			"title": formatted_title,
			"body": formatted_message
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn test_slack_payload_builder() {
		let title = "Test ${title_value}";
		let message = "Test ${message_value}";
		let variables = HashMap::from([
			("title_value".to_string(), "Title".to_string()),
			("message_value".to_string(), "Message".to_string()),
		]);
		let payload = SlackPayloadBuilder.build_payload(title, message, &variables);
		assert_eq!(
			payload,
			json!({
				"blocks": [
					{
						"type": "section",
						"text": {
							"type": "mrkdwn",
							"text": "*Test Title*\n\nTest Message"
						}
					}
				]
			})
		);
	}

	#[test]
	fn test_discord_payload_builder() {
		let title = "Test ${title_value}";
		let message = "Test ${message_value}";
		let variables = HashMap::from([
			("title_value".to_string(), "Title".to_string()),
			("message_value".to_string(), "Message".to_string()),
		]);
		let payload = DiscordPayloadBuilder.build_payload(title, message, &variables);
		assert_eq!(
			payload,
			json!({
				"content": "*Test Title*\n\nTest Message"
			})
		);
	}

	#[test]
	fn test_telegram_payload_builder() {
		let builder = TelegramPayloadBuilder {
			chat_id: "12345".to_string(),
			disable_web_preview: true,
		};
		let title = "Test ${title_value}";
		let message = "Test ${message_value}";
		let variables = HashMap::from([
			("title_value".to_string(), "Title".to_string()),
			("message_value".to_string(), "Message".to_string()),
		]);
		let payload = builder.build_payload(title, message, &variables);
		assert_eq!(
			payload,
			json!({
				"chat_id": "12345",
				"text": "*Test Title* \n\nTest Message",
				"parse_mode": "MarkdownV2",
				"disable_web_page_preview": true
			})
		);
	}

	#[test]
	fn test_generic_webhook_payload_builder() {
		let title = "Test ${title_value}";
		let message = "Test ${message_value}";
		let variables = HashMap::from([
			("title_value".to_string(), "Title".to_string()),
			("message_value".to_string(), "Message".to_string()),
		]);
		let payload = GenericWebhookPayloadBuilder.build_payload(title, message, &variables);
		assert_eq!(
			payload,
			json!({
				"title": "Test Title",
				"body": "Test Message"
			})
		);
	}

	#[test]
	fn test_escape_markdown_v2() {
		// Test for real life examples
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2(
				"*Transaction Alert*\n*Network:* Base Sepolia\n*From:* 0x00001\n*To:* 0x00002\n*Transaction:* [View on Blockscout](https://base-sepolia.blockscout.com/tx/0x00003)"
			),
			"*Transaction Alert*\n*Network:* Base Sepolia\n*From:* 0x00001\n*To:* 0x00002\n*Transaction:* [View on Blockscout](https://base\\-sepolia\\.blockscout\\.com/tx/0x00003)"
		);

		// Test basic special character escaping
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("Hello *world*!"),
			"Hello *world*\\!"
		);

		// Test multiple special characters
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("(test) [test] {test} <test>"),
			"\\(test\\) \\[test\\] \\{test\\} <test\\>"
		);

		// Test markdown code blocks (should be preserved)
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("```code block```"),
			"```code block```"
		);

		// Test inline code (should be preserved)
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("`inline code`"),
			"`inline code`"
		);

		// Test bold text (should be preserved)
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("*bold text*"),
			"*bold text*"
		);

		// Test italic text (should be preserved)
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("_italic text_"),
			"_italic text_"
		);

		// Test strikethrough (should be preserved)
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("~strikethrough~"),
			"~strikethrough~"
		);

		// Test links with special characters
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("[link](https://example.com/test.html)"),
			"[link](https://example\\.com/test\\.html)"
		);

		// Test complex link with special characters in both label and URL
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2(
				"[test!*_]{link}](https://test.com/path[1])"
			),
			"\\[test\\!\\*\\_\\]\\{link\\}\\]\\(https://test\\.com/path\\[1\\]\\)"
		);

		// Test mixed content
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2(
				"Hello *bold* and [link](http://test.com) and `code`"
			),
			"Hello *bold* and [link](http://test\\.com) and `code`"
		);

		// Test escaping backslashes
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("test\\test"),
			"test\\\\test"
		);

		// Test all special characters
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("_*[]()~`>#+-=|{}.!\\"),
			"\\_\\*\\[\\]\\(\\)\\~\\`\\>\\#\\+\\-\\=\\|\\{\\}\\.\\!\\\\",
		);

		// Test nested markdown (outer should be preserved, inner escaped)
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("*bold with [link](http://test.com)*"),
			"*bold with [link](http://test.com)*"
		);

		// Test empty string
		assert_eq!(TelegramPayloadBuilder::escape_markdown_v2(""), "");

		// Test string with only special characters
		assert_eq!(
			TelegramPayloadBuilder::escape_markdown_v2("***"),
			"**\\*" // First * is preserved as markdown, others escaped
		);
	}

	#[test]
	fn test_events_match_reasons_single_event() {
		let variables = HashMap::from([
			(
				"events.0.signature".to_string(),
				"Transfer(address,address,uint256)".to_string(),
			),
			("events.0.args.from".to_string(), "0x1234".to_string()),
			("events.0.args.to".to_string(), "0x5678".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "events");
		assert!(result.is_some());
		let expected = "\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_events_match_reasons_multiple_events() {
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
			(
				"events.1.args.spender".to_string(),
				"0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6".to_string(),
			),
			("events.1.args.value".to_string(), "1000000000".to_string()),
			(
				"events.2.signature".to_string(),
				"ValueChanged(uint256)".to_string(),
			),
			("events.2.args.value".to_string(), "1000000000".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "events");
		assert!(result.is_some());
		let expected = "\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`\n\n*Reason 2*\n\n*Signature:* `Approval(address,address,uint256)`\n\n*Params:*\n\nowner: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\nspender: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\nvalue: `1000000000`\n\n*Reason 3*\n\n*Signature:* `ValueChanged(uint256)`\n\n*Params:*\n\nvalue: `1000000000`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_events_match_reasons_no_events() {
		let variables = HashMap::from([
			("transaction.hash".to_string(), "0x1234".to_string()),
			("monitor.name".to_string(), "Test Monitor".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "events");
		assert!(result.is_none());
	}

	#[test]
	fn test_events_match_reasons_out_of_order() {
		let variables = HashMap::from([
			(
				"events.2.signature".to_string(),
				"ValueChanged(uint256)".to_string(),
			),
			("events.2.args.value".to_string(), "1000000000".to_string()),
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
			(
				"events.1.args.spender".to_string(),
				"0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6".to_string(),
			),
			("events.1.args.value".to_string(), "1000000000".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "events");
		assert!(result.is_some());
		let expected = "\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`\n\n*Reason 2*\n\n*Signature:* `Approval(address,address,uint256)`\n\n*Params:*\n\nowner: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\nspender: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\nvalue: `1000000000`\n\n*Reason 3*\n\n*Signature:* `ValueChanged(uint256)`\n\n*Params:*\n\nvalue: `1000000000`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_format_template_with_all_events() {
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
			(
				"events.1.signature".to_string(),
				"Approval(address,address,uint256)".to_string(),
			),
			(
				"events.1.args.owner".to_string(),
				"0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6".to_string(),
			),
			(
				"events.1.args.spender".to_string(),
				"0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6".to_string(),
			),
			("events.1.args.value".to_string(), "1000000000".to_string()),
		]);

		let result = format_template(template, &variables);
		// Since the template contains ${events}, it should get the match reasons section
		let expected = "Transaction detected: 0x1234567890abcdef\n\n\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`\n\n*Reason 2*\n\n*Signature:* `Approval(address,address,uint256)`\n\n*Params:*\n\nowner: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\nspender: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\nvalue: `1000000000`";
		assert_eq!(result, expected);
	}

	#[test]
	fn test_functions_match_reasons_single_function() {
		let variables = HashMap::from([
			(
				"functions.0.signature".to_string(),
				"transfer(address,uint256)".to_string(),
			),
			("functions.0.args.to".to_string(), "0x1234".to_string()),
			("functions.0.args.amount".to_string(), "1000000".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "functions");
		assert!(result.is_some());
		let expected = "\n\n*Matched Functions:*\n\n*Reason 1*\n\n*Signature:* `transfer(address,uint256)`\n\n*Params:*\n\namount: `1000000`\nto: `0x1234`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_functions_match_reasons_multiple_functions() {
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
			(
				"functions.2.signature".to_string(),
				"mint(uint256)".to_string(),
			),
			("functions.2.args.amount".to_string(), "1000000".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "functions");
		assert!(result.is_some());
		let expected = "\n\n*Matched Functions:*\n\n*Reason 1*\n\n*Signature:* `transfer(address,uint256)`\n\n*Params:*\n\namount: `1000000`\nto: `0x1234`\n\n*Reason 2*\n\n*Signature:* `approve(address,uint256)`\n\n*Params:*\n\namount: `500000`\nspender: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\n\n*Reason 3*\n\n*Signature:* `mint(uint256)`\n\n*Params:*\n\namount: `1000000`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_functions_match_reasons_no_functions() {
		let variables = HashMap::from([
			("transaction.hash".to_string(), "0x1234".to_string()),
			("monitor.name".to_string(), "Test Monitor".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "functions");
		assert!(result.is_none());
	}

	#[test]
	fn test_functions_match_reasons_out_of_order() {
		let variables = HashMap::from([
			(
				"functions.2.signature".to_string(),
				"mint(uint256)".to_string(),
			),
			("functions.2.args.amount".to_string(), "1000000".to_string()),
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

		let result = template_formatter::build_match_reasons(&variables, "functions");
		assert!(result.is_some());
		let expected = "\n\n*Matched Functions:*\n\n*Reason 1*\n\n*Signature:* `transfer(address,uint256)`\n\n*Params:*\n\namount: `1000000`\nto: `0x1234`\n\n*Reason 2*\n\n*Signature:* `approve(address,uint256)`\n\n*Params:*\n\namount: `500000`\nspender: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\n\n*Reason 3*\n\n*Signature:* `mint(uint256)`\n\n*Params:*\n\namount: `1000000`";
		assert_eq!(result.unwrap(), expected);
	}

	#[test]
	fn test_format_template_with_all_functions() {
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

		let result = template_formatter::format_template(template, &variables);
		// Since the template contains ${functions}, it should get the match reasons section
		let expected = "Transaction detected: 0x1234567890abcdef\n\n\n\n*Matched Functions:*\n\n*Reason 1*\n\n*Signature:* `transfer(address,uint256)`\n\n*Params:*\n\namount: `1000000`\nto: `0x1234`\n\n*Reason 2*\n\n*Signature:* `approve(address,uint256)`\n\n*Params:*\n\namount: `500000`\nspender: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`";
		assert_eq!(result, expected);
	}

	#[test]
	fn test_format_template_with_functions_and_events() {
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
			("events.0.args.value".to_string(), "1000000".to_string()),
			(
				"events.1.signature".to_string(),
				"Approval(address,address,uint256)".to_string(),
			),
			(
				"events.1.args.owner".to_string(),
				"0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6".to_string(),
			),
			(
				"events.1.args.spender".to_string(),
				"0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6".to_string(),
			),
			("events.1.args.value".to_string(), "500000".to_string()),
			(
				"functions.0.signature".to_string(),
				"transfer(address,uint256)".to_string(),
			),
			("functions.0.args.to".to_string(), "0x9abc".to_string()),
			("functions.0.args.amount".to_string(), "750000".to_string()),
			(
				"functions.1.signature".to_string(),
				"mint(uint256)".to_string(),
			),
			("functions.1.args.amount".to_string(), "250000".to_string()),
		]);

		let result = template_formatter::format_template(template, &variables);
		// The template contains both ${events} and ${functions}, so both sections should be included
		// Functions are processed before events, so functions section appears first
		let expected = "Transaction detected: 0x1234567890abcdef\n\n\n\n*Matched Functions:*\n\n*Reason 1*\n\n*Signature:* `transfer(address,uint256)`\n\n*Params:*\n\namount: `750000`\nto: `0x9abc`\n\n*Reason 2*\n\n*Signature:* `mint(uint256)`\n\n*Params:*\n\namount: `250000`\n\n\n\n*Matched Events:*\n\n*Reason 1*\n\n*Signature:* `Transfer(address,address,uint256)`\n\n*Params:*\n\nfrom: `0x1234`\nto: `0x5678`\nvalue: `1000000`\n\n*Reason 2*\n\n*Signature:* `Approval(address,address,uint256)`\n\n*Params:*\n\nowner: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\nspender: `0x742d35Cc6634C0532925a3b8D4C9db96C4b4d8b6`\nvalue: `500000`";
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

		let result = template_formatter::format_template(template, &variables);
		// Since there are no events, ${events} should be replaced with empty string
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

		let result = template_formatter::format_template(template, &variables);
		// Since there are no functions, ${functions} should be replaced with empty string
		let expected = "Transaction detected: 0x1234567890abcdef\n\n";
		assert_eq!(result, expected);
	}

	#[test]
	fn test_build_match_reasons_no_index_part() {
		let variables = HashMap::from([
			// Key that starts with prefix but doesn't end with .signature
			("events.0.args.from".to_string(), "0x1234".to_string()),
			// Key that doesn't start with prefix
			("transaction.hash".to_string(), "0x1234".to_string()),
			// Key that starts with prefix but has wrong format
			("events.signature".to_string(), "Transfer".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "events");
		// Should return None since no valid signature keys were found
		assert!(result.is_none());
	}

	#[test]
	fn test_build_match_reasons_no_signature() {
		let variables = HashMap::from([
			// Has the signature key structure but no actual signature value
			("events.0.signature".to_string(), "".to_string()),
			// Has args but no signature
			("events.0.args.from".to_string(), "0x1234".to_string()),
			("events.0.args.to".to_string(), "0x5678".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "events");
		let result_str = result.unwrap();

		assert!(result_str.contains("\n\n*Matched Events:*\n"));
	}

	#[test]
	fn test_build_match_reasons_mixed_valid_and_invalid() {
		let variables = HashMap::from([
			// Valid signature
			(
				"events.0.signature".to_string(),
				"Transfer(address,address,uint256)".to_string(),
			),
			("events.0.args.from".to_string(), "0x1234".to_string()),
			// Invalid signature (empty)
			("events.1.signature".to_string(), "".to_string()),
			("events.1.args.to".to_string(), "0x5678".to_string()),
			// Valid signature
			(
				"events.2.signature".to_string(),
				"Approval(address,address,uint256)".to_string(),
			),
			("events.2.args.owner".to_string(), "0x9abc".to_string()),
		]);

		let result = template_formatter::build_match_reasons(&variables, "events");
		// Should only include events 0 and 2, skipping event 1 due to empty signature
		assert!(result.is_some());
		let result_str = result.unwrap();
		assert!(result_str.contains("Transfer(address,address,uint256)"));
		assert!(!result_str.contains("events.1")); // Should not contain event 1
		assert!(result_str.contains("Approval(address,address,uint256)"));
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

		let result = template_formatter::build_match_reasons(&variables, "events");
		// Should only include the valid event 0, skipping invalid formats
		assert!(result.is_some());
		let result_str = result.unwrap();
		assert!(result_str.contains("Transfer(address,address,uint256)"));
		assert!(!result_str.contains("abc")); // Should not contain invalid index
		assert!(!result_str.contains("-1")); // Should not contain negative index
	}
}
