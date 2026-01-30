//! Domain models and data structures for blockchain monitoring.
//!
//! This module contains all the core data structures used throughout the application:
//!
//! - `blockchain`: Platform-specific implementations for different blockchains
//! - `config`: Configuration loading and validation
//! - `core`: Core domain models (Monitor, Network, Trigger)
//! - `security`: Security models (Secret)

mod blockchain;
mod config;
mod core;
mod security;

// Re-export blockchain types
pub use blockchain::{
	BlockChainType, BlockType, ChainConfiguration, ContractSpec, MonitorMatch, ProcessedBlock,
	TransactionType,
};

pub use blockchain::evm::{
	EVMBaseReceipt, EVMBaseTransaction, EVMBlock, EVMContractSpec, EVMMatchArguments,
	EVMMatchParamEntry, EVMMatchParamsMap, EVMMonitorConfig, EVMMonitorMatch, EVMReceiptLog,
	EVMTransaction, EVMTransactionReceipt,
};

pub use blockchain::stellar::{
	StellarBlock, StellarContractEvent, StellarContractEventParam, StellarContractFunction,
	StellarContractInput, StellarContractSpec, StellarDecodedParamEntry, StellarDecodedTransaction,
	StellarEvent, StellarEventParamLocation, StellarFormattedContractSpec, StellarLedgerInfo,
	StellarMatchArguments, StellarMatchParamEntry, StellarMatchParamsMap, StellarMonitorConfig,
	StellarMonitorMatch, StellarParsedOperationResult, StellarTransaction, StellarTransactionInfo,
};

pub use blockchain::midnight::{
	MidnightBaseTransaction, MidnightBlock, MidnightBlockDigest, MidnightBlockHeader,
	MidnightCallDetails, MidnightClaimMintDetails, MidnightDeploymentDetails, MidnightEvent,
	MidnightEventType, MidnightMaintainDetails, MidnightMatchArguments, MidnightMatchParamEntry,
	MidnightMatchParamsMap, MidnightMonitorConfig, MidnightMonitorMatch, MidnightOperation,
	MidnightPayoutDetails, MidnightPhase, MidnightRpcBlock, MidnightRpcTransactionEnum,
	MidnightTopics, MidnightTransaction, MidnightTxAppliedDetails,
};

pub use blockchain::solana::{
	SolanaBlock,
	SolanaConfirmedBlock,
	SolanaContractEvent,
	SolanaContractEventParam,
	SolanaContractFunction,
	SolanaContractInput,
	SolanaContractSpec,
	SolanaDecodedParamEntry,
	SolanaFormattedContractSpec,
	// IDL types
	SolanaIdlAccount,
	SolanaIdlAccountItem,
	SolanaIdlEnumVariant,
	SolanaIdlError,
	SolanaIdlEvent,
	SolanaIdlEventField,
	SolanaIdlField,
	SolanaIdlInstruction,
	SolanaIdlInstructionAccount,
	SolanaIdlInstructionAccounts,
	SolanaIdlMetadata,
	SolanaIdlPda,
	SolanaIdlSeed,
	SolanaIdlType,
	SolanaIdlTypeComplex,
	SolanaIdlTypeDef,
	SolanaIdlTypeDefTy,
	// Other Solana types
	SolanaInstruction,
	SolanaMatchArguments,
	SolanaMatchParamEntry,
	SolanaMatchParamsMap,
	SolanaMonitorConfig,
	SolanaMonitorMatch,
	SolanaParsedInstruction,
	SolanaParsedInstructionResult,
	SolanaTransaction,
	SolanaTransactionInfo,
	SolanaTransactionMessage,
	SolanaTransactionMeta,
};

// Re-export core types
pub use core::{
	AddressWithSpec, BlockRecoveryConfig, EventCondition, FunctionCondition, MatchConditions,
	Monitor, Network, NotificationMessage, RpcUrl, ScriptLanguage, TransactionCondition,
	TransactionStatus, Trigger, TriggerConditions, TriggerType, TriggerTypeConfig,
	WebhookPayloadMode, SCRIPT_LANGUAGE_EXTENSIONS,
};

// Re-export config types
pub use config::{ConfigError, ConfigLoader};

// Re-export security types
pub use security::{SecretString, SecretValue, SecurityError};
