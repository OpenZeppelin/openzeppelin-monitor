use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{
	policies::ExponentialBackoff, Jitter, RetryTransientMiddleware, RetryableStrategy,
};
use std::time::Duration;

/// Configuration for HTTP retry policies
#[derive(Debug, Clone)]
pub struct HttpRetryConfig {
	/// Maximum number of retries for transient errors
	pub max_retries: u32,
	/// Base duration for exponential backoff calculations
	pub base_for_backoff: u32,
	/// Initial backoff duration before the first retry
	pub initial_backoff: Duration,
	/// Maximum backoff duration for retries
	pub max_backoff: Duration,
	/// Jitter to apply to the backoff duration
	pub jitter: Jitter,
}

impl Default for HttpRetryConfig {
	/// Creates a default configuration with reasonable retry settings
	fn default() -> Self {
		Self {
			max_retries: 3,
			base_for_backoff: 2,
			initial_backoff: Duration::from_millis(250),
			max_backoff: Duration::from_secs(10),
			jitter: Jitter::Full,
		}
	}
}

/// Creates a retryable HTTP client with middleware for a single URL
///
/// # Parameters:
/// - `config`: Configuration for retry policies
/// - `base_client`: The base HTTP client to use
/// - `custom_strategy`: Optional custom retry strategy, complementing the default retry behavior
///
/// # Returns
/// A `ClientWithMiddleware` that includes retry capabilities
///
pub fn create_retryable_http_client<S>(
	config: &HttpRetryConfig,
	base_client: reqwest::Client,
	custom_strategy: Option<S>,
) -> ClientWithMiddleware
where
	S: RetryableStrategy + Send + Sync + 'static,
{
	// Create the retry policy based on the provided configuration
	let retry_policy = ExponentialBackoff::builder()
		.base(config.base_for_backoff)
		.retry_bounds(config.initial_backoff, config.max_backoff)
		.jitter(config.jitter)
		.build_with_max_retries(config.max_retries);

	// If a custom strategy is provided, use it with the retry policy; otherwise, use the retry policy with the default strategy.
	if let Some(strategy) = custom_strategy {
		ClientBuilder::new(base_client).with(
			RetryTransientMiddleware::new_with_policy_and_strategy(retry_policy, strategy),
		)
	} else {
		ClientBuilder::new(base_client)
			.with(RetryTransientMiddleware::new_with_policy(retry_policy))
	}
	.build()
}
