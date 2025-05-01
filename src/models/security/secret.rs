//! Secret management module for handling sensitive data securely.
//!
//! This module provides types and utilities for managing secrets in a secure manner,
//! with automatic memory zeroization and support for multiple secret sources.
//!
//! # Features
//!
//! - Secure memory handling with automatic zeroization
//! - Multiple secret sources (plain text, environment variables, Hashicorp Cloud Vault, etc.)
//! - Type-safe secret resolution
//! - Serde support for configuration files

use oz_keystore::HashicorpCloudClient;
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt;
use std::sync::Arc;
use tokio::sync::OnceCell;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::models::security::error::SecurityError;

/// Trait for vault clients that can retrieve secrets
#[async_trait::async_trait]
pub trait VaultClient: Send + Sync {
	async fn get_secret(&self, name: &str) -> Result<SecretString, SecurityError>;
}

/// Cloud Vault client implementation
#[derive(Clone)]
pub struct CloudVaultClient {
	client: Arc<HashicorpCloudClient>,
}

impl CloudVaultClient {
	#[allow(dead_code)] // TODO: Remove this after testing
	/// Creates a new CloudVaultClient from a HashicorpCloudClient
	pub fn new(client: HashicorpCloudClient) -> Self {
		Self {
			client: Arc::new(client),
		}
	}

	/// Creates a new CloudVaultClient from environment variables
	pub fn from_env() -> Result<Self, Box<SecurityError>> {
		let client_id = env::var("HCP_CLIENT_ID").map_err(|e| {
			Box::new(SecurityError::parse_error(
				format!("Missing HCP_CLIENT_ID environment variable: {}", e),
				None,
				None,
			))
		})?;
		let client_secret = env::var("HCP_CLIENT_SECRET").map_err(|e| {
			Box::new(SecurityError::parse_error(
				format!("Missing HCP_CLIENT_SECRET environment variable: {}", e),
				None,
				None,
			))
		})?;
		let org_id = env::var("HCP_ORG_ID").map_err(|e| {
			Box::new(SecurityError::parse_error(
				format!("Missing HCP_ORG_ID environment variable: {}", e),
				None,
				None,
			))
		})?;
		let project_id = env::var("HCP_PROJECT_ID").map_err(|e| {
			Box::new(SecurityError::parse_error(
				format!("Missing HCP_PROJECT_ID environment variable: {}", e),
				None,
				None,
			))
		})?;
		let app_name = env::var("HCP_APP_NAME").map_err(|e| {
			Box::new(SecurityError::parse_error(
				format!("Missing HCP_APP_NAME environment variable: {}", e),
				None,
				None,
			))
		})?;

		let client =
			HashicorpCloudClient::new(client_id, client_secret, org_id, project_id, app_name);
		Ok(Self {
			client: Arc::new(client),
		})
	}
}

#[async_trait::async_trait]
impl VaultClient for CloudVaultClient {
	async fn get_secret(&self, name: &str) -> Result<SecretString, SecurityError> {
		let secret = self.client.get_secret(name).await.map_err(|e| {
			SecurityError::network_error(
				format!("Failed to get secret from Hashicorp Cloud Vault: {}", e),
				None,
				None,
			)
		})?;
		Ok(SecretString::new(secret.secret.static_version.value))
	}
}

/// Enum representing different vault types
#[derive(Clone)]
pub enum VaultType {
	Cloud(CloudVaultClient),
}

impl VaultType {
	/// Creates a new VaultType from environment variables
	pub fn from_env() -> Result<Self, Box<SecurityError>> {
		// Default to cloud vault for now
		Ok(Self::Cloud(CloudVaultClient::from_env()?))
	}
}

#[async_trait::async_trait]
impl VaultClient for VaultType {
	async fn get_secret(&self, name: &str) -> Result<SecretString, SecurityError> {
		match self {
			Self::Cloud(client) => client.get_secret(name).await,
		}
	}
}

// Global vault client instance
static VAULT_CLIENT: OnceCell<VaultType> = OnceCell::const_new();

/// Gets the global vault client instance, initializing it if necessary
pub async fn get_vault_client() -> Result<&'static VaultType, Box<SecurityError>> {
	VAULT_CLIENT
		.get_or_try_init(|| async { VaultType::from_env() })
		.await
}

/// A type that represents a secret value that can be sourced from different places
/// and ensures proper zeroization of sensitive data.
///
/// This enum provides different ways to store and retrieve secrets:
/// - `Plain`: Direct secret value (wrapped in `SecretString` for secure memory handling)
/// - `Environment`: Environment variable reference
/// - `HashicorpCloudVault`: Hashicorp Cloud Vault reference
///
/// All variants implement `ZeroizeOnDrop` to ensure secure memory cleanup.
#[derive(Debug, Clone, Serialize, Deserialize, ZeroizeOnDrop)]
#[serde(tag = "type", content = "value")]
pub enum SecretValue {
	/// A plain text secret value
	Plain(SecretString),
	/// A secret stored in an environment variable
	Environment(String),
	/// A secret stored in Hashicorp Cloud Vault
	HashicorpCloudVault(String),
}

impl PartialEq for SecretValue {
	fn eq(&self, other: &Self) -> bool {
		match (self, other) {
			(Self::Plain(l0), Self::Plain(r0)) => l0.as_str() == r0.as_str(),
			(Self::Environment(l0), Self::Environment(r0)) => l0 == r0,
			(Self::HashicorpCloudVault(l0), Self::HashicorpCloudVault(r0)) => l0 == r0,
			_ => false,
		}
	}
}

/// A string type that automatically zeroizes its contents when dropped.
///
/// This type ensures that sensitive data like passwords and API keys are securely
/// erased from memory as soon as they're no longer needed. It implements both
/// `Zeroize` and `ZeroizeOnDrop` to guarantee secure memory cleanup.
///
/// # Security
///
/// The underlying string is automatically zeroized when:
/// - The value is dropped
/// - `zeroize()` is called explicitly
/// - The value is moved
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct SecretString(String);

impl PartialEq for SecretString {
	fn eq(&self, other: &Self) -> bool {
		self.0 == other.0
	}
}

impl SecretValue {
	/// Resolves the secret value based on its type.
	///
	/// This method retrieves the actual secret value from its source:
	/// - For `Plain`, returns the wrapped `SecretString`
	/// - For `Environment`, reads the environment variable
	/// - For `HashicorpCloudVault`, fetches the secret from the vault
	///
	/// # Errors
	///
	/// Returns a `SecurityError` if:
	/// - Environment variable is not set
	/// - Vault access fails
	/// - Any other security-related error occurs
	pub async fn resolve(&self) -> Result<SecretString, Box<SecurityError>> {
		match self {
			SecretValue::Plain(secret) => Ok(secret.clone()),
			SecretValue::Environment(env_var) => {
				env::var(env_var).map(SecretString::new).map_err(|e| {
					Box::new(SecurityError::parse_error(
						format!("Failed to get environment variable {}: {}", env_var, e),
						None,
						None,
					))
				})
			}
			SecretValue::HashicorpCloudVault(name) => {
				let client = get_vault_client().await?;
				client.get_secret(name).await.map_err(Box::new)
			}
		}
	}

	/// Checks if the secret value starts with a given prefix
	pub fn starts_with(&self, prefix: &str) -> bool {
		match self {
			SecretValue::Plain(secret) => secret.as_str().starts_with(prefix),
			SecretValue::Environment(env_var) => env_var.starts_with(prefix),
			SecretValue::HashicorpCloudVault(name) => name.starts_with(prefix),
		}
	}

	/// Checks if the secret value is empty
	pub fn is_empty(&self) -> bool {
		match self {
			SecretValue::Plain(secret) => secret.as_str().is_empty(),
			SecretValue::Environment(env_var) => env_var.is_empty(),
			SecretValue::HashicorpCloudVault(name) => name.is_empty(),
		}
	}

	/// Trims the secret value
	pub fn trim(&self) -> &str {
		match self {
			SecretValue::Plain(secret) => secret.as_str().trim(),
			SecretValue::Environment(env_var) => env_var.trim(),
			SecretValue::HashicorpCloudVault(name) => name.trim(),
		}
	}

	/// Returns the secret value as a string
	pub fn as_str(&self) -> &str {
		match self {
			SecretValue::Plain(secret) => secret.as_str(),
			SecretValue::Environment(env_var) => env_var,
			SecretValue::HashicorpCloudVault(name) => name,
		}
	}
}

impl Zeroize for SecretValue {
	/// Securely zeroizes the secret value.
	///
	/// This implementation ensures that all sensitive data is properly cleared:
	/// - For `Plain`, zeroizes the underlying `SecretString`
	/// - For `Environment`, clears the environment variable name
	/// - For `HashicorpCloudVault`, clears the secret name
	fn zeroize(&mut self) {
		match self {
			SecretValue::Plain(secret) => secret.zeroize(),
			SecretValue::Environment(env_var) => {
				// Clear the environment variable name
				env_var.clear();
			}
			SecretValue::HashicorpCloudVault(name) => {
				name.clear();
			}
		}
	}
}

impl SecretString {
	/// Creates a new `SecretString` with the given value.
	///
	/// The value will be automatically zeroized when the `SecretString` is dropped.
	pub fn new(value: String) -> Self {
		Self(value)
	}

	/// Gets a reference to the underlying string.
	///
	/// # Security Note
	///
	/// Be careful with this method as it exposes the secret value.
	/// The reference should be used immediately and not stored.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

impl From<String> for SecretString {
	fn from(value: String) -> Self {
		Self::new(value)
	}
}

impl AsRef<str> for SecretString {
	fn as_ref(&self) -> &str {
		self.as_str()
	}
}

impl fmt::Display for SecretValue {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			SecretValue::Plain(secret) => write!(f, "{}", secret.as_str()),
			SecretValue::Environment(env_var) => write!(f, "{}", env_var),
			SecretValue::HashicorpCloudVault(name) => write!(f, "{}", name),
		}
	}
}

impl AsRef<str> for SecretValue {
	fn as_ref(&self) -> &str {
		match self {
			SecretValue::Plain(secret) => secret.as_ref(),
			SecretValue::Environment(env_var) => env_var,
			SecretValue::HashicorpCloudVault(name) => name,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use once_cell::sync::Lazy;
	use std::collections::HashMap;
	use std::sync::atomic::{AtomicBool, Ordering};
	use std::sync::Mutex;
	use zeroize::Zeroize;

	// Mock environment variables for testing
	static ENV_LOCK: Mutex<()> = Mutex::new(());
	static MOCK_VAULT_VARS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
		let mut map = HashMap::new();
		map.insert("HCP_CLIENT_ID", "test-client-id");
		map.insert("HCP_CLIENT_SECRET", "test-client-secret");
		map.insert("HCP_ORG_ID", "test-org");
		map.insert("HCP_PROJECT_ID", "test-project");
		map.insert("HCP_APP_NAME", "test-app");
		map
	});

	// Helper function to set up mock environment
	fn setup_mock_env() {
		for (key, value) in MOCK_VAULT_VARS.iter() {
			std::env::set_var(key, value);
		}
	}

	// Helper function to clean up mock environment
	fn cleanup_mock_env() {
		for key in MOCK_VAULT_VARS.keys() {
			std::env::remove_var(key);
		}
	}

	// Generic wrapper type that tracks zeroization
	struct TrackedSecret<T: Zeroize> {
		inner: T,
		was_zeroized: Arc<AtomicBool>,
	}

	impl<T: Zeroize> TrackedSecret<T> {
		fn new(value: T, was_zeroized: Arc<AtomicBool>) -> Self {
			Self {
				inner: value,
				was_zeroized,
			}
		}
	}

	impl<T: Zeroize> Zeroize for TrackedSecret<T> {
		fn zeroize(&mut self) {
			self.was_zeroized.store(true, Ordering::SeqCst);
			self.inner.zeroize();
		}
	}

	impl<T: Zeroize> Drop for TrackedSecret<T> {
		fn drop(&mut self) {
			self.zeroize();
		}
	}

	/// Tests that SecretString is zeroized when it goes out of scope
	#[test]
	fn test_secret_string_zeroize_on_drop() {
		let was_zeroized = Arc::new(AtomicBool::new(false));
		let secret = "sensitive_data".to_string();
		let secret_string =
			TrackedSecret::new(SecretString::new(secret.clone()), was_zeroized.clone());

		// Verify initial state
		assert_eq!(secret_string.inner.as_str(), secret);
		assert!(!was_zeroized.load(Ordering::SeqCst));

		// Move secret_string into a new scope
		{
			let _secret_string = secret_string;
			// secret_string should still be accessible
			assert_eq!(_secret_string.inner.as_str(), secret);
			assert!(!was_zeroized.load(Ordering::SeqCst));
		}

		// After the scope ends, the value should be zeroized
		assert!(was_zeroized.load(Ordering::SeqCst));
	}

	/// Tests that SecretValue is zeroized when it goes out of scope
	#[test]
	fn test_secret_value_zeroize_on_drop() {
		let was_zeroized = Arc::new(AtomicBool::new(false));
		let secret = "sensitive_data".to_string();
		let secret_value = TrackedSecret::new(
			SecretValue::Plain(SecretString::new(secret.clone())),
			was_zeroized.clone(),
		);

		// Verify initial state
		assert_eq!(secret_value.inner.as_str(), secret);
		assert!(!was_zeroized.load(Ordering::SeqCst));

		// Move secret_value into a new scope
		{
			let _secret_value = secret_value;
			// secret_value should still be accessible
			assert_eq!(_secret_value.inner.as_str(), secret);
			assert!(!was_zeroized.load(Ordering::SeqCst));
		}

		// After the scope ends, the value should be zeroized
		assert!(was_zeroized.load(Ordering::SeqCst));
	}

	/// Tests environment variable secret resolution
	#[tokio::test]
	async fn test_environment_secret() {
		const TEST_ENV_VAR: &str = "TEST_SECRET_ENV_VAR";
		const TEST_SECRET: &str = "test_secret_value";

		env::set_var(TEST_ENV_VAR, TEST_SECRET);

		let secret = SecretValue::Environment(TEST_ENV_VAR.to_string());
		let resolved = secret.resolve().await.unwrap();

		assert_eq!(resolved.as_str(), TEST_SECRET);

		env::remove_var(TEST_ENV_VAR);
	}

	/// Tests manual zeroization of SecretString
	#[test]
	fn test_secret_string_zeroize() {
		let secret = "sensitive_data".to_string();
		let mut secret_string = SecretString::new(secret.clone());

		assert_eq!(secret_string.as_str(), secret);

		// Manually zeroize
		secret_string.zeroize();
		assert_eq!(secret_string.as_str(), "");
	}

	/// Tests zeroization of all SecretValue variants
	#[test]
	fn test_secret_value_zeroize() {
		let mut plain_secret = SecretValue::Plain(SecretString::new("plain_secret".to_string()));
		let mut env_secret = SecretValue::Environment("ENV_VAR".to_string());
		let mut cloud_vault_secret = SecretValue::HashicorpCloudVault("secret_name".to_string());

		plain_secret.zeroize();
		env_secret.zeroize();
		cloud_vault_secret.zeroize();

		// After zeroize, the values should be cleared
		if let SecretValue::Plain(ref secret) = plain_secret {
			assert_eq!(secret.as_str(), "");
		}

		if let SecretValue::Environment(ref env_var) = env_secret {
			assert_eq!(env_var, "");
		}
		if let SecretValue::HashicorpCloudVault(ref name) = cloud_vault_secret {
			assert_eq!(name, "");
		}
	}

	#[test]
	fn test_cloud_vault_client_from_env_success() {
		let _lock = ENV_LOCK.lock().unwrap();
		setup_mock_env();

		let result = CloudVaultClient::from_env();
		assert!(result.is_ok());

		cleanup_mock_env();
	}

	#[test]
	fn test_cloud_vault_client_from_env_missing_vars() {
		let _lock = ENV_LOCK.lock().unwrap();
		setup_mock_env();

		// Test missing HCP_CLIENT_ID
		std::env::remove_var("HCP_CLIENT_ID");
		let result = CloudVaultClient::from_env();
		assert!(result.is_err());
		assert!(result.err().unwrap().to_string().contains("HCP_CLIENT_ID"));

		// Test missing HCP_CLIENT_SECRET
		setup_mock_env();
		std::env::remove_var("HCP_CLIENT_SECRET");
		let result = CloudVaultClient::from_env();
		assert!(result.is_err());
		assert!(result
			.err()
			.unwrap()
			.to_string()
			.contains("HCP_CLIENT_SECRET"));

		cleanup_mock_env();
	}

	#[tokio::test]
	async fn test_vault_type_from_env() {
		let _lock = ENV_LOCK.lock().unwrap();
		setup_mock_env();

		let result = VaultType::from_env();
		assert!(result.is_ok());
		match result.unwrap() {
			VaultType::Cloud(_) => (), // Expected
		}

		cleanup_mock_env();
	}

	#[tokio::test]
	#[allow(clippy::await_holding_lock)]
	async fn test_get_vault_client() {
		let _lock = ENV_LOCK.lock().unwrap();
		setup_mock_env();

		// First call should initialize the client
		let result = get_vault_client().await;
		assert!(result.is_ok());
		let client = result.unwrap();
		match client {
			VaultType::Cloud(_) => (), // Expected
		}

		// Second call should return the same instance
		let result2 = get_vault_client().await;
		assert!(result2.is_ok());
		assert!(std::ptr::eq(client, result2.unwrap()));

		cleanup_mock_env();
	}

	#[tokio::test]
	#[allow(clippy::await_holding_lock)]
	async fn test_vault_client_get_secret() {
		let mut server = mockito::Server::new_async().await;
		// Mock the token request
		let token_mock = server
			.mock("POST", "/oauth2/token")
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(
				r#"{"access_token": "test-token", "token_type": "Bearer", "expires_in": 3600}"#,
			)
			.create_async()
			.await;

		// Mock the secret request
		let secret_mock = server
			.mock(
				"GET",
				"/secrets/2023-11-28/organizations/test-org/projects/test-project/apps/test-app/secrets/test-secret:open",
			)
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(r#"{"secret": {"static_version": {"value": "test-secret-value"}}}"#)
			.create_async()
			.await;

		// Create the HashicorpCloudClient with the custom client
		let hashicorp_client = HashicorpCloudClient::new(
			"test-client-id".to_string(),
			"test-client-secret".to_string(),
			"test-org".to_string(),
			"test-project".to_string(),
			"test-app".to_string(),
		)
		.with_api_base_url(server.url())
		.with_auth_base_url(server.url());

		let vault_client = CloudVaultClient::new(hashicorp_client);

		// Get the secret
		let result = vault_client.get_secret("test-secret").await;

		// Verify the mocks were called
		token_mock.assert_async().await;
		secret_mock.assert_async().await;

		// Verify the result
		assert!(result.is_ok());
		assert_eq!(result.unwrap().as_str(), "test-secret-value");
	}

	#[test]
	fn test_vault_type_clone() {
		let _lock = ENV_LOCK.lock().unwrap();
		setup_mock_env();

		let vault_type = VaultType::from_env().unwrap();
		let cloned = vault_type.clone();

		match (vault_type, cloned) {
			(VaultType::Cloud(_), VaultType::Cloud(_)) => (), // Expected
		}

		cleanup_mock_env();
	}
}
