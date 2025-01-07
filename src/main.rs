pub mod models;
pub mod repositories;
pub mod services;
pub mod utils;

pub use models::{ConfigLoader, Monitor, Network, Trigger};
pub use repositories::{
    MonitorRepository, MonitorService, NetworkRepository, NetworkService, TriggerRepository,
    TriggerService,
};
pub use services::blockwatcher::BlockWatcherService;
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
use std::sync::Arc;

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

    // Get monitors once before creating the handler
    let monitors = monitor_service.get_all();

    // Create the block handler closure
    let block_handler = Arc::new({
        let trigger_service = trigger_execution_service.clone();
        let filter_service = filter_service.clone();
        let active_monitors = monitors
            .clone()
            .into_values()
            .filter(|m| !m.paused)
            .collect::<Vec<_>>();

        move |block: &BlockType, network: &Network| {
            if active_monitors.is_empty() {
                info!("No active monitors found. Skipping block.");
                return;
            }

            let monitors = active_monitors
                .clone()
                .into_iter()
                .filter(|m| m.networks.contains(&network.slug))
                .collect::<Vec<_>>();

            if monitors.is_empty() {
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

            // // Use block_in_place instead of spawn to process sequentially
            // tokio::task::block_in_place(|| {
            //     tokio::runtime::Handle::current().block_on(async {
            //         let matches = filter_service
            //             .filter_block(&network, &block, &monitors)
            //             .await;
            //         if let Ok(matches) = matches {
            //             for matching_monitor in matches {
            //                 let _ = handle_match(matching_monitor, &trigger_service).await;
            //             }
            //         }
            //     })
            // });

            // Spawn a new task to handle the block processing
            tokio::spawn(async move {
                let client = create_blockchain_client(&network).await.unwrap();
                let matches = filter_service
                    .filter_block(&client, &network, &block, &monitors)
                    .await;
                if let Ok(matches) = matches {
                    for matching_monitor in matches {
                        let _ = handle_match(matching_monitor, &trigger_service).await;
                    }
                }
            });
        }
    });

    // Create block watcher with the handler
    let block_watcher = BlockWatcherService::new(network_service, block_handler).await?;

    // Spawn the watcher in a separate task
    let watcher_handle = tokio::spawn(async move {
        if let Err(e) = block_watcher.start().await {
            error!("Block watcher error: {}", e);
        }
    });

    // Wait for shutdown signal
    info!("Service started. Press Ctrl+C to shutdown");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Shutdown signal received, stopping services...");
        }
    }

    // Gracefully shutdown the watcher
    watcher_handle.abort();
    if let Err(e) = watcher_handle.await {
        error!("Error during watcher shutdown: {}", e);
    }

    info!("Shutdown complete");
    Ok(())
}
