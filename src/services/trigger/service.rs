use std::collections::HashMap;

use super::error::TriggerError;
use crate::repositories::{TriggerRepositoryTrait, TriggerService};
use crate::services::notification::NotificationService;

pub struct TriggerExecutionService<T: TriggerRepositoryTrait> {
    trigger_service: TriggerService<T>,
    notification_service: NotificationService,
}

impl<T: TriggerRepositoryTrait> TriggerExecutionService<T> {
    pub fn new(
        trigger_service: TriggerService<T>,
        notification_service: NotificationService,
    ) -> Self {
        Self {
            trigger_service,
            notification_service,
        }
    }

    pub async fn execute(
        &self,
        trigger_slugs: &[&str],
        variables: HashMap<String, String>,
    ) -> Result<(), TriggerError> {
        for trigger_slug in trigger_slugs {
            let trigger = self
                .trigger_service
                .get(&trigger_slug.to_string())
                .ok_or_else(|| TriggerError::not_found(&trigger_slug.to_string()))?;

            self.notification_service
                .execute(&trigger.config, variables.clone())
                .await
                .map_err(|e| TriggerError::execution_error(e.to_string()))?;
        }
        Ok(())
    }
}
