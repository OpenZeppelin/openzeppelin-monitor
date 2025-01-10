//! Blockchain client interfaces and implementations.
//!
//! Provides abstractions and concrete implementations for interacting with
//! different blockchain networks. Includes:
//! - Generic blockchain client trait
//! - EVM and Stellar specific clients
//! - Network transport implementations
//! - Client factory for creating appropriate implementations

mod client;
mod clients;
mod error;
mod factory;
mod transports;

pub use client::BlockChainClient;
pub use clients::{
    BlockChainClientEnum, EvmClient, EvmClientTrait, StellarClient, StellarClientTrait,
};
pub use error::BlockChainError;
pub use factory::create_blockchain_client;
pub use transports::{HorizonTransportClient, Web3TransportClient};
