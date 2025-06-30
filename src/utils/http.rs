use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{
	policies::ExponentialBackoff, Jitter, RetryTransientMiddleware, RetryableStrategy,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// --- Default values for retry configuration settings ---
fn default_max_attempts() -> u32 {
	3
}

fn default_initial_backoff() -> Duration {
	Duration::from_millis(250)
}

fn default_max_backoff() -> Duration {
	Duration::from_secs(10)
}

fn default_base_for_backoff() -> u32 {
	2
}

/// Serializable setting for jitter in retry policies
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum JitterSetting {
	/// No jitter applied to the backoff duration
	None,
	/// Full jitter applied, randomizing the backoff duration
	#[default]
	Full,
}

/// Configuration for HTTP (RPC and Webhook notifiers) and SMTP (Email notifier) retry policies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RetryConfig {
	/// Maximum number of retries for transient errors
	#[serde(default = "default_max_attempts")]
	pub max_retries: u32,
	/// Base duration for exponential backoff calculations
	#[serde(default = "default_base_for_backoff")]
	pub base_for_backoff: u32,
	/// Initial backoff duration before the first retry
	#[serde(default = "default_initial_backoff")]
	pub initial_backoff: Duration,
	/// Maximum backoff duration for retries
	#[serde(default = "default_max_backoff")]
	pub max_backoff: Duration,
	/// Jitter to apply to the backoff duration
	#[serde(default)]
	pub jitter: JitterSetting,
}

impl Default for RetryConfig {
	/// Creates a default configuration with reasonable retry settings
	fn default() -> Self {
		Self {
			max_retries: default_max_attempts(),
			base_for_backoff: default_base_for_backoff(),
			initial_backoff: default_initial_backoff(),
			max_backoff: default_max_backoff(),
			jitter: JitterSetting::default(),
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
	config: &RetryConfig,
	base_client: reqwest::Client,
	custom_strategy: Option<S>,
) -> ClientWithMiddleware
where
	S: RetryableStrategy + Send + Sync + 'static,
{
	// Determine the jitter setting and create the policy builder accordingly
	let policy_builder = match config.jitter {
		JitterSetting::None => ExponentialBackoff::builder().jitter(Jitter::None),
		JitterSetting::Full => ExponentialBackoff::builder().jitter(Jitter::Full),
	};

	// Create the retry policy based on the provided configuration
	let retry_policy = policy_builder
		.base(config.base_for_backoff)
		.retry_bounds(config.initial_backoff, config.max_backoff)
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
