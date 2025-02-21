//! Retry mechanism for handling transient failures in async operations.
//!
//! This module provides a configurable retry mechanism with exponential backoff
//! for handling temporary failures in async operations. It allows specifying
//! the maximum number of retries, initial delay, and maximum delay between attempts.

use std::time::Duration;

/// Configuration for retry behavior
#[derive(Clone, Debug)]
pub struct RetryConfig {
	/// Maximum number of retry attempts before giving up
	pub max_retries: u32,

	/// Initial delay between retry attempts
	/// This delay will be exponentially increased with each retry
	pub initial_delay: Duration,

	/// Maximum delay between retry attempts
	/// The exponential backoff will not exceed this delay
	pub max_delay: Duration,
}

impl Default for RetryConfig {
	/// Creates a default retry configuration with:
	/// - 2 maximum retries
	/// - 1 second initial delay
	/// - 4 seconds maximum delay
	fn default() -> Self {
		Self {
			max_retries: 2,
			initial_delay: Duration::from_secs(1),
			max_delay: Duration::from_secs(4),
		}
	}
}

/// Handler for retrying operations with exponential backoff
pub struct WithRetry {
	/// Configuration for retry behavior
	config: RetryConfig,
}

impl WithRetry {
	/// Creates a new retry handler with custom configuration
	pub fn new(config: RetryConfig) -> Self {
		Self { config }
	}

	/// Creates a new retry handler with default configuration
	pub fn with_default_config() -> Self {
		Self {
			config: RetryConfig::default(),
		}
	}

	/// Attempts an async operation with configured retry behavior
	///
	/// This method will retry the operation up to the configured maximum number
	/// of times, with exponential backoff between attempts. The delay between
	/// attempts doubles each time but will not exceed the configured maximum delay.
	///
	/// # Arguments
	/// * `operation` - An async operation that returns a Result
	///
	/// # Type Parameters
	/// * `F` - Function type that creates the Future
	/// * `Fut` - Future type returned by the operation
	/// * `T` - Success type of the operation
	/// * `E` - Error type of the operation
	///
	/// # Returns
	/// * `Ok(T)` - If the operation succeeds
	/// * `Err(E)` - If all retry attempts fail
	pub async fn attempt<F, Fut, T, E>(&self, operation: F) -> Result<T, E>
	where
		F: Fn() -> Fut + Send + Sync,
		Fut: std::future::Future<Output = Result<T, E>> + Send,
		T: Send,
		E: std::fmt::Debug + Send,
	{
		let mut attempt = 0;
		loop {
			match operation().await {
				Ok(value) => return Ok(value),
				Err(e) => {
					attempt += 1;
					if attempt >= self.config.max_retries {
						return Err(e);
					}

					let delay =
						(self.config.initial_delay.as_millis() * (1 << (attempt - 1))) as u64;
					let delay =
						Duration::from_millis(delay.min(self.config.max_delay.as_millis() as u64));
					tokio::time::sleep(delay).await;
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::{
		atomic::{AtomicU32, Ordering},
		Arc,
	};

	#[test]
	fn test_default_config() {
		let config = RetryConfig::default();
		assert_eq!(config.max_retries, 2);
		assert_eq!(config.initial_delay, Duration::from_secs(1));
		assert_eq!(config.max_delay, Duration::from_secs(4));
	}

	#[tokio::test]
	async fn test_successful_first_attempt() {
		let retry = WithRetry::with_default_config();
		let result = retry.attempt(|| async { Ok::<_, &str>("success") }).await;
		assert_eq!(result.unwrap(), "success");
	}

	#[tokio::test]
	async fn test_successful_after_retry() {
		let attempts = Arc::new(AtomicU32::new(0));
		let retry = WithRetry::with_default_config();

		let attempts_clone = attempts.clone();
		let result = retry
			.attempt(|| {
				let attempts = attempts_clone.clone();
				async move {
					let current = attempts.fetch_add(1, Ordering::SeqCst);
					if current == 0 {
						Err("first attempt fails")
					} else {
						Ok("success")
					}
				}
			})
			.await;

		assert_eq!(result.unwrap(), "success");
		assert_eq!(attempts.load(Ordering::SeqCst), 2);
	}

	#[tokio::test]
	async fn test_exhausts_retries() {
		let attempts = Arc::new(AtomicU32::new(0));
		let retry = WithRetry::new(RetryConfig {
			max_retries: 3,
			initial_delay: Duration::from_millis(10),
			max_delay: Duration::from_millis(100),
		});

		let attempts_clone = attempts.clone();
		let result = retry
			.attempt(|| {
				let attempts = attempts_clone.clone();
				async move {
					attempts.fetch_add(1, Ordering::SeqCst);
					Err::<&str, _>("always fails")
				}
			})
			.await;

		assert!(result.is_err());
		assert_eq!(attempts.load(Ordering::SeqCst), 3);
	}

	#[tokio::test]
	async fn test_custom_config() {
		let retry = WithRetry::new(RetryConfig {
			max_retries: 1,
			initial_delay: Duration::from_millis(10),
			max_delay: Duration::from_millis(100),
		});

		let attempts = Arc::new(AtomicU32::new(0));
		let attempts_clone = attempts.clone();

		let result = retry
			.attempt(|| {
				let attempts = attempts_clone.clone();
				async move {
					attempts.fetch_add(1, Ordering::SeqCst);
					Err::<&str, _>("fails")
				}
			})
			.await;

		assert!(result.is_err());
		assert_eq!(attempts.load(Ordering::SeqCst), 1);
	}
}
