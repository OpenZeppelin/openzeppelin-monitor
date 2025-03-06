//! Domain models and data structures for blockchain monitoring.
//!
//! This module contains all the core data structures used throughout the application:
//!
//! - `blockchain`: Platform-specific implementations for different blockchains (EVM, Stellar)
//! - `config`: Configuration loading and validation
//! - `core`: Core domain models (Monitor, Network, Trigger)

mod blockchain;
mod config;
mod core;

// Re-export blockchain types
pub use blockchain::{BlockChainType, BlockType, MonitorMatch, ProcessedBlock};

pub use blockchain::evm::{
	EVMBlock, EVMMatchArguments, EVMMatchParamEntry, EVMMatchParamsMap, EVMMonitorMatch,
	EVMTransaction,
};

pub use blockchain::stellar::{
	StellarBlock, StellarDecodedParamEntry, StellarDecodedTransaction, StellarEvent,
	StellarLedgerInfo, StellarMatchArguments, StellarMatchParamEntry, StellarMatchParamsMap,
	StellarMonitorMatch, StellarParsedOperationResult, StellarTransaction, StellarTransactionInfo,
};

// Re-export core types
pub use core::{
	AddressWithABI, EventCondition, FunctionCondition, MatchConditions, Monitor, Network,
	NotificationMessage, RpcUrl, ScriptLanguage, TransactionCondition, TransactionStatus, Trigger,
	TriggerConditions, TriggerType, TriggerTypeConfig,
};

// Re-export config types
pub use config::ConfigLoader;
