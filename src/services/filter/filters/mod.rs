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
	services::{blockchain::BlockChainClientEnum, filter::error::FilterError},
};

#[async_trait]
pub trait BlockFilter {
	async fn filter_block(
		&self,
		client: &BlockChainClientEnum,
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
	pub async fn filter_block(
		&self,
		client: &BlockChainClientEnum,
		network: &Network,
		block: &BlockType,
		monitors: &[Monitor],
	) -> Result<Vec<MonitorMatch>, FilterError> {
		match block {
			BlockType::EVM(_) => {
				let filter = EVMBlockFilter {};
				filter.filter_block(client, network, block, monitors).await
			}
			BlockType::Stellar(_) => {
				let filter = StellarBlockFilter {};
				filter.filter_block(client, network, block, monitors).await
			}
		}
	}
}
