#[path = "mocks/mod.rs"]
mod mocks;

#[cfg(test)]
mod filter_evm_tests {

    use log::info;
    use openzeppelin_monitor::models::{BlockType, Monitor, MonitorMatch, Network, Trigger};
    use openzeppelin_monitor::repositories::TriggerService;
    use openzeppelin_monitor::services::{
        blockchain::create_blockchain_client,
        filter::{handle_match, FilterError, FilterService},
        notification::NotificationService,
        trigger::TriggerExecutionService,
    };
    use serde_json;
    use std::collections::HashMap;
    use std::fs;

    use crate::mocks::MockTriggerRepository;

    #[tokio::test]
    async fn test_filter_block_with_mint_token() -> Result<(), FilterError> {
        let _ = env_logger::builder().is_test(true).try_init();
        // Update paths to use src/tests/fixtures instead of just fixtures
        let blocks_json = fs::read_to_string("tests/fixtures/evm/blocks.json")
            .expect("Failed to read blocks JSON file");
        let monitor_json = fs::read_to_string("tests/fixtures/evm/monitors/monitor.json")
            .expect("Failed to read monitor JSON file");
        let network_json = fs::read_to_string("tests/fixtures/evm/networks/network.json")
            .expect("Failed to read network JSON file");
        let trigger_json = fs::read_to_string("tests/fixtures/evm/triggers/trigger.json")
            .expect("Failed to read trigger JSON file");

        // Parse JSON into their respective types
        let blocks: Vec<BlockType> =
            serde_json::from_str(&blocks_json).expect("Failed to parse blocks JSON");
        let monitor: Monitor =
            serde_json::from_str(&monitor_json).expect("Failed to parse monitor JSON");
        let network: Network =
            serde_json::from_str(&network_json).expect("Failed to parse network JSON");

        // Create filter service
        let filter_service = FilterService::new();

        let client = create_blockchain_client(&network).await.unwrap();

        // Run filter_block with the test data
        let matches = filter_service
            .filter_block(&client, &network, &blocks[0], &[monitor])
            .await?;

        // Assertions
        assert!(!matches.is_empty(), "Should have found matching events");

        let mut mock_trigger_repository = MockTriggerRepository::new();
        let trigger_map: HashMap<String, Trigger> =
            serde_json::from_str(&trigger_json).expect("Failed to parse trigger JSON");
        let triggers = trigger_map.clone();

        mock_trigger_repository
            .expect_load_all()
            .returning(move |_| Ok(triggers.clone()));

        mock_trigger_repository
            .expect_get()
            .returning(move |id| trigger_map.get(id).cloned());

        let trigger_service = TriggerService::new_with_repository(mock_trigger_repository).unwrap();
        let notification_service = NotificationService::new();

        let trigger_execution_service =
            TriggerExecutionService::new(trigger_service, notification_service);

        for matching_monitor in matches {
            match matching_monitor.clone() {
                MonitorMatch::EVM(evm_monitor_match) => {
                    info!(
                        "EVM monitor match: {:?}",
                        evm_monitor_match.transaction.hash()
                    );
                }
                _ => {
                    info!("Unknown monitor match");
                }
            }
            let _ = handle_match(matching_monitor, &trigger_execution_service).await;
        }
        Ok(())
    }
}

mod filter_stellar_tests {
    use log::info;
    use openzeppelin_monitor::{
        models::{
            BlockType, Monitor, MonitorMatch, Network, StellarEvent, StellarTransaction,
            TransactionInfo, Trigger,
        },
        repositories::TriggerService,
        services::{
            blockchain::BlockChainClientEnum,
            filter::{handle_match, FilterError, FilterService},
            notification::NotificationService,
            trigger::TriggerExecutionService,
        },
    };

    use crate::mocks::{MockStellarClientTrait, MockTriggerRepository};

    use serde_json;
    use std::{collections::HashMap, fs};

    #[tokio::test]
    async fn test_filter_block_with_mint_token() -> Result<(), FilterError> {
        let _ = env_logger::builder().is_test(true).try_init();

        // Load test data
        let blocks_json = fs::read_to_string("tests/fixtures/stellar/blocks.json")
            .expect("Failed to read blocks JSON file");
        let monitor_json = fs::read_to_string("tests/fixtures/stellar/monitors/monitor.json")
            .expect("Failed to read monitor JSON file");
        let network_json = fs::read_to_string("tests/fixtures/stellar/networks/network.json")
            .expect("Failed to read network JSON file");
        let events_json = fs::read_to_string("tests/fixtures/stellar/events.json")
            .expect("Failed to read events JSON file");
        let transactions_json = fs::read_to_string("tests/fixtures/stellar/transactions.json")
            .expect("Failed to read transactions JSON file");
        let trigger_json = fs::read_to_string("tests/fixtures/stellar/triggers/trigger.json")
            .expect("Failed to read trigger JSON file");

        // Parse JSON
        let blocks: Vec<BlockType> =
            serde_json::from_str(&blocks_json).expect("Failed to parse blocks JSON");
        let monitor: Monitor =
            serde_json::from_str(&monitor_json).expect("Failed to parse monitor JSON");
        let network: Network =
            serde_json::from_str(&network_json).expect("Failed to parse network JSON");
        let events: Vec<StellarEvent> =
            serde_json::from_str(&events_json).expect("Failed to parse events JSON");
        let transactions: Vec<TransactionInfo> =
            serde_json::from_str(&transactions_json).expect("Failed to parse transactions JSON");

        let mut mock = MockStellarClientTrait::new();
        let mut decoded_transactions: Vec<StellarTransaction> = vec![];

        // Create a StellarTransaction `from` the LedgerTransaction so we decode the envelope
        for tx in &transactions {
            decoded_transactions.push(StellarTransaction::from(tx.clone()));
        }

        mock.expect_get_transactions()
            .times(1)
            .returning(move |_, _| Ok(decoded_transactions.clone()));

        mock.expect_get_events()
            .times(1)
            .returning(move |_, _| Ok(events.clone()));

        // Create a BlockChainClientEnum with the mock
        let mock_client = BlockChainClientEnum::Stellar(Box::new(mock));

        let filter_service = FilterService::new();

        // Run filter_block with the test data
        let matches = filter_service
            .filter_block(&mock_client, &network, &blocks[0], &[monitor])
            .await?;

        // Assertions
        assert!(!matches.is_empty(), "Should have found matching events");

        let mut mock_trigger_repository = MockTriggerRepository::new();
        let trigger_map: HashMap<String, Trigger> =
            serde_json::from_str(&trigger_json).expect("Failed to parse trigger JSON");
        let triggers = trigger_map.clone();

        mock_trigger_repository
            .expect_load_all()
            .returning(move |_| Ok(triggers.clone()));

        mock_trigger_repository
            .expect_get()
            .returning(move |id| trigger_map.get(id).cloned());

        let trigger_service = TriggerService::new_with_repository(mock_trigger_repository).unwrap();
        let notification_service = NotificationService::new();

        let trigger_execution_service =
            TriggerExecutionService::new(trigger_service, notification_service);

        for matching_monitor in matches {
            match matching_monitor.clone() {
                MonitorMatch::Stellar(stellar_monitor_match) => {
                    info!(
                        "Stellar monitor match: {:?}",
                        stellar_monitor_match.transaction.hash()
                    );
                }
                _ => {
                    info!("Unknown monitor match");
                }
            }
            let _ = handle_match(matching_monitor, &trigger_execution_service).await;
        }
        Ok(())
    }
}
