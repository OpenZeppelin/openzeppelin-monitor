use async_trait::async_trait;
use mockall::mock;
use std::collections::HashMap;

use openzeppelin_monitor::{
	models::{BlockType, Monitor, MonitorMatch, Network},
	repositories::{TriggerRepositoryTrait, TriggerService},
	services::{
		blockchain::BlockFilterFactory,
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
