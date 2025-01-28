//! Trigger configuration repository implementation.
//!
//! This module provides storage and retrieval of trigger configurations, which define
//! actions to take when monitor conditions are met. The repository loads trigger
//! configurations from JSON files.

use std::{collections::HashMap, path::Path};

use crate::{
	models::{ConfigLoader, Trigger},
	repositories::error::RepositoryError,
};

/// Repository for storing and retrieving trigger configurations
#[derive(Clone)]
pub struct TriggerRepository {
	/// Map of trigger names to their configurations
	pub triggers: HashMap<String, Trigger>,
}

impl TriggerRepository {
	/// Create a new trigger repository from the given path
	///
	/// Loads all trigger configurations from JSON files in the specified directory
	/// (or default config directory if None is provided).
	pub fn new(path: Option<&Path>) -> Result<Self, RepositoryError> {
		let triggers = Self::load_all(path)?;
		Ok(TriggerRepository { triggers })
	}
}

/// Interface for trigger repository implementations
///
/// This trait defines the standard operations that any trigger repository must support,
/// allowing for different storage backends while maintaining a consistent interface.
pub trait TriggerRepositoryTrait: Clone {
	/// Create a new trigger repository from the given path
	fn new(path: Option<&Path>) -> Result<Self, RepositoryError>
	where
		Self: Sized;

	/// Load all trigger configurations from the given path
	///
	/// If no path is provided, uses the default config directory.
	/// This is a static method that doesn't require an instance.
	fn load_all(path: Option<&Path>) -> Result<HashMap<String, Trigger>, RepositoryError>;

	/// Get a specific trigger by ID
	///
	/// Returns None if the trigger doesn't exist.
	fn get(&self, trigger_id: &str) -> Option<Trigger>;

	/// Get all triggers
	///
	/// Returns a copy of the trigger map to prevent external mutation.
	fn get_all(&self) -> HashMap<String, Trigger>;
}

impl TriggerRepositoryTrait for TriggerRepository {
	fn new(path: Option<&Path>) -> Result<Self, RepositoryError> {
		let triggers = Self::load_all(path)?;
		Ok(TriggerRepository { triggers })
	}

	fn load_all(path: Option<&Path>) -> Result<HashMap<String, Trigger>, RepositoryError> {
		Trigger::load_all(path).map_err(|e| RepositoryError::load_error(format!("Failed: {}", e)))
	}

	fn get(&self, trigger_id: &str) -> Option<Trigger> {
		self.triggers.get(trigger_id).cloned()
	}

	fn get_all(&self) -> HashMap<String, Trigger> {
		self.triggers.clone()
	}
}

/// Service layer for trigger repository operations
///
/// This type provides a higher-level interface for working with trigger configurations,
/// handling repository initialization and access through a trait-based interface.
#[derive(Clone)]
pub struct TriggerService<T: TriggerRepositoryTrait> {
	repository: T,
}

impl<T: TriggerRepositoryTrait> TriggerService<T> {
	/// Create a new trigger service with the default repository implementation
	pub fn new(path: Option<&Path>) -> Result<TriggerService<TriggerRepository>, RepositoryError> {
		let repository = TriggerRepository::new(path)?;
		Ok(TriggerService { repository })
	}

	/// Create a new trigger service with a custom repository implementation
	pub fn new_with_repository(repository: T) -> Result<Self, RepositoryError> {
		Ok(TriggerService { repository })
	}

	/// Create a new trigger service with a specific configuration path
	pub fn new_with_path(
		path: Option<&Path>,
	) -> Result<TriggerService<TriggerRepository>, RepositoryError> {
		let repository = TriggerRepository::new(path)?;
		Ok(TriggerService { repository })
	}

	/// Get a specific trigger by ID
	pub fn get(&self, trigger_id: &str) -> Option<Trigger> {
		self.repository.get(trigger_id)
	}

	/// Get all triggers
	pub fn get_all(&self) -> HashMap<String, Trigger> {
		self.repository.get_all()
	}
}
