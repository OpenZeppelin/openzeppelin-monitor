//! Blockchain client implementations.
//!
//! Contains specific implementations for different blockchain types:
//! - EVM client for Ethereum-compatible chains
//! - Stellar client for Stellar network
//! - Midnight client for Midnight network

mod evm {
	pub mod client;
}
mod stellar {
	pub mod client;
}
mod midnight {
	pub mod client;
}

pub use evm::client::{EvmClient, EvmClientTrait};
pub use midnight::client::{
	MidnightClient, MidnightClientTrait, SubstrateClientTrait as MidnightSubstrateClientTrait,
};
pub use stellar::client::{StellarClient, StellarClientTrait};
