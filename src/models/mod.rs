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
pub use blockchain::{BlockChainType, BlockType, MonitorMatch};

pub use blockchain::evm::{
    EVMBlock, EVMMatchArguments, EVMMatchParamEntry, EVMMatchParamsMap, EVMMonitorMatch,
    EVMTransaction,
};

pub use blockchain::stellar::{
    StellarLedgerInfo, StellarBlock, StellarDecodedParamEntry, StellarEvent, StellarMatchArguments,
    StellarMatchParamEntry, StellarMatchParamsMap, StellarMonitorMatch,
    StellarParsedOperationResult, StellarTransaction, StellarTransactionInfo,
};

// Re-export core types
pub use core::{
    AddressWithABI, EventCondition, FunctionCondition, MatchConditions, Monitor, Network,
    TransactionCondition, TransactionStatus, Trigger, TriggerType, TriggerTypeConfig,
};

// Re-export config types
pub use config::ConfigLoader;
