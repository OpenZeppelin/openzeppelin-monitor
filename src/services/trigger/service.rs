//! Trigger execution service implementation.
//!
//! Provides functionality to execute triggers with variable substitution
//! and notification delivery. Manages trigger lookup and execution flow.

use std::{collections::HashMap, path::Path};

use async_trait::async_trait;

use crate::{
	models::{Monitor, ScriptLanguage},
	repositories::{TriggerRepositoryTrait, TriggerService},
	services::{notification::NotificationService, trigger::error::TriggerError},
};

/// Trait for executing triggers
///
/// This trait must be implemented by all trigger execution services to provide
/// a way to execute triggers.
#[async_trait]
pub trait TriggerExecutionServiceTrait {
	async fn execute(
		&self,
		trigger_slugs: &[String],
		variables: HashMap<String, String>,
	) -> Result<(), TriggerError>;
	async fn load_scripts(
		&self,
		monitors: &[Monitor],
	) -> Result<HashMap<String, (ScriptLanguage, String)>, TriggerError>;
}

/// Service for executing triggers with notifications
///
/// Coordinates trigger lookup, variable substitution, and notification
/// delivery across different notification channels
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
}

#[async_trait]
impl<T: TriggerRepositoryTrait + Send + Sync> TriggerExecutionServiceTrait
	for TriggerExecutionService<T>
{
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
	/// - Returns `TriggerError::ConfigurationError` if trigger configuration is invalid
	async fn execute(
		&self,
		trigger_slugs: &[String],
		variables: HashMap<String, String>,
	) -> Result<(), TriggerError> {
		for trigger_slug in trigger_slugs {
			let trigger = self
				.trigger_service
				.get(trigger_slug)
				.ok_or_else(|| TriggerError::not_found(trigger_slug.to_string()))?;

			self.notification_service
				.execute(&trigger, variables.clone())
				.await
				.map_err(|e| TriggerError::execution_error(e.to_string()))?;
		}
		Ok(())
	}
	/// Loads trigger condition scripts for monitors
	///
	/// # Arguments
	/// * `monitors` - List of monitors containing trigger conditions
	///
	/// # Returns
	/// * `Result<HashMap<String, (ScriptLanguage, String)>, TriggerError>` - Map of monitor names
	///   and script path to their script language and content
	///
	/// # Errors
	/// - Returns `TriggerError::ConfigurationError` if script files cannot be read
	async fn load_scripts(
		&self,
		monitors: &[Monitor],
	) -> Result<HashMap<String, (ScriptLanguage, String)>, TriggerError> {
		let mut scripts = HashMap::new();

		for monitor in monitors {
			// Skip monitors without trigger conditions
			if monitor.trigger_conditions.is_empty() {
				continue;
			}

			// For each monitor, we'll load all its trigger condition scripts
			for condition in &monitor.trigger_conditions {
				let script_path = Path::new(&condition.script_path);

				// Read the script content
				let content = tokio::fs::read_to_string(script_path).await.map_err(|e| {
					TriggerError::configuration_error(format!(
						"Failed to read script file {}: {}",
						condition.script_path, e
					))
				})?;
				// Store the script content with its language
				scripts.insert(
					format!("{}|{}", monitor.name, condition.script_path),
					(condition.language.clone(), content),
				);
			}
		}

		Ok(scripts)
	}
}
