//! Blockchain client interfaces and implementations.
//!
//! Provides abstractions and concrete implementations for interacting with
//! different blockchain networks. Includes:
//!
//! - Generic blockchain client trait
//! - EVM and Stellar specific clients
//! - Network transport implementations
//! - Client factory for creating appropriate implementations
//! - Client pool for managing multiple clients

mod client;
mod clients;
mod error;
mod pool;
mod transports;

pub use client::{BlockChainClient, BlockFilterFactory};
pub use clients::{EvmClient, EvmClientTrait, StellarClient, StellarClientTrait};
pub use error::BlockChainError;
pub use pool::{ClientPool, ClientPoolTrait};
pub use transports::{
	BlockchainTransport, EndpointManager, HorizonTransportClient, RotatingTransport,
	StellarTransportClient, Web3TransportClient,
};
