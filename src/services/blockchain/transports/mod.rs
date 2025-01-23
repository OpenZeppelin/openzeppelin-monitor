//! Network transport implementations for blockchain clients.
//!
//! Provides concrete implementations for different blockchain network protocols:
//! - Web3 transport for EVM chains
//! - Horizon and Stellar RPC transport for Stellar

mod evm {
	pub mod web3;
}
mod stellar {
	pub mod horizon;
	pub mod soroban;
}

pub use evm::web3::Web3TransportClient;
pub use stellar::{horizon::HorizonTransportClient, soroban::StellarTransportClient};
