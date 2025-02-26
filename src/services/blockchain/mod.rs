//! Blockchain client interfaces and implementations.
//!
//! Provides abstractions and concrete implementations for interacting with
//! different blockchain networks. Includes:
//! - Generic blockchain client trait
//! - EVM and Stellar specific clients
//! - Network transport implementations
//! - Error handling for blockchain operations

mod client;
mod clients;
mod error;
mod transports;

pub use client::{BlockChainClient, BlockFilterFactory};
pub use clients::{EvmClient, EvmClientTrait, StellarClient, StellarClientTrait};
pub use error::BlockChainError;
pub use transports::{HorizonTransportClient, Web3TransportClient};
