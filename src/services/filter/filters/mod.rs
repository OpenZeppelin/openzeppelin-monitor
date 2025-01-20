//! Block filtering implementations.
//!
//! Provides trait definition and implementations for filtering blocks
//! across different blockchain types. Includes:
//! - Generic BlockFilter trait
//! - EVM-specific implementation
//! - Stellar-specific implementation

mod evm;
mod stellar;

use async_trait::async_trait;
pub use evm::EVMBlockFilter;
pub use stellar::StellarBlockFilter;

use crate::{
	models::{BlockType, Monitor, MonitorMatch, Network},
	services::{blockchain::BlockFilterFactory, filter::error::FilterError},
};

// TODO: Remove this once we have a better way to handle async functions in traits
#[async_trait]
pub trait BlockFilter {
	type Client;
	async fn filter_block(
		&self,
		client: &Self::Client,
		network: &Network,
		block: &BlockType,
		monitors: &[Monitor],
	) -> Result<Vec<MonitorMatch>, FilterError>;
}

pub struct FilterService {}

impl FilterService {
	pub fn new() -> Self {
		FilterService {}
	}
}

impl Default for FilterService {
	fn default() -> Self {
		Self::new()
	}
}

impl FilterService {
	pub async fn filter_block<T: BlockFilterFactory<T>>(
		&self,
		client: &T,
		network: &Network,
		block: &BlockType,
		monitors: &[Monitor],
	) -> Result<Vec<MonitorMatch>, FilterError> {
		let filter = T::filter();
		filter.filter_block(client, network, block, monitors).await
	}
}
