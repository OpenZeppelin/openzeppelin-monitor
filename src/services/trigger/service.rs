//! Trigger execution service implementation.
//!
//! Provides functionality to execute triggers with variable substitution
//! and notification delivery. Manages trigger lookup and execution flow.

use std::collections::HashMap;

use crate::repositories::{TriggerRepositoryTrait, TriggerService};
use crate::services::notification::NotificationService;
use crate::services::trigger::error::TriggerError;

/// Service for executing triggers with notifications
///
/// Coordinates trigger lookup, variable substitution, and notification
/// delivery across different notification channels.
pub struct TriggerExecutionService<T: TriggerRepositoryTrait> {
    /// Service for trigger management and lookup
    trigger_service: TriggerService<T>,
    /// Service for sending notifications
    notification_service: NotificationService,
}

impl<T: TriggerRepositoryTrait> TriggerExecutionService<T> {
    /// Creates a new trigger execution service
    ///
    /// # Arguments
    /// * `trigger_service` - Service for trigger operations
    /// * `notification_service` - Service for notification delivery
    ///
    /// # Returns
    /// * `Self` - New trigger execution service instance
    pub fn new(
        trigger_service: TriggerService<T>,
        notification_service: NotificationService,
    ) -> Self {
        Self {
            trigger_service,
            notification_service,
        }
    }

    /// Executes multiple triggers with variable substitution
    ///
    /// # Arguments
    /// * `trigger_slugs` - List of trigger identifiers to execute
    /// * `variables` - Variables to substitute in trigger templates
    ///
    /// # Returns
    /// * `Result<(), TriggerError>` - Success or error
    ///
    /// # Errors
    /// - Returns `TriggerError::NotFound` if a trigger cannot be found
    /// - Returns `TriggerError::ExecutionError` if notification delivery fails
    pub async fn execute(
        &self,
        trigger_slugs: &[&str],
        variables: HashMap<String, String>,
    ) -> Result<(), TriggerError> {
        for trigger_slug in trigger_slugs {
            let trigger = self
                .trigger_service
                .get(trigger_slug)
                .ok_or_else(|| TriggerError::not_found(trigger_slug.to_string()))?;

            self.notification_service
                .execute(&trigger.config, variables.clone())
                .await
                .map_err(|e| TriggerError::execution_error(e.to_string()))?;
        }
        Ok(())
    }
}
