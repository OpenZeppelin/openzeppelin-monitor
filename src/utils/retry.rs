use std::time::Duration;

#[derive(Clone, Debug)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(8),
        }
    }
}

pub struct WithRetry {
    config: RetryConfig,
}

impl WithRetry {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self {
            config: RetryConfig::default(),
        }
    }

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
