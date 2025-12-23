//! Blockchain client implementations.
//!
//! Contains specific implementations for different blockchain types:
//! - EVM client for Ethereum-compatible chains
//! - Stellar client for Stellar network
//! - Midnight client for Midnight network
//! - Solana client for Solana network

mod evm {
	pub mod client;
}
mod stellar {
	pub mod client;
	pub mod error;
}
mod midnight {
	pub mod client;
}
mod solana {
	pub mod client;
	pub mod error;
}

pub use evm::client::{EvmClient, EvmClientTrait};
pub use midnight::client::{
	MidnightClient, MidnightClientTrait, SubstrateClientTrait as MidnightSubstrateClientTrait,
};
pub use solana::client::{SignatureInfo, SolanaClient, SolanaClientTrait};
pub use solana::error::SolanaClientError;
pub use stellar::client::{StellarClient, StellarClientTrait};
pub use stellar::error::StellarClientError;
