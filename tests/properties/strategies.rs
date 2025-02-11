use email_address::EmailAddress;
use openzeppelin_monitor::models::{
	AddressWithABI, BlockChainType, EventCondition, FunctionCondition, MatchConditions, Monitor,
	Network, NotificationMessage, RpcUrl, TransactionCondition, TransactionStatus, Trigger,
	TriggerType, TriggerTypeConfig,
};
use proptest::{option, prelude::*};

const MIN_COLLECTION_SIZE: usize = 0;
const MAX_COLLECTION_SIZE: usize = 10;
const MAX_ADDRESSES: usize = 10;

pub fn monitor_strategy(
	available_networks: Vec<String>,
	available_triggers: Vec<String>,
) -> impl Strategy<Value = Monitor> {
	(
		prop::collection::vec(
			prop::sample::select(available_triggers),
			MIN_COLLECTION_SIZE..MAX_COLLECTION_SIZE,
		),
		prop::collection::vec(
			prop::sample::select(available_networks),
			MIN_COLLECTION_SIZE..MAX_COLLECTION_SIZE,
		),
		"[a-zA-Z0-9_]{1,10}".prop_map(|s| s.to_string()),
		proptest::arbitrary::any::<bool>(),
		proptest::collection::vec(
			("[a-zA-Z0-9_]{1,10}".prop_map(|s| s.to_string()))
				.prop_map(|address| AddressWithABI { address, abi: None }),
			MIN_COLLECTION_SIZE..MAX_ADDRESSES,
		),
		match_conditions_strategy(),
	)
		.prop_map(
			|(triggers, networks, name, paused, addresses, match_conditions)| Monitor {
				triggers,
				networks,
				name,
				paused,
				addresses,
				match_conditions,
			},
		)
}

pub fn notification_message_strategy() -> impl Strategy<Value = NotificationMessage> {
	(
		"[a-zA-Z0-9_]{1,50}".prop_map(|s| s.to_string()),
		"[a-zA-Z0-9_]{1,100}".prop_map(|s| s.to_string()),
	)
		.prop_map(|(title, body)| NotificationMessage { title, body })
}

pub fn trigger_strategy() -> impl Strategy<Value = Trigger> {
	prop_oneof![
		// Slack strategy
		(
			"[a-zA-Z0-9_]{1,10}".prop_map(|s| s.to_string()),
			Just(TriggerType::Slack),
			(
				"https://hooks\\.slack\\.com/[a-zA-Z0-9/]+".prop_map(|s| s.to_string()),
				notification_message_strategy(),
			)
				.prop_map(|(slack_url, message)| TriggerTypeConfig::Slack { slack_url, message })
		)
			.prop_map(|(name, trigger_type, config)| Trigger {
				name,
				trigger_type,
				config,
			}),
		// Email strategy
		(
			"[a-zA-Z0-9_]{1,10}".prop_map(|s| s.to_string()),
			Just(TriggerType::Email),
			(
				"smtp\\.[a-z0-9]+\\.com".prop_map(|s| s.to_string()),
				option::of(1..65535u16),
				"[a-zA-Z0-9]+".prop_map(|s| s.to_string()),
				"[a-zA-Z0-9]+".prop_map(|s| s.to_string()),
				notification_message_strategy(),
				"[a-zA-Z0-9]+@[a-z0-9]+\\.com".prop_map(|s| EmailAddress::new_unchecked(&s)),
				proptest::collection::vec(
					"[a-zA-Z0-9]+@[a-z0-9]+\\.com".prop_map(|s| EmailAddress::new_unchecked(&s)),
					1..5,
				),
			)
				.prop_map(
					|(host, port, username, password, message, sender, recipients)| {
						TriggerTypeConfig::Email {
							host,
							port,
							username,
							password,
							message,
							sender,
							recipients,
						}
					}
				)
		)
			.prop_map(|(name, trigger_type, config)| Trigger {
				name,
				trigger_type,
				config,
			}),
		// Webhook strategy
		(
			"[a-zA-Z0-9_]{1,10}".prop_map(|s| s.to_string()),
			Just(TriggerType::Webhook),
			(
				"https://[a-z0-9]+\\.com/webhook".prop_map(|s| s.to_string()),
				option::of(prop_oneof!["GET", "POST", "PUT", "DELETE"].prop_map(|s| s.to_string())),
				option::of(proptest::collection::hash_map(
					"[a-zA-Z-]{1,10}".prop_map(|s| s.to_string()),
					"[a-zA-Z0-9]{1,10}".prop_map(|s| s.to_string()),
					0..5,
				)),
				option::of("[a-zA-Z0-9_]{1,10}".prop_map(|s| s.to_string())),
				notification_message_strategy(),
			)
				.prop_map(|(url, method, headers, secret, message)| {
					TriggerTypeConfig::Webhook {
						url,
						method,
						headers,
						secret,
						message,
					}
				})
		)
			.prop_map(|(name, trigger_type, config)| Trigger {
				name,
				trigger_type,
				config,
			}),
		// Script strategy
		// Disabled for now as it requires a script to be present
		// (
		//     "[a-zA-Z0-9_]{1,10}".prop_map(|s| s.to_string()),
		//     Just(TriggerType::Script),
		//     (
		//         "/[a-z/]+\\.sh".prop_map(|s| s.to_string()),
		//         proptest::collection::vec("[a-zA-Z0-9-]{1,10}".prop_map(|s| s.to_string()),
		// 0..5),     )
		//         .prop_map(|(path, args)| TriggerTypeConfig::Script { path, args })
		// )
		//     .prop_map(|(name, trigger_type, config)| Trigger {
		//         name,
		//         trigger_type,
		//         config,
		//     })
	]
}

pub fn rpc_url_strategy() -> impl Strategy<Value = RpcUrl> {
	(
		Just("rpc".to_string()),
		"(http|https)://[a-z0-9-]+\\.[a-z]{2,}".prop_map(|s| s.to_string()),
		1..=100u32,
	)
		.prop_map(|(type_, url, weight)| RpcUrl { type_, url, weight })
}

pub fn network_strategy() -> impl Strategy<Value = Network> {
	(
		prop_oneof![Just(BlockChainType::EVM), Just(BlockChainType::Stellar)],
		"[a-z0-9_]{1,10}".prop_map(|s| s.to_string()), // slug
		"[a-zA-Z0-9_ ]{1,20}".prop_map(|s| s.to_string()), // name
		proptest::collection::vec(rpc_url_strategy(), 1..3),
		option::of(1..=100u64),                                       // chain_id
		option::of("[a-zA-Z0-9 ]{1,20}".prop_map(|s| s.to_string())), // network_passphrase
		1000..60000u64,                                               // block_time_ms
		1..=20u64,                                                    // confirmation_blocks
		"0 \\*/5 \\* \\* \\* \\*".prop_map(|s| s.to_string()),        // cron_schedule
		Just(Some(1u64)),                                             /* max_past_blocks -
		                                                               * ensure it's always
		                                                               * Some(1) or greater */
		option::of(prop::bool::ANY), // store_blocks
	)
		.prop_map(
			|(
				network_type,
				slug,
				name,
				rpc_urls,
				chain_id,
				network_passphrase,
				block_time_ms,
				confirmation_blocks,
				cron_schedule,
				max_past_blocks,
				store_blocks,
			)| Network {
				network_type,
				slug,
				name,
				rpc_urls,
				chain_id,
				network_passphrase,
				block_time_ms,
				confirmation_blocks,
				cron_schedule,
				max_past_blocks,
				store_blocks,
			},
		)
}

pub fn match_conditions_strategy() -> impl Strategy<Value = MatchConditions> {
	let function_condition_strategy = (
		"[a-zA-Z0-9_]+\\([a-zA-Z0-9,]+\\)".prop_map(|s| s.to_string()),
		option::of("[0-9]+ [><=] [0-9]+".prop_map(|s| s.to_string())),
	)
		.prop_map(|(signature, expression)| FunctionCondition {
			signature,
			expression,
		});

	let event_condition_strategy = (
		"[a-zA-Z0-9_]+\\([a-zA-Z0-9,]+\\)".prop_map(|s| s.to_string()),
		option::of("[0-9]+ [><=] [0-9]+".prop_map(|s| s.to_string())),
	)
		.prop_map(|(signature, expression)| EventCondition {
			signature,
			expression,
		});

	let transaction_condition_strategy = (
		prop_oneof![
			Just(TransactionStatus::Any),
			Just(TransactionStatus::Success),
			Just(TransactionStatus::Failure)
		],
		option::of("[0-9]+ [><=] [0-9]+".prop_map(|s| s.to_string())),
	)
		.prop_map(|(status, expression)| TransactionCondition { status, expression });

	(
		proptest::collection::vec(
			function_condition_strategy,
			MIN_COLLECTION_SIZE..MAX_COLLECTION_SIZE,
		),
		proptest::collection::vec(
			event_condition_strategy,
			MIN_COLLECTION_SIZE..MAX_COLLECTION_SIZE,
		),
		proptest::collection::vec(
			transaction_condition_strategy,
			MIN_COLLECTION_SIZE..MAX_COLLECTION_SIZE,
		),
	)
		.prop_map(|(functions, events, transactions)| MatchConditions {
			functions,
			events,
			transactions,
		})
}
