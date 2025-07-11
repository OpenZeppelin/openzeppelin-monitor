use reqwest::Client;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::DefaultRetryableStrategy;
use std::sync::Arc;

use crate::{
	services::notification::NotificationClientPool,
	utils::{create_retryable_http_client, RetryConfig},
};

/// Creates a default HTTP client with retry capabilities for testing purposes.
pub fn create_test_http_client() -> Arc<ClientWithMiddleware> {
	let retryable_client = create_retryable_http_client::<DefaultRetryableStrategy>(
		&RetryConfig::default(),
		Client::new(),
		None,
	);

	Arc::new(retryable_client)
}

/// Creates a test HTTP client from the notification client pool.
/// Currently used for integration tests
pub async fn get_http_client_from_notification_pool() -> Arc<ClientWithMiddleware> {
	let pool = NotificationClientPool::new();
	let retry_policy = RetryConfig::default();
	let http_client = pool.get_or_create_http_client(&retry_policy).await;
	http_client.unwrap()
}
