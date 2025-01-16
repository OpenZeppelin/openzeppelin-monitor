use openzeppelin_monitor::models::{
	EventCondition, FunctionCondition, MatchConditions, TransactionCondition, TransactionStatus,
};
use proptest::{prelude::*, test_runner::Config};

proptest! {
	#![proptest_config(Config {
		failure_persistence: None,
		..Config::default()
	})]

	#[test]
	fn test_match_conditions_combination(
		has_events in prop::bool::ANY,
		has_functions in prop::bool::ANY,
		has_transactions in prop::bool::ANY
	) {
		let conditions = MatchConditions {
			events: if has_events {
				vec![EventCondition {
					signature: "Transfer(address,address,uint256)".to_string(),
					expression: Some("value > 1000".to_string())
				}]
			} else {
				vec![]
			},
			functions: if has_functions {
				vec![FunctionCondition {
					signature: "transfer(address,uint256)".to_string(),
					expression: Some("amount > 0".to_string())
				}]
			} else {
				vec![]
			},
			transactions: if has_transactions {
				vec![TransactionCondition {
					status: TransactionStatus::Success,
					expression: Some("value > 0".to_string())
				}]
			} else {
				vec![]
			}
		};

		// Test logical combinations
		match (has_events, has_functions, has_transactions) {
			(false, false, false) => {
				prop_assert!(conditions.events.is_empty());
				prop_assert!(conditions.functions.is_empty());
				prop_assert!(conditions.transactions.is_empty());
			},
			(true, false, false) => {
				prop_assert!(!conditions.events.is_empty());
				prop_assert!(conditions.functions.is_empty());
				prop_assert!(conditions.transactions.is_empty());
			},
			(false, true, false) => {
				prop_assert!(conditions.events.is_empty());
				prop_assert!(!conditions.functions.is_empty());
				prop_assert!(conditions.transactions.is_empty());
			},
			(false, false, true) => {
				prop_assert!(conditions.events.is_empty());
				prop_assert!(conditions.functions.is_empty());
				prop_assert!(!conditions.transactions.is_empty());
			},
			(true, true, false) => {
				prop_assert!(!conditions.events.is_empty());
				prop_assert!(!conditions.functions.is_empty());
				prop_assert!(conditions.transactions.is_empty());
			},
			(true, false, true) => {
				prop_assert!(!conditions.events.is_empty());
				prop_assert!(conditions.functions.is_empty());
				prop_assert!(!conditions.transactions.is_empty());
			},
			(false, true, true) => {
				prop_assert!(conditions.events.is_empty());
				prop_assert!(!conditions.functions.is_empty());
				prop_assert!(!conditions.transactions.is_empty());
			},
			(true, true, true) => {
				prop_assert!(!conditions.events.is_empty());
				prop_assert!(!conditions.functions.is_empty());
				prop_assert!(!conditions.transactions.is_empty());
			},
		}
	}
}
