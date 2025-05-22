//! WebSocket configuration for blockchain transports
//!
//! This module provides a configuration for WebSocket transports, including heartbeat intervals,
//! reconnect timeouts, and message timeouts.

use crate::models::Network;
use std::time::Duration;

/// WebSocket configuration for blockchain transports
#[derive(Clone, Debug)]
pub struct WsConfig {
	/// Heartbeat interval for WebSocket connections
	/// How often to send keep-alive pings
	pub heartbeat_interval: Duration,
	/// Reconnect timeout for WebSocket connections
	/// How long to wait before reconnecting
	pub reconnect_timeout: Duration,
	/// Maximum number of reconnect attempts
	/// How many times to try reconnecting
	pub max_reconnect_attempts: u32,
	/// Connection timeout for WebSocket connections
	/// How long to wait for initial connection
	pub connection_timeout: Duration,
	/// Message timeout for WebSocket connections
	/// How long to wait for message responses
	pub message_timeout: Duration,
}

impl Default for WsConfig {
	fn default() -> Self {
		Self {
			heartbeat_interval: Duration::from_secs(30),
			reconnect_timeout: Duration::from_secs(5),
			max_reconnect_attempts: 3,
			connection_timeout: Duration::from_secs(10),
			message_timeout: Duration::from_secs(5),
		}
	}
}

impl WsConfig {
	/// Creates a new WebSocket configuration with default values
	///
	/// # Returns
	/// * `WsConfig` - A new WebSocket configuration with default values
	pub fn new() -> Self {
		Self::default()
	}

	/// Creates a new WebSocket configuration with a single attempt
	///
	/// Mostly for testing purposes
	///
	/// # Returns
	/// * `WsConfig` - A new WebSocket configuration with a single attempt
	pub fn single_attempt() -> Self {
		Self {
			heartbeat_interval: Duration::from_secs(30),
			reconnect_timeout: Duration::from_secs(1),
			max_reconnect_attempts: 1,
			connection_timeout: Duration::from_secs(1),
			message_timeout: Duration::from_secs(1),
		}
	}
	/// Creates a new WebSocket configuration from a network
	///
	/// # Arguments
	/// * `network` - The network to create the configuration from
	///
	/// # Returns
	/// * `WsConfig` - A new WebSocket configuration
	pub fn from_network(_network: &Network) -> Self {
		Self::default()
	}

	/// Sets the heartbeat interval for the WebSocket configuration
	///
	/// # Arguments
	/// * `heartbeat_interval` - The heartbeat interval to set
	///
	/// # Returns
	/// * `WsConfig` - A new WebSocket configuration with the updated heartbeat interval
	pub fn with_heartbeat_interval(mut self, heartbeat_interval: Duration) -> Self {
		self.heartbeat_interval = heartbeat_interval;
		self
	}

	/// Sets the reconnect timeout for the WebSocket configuration
	///
	/// # Arguments
	/// * `reconnect_timeout` - The reconnect timeout to set
	///
	/// # Returns
	/// * `WsConfig` - A new WebSocket configuration with the updated reconnect timeout
	pub fn with_reconnect_timeout(mut self, reconnect_timeout: Duration) -> Self {
		self.reconnect_timeout = reconnect_timeout;
		self
	}

	/// Sets the maximum number of reconnect attempts for the WebSocket configuration
	///
	/// # Arguments
	/// * `max_reconnect_attempts` - The maximum number of reconnect attempts to set
	///
	/// # Returns
	/// * `WsConfig` - A new WebSocket configuration with the updated maximum number of reconnect attempts
	pub fn with_max_reconnect_attempts(mut self, max_reconnect_attempts: u32) -> Self {
		self.max_reconnect_attempts = max_reconnect_attempts;
		self
	}

	/// Sets the connection timeout for the WebSocket configuration
	///
	/// # Arguments
	/// * `connection_timeout` - The connection timeout to set
	///
	/// # Returns
	/// * `WsConfig` - A new WebSocket configuration with the updated connection timeout
	pub fn with_connection_timeout(mut self, connection_timeout: Duration) -> Self {
		self.connection_timeout = connection_timeout;
		self
	}

	/// Sets the message timeout for the WebSocket configuration
	///
	/// # Arguments
	/// * `message_timeout` - The message timeout to set
	///
	/// # Returns
	/// * `WsConfig` - A new WebSocket configuration with the updated message timeout
	pub fn with_message_timeout(mut self, message_timeout: Duration) -> Self {
		self.message_timeout = message_timeout;
		self
	}

	/// Builds the WebSocket configuration
	///
	/// # Returns
	/// * `WsConfig` - A new WebSocket configuration
	pub fn build(self) -> Self {
		Self {
			heartbeat_interval: self.heartbeat_interval,
			reconnect_timeout: self.reconnect_timeout,
			max_reconnect_attempts: self.max_reconnect_attempts,
			connection_timeout: self.connection_timeout,
			message_timeout: self.message_timeout,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_config() {
		let config = WsConfig::default();
		assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
		assert_eq!(config.reconnect_timeout, Duration::from_secs(5));
		assert_eq!(config.max_reconnect_attempts, 3);
		assert_eq!(config.connection_timeout, Duration::from_secs(10));
		assert_eq!(config.message_timeout, Duration::from_secs(5));
	}

	#[test]
	fn test_single_attempt_config() {
		let config = WsConfig::single_attempt();
		assert_eq!(config.heartbeat_interval, Duration::from_secs(30));
		assert_eq!(config.reconnect_timeout, Duration::from_secs(1));
		assert_eq!(config.max_reconnect_attempts, 1);
		assert_eq!(config.connection_timeout, Duration::from_secs(1));
		assert_eq!(config.message_timeout, Duration::from_secs(1));
	}

	#[test]
	fn test_builder_methods() {
		let config = WsConfig::new()
			.with_heartbeat_interval(Duration::from_secs(60))
			.with_reconnect_timeout(Duration::from_secs(10))
			.with_max_reconnect_attempts(5)
			.with_connection_timeout(Duration::from_secs(20))
			.with_message_timeout(Duration::from_secs(15))
			.build();

		assert_eq!(config.heartbeat_interval, Duration::from_secs(60));
		assert_eq!(config.reconnect_timeout, Duration::from_secs(10));
		assert_eq!(config.max_reconnect_attempts, 5);
		assert_eq!(config.connection_timeout, Duration::from_secs(20));
		assert_eq!(config.message_timeout, Duration::from_secs(15));
	}
}
