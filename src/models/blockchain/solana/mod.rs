//! Solana blockchain specific implementations.
//!
//! This module contains data structures and implementations specific to the
//! Solana blockchain, including blocks (slots), transactions, instructions,
//! and monitoring functionality.

mod block;
mod monitor;
mod transaction;

pub use block::{Block as SolanaBlock, ConfirmedBlock as SolanaConfirmedBlock};
pub use monitor::{
	ContractEvent as SolanaContractEvent,
	ContractEventParam as SolanaContractEventParam,
	ContractFunction as SolanaContractFunction,
	ContractInput as SolanaContractInput,
	ContractSpec as SolanaContractSpec,
	DecodedParamEntry as SolanaDecodedParamEntry,
	FormattedContractSpec as SolanaFormattedContractSpec,
	// IDL types
	IdlAccount as SolanaIdlAccount,
	IdlAccountItem as SolanaIdlAccountItem,
	IdlEnumVariant as SolanaIdlEnumVariant,
	IdlError as SolanaIdlError,
	IdlEvent as SolanaIdlEvent,
	IdlEventField as SolanaIdlEventField,
	IdlField as SolanaIdlField,
	IdlInstruction as SolanaIdlInstruction,
	IdlInstructionAccount as SolanaIdlInstructionAccount,
	IdlInstructionAccounts as SolanaIdlInstructionAccounts,
	IdlMetadata as SolanaIdlMetadata,
	IdlPda as SolanaIdlPda,
	IdlSeed as SolanaIdlSeed,
	IdlType as SolanaIdlType,
	IdlTypeComplex as SolanaIdlTypeComplex,
	IdlTypeDef as SolanaIdlTypeDef,
	IdlTypeDefTy as SolanaIdlTypeDefTy,
	// Other monitor types
	MatchArguments as SolanaMatchArguments,
	MatchParamEntry as SolanaMatchParamEntry,
	MatchParamsMap as SolanaMatchParamsMap,
	MonitorConfig as SolanaMonitorConfig,
	MonitorMatch as SolanaMonitorMatch,
	ParsedInstructionResult as SolanaParsedInstructionResult,
};
pub use transaction::{
	Instruction as SolanaInstruction, ParsedInstruction as SolanaParsedInstruction,
	Transaction as SolanaTransaction, TransactionInfo as SolanaTransactionInfo,
	TransactionMessage as SolanaTransactionMessage, TransactionMeta as SolanaTransactionMeta,
};
