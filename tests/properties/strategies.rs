use email_address::EmailAddress;
use openzeppelin_monitor::models::{
	AddressWithABI, BlockChainType, EventCondition, FunctionCondition, MatchConditions, Monitor,
	Network, RpcUrl, ScriptLanguage, TransactionCondition, TransactionStatus, Trigger,
	TriggerConditions, TriggerType, TriggerTypeConfig,
};
use proptest::{option, prelude::*};
use std::os::unix::prelude::ExitStatusExt;

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
		trigger_conditions_strategy(),
	)
		.prop_map(
			|(
				triggers,
				networks,
				name,
				paused,
				addresses,
				match_conditions,
				trigger_conditions,
			)| Monitor {
				triggers,
				networks,
				name,
				paused,
				addresses,
				match_conditions,
				trigger_conditions,
			},
		)
}

pub fn trigger_strategy() -> impl Strategy<Value = Trigger> {
	prop_oneof![
		// Slack strategy
		(
			"[a-zA-Z0-9_]{1,10}".prop_map(|s| s.to_string()),
			Just(TriggerType::Slack),
			(
				"https://hooks\\.slack\\.com/[a-zA-Z0-9/]+".prop_map(|s| s.to_string()),
				"[a-zA-Z0-9]{1,20}".prop_map(|s| s.to_string()),
				"[a-zA-Z0-9]{1,50}".prop_map(|s| s.to_string()),
			)
				.prop_map(|(webhook_url, title, body)| TriggerTypeConfig::Slack {
					webhook_url,
					title,
					body,
				})
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
				"[a-zA-Z0-9]{1,20}".prop_map(|s| s.to_string()),
				"[a-zA-Z0-9]{1,50}".prop_map(|s| s.to_string()),
				"[a-zA-Z0-9]+@[a-z0-9]+\\.com".prop_map(|s| EmailAddress::new_unchecked(&s)),
				proptest::collection::vec(
					"[a-zA-Z0-9]+@[a-z0-9]+\\.com".prop_map(|s| EmailAddress::new_unchecked(&s)),
					1..5,
				),
			)
				.prop_map(
					|(host, port, username, password, subject, body, sender, recipients)| {
						TriggerTypeConfig::Email {
							host,
							port,
							username,
							password,
							subject,
							body,
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
				prop_oneof!["GET", "POST", "PUT", "DELETE"].prop_map(|s| s.to_string()),
				option::of(proptest::collection::hash_map(
					"[a-zA-Z-]{1,10}".prop_map(|s| s.to_string()),
					"[a-zA-Z0-9]{1,10}".prop_map(|s| s.to_string()),
					0..5,
				)),
			)
				.prop_map(|(url, method, headers)| TriggerTypeConfig::Webhook {
					url,
					method,
					headers,
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

pub fn trigger_conditions_strategy() -> impl Strategy<Value = Vec<TriggerConditions>> {
	let script_paths = prop::sample::select(vec![
		"tests/integration/fixtures/filters/test1.py".to_string(),
		"tests/integration/fixtures/filters/test2.py".to_string(),
		"tests/integration/fixtures/filters/test3.py".to_string(),
	]);

	(
		1u32..=100u32,
		script_paths, // Use predefined paths instead of random generation
		"[a-zA-Z0-9_]+".prop_map(|s| s.to_string()),
		Just(ScriptLanguage::Python),
		Just(1000u32),
	)
		.prop_map(
			|(execution_order, script_path, arguments, language, timeout_ms)| {
				vec![TriggerConditions {
					execution_order: Some(execution_order),
					script_path,
					arguments: Some(arguments),
					language,
					timeout_ms,
				}]
			},
		)
}

pub fn process_output_strategy() -> impl Strategy<Value = std::process::Output> {
	// Helper strategy for whitespace
	let whitespace_strategy = prop_oneof![
		Just(""),
		Just(" "),
		Just("  "),
		Just("\t"),
		Just("\n"),
		Just("\r\n")
	]
	.prop_map(|s| s.to_string());

	// Generate random stdout content with possible whitespace
	let stdout_strategy = prop_oneof![
		// Valid single boolean with optional whitespace
		(
			whitespace_strategy.clone(),
			prop_oneof![Just("true"), Just("false")],
			whitespace_strategy.clone()
		)
			.prop_map(|(pre, val, post)| format!("{}{}{}", pre, val, post)),
		// Valid multiple booleans with whitespace and newlines
		prop::collection::vec(
			(
				whitespace_strategy.clone(),
				prop_oneof![Just("true"), Just("false")],
				whitespace_strategy.clone()
			)
				.prop_map(|(pre, val, post)| format!("{}{}{}", pre, val, post)),
			1..5
		)
		.prop_map(|v| v.join("\n")),
		// Invalid content
		"[a-zA-Z0-9]{1,20}".prop_map(|s| s.to_string()),
		// Empty content with possible whitespace
		whitespace_strategy.clone(),
		// Mixed content with whitespace
		prop::collection::vec(
			(
				whitespace_strategy.clone(),
				prop_oneof![
					Just("true".to_string()),
					Just("false".to_string()),
					"[a-zA-Z0-9]{1,10}".prop_map(|s| s.to_string())
				],
				whitespace_strategy.clone()
			)
				.prop_map(|(pre, val, post)| format!("{}{}{}", pre, val, post)),
			1..5
		)
		.prop_map(|v| v.join("\n"))
	];

	// Generate random stderr content
	let stderr_strategy = prop_oneof![
		Just("".to_string()),
		"[a-zA-Z0-9\n]{0,50}".prop_map(|s| s.to_string())
	];

	(stdout_strategy, stderr_strategy, prop::bool::ANY).prop_map(|(stdout, stderr, success)| {
		std::process::Output {
			status: ExitStatusExt::from_raw(if success { 0 } else { 1 }),
			stdout: stdout.into_bytes(),
			stderr: stderr.into_bytes(),
		}
	})
}
