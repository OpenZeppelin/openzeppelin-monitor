//! Monitor implementation for Solana blockchain.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{MatchConditions, Monitor, SolanaBlock, SolanaTransaction};

/// Result of a successful monitor match on a Solana chain
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitorMatch {
	/// Monitor configuration that triggered the match
	pub monitor: Monitor,

	/// Transaction that triggered the match
	pub transaction: SolanaTransaction,

	/// Block (slot) containing the matched transaction
	pub block: SolanaBlock,

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

	/// Whether this is an indexed parameter (for logs/events)
	pub indexed: bool,
}

/// Arguments matched from functions (instructions) and events (logs)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchArguments {
	/// Matched instruction arguments (similar to function calls)
	pub functions: Option<Vec<MatchParamsMap>>,

	/// Matched log arguments (similar to events)
	pub events: Option<Vec<MatchParamsMap>>,
}

/// Parsed result of a Solana program instruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedInstructionResult {
	/// Address of the program that was invoked
	pub program_id: String,

	/// Name of the instruction (if known)
	pub instruction_name: String,

	/// Full instruction signature
	pub instruction_signature: String,

	/// Decoded instruction arguments
	pub arguments: Vec<Value>,
}

/// Decoded parameter from a Solana instruction or log
///
/// This structure represents a single decoded parameter from a program interaction,
/// providing the parameter's value, type information, and indexing status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedParamEntry {
	/// String representation of the parameter value
	pub value: String,

	/// Parameter type (e.g., "pubkey", "u64", "bytes")
	pub kind: String,

	/// Whether this parameter is indexed (for log topics)
	pub indexed: bool,
}

/// Raw contract specification for a Solana program (IDL - Interface Definition Language)
///
/// This structure represents the Anchor IDL format, which is the most common
/// way to describe Solana program interfaces.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ContractSpec {
	/// IDL version
	pub version: String,

	/// Program name
	pub name: String,

	/// Program instructions
	pub instructions: Vec<IdlInstruction>,

	/// Program accounts (optional)
	#[serde(default)]
	pub accounts: Vec<IdlAccount>,

	/// Program events (optional)
	#[serde(default)]
	pub events: Vec<IdlEvent>,

	/// Custom types defined in the program (optional)
	#[serde(default)]
	pub types: Vec<IdlTypeDef>,

	/// Program errors (optional)
	#[serde(default)]
	pub errors: Vec<IdlError>,

	/// Program metadata (optional)
	#[serde(default)]
	pub metadata: Option<IdlMetadata>,
}

/// An instruction definition in the IDL
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlInstruction {
	/// Instruction name
	pub name: String,

	/// Instruction discriminator (8 bytes, base58 or hex encoded)
	#[serde(default)]
	pub discriminator: Option<Vec<u8>>,

	/// Accounts required by the instruction
	#[serde(default)]
	pub accounts: Vec<IdlAccountItem>,

	/// Arguments to the instruction
	#[serde(default)]
	pub args: Vec<IdlField>,

	/// Documentation for the instruction
	#[serde(default)]
	pub docs: Vec<String>,
}

/// An account item in an instruction (can be a single account or nested accounts)
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum IdlAccountItem {
	/// A single account
	Single(IdlInstructionAccount),
	/// A group of accounts
	Composite(IdlInstructionAccounts),
}

impl Default for IdlAccountItem {
	fn default() -> Self {
		IdlAccountItem::Single(IdlInstructionAccount::default())
	}
}

/// A single account in an instruction
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlInstructionAccount {
	/// Account name
	pub name: String,

	/// Whether the account is mutable
	#[serde(default)]
	pub is_mut: bool,

	/// Whether the account is a signer
	#[serde(default)]
	pub is_signer: bool,

	/// Whether the account is optional
	#[serde(default)]
	pub is_optional: bool,

	/// Documentation
	#[serde(default)]
	pub docs: Vec<String>,

	/// PDA seeds (if this is a PDA)
	#[serde(default)]
	pub pda: Option<IdlPda>,
}

/// A group of accounts in an instruction
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlInstructionAccounts {
	/// Group name
	pub name: String,

	/// Accounts in the group
	pub accounts: Vec<IdlAccountItem>,
}

/// PDA (Program Derived Address) definition
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlPda {
	/// Seeds used to derive the PDA
	pub seeds: Vec<IdlSeed>,

	/// Program ID that owns the PDA (optional, defaults to the current program)
	#[serde(default)]
	pub program_id: Option<String>,
}

/// A seed used in PDA derivation
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind")]
pub enum IdlSeed {
	/// A constant seed
	#[serde(rename = "const")]
	Const { value: Value },
	/// A seed from an account
	#[serde(rename = "account")]
	Account { path: String },
	/// A seed from an argument
	#[serde(rename = "arg")]
	Arg { path: String },
}

impl Default for IdlSeed {
	fn default() -> Self {
		IdlSeed::Const { value: Value::Null }
	}
}

/// A field definition (used for instruction args and type fields)
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlField {
	/// Field name
	pub name: String,

	/// Field type
	#[serde(rename = "type")]
	pub field_type: IdlType,

	/// Documentation
	#[serde(default)]
	pub docs: Vec<String>,
}

/// Type definitions in the IDL
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum IdlType {
	/// Primitive type (string representation)
	Primitive(String),
	/// Complex type
	Complex(IdlTypeComplex),
}

impl Default for IdlType {
	fn default() -> Self {
		IdlType::Primitive("u8".to_string())
	}
}

/// Complex type definition
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum IdlTypeComplex {
	/// Array type
	Array(Box<IdlType>, usize),
	/// Vector type
	Vec(Box<IdlType>),
	/// Option type
	Option(Box<IdlType>),
	/// Defined type (reference to a custom type)
	Defined(String),
	/// Tuple type
	Tuple(Vec<IdlType>),
}

impl Default for IdlTypeComplex {
	fn default() -> Self {
		IdlTypeComplex::Defined("Unknown".to_string())
	}
}

/// Account definition in the IDL
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlAccount {
	/// Account name
	pub name: String,

	/// Account discriminator
	#[serde(default)]
	pub discriminator: Option<Vec<u8>>,

	/// Documentation
	#[serde(default)]
	pub docs: Vec<String>,
}

/// Event definition in the IDL
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlEvent {
	/// Event name
	pub name: String,

	/// Event discriminator (8 bytes)
	#[serde(default)]
	pub discriminator: Option<Vec<u8>>,

	/// Event fields
	#[serde(default)]
	pub fields: Vec<IdlEventField>,
}

/// A field in an event
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlEventField {
	/// Field name
	pub name: String,

	/// Field type
	#[serde(rename = "type")]
	pub field_type: IdlType,

	/// Whether this field is indexed
	#[serde(default)]
	pub index: bool,
}

/// Custom type definition
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlTypeDef {
	/// Type name
	pub name: String,

	/// Type definition
	#[serde(rename = "type")]
	pub type_def: IdlTypeDefTy,

	/// Documentation
	#[serde(default)]
	pub docs: Vec<String>,
}

/// The actual type definition (struct or enum)
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind")]
pub enum IdlTypeDefTy {
	/// Struct type
	#[serde(rename = "struct")]
	Struct { fields: Vec<IdlField> },
	/// Enum type
	#[serde(rename = "enum")]
	Enum { variants: Vec<IdlEnumVariant> },
}

impl Default for IdlTypeDefTy {
	fn default() -> Self {
		IdlTypeDefTy::Struct { fields: vec![] }
	}
}

/// An enum variant
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlEnumVariant {
	/// Variant name
	pub name: String,

	/// Variant fields (if any)
	#[serde(default)]
	pub fields: Option<Vec<IdlField>>,
}

/// Error definition
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlError {
	/// Error code
	pub code: u32,

	/// Error name
	pub name: String,

	/// Error message
	#[serde(default)]
	pub msg: String,
}

/// Program metadata
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct IdlMetadata {
	/// Program address
	#[serde(default)]
	pub address: String,
}

impl ContractSpec {
	/// Get instruction by name
	pub fn get_instruction(&self, name: &str) -> Option<&IdlInstruction> {
		self.instructions.iter().find(|i| i.name == name)
	}

	/// Get event by name
	pub fn get_event(&self, name: &str) -> Option<&IdlEvent> {
		self.events.iter().find(|e| e.name == name)
	}

	/// Get the instruction signature for matching
	pub fn get_instruction_signature(&self, name: &str) -> Option<String> {
		self.get_instruction(name).map(|i| {
			let args: Vec<String> = i
				.args
				.iter()
				.map(|a| idl_type_to_string(&a.field_type).to_string())
				.collect();
			format!("{}({})", i.name, args.join(","))
		})
	}

	/// Get the event signature for matching
	pub fn get_event_signature(&self, name: &str) -> Option<String> {
		self.get_event(name).map(|e| {
			let fields: Vec<String> = e
				.fields
				.iter()
				.map(|f| idl_type_to_string(&f.field_type))
				.collect();
			format!("{}({})", e.name, fields.join(","))
		})
	}
}

/// Convert an IDL type to a string representation
fn idl_type_to_string(ty: &IdlType) -> String {
	match ty {
		IdlType::Primitive(s) => s.clone(),
		IdlType::Complex(c) => match c {
			IdlTypeComplex::Array(inner, size) => {
				format!("[{};{}]", idl_type_to_string(inner), size)
			}
			IdlTypeComplex::Vec(inner) => format!("Vec<{}>", idl_type_to_string(inner)),
			IdlTypeComplex::Option(inner) => format!("Option<{}>", idl_type_to_string(inner)),
			IdlTypeComplex::Defined(name) => name.clone(),
			IdlTypeComplex::Tuple(types) => {
				let types_str: Vec<String> = types.iter().map(idl_type_to_string).collect();
				format!("({})", types_str.join(","))
			}
		},
	}
}

/// Display a ContractSpec
impl std::fmt::Display for ContractSpec {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match serde_json::to_string(self) {
			Ok(s) => write!(f, "{}", s),
			Err(e) => {
				tracing::error!("Error serializing contract spec: {:?}", e);
				write!(f, "")
			}
		}
	}
}

/// Human-readable contract specification for a Solana program
///
/// This structure provides a simplified, application-specific view of a Solana program's
/// interface. It transforms the raw IDL into a more accessible format.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct FormattedContractSpec {
	/// List of callable instructions defined in the program
	pub functions: Vec<ContractFunction>,

	/// List of events defined in the program
	pub events: Vec<ContractEvent>,
}

impl From<ContractSpec> for FormattedContractSpec {
	fn from(spec: ContractSpec) -> Self {
		let functions = spec
			.instructions
			.iter()
			.map(|i| {
				let inputs: Vec<ContractInput> = i
					.args
					.iter()
					.enumerate()
					.map(|(idx, arg)| ContractInput {
						index: idx as u32,
						name: arg.name.clone(),
						kind: idl_type_to_string(&arg.field_type),
					})
					.collect();

				let signature = format!(
					"{}({})",
					i.name,
					inputs
						.iter()
						.map(|i| i.kind.clone())
						.collect::<Vec<_>>()
						.join(",")
				);

				ContractFunction {
					name: i.name.clone(),
					inputs,
					signature,
				}
			})
			.collect();

		let events = spec
			.events
			.iter()
			.map(|e| {
				let params: Vec<ContractEventParam> = e
					.fields
					.iter()
					.map(|f| ContractEventParam {
						name: f.name.clone(),
						kind: idl_type_to_string(&f.field_type),
						indexed: f.index,
					})
					.collect();

				let signature = format!(
					"{}({})",
					e.name,
					params
						.iter()
						.map(|p| p.kind.clone())
						.collect::<Vec<_>>()
						.join(",")
				);

				ContractEvent {
					name: e.name.clone(),
					params,
					signature,
				}
			})
			.collect();

		FormattedContractSpec { functions, events }
	}
}

/// Function (instruction) definition within a Solana program specification
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ContractFunction {
	/// Name of the instruction as defined in the program
	pub name: String,

	/// Ordered list of input parameters accepted by the instruction
	pub inputs: Vec<ContractInput>,

	/// Signature of the instruction
	pub signature: String,
}

/// Input parameter specification for a Solana program instruction
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ContractInput {
	/// Zero-based index of the parameter
	pub index: u32,

	/// Parameter name as defined in the program
	pub name: String,

	/// Parameter type
	pub kind: String,
}

/// Event definition within a Solana program specification
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ContractEvent {
	/// Name of the event as defined in the program
	pub name: String,

	/// Ordered list of parameters in the event
	pub params: Vec<ContractEventParam>,

	/// Signature of the event
	pub signature: String,
}

/// Event parameter specification
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ContractEventParam {
	/// Parameter name as defined in the program
	pub name: String,

	/// Parameter type
	pub kind: String,

	/// Whether this parameter is indexed
	pub indexed: bool,
}

/// Solana-specific monitor configuration
///
/// This configuration is used for additional fields in the monitor configuration
/// that are specific to Solana.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct MonitorConfig {}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_contract_spec_creation() {
		let spec = ContractSpec {
			version: "0.1.0".to_string(),
			name: "test_program".to_string(),
			instructions: vec![IdlInstruction {
				name: "transfer".to_string(),
				discriminator: Some(vec![1, 2, 3, 4, 5, 6, 7, 8]),
				accounts: vec![],
				args: vec![
					IdlField {
						name: "amount".to_string(),
						field_type: IdlType::Primitive("u64".to_string()),
						docs: vec![],
					},
					IdlField {
						name: "recipient".to_string(),
						field_type: IdlType::Primitive("pubkey".to_string()),
						docs: vec![],
					},
				],
				docs: vec![],
			}],
			accounts: vec![],
			events: vec![IdlEvent {
				name: "TransferEvent".to_string(),
				discriminator: Some(vec![1, 2, 3, 4, 5, 6, 7, 8]),
				fields: vec![
					IdlEventField {
						name: "from".to_string(),
						field_type: IdlType::Primitive("pubkey".to_string()),
						index: true,
					},
					IdlEventField {
						name: "to".to_string(),
						field_type: IdlType::Primitive("pubkey".to_string()),
						index: true,
					},
					IdlEventField {
						name: "amount".to_string(),
						field_type: IdlType::Primitive("u64".to_string()),
						index: false,
					},
				],
			}],
			types: vec![],
			errors: vec![],
			metadata: None,
		};

		assert_eq!(spec.name, "test_program");
		assert_eq!(spec.instructions.len(), 1);
		assert_eq!(spec.events.len(), 1);

		// Test get_instruction_signature
		let sig = spec.get_instruction_signature("transfer");
		assert_eq!(sig, Some("transfer(u64,pubkey)".to_string()));

		// Test get_event_signature
		let event_sig = spec.get_event_signature("TransferEvent");
		assert_eq!(
			event_sig,
			Some("TransferEvent(pubkey,pubkey,u64)".to_string())
		);
	}

	#[test]
	fn test_formatted_contract_spec() {
		let spec = ContractSpec {
			version: "0.1.0".to_string(),
			name: "test_program".to_string(),
			instructions: vec![IdlInstruction {
				name: "transfer".to_string(),
				discriminator: None,
				accounts: vec![],
				args: vec![IdlField {
					name: "amount".to_string(),
					field_type: IdlType::Primitive("u64".to_string()),
					docs: vec![],
				}],
				docs: vec![],
			}],
			accounts: vec![],
			events: vec![],
			types: vec![],
			errors: vec![],
			metadata: None,
		};

		let formatted = FormattedContractSpec::from(spec);

		assert_eq!(formatted.functions.len(), 1);
		assert_eq!(formatted.functions[0].name, "transfer");
		assert_eq!(formatted.functions[0].signature, "transfer(u64)");
		assert_eq!(formatted.functions[0].inputs.len(), 1);
		assert_eq!(formatted.functions[0].inputs[0].name, "amount");
		assert_eq!(formatted.functions[0].inputs[0].kind, "u64");
	}

	#[test]
	fn test_match_params_map() {
		let params_map = MatchParamsMap {
			signature: "transfer(u64,pubkey)".to_string(),
			args: Some(vec![
				MatchParamEntry {
					name: "amount".to_string(),
					value: "1000000".to_string(),
					kind: "u64".to_string(),
					indexed: false,
				},
				MatchParamEntry {
					name: "recipient".to_string(),
					value: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
					kind: "pubkey".to_string(),
					indexed: false,
				},
			]),
		};

		assert_eq!(params_map.signature, "transfer(u64,pubkey)");
		let args = params_map.args.unwrap();
		assert_eq!(args.len(), 2);
		assert_eq!(args[0].name, "amount");
		assert_eq!(args[0].value, "1000000");
	}

	#[test]
	fn test_idl_type_to_string() {
		assert_eq!(
			idl_type_to_string(&IdlType::Primitive("u64".to_string())),
			"u64"
		);

		assert_eq!(
			idl_type_to_string(&IdlType::Complex(IdlTypeComplex::Vec(Box::new(
				IdlType::Primitive("u8".to_string())
			)))),
			"Vec<u8>"
		);

		assert_eq!(
			idl_type_to_string(&IdlType::Complex(IdlTypeComplex::Option(Box::new(
				IdlType::Primitive("pubkey".to_string())
			)))),
			"Option<pubkey>"
		);

		assert_eq!(
			idl_type_to_string(&IdlType::Complex(IdlTypeComplex::Array(
				Box::new(IdlType::Primitive("u8".to_string())),
				32
			))),
			"[u8;32]"
		);
	}

	#[test]
	fn test_serde_serialization() {
		let spec = ContractSpec {
			version: "0.1.0".to_string(),
			name: "test_program".to_string(),
			instructions: vec![],
			accounts: vec![],
			events: vec![],
			types: vec![],
			errors: vec![],
			metadata: None,
		};

		let serialized = serde_json::to_string(&spec).unwrap();
		let deserialized: ContractSpec = serde_json::from_str(&serialized).unwrap();

		assert_eq!(deserialized.name, "test_program");
		assert_eq!(deserialized.version, "0.1.0");
	}
}
