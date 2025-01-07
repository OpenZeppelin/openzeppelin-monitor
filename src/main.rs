//! Blockchain monitoring service entry point.
//!
//! This binary provides the main entry point for the blockchain monitoring service.
//! It initializes all required services, sets up blockchain watchers for configured
//! networks, and handles graceful shutdown on interrupt signals.
//!
//! The service follows these main steps:
//! 1. Loads configurations from the default directory
//! 2. Initializes core services (monitoring, filtering, notifications)
//! 3. Sets up blockchain watchers for networks with active monitors
//! 4. Processes blocks and triggers notifications based on configured conditions
//! 5. Handles graceful shutdown on Ctrl+C

pub mod models;
pub mod repositories;
pub mod services;
pub mod utils;

pub use models::{ConfigLoader, Monitor, Network, Trigger};
pub use repositories::{
    MonitorRepository, MonitorService, NetworkRepository, NetworkService, TriggerRepository,
    TriggerService,
};
pub use services::blockwatcher::{BlockWatcherService, FileBlockStorage};
pub use services::filter::FilterService;
pub use services::notification::{Notifier, SlackNotifier};

use crate::{
    models::BlockType,
    services::{
        blockchain::create_blockchain_client, filter::handle_match,
        notification::NotificationService, trigger::TriggerExecutionService,
    },
};

use log::{error, info};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();

    // Instantiate all services
    let network_service = NetworkService::<NetworkRepository>::new(None)?;
    let trigger_service = TriggerService::<TriggerRepository>::new(None)?;
    let monitor_service = MonitorService::<MonitorRepository>::new(None)?;
    let notification_service = NotificationService::new();

    let filter_service = Arc::new(FilterService::new());
    let trigger_execution_service = Arc::new(TriggerExecutionService::new(
        trigger_service,
        notification_service,
    ));

    // Get monitors and networks once before creating the handler
    let monitors = monitor_service.get_all();
    let active_monitors = filter_active_monitors(monitors);
    let networks = network_service.get_all();

    // Check if we have any networks with active monitors
    let networks_with_monitors: Vec<Network> = networks
        .clone()
        .into_values()
        .filter(|network| !filter_network_monitors(&active_monitors, &network.slug).is_empty())
        .collect();

    if networks_with_monitors.is_empty() {
        info!("No networks with active monitors found. Exiting...");
        return Ok(());
    }

    // Add shutdown channel
    let (shutdown_tx, _) = broadcast::channel(1);

    // Create the block handler closure
    let block_handler = Arc::new({
        let trigger_service = trigger_execution_service.clone();
        let filter_service = filter_service.clone();
        let shutdown_tx = shutdown_tx.clone();
        let active_monitors = active_monitors;

        move |block: &BlockType, network: &Network| {
            let applicable_monitors = filter_network_monitors(&active_monitors, &network.slug);
            if applicable_monitors.is_empty() {
                info!(
                    "No monitors for network {} to process. Skipping block.",
                    network.slug
                );
                return;
            }

            let trigger_service = trigger_service.clone();
            let filter_service = filter_service.clone();
            let network = network.clone();
            let block = block.clone();
            let mut shutdown_rx = shutdown_tx.subscribe();

            tokio::spawn(async move {
                let client = match create_blockchain_client(&network).await {
                    Ok(client) => client,
                    Err(e) => {
                        error!("Failed to create blockchain client: {}", e);
                        return;
                    }
                };

                tokio::select! {
                    result = filter_service.filter_block(&client, &network, &block, &applicable_monitors) => {
                        match result {
                            Ok(matches) => {
                                for matching_monitor in matches {
                                    if let Err(e) = handle_match(matching_monitor, &trigger_service).await {
                                        error!("Error handling match: {}", e);
                                    }
                                }
                            }
                            Err(e) => error!("Error filtering block: {}", e),
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Shutting down block processing task");
                        return;
                    }
                }
            });
        }
    });

    // Create block watcher service
    let block_watcher = BlockWatcherService::<NetworkRepository, FileBlockStorage>::new(
        network_service,
        FileBlockStorage::new(),
        block_handler,
    )
    .await?;

    // Start watchers for networks that have active monitors
    for network in networks_with_monitors {
        block_watcher.start_network_watcher(&network).await?;
    }

    // Wait for shutdown signal
    info!("Service started. Press Ctrl+C to shutdown");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Shutdown signal received, stopping services...");
            let _ = shutdown_tx.send(());  // Notify all tasks to shutdown

            // Stop all network watchers
            for network in networks.values() {
                if let Err(e) = block_watcher.stop_network_watcher(&network.slug).await {
                    error!("Error stopping watcher for network {}: {}", network.slug, e);
                }
            }
        }
    }

    info!("Shutdown complete");
    Ok(())
}

// Helper functions
fn filter_active_monitors(monitors: HashMap<String, Monitor>) -> Vec<Monitor> {
    monitors
        .into_values()
        .filter(|m| !m.paused)
        .collect::<Vec<_>>()
}

fn filter_network_monitors(monitors: &[Monitor], network_slug: &String) -> Vec<Monitor> {
    monitors
        .iter()
        .filter(|m| m.networks.contains(&network_slug))
        .cloned()
        .collect()
}
