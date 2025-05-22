//! Property-based tests for EVM transaction matching and filtering.
//! Tests cover signature/address normalization, expression evaluation, and transaction matching.

use ethabi::Token;
use openzeppelin_monitor::services::filter::evm_helpers::format_token_value;
use proptest::{prelude::*, test_runner::Config};

// Generator for ethabi Token values
prop_compose! {
	fn generate_token()(
		token_type in prop_oneof![
			Just("address"),
			Just("bytes"),
			Just("uint"),
			Just("bool"),
			Just("string"),
			Just("array"),
		],
		value in any::<u64>(),
		string_value in "[a-zA-Z0-9]{1,10}",
		bytes_len in 1..32usize
	) -> Token {
		match token_type {
			"address" => Token::Address(ethabi::Address::from_low_u64_be(value)),
			"bytes" => {
				let bytes = (0..bytes_len).map(|i| ((i as u64 + value) % 256) as u8).collect::<Vec<u8>>();
				Token::Bytes(bytes)
			},
			"uint" => Token::Uint(ethabi::Uint::from(value)),
			"bool" => Token::Bool(value % 2 == 0),
			"string" => Token::String(string_value),
			"array" => {
				let elements = vec![
					Token::Uint(ethabi::Uint::from(value)),
					Token::Uint(ethabi::Uint::from(value + 1)),
				];
				Token::Array(elements)
			},
			_ => Token::Uint(ethabi::Uint::from(0)),
		}
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn test_format_token_value(
		token in generate_token()
	) {
		let formatted = format_token_value(&token);

		// Result should be a non-empty string
		prop_assert!(!formatted.is_empty());

		// Type-specific assertions
		match token {
			Token::Address(_) => prop_assert!(formatted.starts_with("0x")),
			Token::Bytes(_) | Token::FixedBytes(_) => prop_assert!(formatted.starts_with("0x")),
			Token::Array(_) => {
				prop_assert!(formatted.starts_with('['));
				prop_assert!(formatted.ends_with(']'));
			}
			Token::Tuple(_) => {
				prop_assert!(formatted.starts_with('('));
				prop_assert!(formatted.ends_with(')'));
			}
			_ => {} // Other types don't have specific format requirements
		}

		// The formatted string should be parseable based on the token type
		match token {
			Token::Uint(num) => {
				let parsed: Result<u64, _> = formatted.parse();
				prop_assert!(parsed.is_ok());
				prop_assert_eq!(parsed.unwrap(), num.as_u64());
			}
			Token::Bool(b) => {
				prop_assert_eq!(formatted, b.to_string());
			}
			Token::String(s) => {
				prop_assert_eq!(formatted, s);
			}
			_ => {} // Other types need more complex parsing
		}
	}
}
