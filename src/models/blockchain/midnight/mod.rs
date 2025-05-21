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
pub use event::{
	CallDetails as MidnightCallDetails, ClaimMintDetails as MidnightClaimMintDetails,
	DeploymentDetails as MidnightDeploymentDetails, Event as MidnightEvent,
	EventType as MidnightEventType, MaintainDetails as MidnightMaintainDetails,
	PayoutDetails as MidnightPayoutDetails, Phase as MidnightPhase, Topics as MidnightTopics,
	TxAppliedDetails as MidnightTxAppliedDetails,
};
pub use monitor::{
	MatchArguments as MidnightMatchArguments, MatchParamEntry as MidnightMatchParamEntry,
	MatchParamsMap as MidnightMatchParamsMap, MonitorConfig as MidnightMonitorConfig,
	MonitorMatch as MidnightMonitorMatch,
};
pub use transaction::{
	MidnightRpcTransaction as MidnightBaseTransaction, Operation as MidnightOperation,
	RpcTransaction as MidnightRpcTransactionEnum, Transaction as MidnightTransaction,
};
