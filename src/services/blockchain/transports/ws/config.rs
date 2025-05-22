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
}
