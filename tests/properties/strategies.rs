use email_address::EmailAddress;
use openzeppelin_monitor::models::{
	AddressWithABI, BlockChainType, EventCondition, FunctionCondition, MatchConditions, Monitor,
	Network, NotificationMessage, RpcUrl, ScriptLanguage, TransactionCondition, TransactionStatus,
	Trigger, TriggerConditions, TriggerType, TriggerTypeConfig,
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

pub fn trigger_conditions_strategy() -> impl Strategy<Value = Vec<TriggerConditions>> {
	let script_paths = prop::sample::select(vec![
		"tests/integration/fixtures/filters/evm_filter_block_number.py".to_string(),
		"tests/integration/fixtures/filters/stellar_filter_block_number.py".to_string(),
		"tests/integration/fixtures/filters/evm_filter_block_number.js".to_string(),
		"tests/integration/fixtures/filters/stellar_filter_block_number.js".to_string(),
		"tests/integration/fixtures/filters/evm_filter_block_number.sh".to_string(),
		"tests/integration/fixtures/filters/stellar_filter_block_number.sh".to_string(),
	]);

	(
		script_paths,
		"[a-zA-Z0-9_]+".prop_map(|s| s.to_string()),
		Just(1000u32),
	)
		.prop_map(|(script_path, arguments, timeout_ms)| {
			let language = match script_path.split('.').last() {
				Some("py") => ScriptLanguage::Python,
				Some("js") => ScriptLanguage::JavaScript,
				Some("sh") => ScriptLanguage::Bash,
				_ => ScriptLanguage::Python, // fallback to Python for unknown extensions
			};

			vec![TriggerConditions {
				script_path,
				arguments: Some(arguments),
				language,
				timeout_ms,
			}]
		})
}

pub fn process_output_strategy() -> impl Strategy<Value = std::process::Output> {
	// Helper strategy for debug output lines
	let debug_line_strategy = prop_oneof![
		"[a-zA-Z0-9 ]{1,50}".prop_map(|s| format!("{}...", s)),
		"debugging...".prop_map(|s| s.to_string()),
		"Processing data...".prop_map(|s| s.to_string()),
		"Starting script execution...".prop_map(|s| s.to_string())
	];

	// Generate stdout content with optional debug lines and a final boolean
	let stdout_strategy = (
		// 0 to 5 debug lines
		prop::collection::vec(debug_line_strategy, 0..5),
		// Final boolean output with optional whitespace
		(
			"[ \t]*".prop_map(|s| s.to_string()),
			prop_oneof![Just("true"), Just("false")],
			"[ \t\n]*".prop_map(|s| s.to_string()),
		),
	)
		.prop_map(|(debug_lines, (pre, val, post))| {
			let mut output = String::new();
			for line in debug_lines {
				output.push_str(&line);
				output.push('\n');
			}
			output.push_str(&format!("{}{}{}", pre, val, post));
			output
		});

	// Generate stderr content for error cases
	let stderr_strategy = prop_oneof![
		Just("".to_string()),
		"Script execution failed:.*".prop_map(|s| s.to_string()),
		"ImportError:.*".prop_map(|s| s.to_string()),
		"SyntaxError:.*".prop_map(|s| s.to_string())
	];

	(stdout_strategy, stderr_strategy, prop::bool::ANY).prop_map(|(stdout, stderr, success)| {
		std::process::Output {
			status: ExitStatusExt::from_raw(if success { 0 } else { 1 }),
			stdout: stdout.into_bytes(),
			stderr: stderr.into_bytes(),
		}
	})
}
