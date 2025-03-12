use async_trait::async_trait;
use mockall::mock;
use std::collections::HashMap;

use openzeppelin_monitor::{
	models::{BlockType, Monitor, MonitorMatch, Network, ScriptLanguage},
	repositories::{TriggerRepositoryTrait, TriggerService},
	services::{
		blockchain::BlockFilterFactory,
		blockwatcher::{BlockStorage, BlockTrackerTrait, JobSchedulerTrait},
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
			monitor_match: &MonitorMatch,
			trigger_scripts: &HashMap<String, (ScriptLanguage, String)>,
		) -> Result<(), TriggerError>;
		async fn load_scripts(&self, monitors: &[Monitor]) -> Result<HashMap<String, (ScriptLanguage, String)>, TriggerError>;
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
		async fn save_missed_block(&self, network_slug: &str, block_number: u64) -> Result<(), anyhow::Error>;
		async fn save_last_processed_block(&self, network_slug: &str, block_number: u64) -> Result<(), anyhow::Error>;
		async fn get_last_processed_block(&self, network_slug: &str) -> Result<Option<u64>, anyhow::Error>;
		async fn save_blocks(&self, network_slug: &str, blocks: &[BlockType]) -> Result<(), anyhow::Error>;
		async fn delete_blocks(&self, network_slug: &str) -> Result<(), anyhow::Error>;
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
		 async fn record_block(&self, network: &Network, block_number: u64) -> Result<(), anyhow::Error>;
		 async fn get_last_block(&self, network_slug: &str) -> Option<u64>;
	}
}

mock! {
	pub JobScheduler {}

	#[async_trait]
	impl JobSchedulerTrait for JobScheduler {
		async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
			Ok(Self::default())
		}

		async fn add(&self, _job: tokio_cron_scheduler::Job) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
			Ok(())
		}

		async fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
			Ok(())
		}

		async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
			Ok(())
		}
	}
}
