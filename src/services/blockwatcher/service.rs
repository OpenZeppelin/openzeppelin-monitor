use log::{error, info};
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};

use super::error::BlockWatcherError;
use super::storage::{BlockStorage, FileBlockStorage};

use crate::models::{BlockType, Network};
use crate::repositories::{NetworkRepositoryTrait, NetworkService};
use crate::services::blockchain::{create_blockchain_client, BlockChainClient};

pub struct BlockWatcherService<T: NetworkRepositoryTrait> {
    network_service: Arc<NetworkService<T>>,
    block_storage: Arc<dyn BlockStorage + Send + Sync>,
    block_handler: Arc<dyn Fn(&BlockType, &Network) + Send + Sync>,
}

impl<T: NetworkRepositoryTrait> BlockWatcherService<T> {
    pub async fn new(
        network_service: NetworkService<T>,
        block_handler: Arc<dyn Fn(&BlockType, &Network) + Send + Sync>,
    ) -> Result<Self, BlockWatcherError> {
        Ok(BlockWatcherService {
            network_service: Arc::new(network_service),
            block_storage: Arc::new(FileBlockStorage::new()),
            block_handler,
        })
    }

    pub async fn start(&self) -> Result<(), BlockWatcherError> {
        let networks = self.network_service.get_all();

        if networks.is_empty() {
            info!("No networks found, block watcher will not start");
            return Ok(());
        }

        info!("Scheduling block watchers for {} networks", networks.len());

        let scheduler = JobScheduler::new().await.map_err(|e| {
            BlockWatcherError::scheduler_error(format!("Failed to create scheduler: {}", e))
        })?;

        for (_, network) in networks {
            self.schedule_network_watcher(&scheduler, &network).await?;
        }

        scheduler.start().await.map_err(|e| {
            BlockWatcherError::scheduler_error(format!("Failed to start scheduler: {}", e))
        })?;

        info!("Block watcher started successfully");
        Ok(())
    }

    async fn schedule_network_watcher(
        &self,
        scheduler: &JobScheduler,
        network: &Network,
    ) -> Result<(), BlockWatcherError> {
        let network_clone = network.clone();
        let block_storage = self.block_storage.clone();
        let block_handler = self.block_handler.clone();

        let job = Job::new_async(network.cron_schedule.as_str(), move |_uuid, _l| {
            let network = network_clone.clone();
            let block_storage = block_storage.clone();
            let block_handler = block_handler.clone();

            Box::pin(async move {
                match process_new_blocks(&network, block_storage, block_handler).await {
                    Ok(_) => info!(
                        "Successfully processed blocks for network: {}",
                        network.slug
                    ),
                    Err(e) => error!(
                        "Error processing blocks for network {}: {}",
                        network.slug, e
                    ),
                }
            })
        })
        .map_err(|e| BlockWatcherError::scheduler_error(format!("Failed to create job: {}", e)))?;

        scheduler
            .add(job)
            .await
            .map_err(|e| BlockWatcherError::scheduler_error(format!("Failed to add job: {}", e)))?;

        info!("Scheduled block watcher for network: {}", network.slug);
        Ok(())
    }
}

const DEFAULT_MAX_PAST_BLOCKS: u64 = 10;

async fn process_new_blocks(
    network: &Network,
    block_storage: Arc<dyn BlockStorage + Send + Sync>,
    block_handler: Arc<dyn Fn(&BlockType, &Network) + Send + Sync>,
) -> Result<(), BlockWatcherError> {
    let rpc_client = create_blockchain_client(network).await.map_err(|e| {
        BlockWatcherError::network_error(format!("Failed to create RPC client: {}", e))
    })?;

    let last_processed_block = block_storage
        .get_last_processed_block(&network.slug)
        .await
        .map_err(|e| {
            BlockWatcherError::storage_error(format!("Failed to get last processed block: {}", e))
        })?
        .unwrap_or(0);

    let latest_block = rpc_client.get_latest_block_number().await.map_err(|e| {
        BlockWatcherError::network_error(format!("Failed to get latest block number: {}", e))
    })?;

    let latest_confirmed_block = latest_block.saturating_sub(network.confirmation_blocks);
    let max_past_blocks = network.max_past_blocks.unwrap_or(DEFAULT_MAX_PAST_BLOCKS);

    info!(
        "Processing blocks for network {} ({}). Last processed: {}, Latest confirmed: {} (waiting {} confirmations, max past blocks: {})",
        network.name, network.slug, last_processed_block, latest_confirmed_block, network.confirmation_blocks, max_past_blocks
    );

    let mut blocks = Vec::new();
    if last_processed_block == 0 {
        blocks = rpc_client
            .get_blocks(latest_confirmed_block, None)
            .await
            .map_err(|e| {
                BlockWatcherError::network_error(format!(
                    "Failed to get block {}: {}",
                    latest_confirmed_block, e
                ))
            })?;
    } else if last_processed_block < latest_confirmed_block {
        // Calculate the start block number, using the default if max_past_blocks is not set
        let start_block = std::cmp::max(
            last_processed_block + 1,
            latest_confirmed_block.saturating_sub(max_past_blocks.saturating_sub(1)),
        );

        blocks = rpc_client
            .get_blocks(start_block, Some(latest_confirmed_block))
            .await
            .map_err(|e| {
                BlockWatcherError::network_error(format!(
                    "Failed to get blocks from {} to {}: {}",
                    start_block, latest_confirmed_block, e
                ))
            })?;
    }

    for block in &blocks {
        (block_handler)(block, network);
    }

    if network.store_blocks.unwrap_or(false) {
        // Delete old blocks before saving new ones
        block_storage
            .delete_blocks(&network.slug)
            .await
            .map_err(|e| {
                BlockWatcherError::storage_error(format!("Failed to delete old blocks: {}", e))
            })?;

        block_storage
            .save_blocks(&network.slug, &blocks)
            .await
            .map_err(|e| {
                BlockWatcherError::storage_error(format!("Failed to save blocks: {}", e))
            })?;
    }
    // Update the last processed block
    block_storage
        .save_last_processed_block(&network.slug, latest_confirmed_block)
        .await
        .map_err(|e| {
            BlockWatcherError::storage_error(format!("Failed to save last processed block: {}", e))
        })?;

    Ok(())
}
