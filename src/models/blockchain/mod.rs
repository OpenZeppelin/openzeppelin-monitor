//! Blockchain-specific model implementations.
//!
//! This module contains type definitions and implementations for different
//! blockchain platforms (EVM, Stellar, etc). Each submodule implements the
//! platform-specific logic for blocks, transactions, and event monitoring.

use serde::{Deserialize, Serialize};

pub mod evm;
pub mod stellar;

/// Supported blockchain platform types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlockChainType {
	/// Ethereum Virtual Machine based chains
	EVM,
	/// Stellar blockchain
	Stellar,
	/// Midnight blockchain (not yet implemented)
	Midnight,
	/// Solana blockchain (not yet implemented)
	Solana,
}

/// Block data from different blockchain platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockType {
	/// EVM block and transaction data
	EVM(Box<evm::EVMBlock>),
	/// Stellar ledger and transaction data
	Stellar(Box<stellar::StellarBlock>),
}

/// Monitor match results from different blockchain platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitorMatch {
	/// Matched conditions from EVM chains
	EVM(Box<evm::EVMMonitorMatch>),
	/// Matched conditions from Stellar chains
	Stellar(Box<stellar::StellarMonitorMatch>),
}
