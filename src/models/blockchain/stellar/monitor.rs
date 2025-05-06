//! Monitor implementation for Stellar blockchain.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{MatchConditions, Monitor, StellarBlock, StellarTransaction};

/// Result of a successful monitor match on a Stellar chain
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitorMatch {
	/// Monitor configuration that triggered the match
	pub monitor: Monitor,

	/// Transaction that triggered the match
	pub transaction: StellarTransaction,

	/// Ledger containing the matched transaction
	pub ledger: StellarBlock,

	/// Network slug that the transaction was sent from
	pub network_slug: String,

	/// Conditions that were matched
	pub matched_on: MatchConditions,

	/// Decoded arguments from the matched conditions
	pub matched_on_args: Option<MatchArguments>,
}

/// Collection of decoded parameters from matched conditions
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamsMap {
	/// Function or event signature
	pub signature: String,

	/// Decoded argument values
	pub args: Option<Vec<MatchParamEntry>>,
}

/// Single decoded parameter from a function or event
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchParamEntry {
	/// Parameter name
	pub name: String,

	/// Parameter value
	pub value: String,

	/// Parameter type
	pub kind: String,

	/// Whether this is an indexed parameter
	pub indexed: bool,
}

/// Arguments matched from functions and events
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchArguments {
	/// Matched function arguments
	pub functions: Option<Vec<MatchParamsMap>>,

	/// Matched event arguments
	pub events: Option<Vec<MatchParamsMap>>,
}

/// Parsed result of a Stellar contract operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedOperationResult {
	/// Address of the contract that was called
	pub contract_address: String,

	/// Name of the function that was called
	pub function_name: String,

	/// Full function signature
	pub function_signature: String,

	/// Decoded function arguments
	pub arguments: Vec<Value>,
}

/// Decoded parameter from a Stellar contract function or event
///
/// This structure represents a single decoded parameter from a contract interaction,
/// providing the parameter's value, type information, and indexing status.
/// Similar to EVM event/function parameters but adapted for Stellar's type system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedParamEntry {
	/// String representation of the parameter value
	pub value: String,

	/// Parameter type (e.g., "address", "i128", "bytes")
	pub kind: String,

	/// Whether this parameter is indexed (for event topics)
	pub indexed: bool,
}

/// Contract specification for a Stellar smart contract
///
/// This structure represents the parsed specification of a Stellar smart contract,
/// following the Stellar Contract ABI format. It contains information about all
/// callable functions in the contract, similar to Ethereum's ABI but in Stellar's format.
/// The spec is typically extracted from a contract's WASM bytecode.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ContractSpec {
	/// List of callable functions defined in the contract
	pub functions: Vec<ContractFunction>,
}

/// Function definition within a Stellar contract specification
///
/// Represents a callable function in a Stellar smart contract, including its name
/// and input parameters. This is parsed from the contract's ScSpecFunctionV0 entries
/// and provides a more accessible format for working with contract interfaces.
///
/// # Example
/// ```ignore
/// {
///     "name": "transfer",
///     "inputs": [
///         {"index": 0, "name": "to", "kind": "Address"},
///         {"index": 1, "name": "amount", "kind": "U64"}
///     ]
/// }
/// ```
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContractFunction {
	/// Name of the function as defined in the contract
	pub name: String,

	/// Ordered list of input parameters accepted by the function
	pub inputs: Vec<ContractInput>,
}

/// Input parameter specification for a Stellar contract function
///
/// Describes a single parameter in a contract function, including its position,
/// name, and type. The type (kind) follows Stellar's type system and can include
/// basic types (U64, I64, Address, etc.) as well as complex types (Vec, Map, etc.).
///
/// # Type Examples
/// - Basic types: "U64", "I64", "Address", "Bool", "String"
/// - Complex types: "Vec<Address>", "Map<String,U64>", "Bytes32"
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ContractInput {
	/// Zero-based index of the parameter in the function signature
	pub index: u32,

	/// Parameter name as defined in the contract
	pub name: String,

	/// Parameter type in Stellar's type system format
	pub kind: String,
}
