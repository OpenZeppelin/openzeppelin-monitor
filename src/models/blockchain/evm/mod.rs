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
	MatchArguments as EVMMatchArguments, MatchParamEntry as EVMMatchParamEntry,
	MatchParamsMap as EVMMatchParamsMap, MonitorMatch as EVMMonitorMatch,
};
pub use receipt::{BaseLog as EVMReceiptLog, TransactionReceipt as EVMTransactionReceipt};
pub use transaction::{BaseTransaction as EVMBaseTransaction, Transaction as EVMTransaction};
