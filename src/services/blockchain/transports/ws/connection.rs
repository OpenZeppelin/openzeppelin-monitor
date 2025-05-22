//! WebSocket connection state management
//!
//! This module provides functionality for managing WebSocket connection state,
//! including connection health tracking and activity monitoring.

use std::time::Instant;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// Represents the state of a WebSocket connection
///
/// This struct maintains the WebSocket stream, connection health status,
/// and tracks the last activity timestamp. It provides methods to check
/// connection status and update activity timestamps.
///
/// # Fields
/// * `stream` - The WebSocket stream, if connected
/// * `is_healthy` - Whether the connection is considered healthy
/// * `last_activity` - Timestamp of the last activity on the connection
#[derive(Debug)]
pub struct WebSocketConnection {
	pub stream: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
	pub is_healthy: bool,
	last_activity: Instant,
}

impl Default for WebSocketConnection {
	/// Creates a new WebSocket connection in a disconnected state
	///
	/// The connection starts with:
	/// - No active stream
	/// - Unhealthy status
	/// - Current timestamp as last activity
	fn default() -> Self {
		Self {
			stream: None,
			is_healthy: false,
			last_activity: Instant::now(),
		}
	}
}

impl WebSocketConnection {
	/// Checks if the connection is both established and healthy
	///
	/// A connection is considered connected when:
	/// - It has an active WebSocket stream
	/// - The connection is marked as healthy
	///
	/// # Returns
	/// * `bool` - True if the connection is established and healthy
	pub fn is_connected(&self) -> bool {
		self.stream.is_some() && self.is_healthy
	}

	/// Updates the last activity timestamp to the current time
	///
	/// This method should be called whenever there is activity on the connection
	/// to maintain an accurate record of the last interaction.
	pub fn update_activity(&mut self) {
		self.last_activity = Instant::now();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	#[test]
	fn test_default_connection() {
		let conn = WebSocketConnection::default();
		assert!(!conn.is_connected());
		assert!(!conn.is_healthy);
		assert!(conn.stream.is_none());
	}

	#[test]
	fn test_update_activity() {
		let mut conn = WebSocketConnection::default();
		let initial_activity = conn.last_activity;

		// Wait a bit to ensure the time difference is noticeable
		std::thread::sleep(Duration::from_millis(10));

		conn.update_activity();
		assert!(conn.last_activity > initial_activity);
	}

	#[test]
	fn test_is_connected() {
		let mut conn = WebSocketConnection::default();
		assert!(!conn.is_connected());

		// Test with stream but not healthy
		conn.is_healthy = false;
		assert!(!conn.is_connected());

		// Test with healthy but no stream
		conn.is_healthy = true;
		assert!(!conn.is_connected());
	}
}
