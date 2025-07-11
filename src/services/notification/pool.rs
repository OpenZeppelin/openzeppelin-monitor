use crate::services::blockchain::TransientErrorRetryStrategy;
use crate::services::notification::SmtpConfig;
use crate::utils::client_storage::ClientStorage;
use crate::utils::{create_retryable_http_client, RetryConfig};
use lettre::Tokio1Executor;
use lettre::{transport::smtp::authentication::Credentials, AsyncSmtpTransport};
use reqwest::Client as ReqwestClient;
use reqwest_middleware::ClientWithMiddleware;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NotificationPoolError {
	#[error("Failed to create HTTP client: {0}")]
	HttpClientBuildError(String),

	#[error("Failed to create SMTP client: {0}")]
	SmtpClientBuildError(String),
}

/// Notification client pool that manages HTTP and SMTP clients for sending notifications.
///
/// Provides a thread-safe way to access and create HTTP and SMTP clients
/// for sending notifications. It uses a `ClientStorage` to hold the clients,
/// allowing for efficient reuse and management of HTTP and SMTP connections.
pub struct NotificationClientPool {
	http_clients: ClientStorage<ClientWithMiddleware>,
	smtp_clients: ClientStorage<AsyncSmtpTransport<Tokio1Executor>>,
}

impl NotificationClientPool {
	pub fn new() -> Self {
		Self {
			http_clients: ClientStorage::new(),
			smtp_clients: ClientStorage::new(),
		}
	}

	/// A private, generic method to handle the core logic of getting or creating a client.
	async fn get_or_create_client<T, F>(
		&self,
		key: &str,
		storage: &ClientStorage<T>,
		create_fn: F,
	) -> Result<Arc<T>, NotificationPoolError>
	where
		T: Send + Sync,
		F: FnOnce() -> Result<T, NotificationPoolError>,
	{
		// 1. Fast path (read lock)
		if let Some(client) = storage.clients.read().await.get(key) {
			return Ok(client.clone());
		}

		// 2. Slow path (write lock)
		let mut clients = storage.clients.write().await;
		// 3. Double-check
		if let Some(client) = clients.get(key) {
			return Ok(client.clone());
		}

		// 4. Create and insert
		let new_client = create_fn()?;
		let arc_client = Arc::new(new_client);
		clients.insert(key.to_string(), arc_client.clone());

		Ok(arc_client)
	}

	/// Get or create an HTTP client with retry capabilities.
	///
	/// # Arguments
	/// * `retry_policy` - Configuration for HTTP retry policy
	/// # Returns
	/// * `Result<Arc<ClientWithMiddleware>, NotificationPoolError>` - The HTTP client
	///   wrapped in an `Arc` for shared ownership, or an error if client creation
	///   fails.
	pub async fn get_or_create_http_client(
		&self,
		retry_policy: &RetryConfig,
	) -> Result<Arc<ClientWithMiddleware>, NotificationPoolError> {
		let key = format!("{:?}", retry_policy);
		self.get_or_create_client(&key, &self.http_clients, || {
			let base_client = ReqwestClient::builder()
				.pool_max_idle_per_host(10)
				.pool_idle_timeout(Some(Duration::from_secs(90)))
				.connect_timeout(Duration::from_secs(10))
				.build()
				.map_err(|e| NotificationPoolError::HttpClientBuildError(e.to_string()))?;

			Ok(create_retryable_http_client(
				retry_policy,
				base_client,
				Some(TransientErrorRetryStrategy),
			))
		})
		.await
	}

	/// Get or create an SMTP client for sending emails.
	/// # Arguments
	/// * `smtp_config` - Configuration for the SMTP client, including host,
	///   port, username, and password.
	/// # Returns
	/// * `Result<Arc<AsyncSmtpTransport<Tokio1Executor>>, NotificationPoolError>` - The SMTP client
	///   wrapped in an `Arc` for shared ownership, or an error if client creation
	///   fails.
	pub async fn get_or_create_smtp_client(
		&self,
		smtp_config: &SmtpConfig,
	) -> Result<Arc<AsyncSmtpTransport<Tokio1Executor>>, NotificationPoolError> {
		let key = format!("{:?}", smtp_config);
		self.get_or_create_client(&key, &self.smtp_clients, || {
			let creds =
				Credentials::new(smtp_config.username.clone(), smtp_config.password.clone());
			Ok(
				AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_config.host)
					.map_err(|e| NotificationPoolError::SmtpClientBuildError(e.to_string()))?
					.port(smtp_config.port)
					.credentials(creds)
					.build(),
			)
		})
		.await
	}

	/// Get the number of active HTTP clients in the pool
	#[cfg(test)]
	pub async fn get_active_http_client_count(&self) -> usize {
		self.http_clients.clients.read().await.len()
	}

	/// Get the number of active SMTP clients in the pool
	#[cfg(test)]
	pub async fn get_active_smtp_client_count(&self) -> usize {
		self.smtp_clients.clients.read().await.len()
	}
}

impl Default for NotificationClientPool {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn create_pool() -> NotificationClientPool {
		NotificationClientPool::new()
	}

	#[tokio::test]
	async fn test_pool_init_empty() {
		let pool = create_pool();
		let http_count = pool.get_active_http_client_count().await;
		let smtp_count = pool.get_active_smtp_client_count().await;

		assert_eq!(http_count, 0, "Pool should be empty initially");
		assert_eq!(smtp_count, 0, "Pool should be empty initially");
	}

	#[tokio::test]
	async fn test_pool_get_or_create_http_client() {
		let pool = create_pool();
		let retry_config = RetryConfig::default();
		let client = pool.get_or_create_http_client(&retry_config).await;

		assert!(
			client.is_ok(),
			"Should successfully create or get HTTP client"
		);

		assert_eq!(
			pool.get_active_http_client_count().await,
			1,
			"Pool should have one active HTTP client"
		);
	}

	#[tokio::test]
	async fn test_pool_returns_same_client() {
		let pool = create_pool();
		let retry_config = RetryConfig::default();
		let client1 = pool.get_or_create_http_client(&retry_config).await.unwrap();
		let client2 = pool.get_or_create_http_client(&retry_config).await.unwrap();

		assert!(
			Arc::ptr_eq(&client1, &client2),
			"Should return the same client instance"
		);
		assert_eq!(
			pool.get_active_http_client_count().await,
			1,
			"Pool should still have one active HTTP client"
		);
	}

	#[tokio::test]
	async fn test_pool_concurrent_access() {
		let pool = Arc::new(create_pool());
		let retry_config = RetryConfig::default();

		let num_tasks = 10;
		let mut tasks = Vec::new();

		for _ in 0..num_tasks {
			let pool_clone = Arc::clone(&pool);
			let retry_config = retry_config.clone();
			tasks.push(tokio::spawn(async move {
				let client = pool_clone.get_or_create_http_client(&retry_config).await;
				assert!(
					client.is_ok(),
					"Should successfully create or get HTTP client"
				);
			}));
		}

		let results = futures::future::join_all(tasks).await;

		for result in results {
			assert!(result.is_ok(), "All tasks should complete successfully");
		}
	}

	#[tokio::test]
	async fn test_pool_default() {
		let pool = NotificationClientPool::default();
		let retry_config = RetryConfig::default();

		assert_eq!(
			pool.get_active_http_client_count().await,
			0,
			"Default pool should be empty initially"
		);

		assert_eq!(
			pool.get_active_smtp_client_count().await,
			0,
			"Default pool should be empty initially"
		);

		let client = pool.get_or_create_http_client(&retry_config).await;

		assert!(
			client.is_ok(),
			"Default pool should successfully create or get HTTP client"
		);

		assert_eq!(
			pool.get_active_http_client_count().await,
			1,
			"Default pool should have one active HTTP client"
		);
	}

	#[tokio::test]
	async fn test_pool_returns_different_http_clients_for_different_configs() {
		let pool = create_pool();

		// Config 1 (default)
		let retry_config_1 = RetryConfig::default();

		// Config 2 (different retry count)
		let retry_config_2 = RetryConfig {
			max_retries: 5,
			..Default::default()
		};

		// Get a client for each config
		let client1 = pool
			.get_or_create_http_client(&retry_config_1)
			.await
			.unwrap();
		let client2 = pool
			.get_or_create_http_client(&retry_config_2)
			.await
			.unwrap();

		// Pointers should NOT be equal, as they are different clients
		assert!(
			!Arc::ptr_eq(&client1, &client2),
			"Should return different client instances for different configurations"
		);

		// The pool should now contain two distinct clients
		assert_eq!(
			pool.get_active_http_client_count().await,
			2,
			"Pool should have two active HTTP clients"
		);

		// Getting the first client again should return the original one
		let client1_again = pool
			.get_or_create_http_client(&retry_config_1)
			.await
			.unwrap();
		assert!(
			Arc::ptr_eq(&client1, &client1_again),
			"Should return the same client instance when called again with the same config"
		);

		// Pool size should still be 2
		assert_eq!(
			pool.get_active_http_client_count().await,
			2,
			"Pool should still have two active HTTP clients after getting an existing one"
		);
	}

	#[tokio::test]
	async fn test_pool_returns_different_smtp_clients_for_different_configs() {
		let pool = create_pool();

		// Config 1 (default)
		let smtp_config_1 = SmtpConfig {
			host: "smtp.example.com".to_string(),
			port: 587,
			username: "user1".to_string(),
			password: "pass1".to_string(),
		};

		// Config 2 (different credentials)
		let smtp_config_2 = SmtpConfig {
			host: "smtp.example.com".to_string(),
			port: 587,
			username: "user2".to_string(),
			password: "pass2".to_string(),
		};

		// Get a client for each config
		let client1 = pool
			.get_or_create_smtp_client(&smtp_config_1)
			.await
			.unwrap();
		let client2 = pool
			.get_or_create_smtp_client(&smtp_config_2)
			.await
			.unwrap();

		// Pointers should NOT be equal, as they are different clients
		assert!(
			!Arc::ptr_eq(&client1, &client2),
			"Should return different client instances for different configurations"
		);

		// The pool should now contain two distinct clients
		assert_eq!(
			pool.get_active_smtp_client_count().await,
			2,
			"Pool should have two active SMTP clients"
		);

		// Getting the first client again should return the original one
		let client1_again = pool
			.get_or_create_smtp_client(&smtp_config_1)
			.await
			.unwrap();

		assert!(
			Arc::ptr_eq(&client1, &client1_again),
			"Should return the same client instance when called again with the same config"
		);

		assert_eq!(
			pool.get_active_smtp_client_count().await,
			2,
			"Pool should still have two active SMTP clients after getting an existing one"
		);
	}
}
