//! Blockchain client implementations.
//!
//! Contains specific implementations for different blockchain types:
//! - EVM client for Ethereum-compatible chains
//! - Stellar client for Stellar network
mod evm;
mod stellar;

pub use evm::{EvmClient, EvmClientTrait};
pub use stellar::{StellarClient, StellarClientTrait};
