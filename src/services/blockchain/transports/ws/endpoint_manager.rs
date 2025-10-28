//! Manages the rotation of blockchain WebSocket RPC endpoints
//!
//! This module provides functionality for managing WebSocket connections to blockchain nodes,
//! including:
//! - Automatic failover between multiple RPC endpoints
//! - Weight-based URL selection
//! - Connection health monitoring
//! - Thread-safe URL rotation

use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;

use crate::services::blockchain::transports::{ws::config::WsConfig, RotatingTransport};

/// Manages WebSocket RPC endpoint rotation and failover
///
/// This struct provides thread-safe management of WebSocket connections to blockchain nodes,
/// handling automatic failover between multiple endpoints based on their weights and health.
///
/// # Fields
/// * `active_url` - The currently active WebSocket endpoint URL
/// * `fallback_urls` - List of fallback URLs to use when the active URL fails
/// * `rotation_lock` - Mutex to ensure thread-safe URL rotation
/// * `config` - Configuration settings for WebSocket connections
#[derive(Clone, Debug)]
pub struct EndpointManager {
	/// The currently active WebSocket endpoint URL
	pub active_url: Arc<RwLock<String>>,
	/// List of fallback URLs to use when the active URL fails
	pub fallback_urls: Arc<RwLock<Vec<String>>>,
	/// Mutex to ensure thread-safe URL rotation
	rotation_lock: Arc<Mutex<()>>,
	/// Configuration settings for WebSocket connections
	config: WsConfig,
}

impl EndpointManager {
	/// Creates a new WebSocket endpoint manager
	///
	/// Initializes the endpoint manager with a primary URL and a list of fallback URLs.
	/// The URLs should be pre-sorted by weight, with the highest weight URL as the active one.
	///
	/// # Arguments
	/// * `config` - WebSocket configuration settings
	/// * `active_url` - The initial active WebSocket URL
	/// * `fallback_urls` - List of fallback URLs, pre-sorted by weight
	///
	/// # Returns
	/// A new `EndpointManager` instance
	pub fn new(config: &WsConfig, active_url: &str, fallback_urls: Vec<String>) -> Self {
		Self {
			active_url: Arc::new(RwLock::new(active_url.to_string())),
			fallback_urls: Arc::new(RwLock::new(fallback_urls)),
			rotation_lock: Arc::new(Mutex::new(())),
			config: config.clone(),
		}
	}

	/// Rotates to the next available WebSocket URL
	///
	/// Attempts to connect to a different URL from the fallback list. If successful,
	/// updates the active URL and moves the old active URL to the fallback list.
	///
	/// # Arguments
	/// * `transport` - The transport client implementing the `RotatingTransport` trait
	///
	/// # Returns
	/// * `Ok(())` if rotation was successful
	/// * `Err` if no fallback URLs are available or connection fails
	pub async fn rotate_url<T: RotatingTransport>(
		&self,
		transport: &T,
	) -> Result<(), anyhow::Error> {
		let _guard = self.rotation_lock.lock().await;
		let current_active = self.active_url.read().await.clone();
		let mut attempts = 0;

		while attempts < self.config.max_reconnect_attempts {
			let new_url = {
				let mut fallback_urls = self.fallback_urls.write().await;
				if fallback_urls.is_empty() {
					return Err(anyhow::anyhow!("No fallback URLs available"));
				}

				// Find first URL that's different from current
				let idx = fallback_urls.iter().position(|url| url != &current_active);

				match idx {
					Some(pos) => fallback_urls.remove(pos),
					None => {
						return Err(anyhow::anyhow!("No fallback URLs available"));
					}
				}
			};

			// Use connection timeout from config
			match timeout(
				self.config.connection_timeout,
				transport.try_connect(&new_url),
			)
			.await
			{
				Ok(Ok(_)) => {
					transport.update_client(&new_url).await?;
					{
						let mut active_url = self.active_url.write().await;
						let mut fallback_urls = self.fallback_urls.write().await;
						tracing::debug!(
							"Successful rotation - from: {}, to: {}",
							current_active,
							new_url
						);
						fallback_urls.push(current_active);
						*active_url = new_url;
					}
					return Ok(());
				}
				Ok(Err(e)) => {
					let mut fallback_urls = self.fallback_urls.write().await;
					fallback_urls.push(new_url);
					tracing::warn!("Failed to connect to fallback URL: {}", e);
				}
				Err(_) => {
					let mut fallback_urls = self.fallback_urls.write().await;
					fallback_urls.push(new_url);
					tracing::warn!("Connection timeout during rotation");
				}
			}

			attempts += 1;
			if attempts < self.config.max_reconnect_attempts {
				tokio::time::sleep(self.config.reconnect_timeout).await;
			}
		}

		Err(anyhow::anyhow!(
			"Failed to reconnect after {} attempts",
			self.config.max_reconnect_attempts
		))
	}

	/// Retrieves the currently active WebSocket URL
	///
	/// # Returns
	/// * `Ok(String)` containing the active URL
	/// * `Err` if no active URL is set
	pub async fn get_active_url(&self) -> Result<String, anyhow::Error> {
		let url = self.active_url.read().await;
		if url.is_empty() {
			Err(anyhow::anyhow!("No active URL set"))
		} else {
			Ok(url.clone())
		}
	}

	/// Checks if URL rotation should be attempted
	///
	/// Determines if there are any fallback URLs available for rotation.
	///
	/// # Returns
	/// `true` if rotation should be attempted, `false` otherwise
	pub async fn should_rotate(&self) -> bool {
		let fallback_urls = self.fallback_urls.read().await;
		!fallback_urls.is_empty()
	}
}
