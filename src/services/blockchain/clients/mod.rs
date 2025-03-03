//! Blockchain client implementations.
//!
//! Contains specific implementations for different blockchain types:
//! - EVM client for Ethereum-compatible chains
//! - Stellar client for Stellar network

mod evm {
	pub mod client;
}
mod stellar {
	pub mod client;
}

pub use evm::client::{EvmClient, EvmClientTrait};
pub use stellar::client::{StellarClient, StellarClientTrait};
