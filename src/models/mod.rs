//! Domain models and data structures for blockchain monitoring.
//!
//! This module contains all the core data structures used throughout the application:
//!
//! - `blockchain`: Platform-specific implementations for different blockchains (EVM, Stellar)
//! - `config`: Configuration loading and validation
//! - `core`: Core domain models (Monitor, Network, Trigger)
//! - `security`: Security models (Secret)

mod blockchain;
mod config;
mod core;
mod security;

// Re-export blockchain types
pub use blockchain::{
	BlockChainType, BlockType, ContractSpec, MonitorMatch, ProcessedBlock, TransactionType,
};

pub use blockchain::evm::{
	EVMBaseReceipt, EVMBaseTransaction, EVMBlock, EVMContractSpec, EVMMatchArguments,
	EVMMatchParamEntry, EVMMatchParamsMap, EVMMonitorMatch, EVMReceiptLog, EVMTransaction,
	EVMTransactionReceipt,
};

pub use blockchain::stellar::{
	StellarBlock, StellarContractFunction, StellarContractInput, StellarContractSpec,
	StellarDecodedParamEntry, StellarDecodedTransaction, StellarEvent,
	StellarFormattedContractSpec, StellarLedgerInfo, StellarMatchArguments, StellarMatchParamEntry,
	StellarMatchParamsMap, StellarMonitorMatch, StellarParsedOperationResult, StellarTransaction,
	StellarTransactionInfo,
};

// Re-export core types
pub use core::{
	AddressWithSpec, EventCondition, FunctionCondition, MatchConditions, Monitor, Network,
	NotificationMessage, RpcUrl, ScriptLanguage, TransactionCondition, TransactionStatus, Trigger,
	TriggerConditions, TriggerType, TriggerTypeConfig,
};

// Re-export config types
pub use config::{ConfigError, ConfigLoader};

// Re-export security types
pub use security::{SecretString, SecretValue, SecurityError};
