//! Core domain models for the blockchain monitoring system.
//!
//! This module contains the fundamental data structures that represent:
//! - Monitors: Configuration for watching blockchain activity
//! - Networks: Blockchain network definitions and connection details
//! - Triggers: Actions to take when monitored conditions are met

mod monitor;
mod network;
mod trigger;

pub use monitor::{
	AddressWithSpec, EventCondition, FunctionCondition, MatchConditions, Monitor, ScriptLanguage,
	TransactionCondition, TransactionStatus, TriggerConditions,
};
pub use network::{Network, RpcUrl};
pub use trigger::{NotificationMessage, Trigger, TriggerType, TriggerTypeConfig};
