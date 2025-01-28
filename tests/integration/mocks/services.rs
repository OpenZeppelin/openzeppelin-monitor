use async_trait::async_trait;
use mockall::mock;
use std::collections::HashMap;

use openzeppelin_monitor::{
	models::{BlockType, Monitor, MonitorMatch, Network},
	repositories::{TriggerRepositoryTrait, TriggerService},
	services::{
		blockchain::BlockFilterFactory,
		blockwatcher::{BlockStorage, BlockTrackerTrait, BlockWatcherError},
		filter::FilterError,
		notification::NotificationService,
		trigger::{TriggerError, TriggerExecutionServiceTrait},
	},
};

mock! {
	pub TriggerExecutionService<T: TriggerRepositoryTrait + Send + Sync + 'static> {
		pub fn new(trigger_service: TriggerService<T>, notification_service: NotificationService) -> Self;
	}

	#[async_trait]
	impl<T: TriggerRepositoryTrait + Send + Sync + 'static> TriggerExecutionServiceTrait for TriggerExecutionService<T> {
		async fn execute(
			&self,
			trigger_slugs: &[String],
			variables: HashMap<String, String>,
		) -> Result<(), TriggerError>;
	}
}

mock! {
	pub FilterService {
		pub fn new() -> Self;

		pub async fn filter_block<T: BlockFilterFactory<T> + Send + Sync + 'static>(
			&self,
			client: &T,
			network: &Network,
			block: &BlockType,
			monitors: &[Monitor],
		) -> Result<Vec<MonitorMatch>, FilterError>;
	}
}

mock! {
	pub BlockStorage {}
	#[async_trait]
	impl BlockStorage for BlockStorage {
		async fn save_missed_block(&self, network_slug: &str, block_number: u64) -> Result<(), BlockWatcherError>;
		async fn save_last_processed_block(&self, network_slug: &str, block_number: u64) -> Result<(), BlockWatcherError>;
		async fn get_last_processed_block(&self, network_slug: &str) -> Result<Option<u64>, BlockWatcherError>;
		async fn save_blocks(&self, network_slug: &str, blocks: &[BlockType]) -> Result<(), BlockWatcherError>;
		async fn delete_blocks(&self, network_slug: &str) -> Result<(), BlockWatcherError>;
	}

	impl Clone for BlockStorage {
		fn clone(&self) -> Self {
			self.clone()
		}
	}
}

mock! {
	pub BlockTracker<S: BlockStorage + 'static> {}

	#[async_trait]
	impl<S: BlockStorage + 'static> BlockTrackerTrait<S> for BlockTracker<S> {
		 fn new(history_size: usize, storage: Option<std::sync::Arc<S> >) -> Self;
		 async fn record_block(&self, network: &Network, block_number: u64);
		 async fn get_last_block(&self, network_slug: &str) -> Option<u64>;
	}
}
