//! Network transport implementations for blockchain clients.
//!
//! Provides concrete implementations for different blockchain network protocols:
//! - Web3 transport for EVM chains
//! - Horizon and Stellar RPC transport for Stellar

mod horizon;
mod stellar;
mod web3;

pub use horizon::HorizonTransportClient;
pub use stellar::StellarTransportClient;
pub use web3::Web3TransportClient;
