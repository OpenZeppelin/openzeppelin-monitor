use std::collections::HashMap;

use crate::{
    models::{EVMTransaction, MonitorMatch, StellarTransaction},
    repositories::TriggerRepositoryTrait,
    services::trigger::TriggerExecutionService,
};

use super::{
    helpers::evm::{h160_to_string, h256_to_string},
    FilterError,
};

pub async fn handle_match<T: TriggerRepositoryTrait>(
    matching_monitor: MonitorMatch,
    trigger_service: &TriggerExecutionService<T>,
) -> Result<(), FilterError> {
    match matching_monitor {
        MonitorMatch::EVM(evm_monitor_match) => {
            let transaction = EVMTransaction::from(evm_monitor_match.transaction.clone());
            // Convert transaction data to a HashMap
            let mut data = HashMap::new();
            data.insert(
                "transaction_hash".to_string(),
                h256_to_string(*transaction.hash()),
            );
            data.insert(
                "transaction_from".to_string(),
                h160_to_string(*transaction.sender()),
            );
            data.insert(
                "transaction_value".to_string(),
                transaction.value().to_string(),
            );
            if let Some(to) = transaction.to() {
                data.insert("transaction_to".to_string(), h160_to_string(*to));
            }
            data.insert("monitor_name".to_string(), evm_monitor_match.monitor.name);

            // We convert all matched events/functions arguments to a HashMap so it can be used in the body template of the trigger
            // This is formatted as follows:
            // "monitor_name": "Transfer USDT Token"
            // "transaction_hash": "0x99139c8f64b9b939678e261e1553660b502d9fd01c2ab1516e699ee6c8cc5791",
            // "transaction_from": "0xf401346fd255e034a2e43151efe1d68c1e0f8ca5",
            // "transaction_to": "0x0000000000001ff3684f28c67538d4d072c22734",
            // "transaction_value": "24504000000000000",
            // "event_0_signature": "Transfer(address,address,uint256)",
            // "event_0_to": "0x70bf6634ee8cb27d04478f184b9b8bb13e5f4710",
            // "event_0_from": "0x2e8135be71230c6b1b4045696d41c09db0414226",
            // "event_0_value": "88248701",
            // "event_1_signature": "Transfer(address,address,uint256)",
            // "event_1_from": "0x70bf6634ee8cb27d04478f184b9b8bb13e5f4710",
            // "event_1_to": "0x7afa9d836d2fccf172b66622625e56404e465dbd",
            // "event_1_value": "750113",

            let matched_args: HashMap<String, String> =
                if let Some(args) = &evm_monitor_match.matched_on_args {
                    let mut map = HashMap::new();
                    if let Some(functions) = &args.functions {
                        for (idx, func) in functions.iter().enumerate() {
                            // First add the signature
                            map.insert(
                                format!("function_{}_signature", idx),
                                func.signature.clone(),
                            );
                            // Then add all arguments
                            if let Some(func_args) = &func.args {
                                for arg in func_args {
                                    map.insert(
                                        format!("function_{}_{}", idx, arg.name),
                                        arg.value.clone(),
                                    );
                                }
                            }
                        }
                    }
                    if let Some(events) = &args.events {
                        for (idx, event) in events.iter().enumerate() {
                            // First add the signature
                            map.insert(format!("event_{}_signature", idx), event.signature.clone());
                            // Then add all arguments
                            if let Some(event_args) = &event.args {
                                for arg in event_args {
                                    map.insert(
                                        format!("event_{}_{}", idx, arg.name),
                                        arg.value.clone(),
                                    );
                                }
                            }
                        }
                    }
                    map
                } else {
                    HashMap::new()
                };

            data.extend(matched_args);
            let _ = trigger_service
                .execute(
                    &evm_monitor_match
                        .monitor
                        .triggers
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>(),
                    data,
                )
                .await;
        }
        MonitorMatch::Stellar(stellar_monitor_match) => {
            let transaction = StellarTransaction::from(stellar_monitor_match.transaction.clone());
            // Convert transaction data to a HashMap
            let mut data = HashMap::new();
            data.insert(
                "transaction_hash".to_string(),
                transaction.hash().to_string(),
            );

            // TODO: Add sender and value to the data so it can be used in the body template of the trigger
            // data.insert(
            //     "transaction_from".to_string(),
            //     transaction.sender().to_string(),
            // );
            // data.insert(
            //     "transaction_value".to_string(),
            //     transaction.value().to_string(),
            // );
            // if let Some(to) = transaction.to() {
            //     data.insert("transaction_to".to_string(), to.to_string());
            // }
            data.insert(
                "monitor_name".to_string(),
                stellar_monitor_match.monitor.name,
            );

            let matched_args: HashMap<String, String> =
                if let Some(args) = &stellar_monitor_match.matched_on_args {
                    let mut map = HashMap::new();
                    if let Some(functions) = &args.functions {
                        for (idx, func) in functions.iter().enumerate() {
                            // First add the signature
                            map.insert(
                                format!("function_{}_signature", idx),
                                func.signature.clone(),
                            );
                            // Then add all arguments
                            if let Some(func_args) = &func.args {
                                for arg in func_args {
                                    map.insert(
                                        format!("function_{}_{}", idx, arg.name),
                                        arg.value.clone(),
                                    );
                                }
                            }
                        }
                    }
                    if let Some(events) = &args.events {
                        for (idx, event) in events.iter().enumerate() {
                            // First add the signature
                            map.insert(format!("event_{}_signature", idx), event.signature.clone());
                            // Then add all arguments
                            if let Some(event_args) = &event.args {
                                for arg in event_args {
                                    map.insert(
                                        format!("event_{}_{}", idx, arg.name),
                                        arg.value.clone(),
                                    );
                                }
                            }
                        }
                    }
                    map
                } else {
                    HashMap::new()
                };

            data.extend(matched_args);
            let _ = trigger_service
                .execute(
                    &stellar_monitor_match
                        .monitor
                        .triggers
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>(),
                    data,
                )
                .await;
        }
    }
    Ok(())
}
