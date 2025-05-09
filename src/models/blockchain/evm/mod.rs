//! Ethereum Virtual Machine (EVM) blockchain specific implementations.
//!
//! This module contains data structures and implementations specific to EVM-based
//! blockchains, including blocks, transactions, and monitoring functionality.

mod block;
mod monitor;
mod receipt;
mod transaction;

pub use block::Block as EVMBlock;
pub use monitor::{
	ContractSpec as EVMContractSpec, EVMMonitorMatch, MatchArguments as EVMMatchArguments,
	MatchParamEntry as EVMMatchParamEntry, MatchParamsMap as EVMMatchParamsMap,
};
pub use receipt::{
	BaseLog as EVMReceiptLog, BaseReceipt as EVMBaseReceipt,
	TransactionReceipt as EVMTransactionReceipt,
};
pub use transaction::{BaseTransaction as EVMBaseTransaction, Transaction as EVMTransaction};
