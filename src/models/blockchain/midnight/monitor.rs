//! Midnight monitor data structures.
//!
//! This module provides data structures for monitoring and matching transactions
//! on the Midnight blockchain. It includes types for representing monitor matches,
//! parameters, and configuration.

use serde::{Deserialize, Serialize};

use crate::models::{MatchConditions, MidnightTransaction, Monitor, SecretValue};

/// Result of a successful monitor match on an Midnight chain
///
/// This struct represents the result of a successful match between a monitor
/// configuration and a transaction on the Midnight blockchain. It contains
/// information about the matched transaction, conditions, and decoded arguments.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitorMatch {
	/// Monitor configuration that triggered the match
	pub monitor: Monitor,

	/// Transaction that triggered the match
	pub transaction: MidnightTransaction,

	/// Network slug that the transaction was sent from
	pub network_slug: String,

	/// Conditions that were matched
	pub matched_on: MatchConditions,

	/// Decoded arguments from the matched conditions
	pub matched_on_args: Option<MatchArguments>,
}

/// Collection of decoded parameters from matched conditions
///
/// This struct represents a collection of decoded parameters from a function
/// or event signature, including the signature itself and its arguments.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamsMap {
	/// Function or event signature
	pub signature: String,

	/// Decoded argument values
	pub args: Option<Vec<MatchParamEntry>>,

	/// Raw function/event signature as bytes
	pub hex_signature: Option<String>,
}

/// Single decoded parameter from a function or event
///
/// This struct represents a single decoded parameter from a function or event,
/// including its name, value, type, and whether it's indexed (for events).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamEntry {
	/// Parameter name
	pub name: String,

	/// Parameter value
	pub value: String,

	/// Whether this is an indexed parameter (for events)
	pub indexed: bool,

	/// Parameter type (uint256, address, etc)
	pub kind: String,
}

/// Arguments matched from functions and events
///
/// This struct contains collections of matched function and event arguments,
/// organized by their respective types.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchArguments {
	/// Matched function arguments
	pub functions: Option<Vec<MatchParamsMap>>,

	/// Matched event arguments
	pub events: Option<Vec<MatchParamsMap>>,
}

/// Midnight-specific configuration
///
/// This configuration is used for additional fields in the monitor configuration
/// that are specific to Midnight. It includes viewing keys for decrypting
/// transaction data.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct MonitorConfig {
	/// List of hex encoded viewing keys for decrypting transaction data
	#[serde(default)]
	pub viewing_keys: Vec<SecretValue>,
}
