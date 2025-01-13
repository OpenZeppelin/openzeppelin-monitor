use email_address::EmailAddress;
use openzeppelin_monitor::models::{
    AddressWithABI, BlockChainType, ConfigLoader, EventCondition, FunctionCondition,
    MatchConditions, Monitor, Network, RpcUrl, TransactionCondition, TransactionStatus, Trigger,
    TriggerType, TriggerTypeConfig,
};
use openzeppelin_monitor::repositories::{MonitorRepository, MonitorRepositoryTrait};
use proptest::option;
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::Config;

const MIN_TEST_CASES: usize = 1;
const MAX_TEST_CASES: usize = 10;
const MIN_COLLECTION_SIZE: usize = 0;
const MAX_COLLECTION_SIZE: usize = 10;
const MAX_ADDRESSES: usize = 10;

// Custom strategies for generating Monitor, Network, and Trigger
fn monitor_strategy(
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

fn trigger_strategy() -> impl Strategy<Value = Trigger> {
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
                    |(host, port, username, password, subject, body, sender, receipients)| {
                        TriggerTypeConfig::Email {
                            host,
                            port,
                            username,
                            password,
                            subject,
                            body,
                            sender,
                            receipients,
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
        //         proptest::collection::vec("[a-zA-Z0-9-]{1,10}".prop_map(|s| s.to_string()), 0..5),
        //     )
        //         .prop_map(|(path, args)| TriggerTypeConfig::Script { path, args })
        // )
        //     .prop_map(|(name, trigger_type, config)| Trigger {
        //         name,
        //         trigger_type,
        //         config,
        //     })
    ]
}

fn rpc_url_strategy() -> impl Strategy<Value = RpcUrl> {
    (
        Just("rpc".to_string()),
        "(http|https)://[a-z0-9-]+\\.[a-z]{2,}".prop_map(|s| s.to_string()),
        1..=100u32,
    )
        .prop_map(|(type_, url, weight)| RpcUrl { type_, url, weight })
}

fn network_strategy() -> impl Strategy<Value = Network> {
    (
        prop_oneof![Just(BlockChainType::EVM), Just(BlockChainType::Stellar)],
        "[a-z0-9_]{1,10}".prop_map(|s| s.to_string()), // slug
        "[a-zA-Z0-9_ ]{1,20}".prop_map(|s| s.to_string()), // name
        proptest::collection::vec(rpc_url_strategy(), 1..3),
        option::of(1..=100u64), // chain_id
        option::of("[a-zA-Z0-9 ]{1,20}".prop_map(|s| s.to_string())), // network_passphrase
        1000..60000u64,         // block_time_ms
        1..=20u64,              // confirmation_blocks
        "\\*/5 \\* \\* \\* \\*".prop_map(|s| s.to_string()), // cron_schedule
        Just(Some(1u64)),       // max_past_blocks - ensure it's always Some(1) or greater
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

fn match_conditions_strategy() -> impl Strategy<Value = MatchConditions> {
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

proptest! {
    #![proptest_config(Config {
        failure_persistence: None,
        ..Config::default()
    })]
    #[test]
    fn validate_monitor_references_test(
        triggers in proptest::collection::hash_map(
            "[a-zA-Z0-9_]{1,10}",
            trigger_strategy(),
            MIN_TEST_CASES..MAX_TEST_CASES
        ),
        networks in proptest::collection::hash_map(
            "[a-zA-Z0-9_]{1,10}",
            network_strategy(),
            MIN_TEST_CASES..MAX_TEST_CASES
        ),
    ) {
        // Get the available network and trigger names
        let network_names: Vec<String> = networks.keys().cloned().collect();
        let trigger_names: Vec<String> = triggers.keys().cloned().collect();

        // Generate monitors using only available networks and triggers
        let monitors = proptest::collection::hash_map(
            "[a-zA-Z0-9_]{1,10}",
            monitor_strategy(network_names.clone(), trigger_names.clone()),
            MIN_TEST_CASES..MAX_TEST_CASES
        ).new_tree(&mut proptest::test_runner::TestRunner::default())
        .unwrap()
        .current();

        // Test that valid monitors pass validation
        let result = MonitorRepository::validate_monitor_references(
            &monitors,
            &triggers,
            &networks,
        );
        assert!(result.is_ok());

        // Test that invalid references fail validation
        let mut invalid_monitors = monitors.clone();
        for monitor in invalid_monitors.values_mut() {
            monitor.triggers.push("non_existent_trigger".to_string());
            monitor.networks.push("non_existent_network".to_string());
        }

        let invalid_result = MonitorRepository::validate_monitor_references(
            &invalid_monitors,
            &triggers,
            &networks,
        );
        assert!(invalid_result.is_err());
    }

    #[test]
    fn repository_roundtrip_test(
        monitors in proptest::collection::hash_map(
            "[a-zA-Z0-9_]{1,10}",
            monitor_strategy(vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
            ], vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
            ]),
            MIN_TEST_CASES..MAX_TEST_CASES
        )
    ) {
        // Simulate saving and reloading from a repository
        let repo = MonitorRepository { monitors: monitors.clone() };
        let reloaded_monitors = repo.get_all();

        assert_eq!(monitors, reloaded_monitors); // Ensure roundtrip consistency
    }

    #[test]
    fn get_by_id_consistency_test(
        monitors in proptest::collection::hash_map(
            "[a-zA-Z0-9_]{1,10}",
            monitor_strategy(vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
            ], vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
            ]),
            MIN_TEST_CASES..MAX_TEST_CASES
        )
    ) {
        // Simulate getting monitors by ID
        let repo = MonitorRepository { monitors: monitors.clone() };
        for (id, monitor) in &monitors {
            let retrieved = repo.get(id);
            assert_eq!(Some(monitor.clone()), retrieved); // Ensure consistency for each ID
        }
    }

    #[test]
    fn validate_trigger_config_test(
        triggers in proptest::collection::vec(
            trigger_strategy(),
            MIN_TEST_CASES..MAX_TEST_CASES
        )
    ) {
        for trigger in triggers {
            // Valid trigger should pass validation
            assert!(trigger.validate().is_ok());

            // Test invalid cases
            match &trigger.config {
                TriggerTypeConfig::Slack { webhook_url: _, title: _, body: _ } => {
                    let mut invalid_trigger = trigger.clone();
                    if let TriggerTypeConfig::Slack { webhook_url: url, .. } = &mut invalid_trigger.config {
                        *url = "not-a-url".to_string(); // Invalid URL format
                    }
                    assert!(invalid_trigger.validate().is_err());

                    // Test empty title
                    let mut invalid_trigger = trigger.clone();
                    if let TriggerTypeConfig::Slack { title: t, .. } = &mut invalid_trigger.config {
                        *t = "".to_string();
                    }
                    assert!(invalid_trigger.validate().is_err());
                },
                TriggerTypeConfig::Email { host: _, port: _, username: _, password: _, subject: _, body: _, sender: _, receipients: _ } => {
                    // Test empty recipients
                    let mut invalid_trigger = trigger.clone();
                    if let TriggerTypeConfig::Email { receipients: r, .. } = &mut invalid_trigger.config {
                        r.clear();
                    }
                    assert!(invalid_trigger.validate().is_err());

                    // Test invalid host
                    let mut invalid_trigger = trigger.clone();
                    if let TriggerTypeConfig::Email { host: h, .. } = &mut invalid_trigger.config {
                        *h = "not-a-host".to_string();
                    }
                    assert!(invalid_trigger.validate().is_err());

                    // Test whitespace-only subject
                    let mut invalid_trigger = trigger.clone();
                    if let TriggerTypeConfig::Email { subject: s, .. } = &mut invalid_trigger.config {
                        *s = "   ".to_string();
                    }
                    assert!(invalid_trigger.validate().is_err());
                },
                TriggerTypeConfig::Webhook { url: _, method: _, headers: _ } => {
                    // Test invalid method
                    let mut invalid_trigger = trigger.clone();
                    if let TriggerTypeConfig::Webhook { method: m, .. } = &mut invalid_trigger.config {
                        *m = "INVALID_METHOD".to_string();
                    }
                    assert!(invalid_trigger.validate().is_err());

                    // Test invalid URL
                    let mut invalid_trigger = trigger.clone();
                    if let TriggerTypeConfig::Webhook { url: u, .. } = &mut invalid_trigger.config {
                        *u = "not-a-url".to_string();
                    }
                    assert!(invalid_trigger.validate().is_err());
                },
                TriggerTypeConfig::Script { path: _, args: _     } => {
                    // Test invalid path
                    let mut invalid_trigger = trigger.clone();
                    if let TriggerTypeConfig::Script { path: p, .. } = &mut invalid_trigger.config {
                        *p = "invalid/path/no-extension".to_string();
                    }
                    assert!(invalid_trigger.validate().is_err());

                    // Test empty path
                    let mut invalid_trigger = trigger.clone();
                    if let TriggerTypeConfig::Script { path: p, .. } = &mut invalid_trigger.config {
                        *p = "".to_string();
                    }
                    assert!(invalid_trigger.validate().is_err());
                }
            }
        }
    }

    #[test]
    fn validate_network_config_test(
        networks in proptest::collection::vec(
            network_strategy(),
            MIN_TEST_CASES..MAX_TEST_CASES
        )
    ) {
        for network in networks {
            // Valid network should pass validation
            assert!(network.validate().is_ok());

            // Test invalid cases
            let mut invalid_network = network.clone();
            invalid_network.block_time_ms = 50; // Too low block time
            assert!(invalid_network.validate().is_err());

            let mut invalid_network = network.clone();
            invalid_network.confirmation_blocks = 0; // Invalid confirmation blocks
            assert!(invalid_network.validate().is_err());

            let mut invalid_network = network.clone();
            invalid_network.rpc_urls[0].url = "invalid-url".to_string(); // Invalid RPC URL
            assert!(invalid_network.validate().is_err());

            let mut invalid_network = network.clone();
            invalid_network.slug = "INVALID_SLUG".to_string(); // Invalid slug with uppercase
            assert!(invalid_network.validate().is_err());
        }
    }

    #[test]
    fn validate_monitor_config_test(
        monitors in proptest::collection::vec(
            monitor_strategy(
                vec!["network1".to_string()],
                vec!["trigger1".to_string()]
            ),
            MIN_TEST_CASES..MAX_TEST_CASES
        )
    ) {
        for monitor in monitors {
            // Valid monitor should pass validation
            assert!(monitor.validate().is_ok());

            // Test invalid function signature
            let mut invalid_monitor = monitor.clone();
            if let Some(func) = invalid_monitor.match_conditions.functions.first_mut() {
                func.signature = "invalid_signature".to_string(); // Missing parentheses
                assert!(invalid_monitor.validate().is_err());
            }

            // Test invalid event signature
            let mut invalid_monitor = monitor.clone();
            if let Some(event) = invalid_monitor.match_conditions.events.first_mut() {
                event.signature = "invalid_signature".to_string(); // Missing parentheses
                assert!(invalid_monitor.validate().is_err());
            }
        }
    }
}
