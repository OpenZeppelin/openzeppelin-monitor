//! Blockchain-specific model implementations.
//!
//! This module contains type definitions and implementations for different
//! blockchain platforms (EVM, Stellar, Midnight, Solana, etc). Each submodule implements the
//! platform-specific logic for blocks, transactions, and event monitoring.

use serde::{Deserialize, Serialize};

pub mod evm;
pub mod midnight;
pub mod solana;
pub mod stellar;

/// Supported blockchain platform types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(deny_unknown_fields)]
pub enum BlockChainType {
	/// Ethereum Virtual Machine based chains
	EVM,
	/// Stellar blockchain
	Stellar,
	/// Midnight blockchain
	Midnight,
	/// Solana blockchain
	Solana,
}

/// Block data from different blockchain platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockType {
	/// EVM block and transaction data
	///
	/// # Note
	/// Box is used here to equalize the enum variants
	EVM(Box<evm::EVMBlock>),
	/// Stellar ledger and transaction data
	///
	/// # Note
	/// Box is used here to equalize the enum variants
	Stellar(Box<stellar::StellarBlock>),
	/// Midnight block and transaction data
	///
	/// # Note
	/// Box is used here to equalize the enum variants
	Midnight(Box<midnight::MidnightBlock>),
	/// Solana slot and transaction data
	///
	/// # Note
	/// Box is used here to equalize the enum variants
	Solana(Box<solana::SolanaBlock>),
}

impl BlockType {
	pub fn number(&self) -> Option<u64> {
		match self {
			BlockType::EVM(b) => b.number(),
			BlockType::Stellar(b) => b.number(),
			BlockType::Midnight(b) => b.number(),
			BlockType::Solana(b) => b.number(),
		}
	}
}

/// Transaction data from different blockchain platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum TransactionType {
	/// EVM transaction
	EVM(evm::EVMTransaction),
	/// Stellar transaction
	Stellar(Box<stellar::StellarTransaction>),
	/// Midnight transaction
	Midnight(midnight::MidnightTransaction),
	/// Solana transaction
	Solana(Box<solana::SolanaTransaction>),
}

/// Contract spec from different blockchain platforms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ContractSpec {
	/// EVM contract spec
	EVM(evm::EVMContractSpec),
	/// Stellar contract spec
	Stellar(stellar::StellarContractSpec),
	/// Midnight contract spec
	Midnight,
	/// Solana contract spec (IDL)
	Solana(solana::SolanaContractSpec),
}

/// Monitor match results from different blockchain platforms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitorMatch {
	/// Matched conditions from EVM chains
	///
	/// # Note
	/// Box is used here to equalize the enum variants
	EVM(Box<evm::EVMMonitorMatch>),
	/// Matched conditions from Stellar chains
	///
	/// # Note
	/// Box is used here to equalize the enum variants
	Stellar(Box<stellar::StellarMonitorMatch>),
	/// Matched conditions from Midnight chains
	///
	/// # Note
	/// Box is used here to equalize the enum variants
	Midnight(Box<midnight::MidnightMonitorMatch>),
	/// Matched conditions from Solana chains
	///
	/// # Note
	/// Box is used here to equalize the enum variants
	Solana(Box<solana::SolanaMonitorMatch>),
}

/// Chain-specific configuration
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ChainConfiguration {
	/// Midnight-specific configuration
	#[serde(skip_serializing_if = "Option::is_none")]
	pub midnight: Option<midnight::MidnightMonitorConfig>,

	/// EVM-specific configuration
	#[serde(skip_serializing_if = "Option::is_none")]
	pub evm: Option<evm::EVMMonitorConfig>,

	/// Stellar-specific configuration
	#[serde(skip_serializing_if = "Option::is_none")]
	pub stellar: Option<stellar::StellarMonitorConfig>,

	/// Solana-specific configuration
	#[serde(skip_serializing_if = "Option::is_none")]
	pub solana: Option<solana::SolanaMonitorConfig>,
}

/// Structure to hold block processing results
///
/// This is used to pass the results of block processing to the trigger handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedBlock {
	pub block_number: u64,
	pub network_slug: String,
	pub processing_results: Vec<MonitorMatch>,
}
