use async_trait::async_trait;
use mockall::mock;
use std::collections::HashMap;

use openzeppelin_monitor::{
	repositories::{TriggerRepositoryTrait, TriggerService},
	services::{
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
