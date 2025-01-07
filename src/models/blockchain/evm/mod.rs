//! Ethereum Virtual Machine (EVM) blockchain specific implementations.
//!
//! This module contains data structures and implementations specific to EVM-based
//! blockchains, including blocks, transactions, and monitoring functionality.

mod block;
mod monitor;
mod transaction;

pub use block::Block as EVMBlock;
pub use monitor::{
    EVMMonitorMatch, MatchArguments as EVMMatchArguments, MatchParamEntry as EVMMatchParamEntry,
    MatchParamsMap as EVMMatchParamsMap,
};
pub use transaction::Transaction as EVMTransaction;
