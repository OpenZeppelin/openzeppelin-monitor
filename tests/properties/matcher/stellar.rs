use openzeppelin_monitor::services::filter::helpers::stellar::{
	are_same_signature, normalize_address, normalize_signature,
};
use proptest::{prelude::*, test_runner::Config};

prop_compose! {
	fn valid_signatures()(
		name in "[a-zA-Z][a-zA-Z0-9_]*",
		count in 0..5usize
	)(
		name in Just(name),
		params in prop::collection::vec(
			prop_oneof![
				Just("Address"),
				Just("U32"),
				Just("I32"),
				Just("U64"),
				Just("I64"),
				Just("U128"),
				Just("I128"),
				Just("U256"),
				Just("I256"),
				Just("String"),
				Just("Symbol"),
				Just("Vec"),
				Just("Map"),
				Just("Bool"),
				Just("Bytes")
			],
			count..=count
		)
	) -> String {
		format!("{}({})", name, params.join(","))
	}
}

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]
	#[test]
	fn test_signature_normalization(
		sig1 in valid_signatures(),
		spaces in " *"
	) {
		// First add random spaces between characters
		let with_spaces = sig1.chars()
			.flat_map(|c| vec![c, spaces.chars().next().unwrap_or(' ')])
			.collect::<String>();

		// Then randomly capitalize alphabetic characters
		let sig2 = with_spaces.chars()
			.map(|c| if c.is_alphabetic() && rand::random() {
				c.to_ascii_uppercase()
			} else {
				c
			})
			.collect::<String>();

		prop_assert!(are_same_signature(&sig1, &sig2));
		prop_assert_eq!(normalize_signature(&sig1), normalize_signature(&sig2));
	}

	#[test]
	fn test_address_normalization(
		addr in "[A-Za-z0-9]{56}",
		spaces in " *"
	) {
		// Create two versions of the same address with different spacing/casing
		let addr1 = format!("G{}", &addr);
		let addr2 = format!("G{}{}", spaces, addr.to_uppercase());

		prop_assert_eq!(
			normalize_address(&addr1),
			normalize_address(&addr2)
		);
	}

	#[test]
	fn test_invalid_signature(
		name1 in "[a-zA-Z][a-zA-Z0-9_]*",
		name2 in "[a-zA-Z][a-zA-Z0-9_]*",
		params in prop::collection::vec(
			prop_oneof![
				Just("Address"),
				Just("U32"),
				Just("I32"),
				Just("U64"),
				Just("I64"),
				Just("U128"),
				Just("I128"),
				Just("U256"),
				Just("I256"),
				Just("String"),
				Just("Symbol"),
				Just("Vec"),
				Just("Map"),
				Just("Bool"),
				Just("Bytes")
			],
			0..5
		)
	) {
		prop_assume!(name1 != name2);

		let sig1 = format!("{}({})", name1, params.join(","));
		let sig2 = format!("{}({})", name2, params.join(","));

		// Different function names should not match
		prop_assert!(!are_same_signature(&sig1, &sig2));

		// If we have parameters, test with different parameter counts
		if !params.is_empty() {
			let shorter_params = params[..params.len()-1].join(",");
			let sig3 = format!("{}({})", name1, shorter_params);

			// Different parameter counts should not match
			prop_assert!(!are_same_signature(&sig1, &sig3));
		}
	}
}
