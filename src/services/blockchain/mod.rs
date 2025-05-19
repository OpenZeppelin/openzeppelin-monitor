//! Blockchain client interfaces and implementations.
//!
//! Provides abstractions and concrete implementations for interacting with
//! different blockchain networks. Includes:
//!
//! - Generic blockchain client trait
//! - Chain specific clients
//! - Network transport implementations
//! - Error handling for blockchain operations
//! - Client pool for managing multiple clients

mod client;
mod clients;
mod error;
mod pool;
mod transports;

pub use client::{BlockChainClient, BlockFilterFactory};
pub use clients::{
	EvmClient, EvmClientTrait, MidnightClient, MidnightClientTrait, StellarClient,
	StellarClientTrait,
};
pub use error::BlockChainError;
pub use pool::{ClientPool, ClientPoolTrait};
pub use transports::{
	BlockchainTransport, EVMTransportClient, EndpointManager, HttpTransportClient,
	MidnightTransportClient, RotatingTransport, StellarTransportClient,
	TransientErrorRetryStrategy, WsTransportClient,
};
