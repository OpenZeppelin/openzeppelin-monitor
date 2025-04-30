//! Monitor configuration repository implementation.
//!
//! This module provides storage and retrieval of monitor configurations, including
//! validation of references to networks and triggers. The repository loads monitor
//! configurations from JSON files and ensures all referenced components exist.

#![allow(clippy::result_large_err)]

use std::{collections::HashMap, marker::PhantomData, path::Path};

use async_trait::async_trait;

use crate::{
	models::{ConfigLoader, Monitor, Network, ScriptLanguage, Trigger},
	repositories::{
		error::RepositoryError,
		network::{NetworkRepository, NetworkRepositoryTrait, NetworkService},
		trigger::{TriggerRepository, TriggerRepositoryTrait, TriggerService},
	},
};

/// Static mapping of script languages to their file extensions
const LANGUAGE_EXTENSIONS: &[(&ScriptLanguage, &str)] = &[
	(&ScriptLanguage::Python, "py"),
	(&ScriptLanguage::JavaScript, "js"),
	(&ScriptLanguage::Bash, "sh"),
];

/// Repository for storing and retrieving monitor configurations
#[derive(Clone)]
pub struct MonitorRepository<
	N: NetworkRepositoryTrait + Send + 'static,
	T: TriggerRepositoryTrait + Send + 'static,
> {
	/// Map of monitor names to their configurations
	pub monitors: HashMap<String, Monitor>,
	_network_repository: PhantomData<N>,
	_trigger_repository: PhantomData<T>,
}

impl<
		N: NetworkRepositoryTrait + Send + Sync + 'static,
		T: TriggerRepositoryTrait + Send + Sync + 'static,
	> MonitorRepository<N, T>
{
	/// Create a new monitor repository from the given path
	///
	/// Loads all monitor configurations from JSON files in the specified directory
	/// (or default config directory if None is provided).
	pub async fn new(
		path: Option<&Path>,
		network_service: Option<NetworkService<N>>,
		trigger_service: Option<TriggerService<T>>,
	) -> Result<Self, RepositoryError> {
		let monitors = Self::load_all(path, network_service, trigger_service).await?;
		Ok(MonitorRepository {
			monitors,
			_network_repository: PhantomData,
			_trigger_repository: PhantomData,
		})
	}

	/// Create a new monitor repository from a list of monitors
	pub fn new_with_monitors(monitors: HashMap<String, Monitor>) -> Self {
		MonitorRepository {
			monitors,
			_network_repository: PhantomData,
			_trigger_repository: PhantomData,
		}
	}

	/// Returns an error if any monitor references a non-existent network or trigger.
	pub fn validate_monitor_references(
		monitors: &HashMap<String, Monitor>,
		triggers: &HashMap<String, Trigger>,
		networks: &HashMap<String, Network>,
	) -> Result<(), RepositoryError> {
		let mut validation_errors = Vec::new();
		let mut metadata = HashMap::new();

		for (monitor_name, monitor) in monitors {
			// Validate trigger references
			for trigger_id in &monitor.triggers {
				if !triggers.contains_key(trigger_id) {
					validation_errors.push(format!(
						"Monitor '{}' references non-existent trigger '{}'",
						monitor_name, trigger_id
					));
					metadata.insert(
						format!("monitor_{}_invalid_trigger", monitor_name),
						trigger_id.clone(),
					);
				}
			}

			// Validate network references
			for network_slug in &monitor.networks {
				if !networks.contains_key(network_slug) {
					validation_errors.push(format!(
						"Monitor '{}' references non-existent network '{}'",
						monitor_name, network_slug
					));
					metadata.insert(
						format!("monitor_{}_invalid_network", monitor_name),
						network_slug.clone(),
					);
				}
			}

			// Validate custom trigger conditions
			for condition in &monitor.trigger_conditions {
				let script_path = Path::new(&condition.script_path);
				if !script_path.exists() {
					validation_errors.push(format!(
						"Monitor '{}' has a custom filter script that does not exist: {}",
						monitor_name, condition.script_path
					));
				}

				// Validate file extension matches the specified language
				let expected_extension = match LANGUAGE_EXTENSIONS
					.iter()
					.find(|(lang, _)| *lang == &condition.language)
					.map(|(_, ext)| *ext)
				{
					Some(ext) => ext,
					None => {
						validation_errors.push(format!(
							"Monitor '{}' uses unsupported script language {:?}",
							monitor_name, condition.language
						));
						continue;
					}
				};

				match script_path.extension().and_then(|ext| ext.to_str()) {
					Some(ext) if ext == expected_extension => (), // Valid extension
					_ => validation_errors.push(format!(
						"Monitor '{}' has a custom filter script with invalid extension - must be \
						 .{} for {:?} language: {}",
						monitor_name, expected_extension, condition.language, condition.script_path
					)),
				}

				if condition.timeout_ms == 0 {
					validation_errors.push(format!(
						"Monitor '{}' should have a custom filter timeout_ms greater than 0",
						monitor_name
					));
				}
			}
		}

		if !validation_errors.is_empty() {
			return Err(RepositoryError::validation_error(
				format!(
					"Configuration validation failed:\n{}",
					validation_errors.join("\n"),
				),
				None,
				Some(metadata),
			));
		}

		Ok(())
	}
}

/// Interface for monitor repository implementations
///
/// This trait defines the standard operations that any monitor repository must support,
/// allowing for different storage backends while maintaining a consistent interface.
#[async_trait]
pub trait MonitorRepositoryTrait<
	N: NetworkRepositoryTrait + Send + 'static,
	T: TriggerRepositoryTrait + Send + 'static,
>: Clone + Send
{
	/// Create a new monitor repository from the given path
	async fn new(
		path: Option<&Path>,
		network_service: Option<NetworkService<N>>,
		trigger_service: Option<TriggerService<T>>,
	) -> Result<Self, RepositoryError>
	where
		Self: Sized;

	/// Load all monitor configurations from the given path
	///
	/// If no path is provided, uses the default config directory.
	/// Also validates references to networks and triggers.
	/// This is a static method that doesn't require an instance.
	async fn load_all(
		path: Option<&Path>,
		network_service: Option<NetworkService<N>>,
		trigger_service: Option<TriggerService<T>>,
	) -> Result<HashMap<String, Monitor>, RepositoryError>;

	/// Load a monitor from a specific path
	///
	/// Loads a monitor configuration from a specific path and validates all network and trigger references.
	async fn load_from_path(
		&self,
		path: Option<&Path>,
		network_service: Option<NetworkService<N>>,
		trigger_service: Option<TriggerService<T>>,
	) -> Result<Monitor, RepositoryError>;

	/// Get a specific monitor by ID
	///
	/// Returns None if the monitor doesn't exist.
	fn get(&self, monitor_id: &str) -> Option<Monitor>;

	/// Get all monitors
	///
	/// Returns a copy of the monitor map to prevent external mutation.
	fn get_all(&self) -> HashMap<String, Monitor>;
}

#[async_trait]
impl<
		N: NetworkRepositoryTrait + Send + Sync + 'static,
		T: TriggerRepositoryTrait + Send + Sync + 'static,
	> MonitorRepositoryTrait<N, T> for MonitorRepository<N, T>
{
	async fn new(
		path: Option<&Path>,
		network_service: Option<NetworkService<N>>,
		trigger_service: Option<TriggerService<T>>,
	) -> Result<Self, RepositoryError> {
		MonitorRepository::new(path, network_service, trigger_service).await
	}

	async fn load_all(
		path: Option<&Path>,
		network_service: Option<NetworkService<N>>,
		trigger_service: Option<TriggerService<T>>,
	) -> Result<HashMap<String, Monitor>, RepositoryError> {
		let monitors = Monitor::load_all(path).await.map_err(|e| {
			RepositoryError::load_error(
				"Failed to load monitors",
				Some(Box::new(e)),
				Some(HashMap::from([(
					"path".to_string(),
					path.map_or_else(|| "default".to_string(), |p| p.display().to_string()),
				)])),
			)
		})?;

		let networks = match network_service {
			Some(service) => service.get_all(),
			None => {
				NetworkRepository::new(None)
					.await
					.map_err(|e| {
						RepositoryError::load_error(
							"Failed to load networks for monitor validation",
							Some(Box::new(e)),
							None,
						)
					})?
					.networks
			}
		};

		let triggers = match trigger_service {
			Some(service) => service.get_all(),
			None => {
				TriggerRepository::new(None)
					.await
					.map_err(|e| {
						RepositoryError::load_error(
							"Failed to load triggers for monitor validation",
							Some(Box::new(e)),
							None,
						)
					})?
					.triggers
			}
		};

		Self::validate_monitor_references(&monitors, &triggers, &networks)?;
		Ok(monitors)
	}

	/// Load a monitor from a specific path
	///
	/// Loads a monitor configuration from a specific path and validates all network and trigger references.
	async fn load_from_path(
		&self,
		path: Option<&Path>,
		network_service: Option<NetworkService<N>>,
		trigger_service: Option<TriggerService<T>>,
	) -> Result<Monitor, RepositoryError> {
		match path {
			Some(path) => {
				let monitor = Monitor::load_from_path(path).await.map_err(|e| {
					RepositoryError::load_error(
						"Failed to load monitors",
						Some(Box::new(e)),
						Some(HashMap::from([(
							"path".to_string(),
							path.display().to_string(),
						)])),
					)
				})?;

				let networks = match network_service {
					Some(service) => service.get_all(),
					None => NetworkRepository::new(None).await?.networks,
				};

				let triggers = match trigger_service {
					Some(service) => service.get_all(),
					None => TriggerRepository::new(None).await?.triggers,
				};
				let monitors = HashMap::from([(monitor.name.clone(), monitor)]);
				Self::validate_monitor_references(&monitors, &triggers, &networks)?;
				match monitors.values().next() {
					Some(monitor) => Ok(monitor.clone()),
					None => Err(RepositoryError::load_error("No monitors found", None, None)),
				}
			}
			None => Err(RepositoryError::load_error(
				"Failed to load monitors",
				None,
				None,
			)),
		}
	}

	fn get(&self, monitor_id: &str) -> Option<Monitor> {
		self.monitors.get(monitor_id).cloned()
	}

	fn get_all(&self) -> HashMap<String, Monitor> {
		self.monitors.clone()
	}
}

/// Service layer for monitor repository operations
///
/// This type provides a higher-level interface for working with monitor configurations,
/// handling repository initialization and access through a trait-based interface.
/// It also ensures that all monitor references to networks and triggers are valid.
#[derive(Clone)]
pub struct MonitorService<
	M: MonitorRepositoryTrait<N, T> + Send,
	N: NetworkRepositoryTrait + Send + Sync + 'static,
	T: TriggerRepositoryTrait + Send + Sync + 'static,
> {
	repository: M,
	_network_repository: PhantomData<N>,
	_trigger_repository: PhantomData<T>,
}

impl<
		M: MonitorRepositoryTrait<N, T> + Send,
		N: NetworkRepositoryTrait + Send + Sync + 'static,
		T: TriggerRepositoryTrait + Send + Sync + 'static,
	> MonitorService<M, N, T>
{
	/// Create a new monitor service with the default repository implementation
	///
	/// Loads monitor configurations from the specified path (or default config directory)
	/// and validates all network and trigger references.
	pub async fn new(
		path: Option<&Path>,
		network_service: Option<NetworkService<N>>,
		trigger_service: Option<TriggerService<T>>,
	) -> Result<MonitorService<M, N, T>, RepositoryError> {
		let repository = M::new(path, network_service, trigger_service).await?;
		Ok(MonitorService {
			repository,
			_network_repository: PhantomData,
			_trigger_repository: PhantomData,
		})
	}

	/// Create a new monitor service with a specific configuration path
	///
	/// Similar to `new()` but makes the path parameter more explicit.
	pub async fn new_with_path(
		path: Option<&Path>,
	) -> Result<MonitorService<M, N, T>, RepositoryError> {
		let repository = M::new(path, None, None).await?;
		Ok(MonitorService {
			repository,
			_network_repository: PhantomData,
			_trigger_repository: PhantomData,
		})
	}

	/// Create a new monitor service with a custom repository implementation
	///
	/// Allows for using alternative storage backends that implement the MonitorRepositoryTrait.
	pub fn new_with_repository(repository: M) -> Result<Self, RepositoryError> {
		Ok(MonitorService {
			repository,
			_network_repository: PhantomData,
			_trigger_repository: PhantomData,
		})
	}

	/// Get a specific monitor by ID
	///
	/// Returns None if the monitor doesn't exist.
	pub fn get(&self, monitor_id: &str) -> Option<Monitor> {
		self.repository.get(monitor_id)
	}

	/// Get all monitors
	///
	/// Returns a copy of the monitor map to prevent external mutation.
	pub fn get_all(&self) -> HashMap<String, Monitor> {
		self.repository.get_all()
	}

	/// Load a monitor from a specific path
	///
	/// Loads a monitor configuration from a specific path and validates all network and trigger references.
	pub async fn load_from_path(
		&self,
		path: Option<&Path>,
		network_service: Option<NetworkService<N>>,
		trigger_service: Option<TriggerService<T>>,
	) -> Result<Monitor, RepositoryError> {
		self.repository
			.load_from_path(path, network_service, trigger_service)
			.await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{models::ScriptLanguage, utils::tests::builders::evm::monitor::MonitorBuilder};
	use std::fs;
	use tempfile::TempDir;

	#[test]
	fn test_validate_custom_trigger_conditions() {
		let temp_dir = TempDir::new().unwrap();
		let script_path = temp_dir.path().join("test_script.py");
		fs::write(&script_path, "print('test')").unwrap();

		let mut monitors = HashMap::new();
		let triggers = HashMap::new();
		let networks = HashMap::new();

		// Test valid configuration
		let monitor = MonitorBuilder::new()
			.name("test_monitor")
			.networks(vec![])
			.trigger_condition(
				script_path.to_str().unwrap(),
				1000,
				ScriptLanguage::Python,
				None,
			)
			.build();
		monitors.insert("test_monitor".to_string(), monitor);

		let result =
			MonitorRepository::<NetworkRepository, TriggerRepository>::validate_monitor_references(
				&monitors, &triggers, &networks,
			);
		assert!(result.is_ok());

		// Test non-existent script
		let monitor_bad_path = MonitorBuilder::new()
			.name("test_monitor_bad_path")
			.trigger_condition("non_existent_script.py", 1000, ScriptLanguage::Python, None)
			.build();
		monitors.insert("test_monitor_bad_path".to_string(), monitor_bad_path);

		let err =
			MonitorRepository::<NetworkRepository, TriggerRepository>::validate_monitor_references(
				&monitors, &triggers, &networks,
			)
			.unwrap_err();
		assert!(err.to_string().contains("does not exist"));

		// Test wrong extension
		let wrong_ext_path = temp_dir.path().join("test_script.js");
		fs::write(&wrong_ext_path, "print('test')").unwrap();

		let monitor_wrong_ext = MonitorBuilder::new()
			.name("test_monitor_wrong_ext")
			.trigger_condition(
				wrong_ext_path.to_str().unwrap(),
				1000,
				ScriptLanguage::Python,
				None,
			)
			.build();
		monitors.clear();
		monitors.insert("test_monitor_wrong_ext".to_string(), monitor_wrong_ext);

		let err =
			MonitorRepository::<NetworkRepository, TriggerRepository>::validate_monitor_references(
				&monitors, &triggers, &networks,
			)
			.unwrap_err();
		assert!(err.to_string().contains(
			"Monitor 'test_monitor_wrong_ext' has a custom filter script with invalid extension - \
			 must be .py for Python language"
		));

		// Test zero timeout
		let monitor_zero_timeout = MonitorBuilder::new()
			.name("test_monitor_zero_timeout")
			.trigger_condition(
				script_path.to_str().unwrap(),
				0,
				ScriptLanguage::Python,
				None,
			)
			.build();
		monitors.clear();
		monitors.insert(
			"test_monitor_zero_timeout".to_string(),
			monitor_zero_timeout,
		);

		let err =
			MonitorRepository::<NetworkRepository, TriggerRepository>::validate_monitor_references(
				&monitors, &triggers, &networks,
			)
			.unwrap_err();
		assert!(err.to_string().contains("timeout_ms greater than 0"));
	}

	#[tokio::test]
	async fn test_load_error_messages() {
		// Test with invalid path to trigger load error
		let invalid_path = Path::new("/non/existent/path");
		let result = MonitorRepository::<NetworkRepository, TriggerRepository>::load_all(
			Some(invalid_path),
			None,
			None,
		)
		.await;

		assert!(result.is_err());
		let err = result.unwrap_err();
		match err {
			RepositoryError::LoadError(message) => {
				assert!(message.to_string().contains("Failed to load monitors"));
			}
			_ => panic!("Expected RepositoryError::LoadError"),
		}
	}

	#[test]
	fn test_network_validation_error() {
		// Create a monitor with a reference to a non-existent network
		let mut monitors = HashMap::new();
		let monitor = MonitorBuilder::new()
			.name("test_monitor")
			.networks(vec!["non_existent_network".to_string()])
			.build();
		monitors.insert("test_monitor".to_string(), monitor);

		// Empty networks and triggers
		let networks = HashMap::new();
		let triggers = HashMap::new();

		// Validate should fail due to non-existent network reference
		let result =
			MonitorRepository::<NetworkRepository, TriggerRepository>::validate_monitor_references(
				&monitors, &triggers, &networks,
			);

		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.to_string().contains("references non-existent network"));
	}

	#[test]
	fn test_trigger_validation_error() {
		// Create a monitor with a reference to a non-existent trigger
		let mut monitors = HashMap::new();
		let monitor = MonitorBuilder::new()
			.name("test_monitor")
			.triggers(vec!["non_existent_trigger".to_string()])
			.build();
		monitors.insert("test_monitor".to_string(), monitor);

		// Empty networks and triggers
		let networks = HashMap::new();
		let triggers = HashMap::new();

		// Validate should fail due to non-existent trigger reference
		let result =
			MonitorRepository::<NetworkRepository, TriggerRepository>::validate_monitor_references(
				&monitors, &triggers, &networks,
			);

		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.to_string().contains("references non-existent trigger"));
	}

	#[tokio::test]
	async fn test_load_from_path_error_handling() {
		// Create a temporary directory for testing
		let temp_dir = TempDir::new().unwrap();
		let invalid_path = temp_dir.path().join("non_existent_monitor.json");

		// Create a repository instance
		let repository =
			MonitorRepository::<NetworkRepository, TriggerRepository>::new_with_monitors(
				HashMap::new(),
			);

		// Attempt to load from non-existent path
		let result = repository
			.load_from_path(Some(&invalid_path), None, None)
			.await;

		// Verify error handling
		assert!(result.is_err());
		let err = result.unwrap_err();
		match err {
			RepositoryError::LoadError(message) => {
				assert!(message.to_string().contains("Failed to load monitors"));
				// Verify the error contains the path in its metadata
				assert!(message
					.to_string()
					.contains(&invalid_path.display().to_string()));
			}
			_ => panic!("Expected RepositoryError::LoadError"),
		}
	}
}
