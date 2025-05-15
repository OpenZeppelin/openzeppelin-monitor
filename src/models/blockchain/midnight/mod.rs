//! Midnight blockchain specific implementations.
//!
//! This module contains data structures and implementations specific to the
//! Midnight blockchain, including blocks, transactions
//! and monitoring functionality.

mod block;
mod event;
mod monitor;
mod transaction;

pub use block::{
	Block as MidnightBlock, BlockDigest as MidnightBlockDigest, BlockHeader as MidnightBlockHeader,
	RpcBlock as MidnightRpcBlock,
};
pub use event::Event as MidnightEvent;
pub use monitor::{
	MatchArguments as MidnightMatchArguments, MatchParamEntry as MidnightMatchParamEntry,
	MatchParamsMap as MidnightMatchParamsMap, MonitorConfig as MidnightMonitorConfig,
	MonitorMatch as MidnightMonitorMatch,
};
pub use transaction::{
	Operation as MidnightOperation, RpcTransaction as MidnightRpcTransactionEnum,
	Transaction as MidnightTransaction,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub enum MidnightChainType {
	Development,
	Live,
	Local,
	Custom(String),
}

impl std::str::FromStr for MidnightChainType {
	type Err = anyhow::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"Development" => Ok(Self::Development),
			"Live" => Ok(Self::Live),
			"Local" => Ok(Self::Local),
			_ => Err(anyhow::anyhow!("Invalid chain type: {}", s)),
		}
	}
}
