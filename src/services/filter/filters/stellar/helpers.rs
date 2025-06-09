//! Helper functions for Stellar-specific operations.
//!
//! This module provides utility functions for working with Stellar-specific data types
//! and formatting, including address normalization, XDR value parsing, and
//! operation processing.

use alloy::primitives::{I256, U256};
use hex::encode;
use serde_json::{json, Value};
use std::fmt;
use std::fmt::Display;
// NOTE: this may be moved to stellar_xdr in the future
use soroban_spec::read;
use std::collections::BTreeMap;
use stellar_strkey::{ed25519::PublicKey as StrkeyPublicKey, Contract};
use stellar_xdr::curr::{
	AccountId, ContractExecutable, Hash, HostFunction, Int128Parts, Int256Parts,
	InvokeHostFunctionOp, LedgerEntryData, LedgerKey, LedgerKeyContractCode, Limits, PublicKey,
	ReadXdr, ScAddress, ScMapEntry, ScSpecEntry, ScSpecTypeDef, ScVal, UInt128Parts, UInt256Parts,
};

use crate::models::{
	StellarContractFunction, StellarContractInput, StellarDecodedParamEntry,
	StellarFormattedContractSpec, StellarParsedOperationResult,
};

/// Represents all possible Stellar smart contract types
#[derive(Debug, Clone, PartialEq)]
pub enum StellarType {
	/// Boolean type
	Bool,
	/// Void type (null/empty)
	Void,
	/// 32-bit unsigned integer
	U32,
	/// 32-bit signed integer
	I32,
	/// 64-bit unsigned integer
	U64,
	/// 64-bit signed integer
	I64,
	/// 128-bit unsigned integer
	U128,
	/// 128-bit signed integer
	I128,
	/// 256-bit unsigned integer
	U256,
	/// 256-bit signed integer
	I256,
	/// Byte array, optionally with fixed length
	Bytes(Option<u32>), // None for variable length, Some(n) for BytesN
	/// String type
	String,
	/// Symbol type (enum-like string)
	Symbol,
	/// Vector of values
	Vec(Box<StellarType>),
	/// Map of key-value pairs
	Map(Box<StellarType>, Box<StellarType>),
	/// Tuple of values
	Tuple(Box<StellarType>),
	/// Stellar address type (account or contract)
	Address,
	/// Timepoint type (Unix timestamp)
	Timepoint,
	/// Duration type (time interval)
	Duration,
	/// Union type (multiple possible types)
	Union(Vec<StellarType>),
	/// Sequence type (ordered list of types)
	Sequence(Vec<StellarType>),
	/// User-defined type
	Udt(String),
}

/// Represents all possible Stellar smart contract values
#[derive(Debug, Clone)]
pub enum StellarValue {
	/// Boolean value
	Bool(bool),
	/// Void value (null/empty)
	Void,
	/// 32-bit unsigned integer value
	U32(u32),
	/// 32-bit signed integer value
	I32(i32),
	/// 64-bit unsigned integer value
	U64(u64),
	/// 64-bit signed integer value
	I64(i64),
	/// 128-bit unsigned integer value (as string)
	U128(String), // Using string for large numbers
	/// 128-bit signed integer value (as string)
	I128(String),
	/// 256-bit unsigned integer value (as string)
	U256(String),
	/// 256-bit signed integer value (as string)
	I256(String),
	/// Byte array value
	Bytes(Vec<u8>),
	/// String value
	String(String),
	/// Symbol value
	Symbol(String),
	/// Vector of values
	Vec(Vec<StellarValue>),
	/// Map of key-value pairs
	Map(BTreeMap<String, StellarValue>),
	/// Tuple of values
	Tuple(Vec<StellarValue>),
	/// Stellar address value
	Address(String),
	/// Timepoint value
	Timepoint(u64),
	/// Duration value
	Duration(u64),
	/// User-defined type value
	Udt(String),
}

impl From<ScVal> for StellarValue {
	/// Converts a Stellar Contract Value (ScVal) into a StellarValue.
	///
	/// # Arguments
	/// * `val` - The ScVal to convert
	///
	/// # Returns
	/// A StellarValue representing the input ScVal
	fn from(val: ScVal) -> Self {
		match val {
			ScVal::Bool(b) => StellarValue::Bool(b),
			ScVal::Void => StellarValue::Void,
			ScVal::U32(n) => StellarValue::U32(n),
			ScVal::I32(n) => StellarValue::I32(n),
			ScVal::U64(n) => StellarValue::U64(n),
			ScVal::I64(n) => StellarValue::I64(n),
			ScVal::Timepoint(t) => StellarValue::Timepoint(t.0),
			ScVal::Duration(d) => StellarValue::Duration(d.0),
			ScVal::U128(n) => StellarValue::U128(combine_u128(&n)),
			ScVal::I128(n) => StellarValue::I128(combine_i128(&n)),
			ScVal::U256(n) => StellarValue::U256(combine_u256(&n)),
			ScVal::I256(n) => StellarValue::I256(combine_i256(&n)),
			ScVal::Bytes(b) => StellarValue::Bytes(b.to_vec()),
			ScVal::String(s) => StellarValue::String(s.to_string()),
			ScVal::Symbol(s) => StellarValue::Symbol(s.to_string()),
			ScVal::Vec(Some(vec)) => StellarValue::Vec(
				vec.0
					.iter()
					.map(|v| StellarValue::from(v.clone()))
					.collect(),
			),
			ScVal::Map(Some(map)) => {
				let mut btree = BTreeMap::new();
				for ScMapEntry { key, val } in map.0.iter() {
					let key_str = match StellarValue::from(key.clone()) {
						StellarValue::String(s) => s,
						other => other.to_string(),
					};
					btree.insert(key_str, StellarValue::from(val.clone()));
				}
				StellarValue::Map(btree)
			}
			ScVal::Address(addr) => StellarValue::Address(match addr {
				ScAddress::Contract(hash) => Contract(hash.0).to_string(),
				ScAddress::Account(account_id) => match account_id {
					AccountId(PublicKey::PublicKeyTypeEd25519(key)) => {
						StrkeyPublicKey(key.0).to_string()
					}
				},
			}),
			_ => StellarValue::Void,
		}
	}
}

impl From<ScSpecTypeDef> for StellarType {
	/// Converts a Stellar Contract Specification Type Definition into a StellarType.
	///
	/// # Arguments
	/// * `type_def` - The ScSpecTypeDef to convert
	///
	/// # Returns
	/// A StellarType representing the input type definition
	fn from(type_def: ScSpecTypeDef) -> Self {
		match type_def {
			ScSpecTypeDef::Map(t) => StellarType::Map(
				Box::new(StellarType::from(*t.key_type)),
				Box::new(StellarType::from(*t.value_type)),
			),
			ScSpecTypeDef::Vec(t) => StellarType::Vec(Box::new(StellarType::from(*t.element_type))),
			ScSpecTypeDef::BytesN(bytes_n) => StellarType::Bytes(Some(bytes_n.n)),
			ScSpecTypeDef::Tuple(t) => {
				let types: Vec<StellarType> = t
					.value_types
					.iter()
					.map(|t| StellarType::from(t.clone()))
					.collect();
				StellarType::Tuple(Box::new(StellarType::Sequence(types)))
			}
			ScSpecTypeDef::U128 => StellarType::U128,
			ScSpecTypeDef::I128 => StellarType::I128,
			ScSpecTypeDef::U256 => StellarType::U256,
			ScSpecTypeDef::I256 => StellarType::I256,
			ScSpecTypeDef::Address => StellarType::Address,
			ScSpecTypeDef::Bool => StellarType::Bool,
			ScSpecTypeDef::Symbol => StellarType::Symbol,
			ScSpecTypeDef::String => StellarType::String,
			ScSpecTypeDef::Bytes => StellarType::Bytes(None),
			ScSpecTypeDef::U32 => StellarType::U32,
			ScSpecTypeDef::I32 => StellarType::I32,
			ScSpecTypeDef::U64 => StellarType::U64,
			ScSpecTypeDef::I64 => StellarType::I64,
			ScSpecTypeDef::Timepoint => StellarType::Timepoint,
			ScSpecTypeDef::Duration => StellarType::Duration,
			ScSpecTypeDef::Void => StellarType::Void,
			ScSpecTypeDef::Udt(udt) => StellarType::Udt(udt.name.to_string()),
			_ => StellarType::Void,
		}
	}
}

impl From<Value> for StellarType {
	/// Converts a JSON Value into a StellarType.
	///
	/// # Arguments
	/// * `value` - The JSON Value to convert
	///
	/// # Returns
	/// A StellarType representing the input JSON value
	fn from(value: Value) -> Self {
		match value {
			Value::Number(n) => {
				if n.is_u64() {
					StellarType::U64
				} else {
					StellarType::I64 // Fallback
				}
			}
			Value::Bool(_) => StellarType::Bool,
			Value::String(s) => {
				if is_address(&s) {
					StellarType::Address
				} else {
					StellarType::String
				}
			}
			Value::Array(_) => StellarType::Vec(Box::new(StellarType::Void)), // Generic vector
			Value::Object(_) => {
				StellarType::Map(Box::new(StellarType::String), Box::new(StellarType::Void))
			} // Generic map
			Value::Null => StellarType::Void,
		}
	}
}

impl StellarValue {
	/// Gets the type of this Stellar value.
	///
	/// # Returns
	/// A StellarType representing the type of this value
	pub fn get_type(&self) -> StellarType {
		match self {
			StellarValue::Bool(_) => StellarType::Bool,
			StellarValue::Void => StellarType::Void,
			StellarValue::U32(_) => StellarType::U32,
			StellarValue::I32(_) => StellarType::I32,
			StellarValue::U64(_) => StellarType::U64,
			StellarValue::I64(_) => StellarType::I64,
			StellarValue::U128(_) => StellarType::U128,
			StellarValue::I128(_) => StellarType::I128,
			StellarValue::U256(_) => StellarType::U256,
			StellarValue::I256(_) => StellarType::I256,
			StellarValue::Bytes(b) => {
				if b.is_empty() {
					StellarType::Bytes(None)
				} else {
					StellarType::Bytes(Some(b.len() as u32))
				}
			}
			StellarValue::String(_) => StellarType::String,
			StellarValue::Symbol(_) => StellarType::Symbol,
			StellarValue::Vec(v) => {
				// Get all unique types in the vector
				let mut types: Vec<StellarType> =
					v.iter().map(|val| val.get_type()).collect::<Vec<_>>();
				// types.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
				types.dedup();

				if types.is_empty() {
					StellarType::Vec(Box::new(StellarType::Void))
				} else if types.len() == 1 {
					// If all elements are the same type, use that
					StellarType::Vec(Box::new(types[0].clone()))
				} else {
					// If elements have different types, create a union type
					StellarType::Vec(Box::new(StellarType::Union(types)))
				}
			}
			StellarValue::Map(m) => {
				// Get all unique value types in the map
				let mut types: Vec<StellarType> =
					m.values().map(|val| val.get_type()).collect::<Vec<_>>();
				// types.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
				types.dedup();

				if types.is_empty() {
					StellarType::Map(Box::new(StellarType::String), Box::new(StellarType::Void))
				} else if types.len() == 1 {
					// If all values are the same type, use that
					StellarType::Map(Box::new(StellarType::String), Box::new(types[0].clone()))
				} else {
					// If values have different types, create a union type
					StellarType::Map(
						Box::new(StellarType::String),
						Box::new(StellarType::Union(types)),
					)
				}
			}
			StellarValue::Tuple(v) => {
				// For tuples, preserve all types in order
				let types: Vec<StellarType> = v.iter().map(|val| val.get_type()).collect();
				if types.is_empty() {
					StellarType::Tuple(Box::new(StellarType::Void))
				} else {
					StellarType::Tuple(Box::new(StellarType::Sequence(types)))
				}
			}
			StellarValue::Address(_) => StellarType::Address,
			StellarValue::Timepoint(_) => StellarType::Timepoint,
			StellarValue::Duration(_) => StellarType::Duration,
			StellarValue::Udt(name) => StellarType::Udt(name.clone()),
		}
	}

	/// Converts this Stellar value to a JSON value.
	///
	/// # Returns
	/// A serde_json::Value representing this Stellar value
	pub fn to_json(&self) -> Value {
		match self {
			StellarValue::Bool(b) => json!(b),
			StellarValue::Void => json!(null),
			StellarValue::U32(n) => json!(n),
			StellarValue::I32(n) => json!(n),
			StellarValue::U64(n) => json!(n),
			StellarValue::I64(n) => json!(n),
			StellarValue::U128(s) => json!({"type": "U128", "value": s}),
			StellarValue::I128(s) => json!({"type": "I128", "value": s}),
			StellarValue::U256(s) => json!({"type": "U256", "value": s}),
			StellarValue::I256(s) => json!({"type": "I256", "value": s}),
			StellarValue::Bytes(b) => json!(encode(b)),
			StellarValue::String(s) => json!(s),
			StellarValue::Symbol(s) => json!(s),
			StellarValue::Vec(v) => json!(v.iter().map(|x| x.to_json()).collect::<Vec<_>>()),
			StellarValue::Map(m) => {
				let map: serde_json::Map<String, Value> =
					m.iter().map(|(k, v)| (k.clone(), v.to_json())).collect();
				json!(map)
			}
			StellarValue::Tuple(v) => json!(v.iter().map(|x| x.to_json()).collect::<Vec<_>>()),
			StellarValue::Address(a) => json!(a),
			StellarValue::Timepoint(t) => json!(t),
			StellarValue::Duration(d) => json!(d),
			StellarValue::Udt(name) => json!(name),
		}
	}

	/// Creates a decoded parameter entry from this Stellar value.
	///
	/// # Arguments
	/// * `indexed` - Whether this parameter is indexed
	///
	/// # Returns
	/// A StellarDecodedParamEntry containing the value and its type
	pub fn to_param_entry(&self, indexed: bool) -> StellarDecodedParamEntry {
		StellarDecodedParamEntry {
			value: self.to_string(),
			kind: self.get_type().to_string(),
			indexed,
		}
	}
}

impl Display for StellarValue {
	/// Formats a StellarValue as a string.
	///
	/// # Arguments
	/// * `f` - The formatter to write to
	///
	/// # Returns
	/// A fmt::Result indicating success or failure
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			StellarValue::Bool(b) => write!(f, "{}", b),
			StellarValue::Void => write!(f, "null"),
			StellarValue::U32(n) => write!(f, "{}", n),
			StellarValue::I32(n) => write!(f, "{}", n),
			StellarValue::U64(n) => write!(f, "{}", n),
			StellarValue::I64(n) => write!(f, "{}", n),
			StellarValue::U128(s) => write!(f, "{}", s),
			StellarValue::I128(s) => write!(f, "{}", s),
			StellarValue::U256(s) => write!(f, "{}", s),
			StellarValue::I256(s) => write!(f, "{}", s),
			StellarValue::Bytes(b) => write!(f, "{}", encode(b)),
			StellarValue::String(s) => write!(f, "{}", s),
			StellarValue::Symbol(s) => write!(f, "{}", s),
			StellarValue::Vec(v) => {
				let items: Vec<String> = v.iter().map(|x| x.to_string()).collect();
				write!(f, "[{}]", items.join(","))
			}
			StellarValue::Map(m) => {
				let items: Vec<String> = m.iter().map(|(k, v)| format!("{}:{}", k, v)).collect();
				write!(f, "{{{}}}", items.join(","))
			}
			StellarValue::Tuple(v) => {
				let items: Vec<String> = v.iter().map(|x| x.to_string()).collect();
				write!(f, "({})", items.join(","))
			}
			StellarValue::Address(a) => write!(f, "{}", a),
			StellarValue::Timepoint(t) => write!(f, "{}", t),
			StellarValue::Duration(d) => write!(f, "{}", d),
			StellarValue::Udt(name) => write!(f, "{}", name),
		}
	}
}

impl fmt::Display for StellarType {
	/// Formats a StellarType as a string.
	///
	/// # Arguments
	/// * `f` - The formatter to write to
	///
	/// # Returns
	/// A fmt::Result indicating success or failure
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			StellarType::Bool => write!(f, "Bool"),
			StellarType::Void => write!(f, "Void"),
			StellarType::U32 => write!(f, "U32"),
			StellarType::I32 => write!(f, "I32"),
			StellarType::U64 => write!(f, "U64"),
			StellarType::I64 => write!(f, "I64"),
			StellarType::U128 => write!(f, "U128"),
			StellarType::I128 => write!(f, "I128"),
			StellarType::U256 => write!(f, "U256"),
			StellarType::I256 => write!(f, "I256"),
			StellarType::Bytes(Some(n)) => write!(f, "Bytes{}", n),
			StellarType::Bytes(None) => write!(f, "Bytes"),
			StellarType::String => write!(f, "String"),
			StellarType::Symbol => write!(f, "Symbol"),
			StellarType::Vec(t) => write!(f, "Vec<{}>", t),
			StellarType::Map(k, v) => write!(f, "Map<{},{}>", k, v),
			StellarType::Tuple(t) => write!(f, "Tuple<{}>", t),
			StellarType::Address => write!(f, "Address"),
			StellarType::Timepoint => write!(f, "Timepoint"),
			StellarType::Duration => write!(f, "Duration"),
			StellarType::Udt(name) => write!(f, "{}", name),
			StellarType::Union(types) => {
				write!(
					f,
					"{}",
					types
						.iter()
						.map(|t| t.to_string())
						.collect::<Vec<_>>()
						.join(",")
				)
			}
			StellarType::Sequence(types) => {
				write!(
					f,
					"{}",
					types
						.iter()
						.map(|t| t.to_string())
						.collect::<Vec<_>>()
						.join(",")
				)
			}
		}
	}
}

/// Combines the parts of a UInt256 into a single string representation.
///
/// # Arguments
/// * `n` - The UInt256Parts containing the 4 64-bit components
///
/// # Returns
/// A string representation of the combined 256-bit unsigned integer
pub fn combine_u256(n: &UInt256Parts) -> String {
	let result = U256::from_limbs([n.lo_lo, n.lo_hi, n.hi_lo, n.hi_hi]);
	result.to_string()
}

/// Combines the parts of an Int256 into a single string representation.
/// Note: hi_hi is signed (i64) while other components are unsigned (u64)
///
/// # Arguments
/// * `n` - The Int256Parts containing the signed hi_hi and 3 unsigned components
///
/// # Returns
/// A string representation of the combined 256-bit signed integer
pub fn combine_i256(n: &Int256Parts) -> String {
	// First create unsigned value from the limbs
	let unsigned = U256::from_limbs([n.lo_lo, n.lo_hi, n.hi_lo, n.hi_hi as u64]);

	// If hi_hi is negative, we need to handle the sign
	if n.hi_hi < 0 {
		// Create I256 and negate if necessary
		let signed = I256::from_raw(unsigned);
		// If hi_hi was negative, we need to adjust the value
		// by subtracting 2^256 from it
		(-signed).to_string()
	} else {
		// If hi_hi was non-negative, we can use the unsigned value directly
		I256::from_raw(unsigned).to_string()
	}
}

/// Combines the parts of a UInt128 into a single string representation.
///
/// # Arguments
/// * `n` - The UInt128Parts containing the 2 64-bit components
///
/// # Returns
/// A string representation of the combined 128-bit unsigned integer
pub fn combine_u128(n: &UInt128Parts) -> String {
	(((n.hi as u128) << 64) | (n.lo as u128)).to_string()
}

/// Combines the parts of an Int128 into a single string representation.
///
/// # Arguments
/// * `n` - The Int128Parts containing the 2 64-bit components
///
/// # Returns
/// A string representation of the combined 128-bit signed integer
pub fn combine_i128(n: &Int128Parts) -> String {
	(((n.hi as i128) << 64) | (n.lo as i128)).to_string()
}

/// Gets the function signature from a Stellar Contract Specification Entry.
///
/// # Arguments
/// * `entry` - The ScSpecEntry to get the signature for
///
/// # Returns
/// A string representing the function signature in the format "function_name(type1,type2,...)"
fn get_function_signature_from_spec_entry(entry: &ScSpecEntry) -> String {
	match entry {
		ScSpecEntry::FunctionV0(func) => {
			let function_name = func.name.to_string();
			let arg_types: Vec<String> = func
				.inputs
				.iter()
				.map(|input| StellarType::from(input.type_.clone()).to_string())
				.collect();
			format!("{}({})", function_name, arg_types.join(","))
		}
		_ => "unknown_function()".to_string(),
	}
}

/// Gets the function signature for a Stellar host function operation.
///
/// # Arguments
/// * `invoke_op` - The InvokeHostFunctionOp to get the signature for
/// * `contract_spec` - Optional contract spec containing type information
///
/// # Returns
/// A string representing the function signature in the format "function_name(type1,type2,...)"
pub fn get_function_signature(
	invoke_op: &InvokeHostFunctionOp,
	contract_spec: Option<&StellarFormattedContractSpec>,
) -> String {
	match &invoke_op.host_function {
		HostFunction::InvokeContract(args) => {
			let function_name = args.function_name.to_string();

			// If we have a contract spec, try to find the matching function
			if let Some(spec) = contract_spec {
				// Get the runtime types of the arguments
				let arg_types: Vec<String> = args
					.args
					.iter()
					.map(|arg| StellarValue::from(arg.clone()).get_type().to_string())
					.collect();

				// Find a function that matches both name and parameter types
				if let Some(function) = spec.functions.iter().find(|f| {
					f.name == function_name
						&& f.inputs.len() == args.args.len()
						&& f.inputs
							.iter()
							.zip(arg_types.iter())
							.all(|(input, arg_type)| {
								// This is a best-effort attempt to match the types
								// For UDTs, we need to be more lenient in type matching
								// since ScVal will show the concrete type structure
								// For example: Map<Request> could be Map<String, Union<Address, U32>>
								// So we need to just match the base type
								const LENIENT_TYPES: [&str; 3] = ["Vec<", "Map<", "Tuple<"];
								if LENIENT_TYPES.iter().any(|prefix| {
									input.kind.starts_with(prefix) && arg_type.starts_with(prefix)
								}) {
									true
								} else {
									// For basic types, require exact match
									input.kind == *arg_type
								}
							})
				}) {
					// Use the pre-computed signature from the contract spec
					return function.signature.clone();
				}
			}

			// Fallback to runtime type inference if no spec or function not found
			let types: Vec<String> = args
				.args
				.iter()
				.map(|arg| StellarValue::from(arg.clone()).get_type().to_string())
				.collect();
			format!("{}({})", function_name, types.join(","))
		}
		_ => "unknown_function()".to_string(),
	}
}

/// Processes a Stellar host function operation into a parsed result.
///
/// # Arguments
/// * `invoke_op` - The InvokeHostFunctionOp to process
/// * `contract_specs` - Optional contract spec containing type information
///
/// # Returns
/// A tuple containing:
/// * A StellarParsedOperationResult with the processed operation details
/// * An optional StellarContractSpec if a matching contract was found
pub fn process_invoke_host_function(
	invoke_op: &InvokeHostFunctionOp,
	contract_specs: Option<&[(String, StellarFormattedContractSpec)]>,
) -> (
	StellarParsedOperationResult,
	Option<StellarFormattedContractSpec>,
) {
	match &invoke_op.host_function {
		HostFunction::InvokeContract(args) => {
			let contract_address = match &args.contract_address {
				ScAddress::Contract(hash) => Contract(hash.0).to_string(),
				ScAddress::Account(account_id) => match account_id {
					AccountId(PublicKey::PublicKeyTypeEd25519(key)) => {
						StrkeyPublicKey(key.0).to_string()
					}
				},
			};

			let function_name = args.function_name.to_string();

			let arguments = args
				.args
				.iter()
				.map(|arg| StellarValue::from(arg.clone()).to_json())
				.collect();

			// Get contract spec for the operation
			let contract_spec = contract_specs.and_then(|specs| {
				specs
					.iter()
					.find(|(addr, _)| are_same_address(addr, &contract_address))
					.map(|(_, spec)| spec)
			});

			(
				StellarParsedOperationResult {
					contract_address,
					function_name,
					function_signature: get_function_signature(invoke_op, contract_spec),
					arguments,
				},
				contract_spec.cloned(),
			)
		}
		_ => (
			StellarParsedOperationResult {
				contract_address: "".to_string(),
				function_name: "".to_string(),
				function_signature: "".to_string(),
				arguments: vec![],
			},
			None,
		),
	}
}

/// Checks if a string is a valid Stellar address.
///
/// # Arguments
/// * `address` - The string to check
///
/// # Returns
/// `true` if the string is a valid Stellar address, `false` otherwise
pub fn is_address(address: &str) -> bool {
	StrkeyPublicKey::from_string(address).is_ok() || Contract::from_string(address).is_ok()
}

/// Compares two Stellar addresses for equality, ignoring case and whitespace.
///
/// # Arguments
/// * `address1` - First address to compare
/// * `address2` - Second address to compare
///
/// # Returns
/// `true` if the addresses are equivalent, `false` otherwise
pub fn are_same_address(address1: &str, address2: &str) -> bool {
	normalize_address(address1) == normalize_address(address2)
}

/// Normalizes a Stellar address by removing whitespace and converting to lowercase.
///
/// # Arguments
/// * `address` - The address string to normalize
///
/// # Returns
/// The normalized address string
pub fn normalize_address(address: &str) -> String {
	address.trim().replace(" ", "").to_lowercase()
}

/// Compares two Stellar function signatures for equality, ignoring case and whitespace.
///
/// # Arguments
/// * `signature1` - First signature to compare
/// * `signature2` - Second signature to compare
///
/// # Returns
/// `true` if the signatures are equivalent, `false` otherwise
pub fn are_same_signature(signature1: &str, signature2: &str) -> bool {
	normalize_signature(signature1) == normalize_signature(signature2)
}

/// Normalizes a Stellar function signature by removing whitespace and converting to lowercase.
///
/// # Arguments
/// * `signature` - The signature string to normalize
///
/// # Returns
/// The normalized signature string
pub fn normalize_signature(signature: &str) -> String {
	signature.trim().replace(" ", "").to_lowercase()
}

/// Parses a Stellar Contract Value into a decoded parameter entry.
///
/// # Arguments
/// * `val` - The ScVal to parse
/// * `indexed` - Whether this parameter is indexed
///
/// # Returns
/// An Option containing the decoded parameter entry if successful
pub fn parse_sc_val(val: &ScVal, indexed: bool) -> Option<StellarDecodedParamEntry> {
	match val {
		ScVal::Bool(b) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "Bool".to_string(),
			value: b.to_string(),
		}),
		ScVal::U32(n) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "U32".to_string(),
			value: n.to_string(),
		}),
		ScVal::I32(n) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "I32".to_string(),
			value: n.to_string(),
		}),
		ScVal::U64(n) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "U64".to_string(),
			value: n.to_string(),
		}),
		ScVal::I64(n) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "I64".to_string(),
			value: n.to_string(),
		}),
		ScVal::Timepoint(t) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "Timepoint".to_string(),
			value: t.0.to_string(),
		}),
		ScVal::Duration(d) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "Duration".to_string(),
			value: d.0.to_string(),
		}),
		ScVal::U128(u128val) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "U128".to_string(),
			value: combine_u128(u128val),
		}),
		ScVal::I128(i128val) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "I128".to_string(),
			value: combine_i128(i128val),
		}),
		ScVal::U256(u256val) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "U256".to_string(),
			value: combine_u256(u256val),
		}),
		ScVal::I256(i256val) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "I256".to_string(),
			value: combine_i256(i256val),
		}),
		ScVal::Bytes(bytes) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "Bytes".to_string(),
			value: encode(bytes),
		}),
		ScVal::String(s) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "String".to_string(),
			value: s.to_string(),
		}),
		ScVal::Symbol(s) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "Symbol".to_string(),
			value: s.to_string(),
		}),
		ScVal::Vec(Some(vec)) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "Vec".to_string(),
			value: serde_json::to_string(&vec).unwrap_or_default(),
		}),
		ScVal::Map(Some(map)) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "Map".to_string(),
			value: serde_json::to_string(&map).unwrap_or_default(),
		}),
		ScVal::Address(addr) => Some(StellarDecodedParamEntry {
			indexed,
			kind: "Address".to_string(),
			value: match addr {
				ScAddress::Contract(hash) => Contract(hash.0).to_string(),
				ScAddress::Account(account_id) => match account_id {
					AccountId(PublicKey::PublicKeyTypeEd25519(key)) => {
						StrkeyPublicKey(key.0).to_string()
					}
				},
			},
		}),
		_ => None,
	}
}

/// Parses XDR-encoded bytes into a decoded parameter entry.
///
/// # Arguments
/// * `bytes` - The XDR-encoded bytes to parse
/// * `indexed` - Whether this parameter is indexed
///
/// # Returns
/// An Option containing the decoded parameter entry if successful
pub fn parse_xdr_value(bytes: &[u8], indexed: bool) -> Option<StellarDecodedParamEntry> {
	match ScVal::from_xdr(bytes, Limits::none()) {
		Ok(scval) => {
			let value = StellarValue::from(scval);
			Some(value.to_param_entry(indexed))
		}
		Err(e) => {
			tracing::debug!("Failed to parse XDR bytes: {}", e);
			None
		}
	}
}

/// Get the kind of a value from a JSON value.
///
/// # Arguments
/// * `value` - The JSON value to get the kind for
///
/// # Returns
/// A string representing the kind of the value
pub fn get_kind_from_value(value: &Value) -> String {
	match value {
		Value::Number(n) => {
			if n.is_u64() {
				"U64".to_string()
			} else if n.is_i64() {
				"I64".to_string()
			} else if n.is_f64() {
				"F64".to_string()
			} else {
				"I64".to_string() // fallback
			}
		}
		Value::Bool(_) => "Bool".to_string(),
		Value::String(s) => {
			if is_address(s) {
				"Address".to_string()
			} else {
				"String".to_string()
			}
		}
		Value::Array(_) => "Vec".to_string(),
		Value::Object(_) => "Map".to_string(),
		Value::Null => "Null".to_string(),
	}
}

/// Creates a LedgerKey for the contract instance.
///
/// # Arguments
/// * `contract_id` - The contract ID in Stellar strkey format (starts with 'C')
///
/// # Returns
/// A Result containing the LedgerKey if successful, or an error if the contract ID is invalid
pub fn get_contract_instance_ledger_key(contract_id: &str) -> Result<LedgerKey, anyhow::Error> {
	let contract_id = contract_id.to_uppercase();
	let contract_address = match Contract::from_string(contract_id.as_str()) {
		Ok(contract) => ScAddress::Contract(Hash(contract.0)),
		Err(err) => {
			return Err(anyhow::anyhow!("Failed to decode contract ID: {}", err));
		}
	};

	Ok(LedgerKey::ContractData(
		stellar_xdr::curr::LedgerKeyContractData {
			contract: contract_address,
			key: ScVal::LedgerKeyContractInstance,
			durability: stellar_xdr::curr::ContractDataDurability::Persistent,
		},
	))
}

/// Extracts contract code ledger key from a contract's XDR-encoded executable.
///
/// # Arguments
/// * `wasm_hash` - The WASM hash of the contract code
///
/// # Returns
/// A Result containing the LedgerKey if successful, or an error if the hash is invalid
pub fn get_contract_code_ledger_key(wasm_hash: &str) -> Result<LedgerKey, anyhow::Error> {
	Ok(LedgerKey::ContractCode(LedgerKeyContractCode {
		hash: wasm_hash.parse::<Hash>()?,
	}))
}

/// Get WASM code from a contract's XDR-encoded executable.
///
/// # Arguments
/// * `ledger_entry_data` - The XDR-encoded contract data
///
/// # Returns
/// A Result containing the WASM code as a hex string if successful, or an error if parsing fails
pub fn get_wasm_code_from_ledger_entry_data(
	ledger_entry_data: &str,
) -> Result<String, anyhow::Error> {
	let val = match LedgerEntryData::from_xdr_base64(ledger_entry_data.as_bytes(), Limits::none()) {
		Ok(val) => val,
		Err(e) => {
			return Err(anyhow::anyhow!("Failed to parse contract data XDR: {}", e));
		}
	};

	if let LedgerEntryData::ContractCode(data) = val {
		Ok(hex::encode(data.code))
	} else {
		Err(anyhow::anyhow!("XDR value is not a contract code entry"))
	}
}

/// Get WASM hash from a contract's XDR-encoded executable.
///
/// # Arguments
/// * `ledger_entry_data` - The XDR-encoded contract data
///
/// # Returns
/// A Result containing the WASM hash as a hex string if successful, or an error if parsing fails
pub fn get_wasm_hash_from_ledger_entry_data(
	ledger_entry_data: &str,
) -> Result<String, anyhow::Error> {
	let val = match LedgerEntryData::from_xdr_base64(ledger_entry_data.as_bytes(), Limits::none()) {
		Ok(val) => val,
		Err(e) => {
			return Err(anyhow::anyhow!("Failed to parse contract data XDR: {}", e));
		}
	};

	if let LedgerEntryData::ContractData(data) = val {
		if let ScVal::ContractInstance(instance) = data.val {
			if let ContractExecutable::Wasm(wasm) = instance.executable {
				Ok(hex::encode(wasm.0))
			} else {
				Err(anyhow::anyhow!("Contract executable is not WASM"))
			}
		} else {
			Err(anyhow::anyhow!("XDR value is not a contract instance"))
		}
	} else {
		Err(anyhow::anyhow!("XDR value is not a contract data entry"))
	}
}

/// Convert a hexadecimal string to a byte vector.
///
/// # Arguments
/// * `hex_string` - The hex string to convert
///
/// # Returns
/// A Result containing the byte vector if successful, or a hex::FromHexError if conversion fails
pub fn hex_to_bytes(hex_string: &str) -> Result<Vec<u8>, hex::FromHexError> {
	hex::decode(hex_string)
}

/// Parse a WASM contract from hex and return a vector of ScSpecEntry.
///
/// # Arguments
/// * `wasm_hex` - The hex-encoded WASM contract
///
/// # Returns
/// A Result containing a vector of ScSpecEntry if successful, or an error if parsing fails
pub fn get_contract_spec(wasm_hex: &str) -> Result<Vec<ScSpecEntry>, anyhow::Error> {
	match hex_to_bytes(wasm_hex) {
		Ok(wasm_bytes) => match read::from_wasm(&wasm_bytes) {
			Ok(spec) => Ok(spec),
			Err(e) => Err(anyhow::anyhow!("Failed to parse contract spec: {}", e)),
		},
		Err(e) => Err(anyhow::anyhow!("Failed to decode hex: {}", e)),
	}
}

/// Get contract spec functions from a contract spec.
///
/// # Arguments
/// * `spec_entries` - Vector of contract spec entries
///
/// # Returns
/// A vector of contract spec entries which are functions
pub fn get_contract_spec_functions(spec_entries: Vec<ScSpecEntry>) -> Vec<ScSpecEntry> {
	spec_entries
		.into_iter()
		.filter_map(|entry| match entry {
			ScSpecEntry::FunctionV0(func) => Some(ScSpecEntry::FunctionV0(func)),
			_ => None,
		})
		.collect()
}

/// Parse contract spec functions and populate input parameters.
///
/// # Arguments
/// * `spec_entries` - Vector of contract spec entries
///
/// # Returns
/// A vector of StellarContractFunction with populated input parameters
pub fn get_contract_spec_with_function_input_parameters(
	spec_entries: Vec<ScSpecEntry>,
) -> Vec<StellarContractFunction> {
	spec_entries
		.into_iter()
		.filter_map(|entry| match entry.clone() {
			ScSpecEntry::FunctionV0(func) => Some(StellarContractFunction {
				name: func.name.to_string(),
				signature: get_function_signature_from_spec_entry(&entry),
				inputs: func
					.inputs
					.iter()
					.enumerate()
					.map(|(index, input)| StellarContractInput {
						index: index as u32,
						name: input.name.to_string(),
						kind: StellarType::from(input.type_.clone()).to_string(),
					})
					.collect(),
			}),
			_ => None,
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;
	use std::str::FromStr;
	use stellar_xdr::curr::{
		AccountId, ContractDataEntry, Hash, Int128Parts, LedgerEntryData, PublicKey,
		ScContractInstance, ScMap, ScSpecEntry, ScSpecFunctionInputV0, ScSpecFunctionV0,
		ScSpecTypeDef, ScSpecTypeMap, ScSpecTypeOption, ScSpecTypeTuple, ScSpecTypeUdt,
		ScSpecTypeVec, ScSpecUdtEnumV0, ScString, ScSymbol, ScVal, SequenceNumber, String32,
		StringM, Uint256, WriteXdr,
	};

	fn create_test_function_entry(
		name: &str,
		inputs: Vec<(u32, &str, ScSpecTypeDef)>,
	) -> ScSpecEntry {
		ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
			doc: StringM::<1024>::from_str("").unwrap(),
			name: ScSymbol(StringM::<32>::from_str(name).unwrap()),
			inputs: inputs
				.into_iter()
				.map(|(_, name, type_)| ScSpecFunctionInputV0 {
					doc: StringM::<1024>::from_str("").unwrap(),
					name: StringM::<30>::from_str(name).unwrap(),
					type_,
				})
				.collect::<Vec<_>>()
				.try_into()
				.unwrap(),
			outputs: vec![].try_into().unwrap(),
		})
	}

	#[test]
	fn test_combine_number_functions() {
		// Test U256
		let u256 = UInt256Parts {
			hi_hi: 1,
			hi_lo: 2,
			lo_hi: 3,
			lo_lo: 4,
		};
		assert_eq!(
			combine_u256(&u256),
			"6277101735386680764516354157049543343084444891548699590660"
		);

		// Test I256
		let i256 = Int256Parts {
			hi_hi: 1,
			hi_lo: 2,
			lo_hi: 3,
			lo_lo: 4,
		};
		assert_eq!(
			combine_i256(&i256),
			"6277101735386680764516354157049543343084444891548699590660"
		);

		// Test U128
		let u128 = UInt128Parts { hi: 1, lo: 2 };
		assert_eq!(combine_u128(&u128), "18446744073709551618");

		// Test I128
		let i128 = Int128Parts { hi: 1, lo: 2 };
		assert_eq!(combine_i128(&i128), "18446744073709551618");
	}

	#[test]
	fn test_get_function_signature() {
		let function_name: String = "test_function".into();
		let args = vec![
			ScVal::I32(1),
			ScVal::String(ScString("test".try_into().unwrap())),
			ScVal::Bool(true),
		];
		let invoke_op = InvokeHostFunctionOp {
			host_function: HostFunction::InvokeContract(stellar_xdr::curr::InvokeContractArgs {
				contract_address: ScAddress::Contract(Hash([0; 32])),
				function_name: function_name.clone().try_into().unwrap(),
				args: args.try_into().unwrap(),
			}),
			auth: vec![].try_into().unwrap(),
		};

		assert_eq!(
			get_function_signature(&invoke_op, None),
			"test_function(I32,String,Bool)"
		);
	}

	#[test]
	fn test_address_functions() {
		// Test address validation
		let valid_ed25519 = "GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI";
		let valid_contract = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
		let invalid_address = "invalid_address";

		assert!(is_address(valid_ed25519));
		assert!(is_address(valid_contract));
		assert!(!is_address(invalid_address));

		// Test address comparison
		assert!(are_same_address(
			"GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI",
			"gbzxn7pirzgnmhga7muuuf4gwpy5aypv6ly4uv2gl6vjgiqrxfdnmadi"
		));
		assert!(!are_same_address(valid_ed25519, valid_contract));

		// Test address normalization
		assert_eq!(
			normalize_address(" GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI "),
			"gbzxn7pirzgnmhga7muuuf4gwpy5aypv6ly4uv2gl6vjgiqrxfdnmadi"
		);
	}

	#[test]
	fn test_signature_functions() {
		// Test signature comparison
		assert!(are_same_signature(
			"test_function(int32)",
			"test_function( int32 )"
		));
		assert!(!are_same_signature(
			"test_function(int32)",
			"test_function(int64)"
		));

		// Test signature normalization
		assert_eq!(
			normalize_signature(" test_function( int32 ) "),
			"test_function(int32)"
		);
	}

	#[test]
	fn test_parse_sc_val() {
		// Test basic types
		let bool_val = parse_sc_val(&ScVal::Bool(true), false).unwrap();
		assert_eq!(bool_val.kind, "Bool");
		assert_eq!(bool_val.value, "true");

		let int_val = parse_sc_val(&ScVal::I32(-42), true).unwrap();
		assert_eq!(int_val.kind, "I32");
		assert_eq!(int_val.value, "-42");
		assert!(int_val.indexed);

		// Test complex types
		let bytes_val =
			parse_sc_val(&ScVal::Bytes(vec![1, 2, 3].try_into().unwrap()), false).unwrap();
		assert_eq!(bytes_val.kind, "Bytes");
		assert_eq!(bytes_val.value, "010203");

		let string_val =
			parse_sc_val(&ScVal::String(ScString("test".try_into().unwrap())), false).unwrap();
		assert_eq!(string_val.kind, "String");
		assert_eq!(string_val.value, "test");
	}

	#[test]
	fn test_get_kind_from_value() {
		assert_eq!(get_kind_from_value(&json!(-42)), "I64");
		assert_eq!(get_kind_from_value(&json!(42)), "U64");
		assert_eq!(get_kind_from_value(&json!(42.5)), "F64");
		assert_eq!(get_kind_from_value(&json!(true)), "Bool");
		assert_eq!(get_kind_from_value(&json!("test")), "String");
		assert_eq!(
			get_kind_from_value(&json!(
				"GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI"
			)),
			"Address"
		);
		assert_eq!(get_kind_from_value(&json!([1, 2, 3])), "Vec");
		assert_eq!(get_kind_from_value(&json!({"key": "value"})), "Map");
		assert_eq!(get_kind_from_value(&json!(null)), "Null");
	}

	#[test]
	fn test_get_contract_instance_ledger_key() {
		// Test valid contract ID
		let contract_id = "CA6PUJLBYKZKUEKLZJMKBZLEKP2OTHANDEOWSFF44FTSYLKQPIICCJBE";
		let ledger_key = get_contract_instance_ledger_key(contract_id);
		assert!(ledger_key.is_ok());

		match ledger_key.unwrap() {
			LedgerKey::ContractData(data) => {
				assert_eq!(data.contract.to_string(), contract_id);
				assert!(matches!(data.key, ScVal::LedgerKeyContractInstance));
				assert_eq!(
					data.durability,
					stellar_xdr::curr::ContractDataDurability::Persistent
				);
			}
			_ => panic!("Expected LedgerKey::ContractData, got something else"),
		}

		// Test invalid contract ID
		let invalid_contract_id = "invalid_contract_id";
		let result = get_contract_instance_ledger_key(invalid_contract_id);
		assert!(result.is_err());
	}

	#[test]
	fn test_get_contract_code_ledger_key() {
		// Test valid WASM hash
		let wasm_hash = "b54ba37b7bb7dd69a7759caa9eec70e9e13615ba3b009fc23c4626ae9dffa27f";
		let ledger_key = get_contract_code_ledger_key(wasm_hash);
		assert!(ledger_key.is_ok());

		match ledger_key.unwrap() {
			LedgerKey::ContractCode(data) => {
				assert_eq!(hex::encode(data.hash.0), wasm_hash);
			}
			_ => panic!("Expected LedgerKey::ContractCode, got something else"),
		}

		// Test invalid WASM hash
		let invalid_hash = "invalid_hash";
		let result = get_contract_code_ledger_key(invalid_hash);
		assert!(result.is_err());
	}

	#[test]
	fn test_get_wasm_code_from_ledger_entry_data() {
		// Test with valid contract code XDR
		let contract_code_xdr = "AAAABwAAAAEAAAAAAAAAAAAAAEAAAAAFAAAAAwAAAAAAAAAEAAAAAAAAAAAAAAAEAAAABQAAAAAK2r5DjlOc9ad6/YGX+OJcgiyi0nupnY4OMbgLdADJAwAAAkYAYXNtAQAAAAEVBGACfn4BfmADfn5+AX5gAAF+YAAAAhkEAWwBMAAAAWwBMQAAAWwBXwABAWwBOAAAAwYFAgIDAwMFAwEAEAYZA38BQYCAwAALfwBBgIDAAAt/AEGAgMAACwc1BQZtZW1vcnkCAAlpbmNyZW1lbnQABQFfAAgKX19kYXRhX2VuZAMBC19faGVhcF9iYXNlAwIKpAEFCgBCjrrQr4bUOQuFAQIBfwJ+QQAhAAJAAkACQBCEgICAACIBQgIQgICAgABCAVINACABQgIQgYCAgAAiAkL/AYNCBFINASACQiCIpyEACyAAQQFqIgBFDQEgASAArUIghkIEhCICQgIQgoCAgAAaQoSAgICgBkKEgICAwAwQg4CAgAAaIAIPCwALEIaAgIAAAAsJABCHgICAAAALAwAACwIACwBzDmNvbnRyYWN0c3BlY3YwAAAAAAAAAEBJbmNyZW1lbnQgaW5jcmVtZW50cyBhbiBpbnRlcm5hbCBjb3VudGVyLCBhbmQgcmV0dXJucyB0aGUgdmFsdWUuAAAACWluY3JlbWVudAAAAAAAAAAAAAABAAAABAAeEWNvbnRyYWN0ZW52bWV0YXYwAAAAAAAAABYAAAAAAG8OY29udHJhY3RtZXRhdjAAAAAAAAAABXJzdmVyAAAAAAAABjEuODYuMAAAAAAAAAAAAAhyc3Nka3ZlcgAAAC8yMi4wLjcjMjExNTY5YWE0OWM4ZDg5Njg3N2RmY2ExZjJlYjRmZTkwNzExMjFjOAAAAA==";
		let result = get_wasm_code_from_ledger_entry_data(contract_code_xdr);
		assert!(result.is_ok());
		assert!(!result.unwrap().is_empty());

		// Test with invalid XDR
		let invalid_xdr = "invalid_xdr";
		let result = get_wasm_code_from_ledger_entry_data(invalid_xdr);
		assert!(result.is_err());
	}

	#[test]
	fn test_get_wasm_hash_from_ledger_entry_data() {
		// Test with valid contract data XDR
		let contract_data_xdr = "AAAABgAAAAAAAAABPPolYcKyqhFLylig5WRT9OmcDRkdaRS84WcsLVB6ECEAAAAUAAAAAQAAABMAAAAAtUuje3u33WmndZyqnuxw6eE2Fbo7AJ/CPEYmrp3/on8AAAABAAAAGwAAABAAAAABAAAAAQAAAA8AAAAFQWRtaW4AAAAAAAASAAAAAAAAAAAr0oWKHrJeX0w1hthij/qKv7Is8fIcfOqCw8DE8hCv1AAAABAAAAABAAAAAQAAAA8AAAAgRW1BZG1pblRyYW5zZmVyT3duZXJzaGlwRGVhZGxpbmUAAAAFAAAAAAAAAAAAAAAQAAAAAQAAAAEAAAAPAAAADUVtUGF1c2VBZG1pbnMAAAAAAAAQAAAAAQAAAAEAAAASAAAAAAAAAAA8yszQGJL36+gDDefIc7OTiY9tpNcdW7wAwiDj7kD7igAAABAAAAABAAAAAQAAAA8AAAAORW1lcmdlbmN5QWRtaW4AAAAAABIAAAAAAAAAAI2fE7ENFLaHlc9iL3RcgwMgp2J1YxSKwGCukW/LD/GLAAAAEAAAAAEAAAABAAAADwAAAAtGZWVGcmFjdGlvbgAAAAADAAAACgAAABAAAAABAAAAAQAAAA8AAAAURnV0dXJlRW1lcmdlbmN5QWRtaW4AAAASAAAAAAAAAACNnxOxDRS2h5XPYi90XIMDIKdidWMUisBgrpFvyw/xiwAAABAAAAABAAAAAQAAAA8AAAAKRnV0dXJlV0FTTQAAAAAADQAAACC1S6N7e7fdaad1nKqe7HDp4TYVujsAn8I8Riaunf+ifwAAABAAAAABAAAAAQAAAA8AAAANSXNLaWxsZWRDbGFpbQAAAAAAAAAAAAAAAAAAEAAAAAEAAAABAAAADwAAAA9PcGVyYXRpb25zQWRtaW4AAAAAEgAAAAAAAAAAawffS4d6dcWLRYJMVrBe5Z7Er4qwuMl5py8UWBe2lQQAAAAQAAAAAQAAAAEAAAAPAAAACE9wZXJhdG9yAAAAEgAAAAAAAAAAr4UDYWd/ywvTsSRB0NRM2w7KoisPZcPb4fpZk+XD67QAAAAQAAAAAQAAAAEAAAAPAAAAClBhdXNlQWRtaW4AAAAAABIAAAAAAAAAADzAe929VHnCmayZRVHmn90SJaJYM9yQ/RXerE7FSrO8AAAAEAAAAAEAAAABAAAADwAAAAVQbGFuZQAAAAAAABIAAAABgBdpEMDtExocHiH9irvJRhjmZINGNLCz+nLu8EuXI4QAAAAQAAAAAQAAAAEAAAAPAAAAEFBvb2xSZXdhcmRDb25maWcAAAARAAAAAQAAAAIAAAAPAAAACmV4cGlyZWRfYXQAAAAAAAUAAAAAaBo0XQAAAA8AAAADdHBzAAAAAAkAAAAAAAAAAAAAAAABlybMAAAAEAAAAAEAAAABAAAADwAAAA5Qb29sUmV3YXJkRGF0YQAAAAAAEQAAAAEAAAAEAAAADwAAAAthY2N1bXVsYXRlZAAAAAAJAAAAAAAAAAAAAgE4bXnnJwAAAA8AAAAFYmxvY2sAAAAAAAAFAAAAAAAAJWIAAAAPAAAAB2NsYWltZWQAAAAACQAAAAAAAAAAAAFXq2yzyG0AAAAPAAAACWxhc3RfdGltZQAAAAAAAAUAAAAAaBn52gAAABAAAAABAAAAAQAAAA8AAAAIUmVzZXJ2ZUEAAAAJAAAAAAAAAAAAAB1oFMw4UgAAABAAAAABAAAAAQAAAA8AAAAIUmVzZXJ2ZUIAAAAJAAAAAAAAAAAAAAd4z/xMMwAAABAAAAABAAAAAQAAAA8AAAAPUmV3YXJkQm9vc3RGZWVkAAAAABIAAAABVCi4nfTpos57F0VW+/5+Krm6FIDOc/fmXYeO1cqQsvMAAAAQAAAAAQAAAAEAAAAPAAAAEFJld2FyZEJvb3N0VG9rZW4AAAASAAAAASIlZ96nAI13nWy5EBefhUlzbfGIhg7o/IbKOIDSY/gYAAAAEAAAAAEAAAABAAAADwAAAAtSZXdhcmRUb2tlbgAAAAASAAAAASiFL2jBmEiONG+xIS7VApBTdhzCT0UzkuNTmCAbCCXnAAAAEAAAAAEAAAABAAAADwAAAAZSb3V0ZXIAAAAAABIAAAABYDO0JQ5wTjFPsGSXPRhduSLK4L0nK6W/8ZqsVw8SrC8AAAAQAAAAAQAAAAEAAAAPAAAABlRva2VuQQAAAAAAEgAAAAEltPzYWa7C+mNIQ4xImzw8EMmLbSG+T9PLMMtolT75dwAAABAAAAABAAAAAQAAAA8AAAAGVG9rZW5CAAAAAAASAAAAAa3vzlmu5Slo92Bh1JTCUlt1ZZ+kKWpl9JnvKeVkd+SWAAAAEAAAAAEAAAABAAAADwAAAA9Ub2tlbkZ1dHVyZVdBU00AAAAADQAAACBZas6LhVQ2R4USghouDssClzsbrQpAV9xUH9DKTXzwNwAAABAAAAABAAAAAQAAAA8AAAAKVG9rZW5TaGFyZQAAAAAAEgAAAAEqpeMcjYsAxBrCOmmY11UUmCNpWA4zXZL6+xGf1/A59gAAABAAAAABAAAAAQAAAA8AAAALVG90YWxTaGFyZXMAAAAACQAAAAAAAAAAAAAN/kuKFPkAAAAQAAAAAQAAAAEAAAAPAAAAD1VwZ3JhZGVEZWFkbGluZQAAAAAFAAAAAAAAAAAAAAAQAAAAAQAAAAEAAAAPAAAADVdvcmtpbmdTdXBwbHkAAAAAAAAJAAAAAAAAAAAAAA9BrWpi/w==";
		let result = get_wasm_hash_from_ledger_entry_data(contract_data_xdr);
		assert!(result.is_ok());
		assert_eq!(
			result.unwrap(),
			"b54ba37b7bb7dd69a7759caa9eec70e9e13615ba3b009fc23c4626ae9dffa27f"
		);

		// Test with invalid XDR
		let invalid_xdr = "invalid_xdr";
		let result = get_wasm_hash_from_ledger_entry_data(invalid_xdr);
		assert!(result.is_err());
	}

	#[test]
	fn test_hex_to_bytes() {
		// Test valid hex string
		let hex_string = "48656c6c6f"; // "Hello" in hex
		let result = hex_to_bytes(hex_string);
		assert!(result.is_ok());
		assert_eq!(result.unwrap(), vec![72, 101, 108, 108, 111]);

		// Test invalid hex string
		let invalid_hex = "invalid";
		let result = hex_to_bytes(invalid_hex);
		assert!(result.is_err());
	}

	#[test]
	fn test_get_contract_spec() {
		// Test with valid WASM hex
		let wasm_hex = "0061736d0100000001150460027e7e017e60037e7e7e017e6000017e600000021904016c01300000016c01310000016c015f0001016c01380000030605020203030305030100100619037f01418080c0000b7f00418080c0000b7f00418080c0000b073505066d656d6f7279020009696e6372656d656e740005015f00080a5f5f646174615f656e6403010b5f5f686561705f6261736503020aa401050a00428ebad0af86d4390b850102017f027e41002100024002400240108480808000220142021080808080004201520d0020014202108180808000220242ff01834204520d012002422088a721000b200041016a2200450d0120012000ad422086420484220242021082808080001a4284808080a0064284808080c00c1083808080001a20020f0b000b108680808000000b0900108780808000000b0300000b02000b00730e636f6e74726163747370656376300000000000000040496e6372656d656e7420696e6372656d656e747320616e20696e7465726e616c20636f756e7465722c20616e642072657475726e73207468652076616c75652e00000009696e6372656d656e74000000000000000000000100000004001e11636f6e7472616374656e766d6574617630000000000000001600000000006f0e636f6e74726163746d65746176300000000000000005727376657200000000000006312e38362e3000000000000000000008727373646b7665720000002f32322e302e37233231313536396161343963386438393638373764666361316632656234666539303731313231633800";
		let result = get_contract_spec(wasm_hex);
		assert!(result.is_ok());
		assert!(!result.unwrap().is_empty());

		// Test with invalid WASM hex
		let invalid_hex = "invalid";
		let result = get_contract_spec(invalid_hex);
		assert!(result.is_err());
	}

	#[test]
	fn test_get_contract_spec_functions() {
		let spec_entries = vec![
			create_test_function_entry(
				"transfer",
				vec![
					(0, "to", ScSpecTypeDef::Address),
					(1, "amount", ScSpecTypeDef::U64),
				],
			),
			create_test_function_entry(
				"complexFunction",
				vec![
					(
						0,
						"addresses",
						ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
							element_type: Box::new(ScSpecTypeDef::Address),
						})),
					),
					(
						1,
						"data",
						ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
							key_type: Box::new(ScSpecTypeDef::String),
							value_type: Box::new(ScSpecTypeDef::U64),
						})),
					),
				],
			),
		];

		let result = get_contract_spec_functions(spec_entries);
		assert_eq!(result.len(), 2);
		assert!(matches!(result[0], ScSpecEntry::FunctionV0(_)));
		assert!(matches!(result[1], ScSpecEntry::FunctionV0(_)));

		// Unknown function
		let unknown_func = ScSpecEntry::UdtEnumV0(ScSpecUdtEnumV0 {
			doc: StringM::<1024>::from_str("unknown_function").unwrap(),
			lib: StringM::<80>::from_str("unknown_function").unwrap(),
			cases: vec![].try_into().unwrap(),
			name: StringM::<60>::from_str("unknown_function").unwrap(),
		});
		let result = get_contract_spec_functions(vec![unknown_func]);
		assert!(result.is_empty());
	}

	#[test]
	fn test_get_contract_spec_with_function_input_parameters() {
		let spec_entries = vec![
			create_test_function_entry(
				"simple_function",
				vec![
					(0, "param1", ScSpecTypeDef::U64),
					(1, "param2", ScSpecTypeDef::String),
				],
			),
			create_test_function_entry(
				"complex_function",
				vec![
					(
						0,
						"addresses",
						ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
							element_type: Box::new(ScSpecTypeDef::Address),
						})),
					),
					(
						1,
						"data",
						ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
							key_type: Box::new(ScSpecTypeDef::String),
							value_type: Box::new(ScSpecTypeDef::U64),
						})),
					),
				],
			),
		];

		let result = get_contract_spec_with_function_input_parameters(spec_entries);

		assert_eq!(result.len(), 2);

		// Check simple function
		let simple_func = &result[0];
		assert_eq!(simple_func.name, "simple_function");
		assert_eq!(simple_func.inputs.len(), 2);
		assert_eq!(simple_func.inputs[0].name, "param1");
		assert_eq!(simple_func.inputs[0].kind, "U64");
		assert_eq!(simple_func.inputs[1].name, "param2");
		assert_eq!(simple_func.inputs[1].kind, "String");

		// Check complex function
		let complex_func = &result[1];
		assert_eq!(complex_func.name, "complex_function");
		assert_eq!(complex_func.inputs.len(), 2);
		assert_eq!(complex_func.inputs[0].name, "addresses");
		assert_eq!(complex_func.inputs[0].kind, "Vec<Address>");
		assert_eq!(complex_func.inputs[1].name, "data");
		assert_eq!(complex_func.inputs[1].kind, "Map<String,U64>");

		// Unknown function
		let unknown_func = ScSpecEntry::UdtEnumV0(ScSpecUdtEnumV0 {
			doc: StringM::<1024>::from_str("unknown_function").unwrap(),
			lib: StringM::<80>::from_str("unknown_function").unwrap(),
			cases: vec![].try_into().unwrap(),
			name: StringM::<60>::from_str("unknown_function").unwrap(),
		});
		let result = get_contract_spec_with_function_input_parameters(vec![unknown_func]);
		assert!(result.is_empty());
	}

	#[test]
	fn test_get_wasm_code_from_ledger_entry_data_errors() {
		// Test non-contract code entry
		let non_code_entry = LedgerEntryData::Account(stellar_xdr::curr::AccountEntry {
			account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256::from([0; 32]))),
			balance: 0,
			seq_num: SequenceNumber(0),
			num_sub_entries: 0,
			inflation_dest: None,
			flags: 0,
			home_domain: String32::from(StringM::<32>::from_str("").unwrap()),
			thresholds: stellar_xdr::curr::Thresholds([0; 4]),
			signers: vec![].try_into().unwrap(),
			ext: stellar_xdr::curr::AccountEntryExt::V0,
		});
		let xdr = non_code_entry.to_xdr_base64(Limits::none()).unwrap();
		let result = get_wasm_code_from_ledger_entry_data(&xdr);
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("not a contract code entry"));
	}

	#[test]
	fn test_get_wasm_hash_from_ledger_entry_data_errors() {
		// Test non-contract data entry
		let non_data_entry = LedgerEntryData::Account(stellar_xdr::curr::AccountEntry {
			account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256::from([0; 32]))),
			balance: 0,
			seq_num: SequenceNumber(0),
			num_sub_entries: 0,
			inflation_dest: None,
			flags: 0,
			home_domain: String32::from(StringM::<32>::from_str("").unwrap()),
			thresholds: stellar_xdr::curr::Thresholds([0; 4]),
			signers: vec![].try_into().unwrap(),
			ext: stellar_xdr::curr::AccountEntryExt::V0,
		});
		let xdr = non_data_entry.to_xdr_base64(Limits::none()).unwrap();
		let result = get_wasm_hash_from_ledger_entry_data(&xdr);
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("not a contract data entry"));

		// Test non-contract instance
		let non_instance_data = LedgerEntryData::ContractData(ContractDataEntry {
			ext: stellar_xdr::curr::ExtensionPoint::V0,
			contract: ScAddress::Contract(Hash([0; 32])),
			key: ScVal::Bool(true),
			durability: stellar_xdr::curr::ContractDataDurability::Persistent,
			val: ScVal::Bool(true),
		});
		let xdr = non_instance_data.to_xdr_base64(Limits::none()).unwrap();
		let result = get_wasm_hash_from_ledger_entry_data(&xdr);
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("not a contract instance"));

		// Test non-WASM executable
		let non_wasm_instance = LedgerEntryData::ContractData(ContractDataEntry {
			ext: stellar_xdr::curr::ExtensionPoint::V0,
			contract: ScAddress::Contract(Hash([0; 32])),
			key: ScVal::LedgerKeyContractInstance,
			durability: stellar_xdr::curr::ContractDataDurability::Persistent,
			val: ScVal::ContractInstance(ScContractInstance {
				executable: ContractExecutable::StellarAsset,
				storage: Some(ScMap(vec![].try_into().unwrap())),
			}),
		});
		let xdr = non_wasm_instance.to_xdr_base64(Limits::none()).unwrap();
		let result = get_wasm_hash_from_ledger_entry_data(&xdr);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("not WASM"));
	}

	#[test]
	fn test_get_contract_spec_errors() {
		// Test invalid WASM hex
		let invalid_hex = "invalid_hex";
		let result = get_contract_spec(invalid_hex);
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Failed to decode hex"));

		// Test invalid WASM format
		let invalid_wasm = "0000000000000000000000000000000000000000000000000000000000000000";
		let result = get_contract_spec(invalid_wasm);
		assert!(result.is_err());
		assert!(result
			.unwrap_err()
			.to_string()
			.contains("Failed to parse contract spec"));
	}

	#[test]
	fn test_get_type() {
		assert_eq!(StellarValue::Bool(true).get_type(), StellarType::Bool);
		assert_eq!(StellarValue::Void.get_type(), StellarType::Void);
		assert_eq!(StellarValue::U32(42).get_type(), StellarType::U32);
		assert_eq!(StellarValue::I32(-42).get_type(), StellarType::I32);
		assert_eq!(StellarValue::U64(42).get_type(), StellarType::U64);
		assert_eq!(StellarValue::I64(-42).get_type(), StellarType::I64);
		assert_eq!(
			StellarValue::U128("42".to_string()).get_type(),
			StellarType::U128
		);
		assert_eq!(
			StellarValue::I128("-42".to_string()).get_type(),
			StellarType::I128
		);
		assert_eq!(
			StellarValue::U256("42".to_string()).get_type(),
			StellarType::U256
		);
		assert_eq!(
			StellarValue::I256("-42".to_string()).get_type(),
			StellarType::I256
		);
		assert_eq!(
			StellarValue::Bytes(vec![]).get_type(),
			StellarType::Bytes(None)
		);
		// Test Bytes32
		let bytes32 = vec![1; 32];
		assert_eq!(
			StellarValue::Bytes(bytes32).get_type(),
			StellarType::Bytes(Some(32))
		);
		assert_eq!(
			StellarValue::String("test".to_string()).get_type(),
			StellarType::String
		);
		assert_eq!(
			StellarValue::Symbol("test".to_string()).get_type(),
			StellarType::Symbol
		);
		assert_eq!(
			StellarValue::Vec(vec![StellarValue::U32(42)]).get_type(),
			StellarType::Vec(Box::new(StellarType::U32))
		);
		assert_eq!(
			StellarValue::Map(BTreeMap::new()).get_type(),
			StellarType::Map(Box::new(StellarType::String), Box::new(StellarType::Void))
		);
		assert_eq!(
			StellarValue::Tuple(vec![StellarValue::U32(42)]).get_type(),
			StellarType::Tuple(Box::new(StellarType::Sequence(vec![StellarType::U32])))
		);
		assert_eq!(
			StellarValue::Address("test".to_string()).get_type(),
			StellarType::Address
		);
		assert_eq!(
			StellarValue::Timepoint(42).get_type(),
			StellarType::Timepoint
		);
		assert_eq!(StellarValue::Duration(42).get_type(), StellarType::Duration);

		// Test nested complex types
		let nested_tuple = StellarValue::Tuple(vec![
			StellarValue::Tuple(vec![StellarValue::U32(42)]),
			StellarValue::Vec(vec![StellarValue::String("test".to_string())]),
		]);
		assert_eq!(
			nested_tuple.get_type(),
			StellarType::Tuple(Box::new(StellarType::Sequence(vec![
				StellarType::Tuple(Box::new(StellarType::Sequence(vec![StellarType::U32]))),
				StellarType::Vec(Box::new(StellarType::String)),
			])))
		);

		let tuple_with_map = StellarValue::Tuple(vec![
			StellarValue::Map({
				let mut map = BTreeMap::new();
				map.insert("key".to_string(), StellarValue::U32(42));
				map
			}),
			StellarValue::U64(123),
		]);

		assert_eq!(
			tuple_with_map.get_type(),
			StellarType::Tuple(Box::new(StellarType::Sequence(vec![
				StellarType::Map(Box::new(StellarType::String), Box::new(StellarType::U32)),
				StellarType::U64,
			])))
		);

		assert_eq!(
			StellarValue::Address("test".to_string()).to_string(),
			"test"
		);
		assert_eq!(StellarValue::Timepoint(42).to_string(), "42");
		assert_eq!(StellarValue::Duration(42).to_string(), "42");

		// Test Udt
		assert_eq!(
			StellarValue::Udt("Request".to_string()).to_string(),
			"Request"
		);
	}

	#[test]
	fn test_stellar_type_display() {
		assert_eq!(StellarType::Bool.to_string(), "Bool");
		assert_eq!(StellarType::Void.to_string(), "Void");
		assert_eq!(StellarType::U32.to_string(), "U32");
		assert_eq!(StellarType::I32.to_string(), "I32");
		assert_eq!(StellarType::U64.to_string(), "U64");
		assert_eq!(StellarType::I64.to_string(), "I64");
		assert_eq!(StellarType::U128.to_string(), "U128");
		assert_eq!(StellarType::I128.to_string(), "I128");
		assert_eq!(StellarType::U256.to_string(), "U256");
		assert_eq!(StellarType::I256.to_string(), "I256");
		assert_eq!(StellarType::Bytes(None).to_string(), "Bytes");
		assert_eq!(StellarType::Bytes(Some(32)).to_string(), "Bytes32");
		assert_eq!(StellarType::String.to_string(), "String");
		assert_eq!(StellarType::Symbol.to_string(), "Symbol");
		assert_eq!(
			StellarType::Vec(Box::new(StellarType::U32)).to_string(),
			"Vec<U32>"
		);
		assert_eq!(
			StellarType::Map(Box::new(StellarType::String), Box::new(StellarType::U32)).to_string(),
			"Map<String,U32>"
		);
		assert_eq!(
			StellarType::Tuple(Box::new(StellarType::U32)).to_string(),
			"Tuple<U32>"
		);
		assert_eq!(StellarType::Address.to_string(), "Address");
		assert_eq!(StellarType::Timepoint.to_string(), "Timepoint");
		assert_eq!(StellarType::Duration.to_string(), "Duration");

		// Test nested complex types
		assert_eq!(
			StellarType::Vec(Box::new(StellarType::Vec(Box::new(StellarType::U32)))).to_string(),
			"Vec<Vec<U32>>"
		);
		assert_eq!(
			StellarType::Map(
				Box::new(StellarType::String),
				Box::new(StellarType::Map(
					Box::new(StellarType::String),
					Box::new(StellarType::U32)
				))
			)
			.to_string(),
			"Map<String,Map<String,U32>>"
		);
		assert_eq!(
			StellarType::Tuple(Box::new(StellarType::Tuple(Box::new(StellarType::U32))))
				.to_string(),
			"Tuple<Tuple<U32>>"
		);
	}

	#[test]
	fn test_stellar_value_to_string() {
		assert_eq!(StellarValue::Bool(true).to_string(), "true");
		assert_eq!(StellarValue::Void.to_string(), "null");
		assert_eq!(StellarValue::U32(42).to_string(), "42");
		assert_eq!(StellarValue::I32(-42).to_string(), "-42");
		assert_eq!(StellarValue::U64(42).to_string(), "42");
		assert_eq!(StellarValue::I64(-42).to_string(), "-42");
		assert_eq!(StellarValue::U128("42".to_string()).to_string(), "42");
		assert_eq!(StellarValue::I128("-42".to_string()).to_string(), "-42");
		assert_eq!(StellarValue::U256("42".to_string()).to_string(), "42");
		assert_eq!(StellarValue::I256("-42".to_string()).to_string(), "-42");
		assert_eq!(StellarValue::Bytes(vec![1, 2, 3]).to_string(), "010203");
		assert_eq!(StellarValue::String("test".to_string()).to_string(), "test");
		assert_eq!(StellarValue::Symbol("test".to_string()).to_string(), "test");
		assert_eq!(
			StellarValue::Vec(vec![StellarValue::U32(42)]).to_string(),
			"[42]"
		);
		assert_eq!(
			StellarValue::Map({
				let mut map = BTreeMap::new();
				map.insert("key".to_string(), StellarValue::U32(42));
				map
			})
			.to_string(),
			"{key:42}"
		);
		assert_eq!(
			StellarValue::Tuple(vec![StellarValue::U32(42)]).to_string(),
			"(42)"
		);
		assert_eq!(
			StellarValue::Address("test".to_string()).to_string(),
			"test"
		);
		assert_eq!(StellarValue::Timepoint(42).to_string(), "42");
		assert_eq!(StellarValue::Duration(42).to_string(), "42");

		// Test nested complex values
		assert_eq!(
			StellarValue::Vec(vec![
				StellarValue::Vec(vec![StellarValue::U32(42), StellarValue::U32(43)]),
				StellarValue::Vec(vec![StellarValue::String("test".to_string())])
			])
			.to_string(),
			"[[42,43],[test]]"
		);

		assert_eq!(
			StellarValue::Map({
				let mut map = BTreeMap::new();
				map.insert(
					"nested".to_string(),
					StellarValue::Map({
						let mut inner_map = BTreeMap::new();
						inner_map.insert("value".to_string(), StellarValue::U32(42));
						inner_map
					}),
				);
				map
			})
			.to_string(),
			"{nested:{value:42}}"
		);

		assert_eq!(
			StellarValue::Tuple(vec![
				StellarValue::Tuple(vec![StellarValue::U32(42), StellarValue::U32(43)]),
				StellarValue::Vec(vec![StellarValue::String("test".to_string())])
			])
			.to_string(),
			"((42,43),[test])"
		);
	}

	#[test]
	fn test_stellar_value_to_json() {
		assert_eq!(StellarValue::Bool(true).to_json(), json!(true));
		assert_eq!(StellarValue::Void.to_json(), json!(null));
		assert_eq!(StellarValue::U32(42).to_json(), json!(42));
		assert_eq!(StellarValue::I32(-42).to_json(), json!(-42));
		assert_eq!(StellarValue::U64(42).to_json(), json!(42));
		assert_eq!(StellarValue::I64(-42).to_json(), json!(-42));
		assert_eq!(
			StellarValue::U128("42".to_string()).to_json(),
			json!({"type": "U128", "value": "42"})
		);
		assert_eq!(
			StellarValue::I128("-42".to_string()).to_json(),
			json!({"type": "I128", "value": "-42"})
		);
		assert_eq!(
			StellarValue::U256("42".to_string()).to_json(),
			json!({"type": "U256", "value": "42"})
		);
		assert_eq!(
			StellarValue::I256("-42".to_string()).to_json(),
			json!({"type": "I256", "value": "-42"})
		);
		assert_eq!(
			StellarValue::Bytes(vec![1, 2, 3]).to_json(),
			json!("010203")
		);
		assert_eq!(
			StellarValue::String("test".to_string()).to_json(),
			json!("test")
		);
		assert_eq!(
			StellarValue::Symbol("test".to_string()).to_json(),
			json!("test")
		);
		assert_eq!(
			StellarValue::Vec(vec![StellarValue::U32(42)]).to_json(),
			json!([42])
		);
		assert_eq!(
			StellarValue::Map({
				let mut map = BTreeMap::new();
				map.insert("key".to_string(), StellarValue::U32(42));
				map
			})
			.to_json(),
			json!({"key": 42})
		);
		assert_eq!(
			StellarValue::Tuple(vec![StellarValue::U32(42)]).to_json(),
			json!([42])
		);
		assert_eq!(
			StellarValue::Address("test".to_string()).to_json(),
			json!("test")
		);
		assert_eq!(StellarValue::Timepoint(42).to_json(), json!(42));
		assert_eq!(StellarValue::Duration(42).to_json(), json!(42));
		assert_eq!(
			StellarValue::Udt("Request".to_string()).to_json(),
			json!("Request")
		);

		// Test nested complex values
		assert_eq!(
			StellarValue::Vec(vec![
				StellarValue::Vec(vec![StellarValue::U32(42)]),
				StellarValue::Vec(vec![StellarValue::String("test".to_string())])
			])
			.to_json(),
			json!([[42], ["test"]])
		);

		assert_eq!(
			StellarValue::Map({
				let mut map = BTreeMap::new();
				map.insert(
					"nested".to_string(),
					StellarValue::Map({
						let mut inner_map = BTreeMap::new();
						inner_map.insert("value".to_string(), StellarValue::U32(42));
						inner_map
					}),
				);
				map
			})
			.to_json(),
			json!({"nested": {"value": 42}})
		);

		assert_eq!(
			StellarValue::Tuple(vec![
				StellarValue::Tuple(vec![StellarValue::U32(42), StellarValue::U32(43)]),
				StellarValue::Vec(vec![StellarValue::String("test".to_string())])
			])
			.to_json(),
			json!([[42, 43], ["test"]])
		);
	}

	#[test]
	fn test_udt_types() {
		// Test simple UDT
		let udt = ScSpecTypeDef::Udt(ScSpecTypeUdt {
			name: StringM::<60>::from_str("Request").unwrap(),
		});
		assert_eq!(StellarType::from(udt).to_string(), "Request");

		// Test Vec of UDT
		let vec_udt = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
			element_type: Box::new(ScSpecTypeDef::Udt(ScSpecTypeUdt {
				name: StringM::<60>::from_str("Request").unwrap(),
			})),
		}));
		assert_eq!(StellarType::from(vec_udt).to_string(), "Vec<Request>");

		// Test Map with UDT value
		let map_udt = ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
			key_type: Box::new(ScSpecTypeDef::String),
			value_type: Box::new(ScSpecTypeDef::Udt(ScSpecTypeUdt {
				name: StringM::<60>::from_str("Request").unwrap(),
			})),
		}));
		assert_eq!(
			StellarType::from(map_udt).to_string(),
			"Map<String,Request>"
		);

		// Test Tuple with UDT
		let tuple_udt = ScSpecTypeDef::Tuple(Box::new(ScSpecTypeTuple {
			value_types: vec![
				ScSpecTypeDef::Udt(ScSpecTypeUdt {
					name: StringM::<60>::from_str("Request").unwrap(),
				}),
				ScSpecTypeDef::U64,
			]
			.try_into()
			.unwrap(),
		}));
		assert_eq!(
			StellarType::from(tuple_udt).to_string(),
			"Tuple<Request,U64>"
		);

		// Test nested UDT in Vec
		let nested_vec_udt = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
			element_type: Box::new(ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
				element_type: Box::new(ScSpecTypeDef::Udt(ScSpecTypeUdt {
					name: StringM::<60>::from_str("Request").unwrap(),
				})),
			}))),
		}));
		assert_eq!(
			StellarType::from(nested_vec_udt).to_string(),
			"Vec<Vec<Request>>"
		);
	}

	#[test]
	fn test_get_function_signature_from_spec_entry() {
		// Test simple function with basic types
		let simple_func = create_test_function_entry(
			"simple_function",
			vec![
				(0, "param1", ScSpecTypeDef::U64),
				(1, "param2", ScSpecTypeDef::String),
			],
		);
		assert_eq!(
			get_function_signature_from_spec_entry(&simple_func),
			"simple_function(U64,String)"
		);

		// Unknown function
		let unknown_func = ScSpecEntry::UdtEnumV0(ScSpecUdtEnumV0 {
			doc: StringM::<1024>::from_str("unknown_function").unwrap(),
			lib: StringM::<80>::from_str("unknown_function").unwrap(),
			cases: vec![].try_into().unwrap(),
			name: StringM::<60>::from_str("unknown_function").unwrap(),
		});
		assert_eq!(
			get_function_signature_from_spec_entry(&unknown_func),
			"unknown_function()"
		);

		// Test function with UDT
		let udt_func = create_test_function_entry(
			"udt_function",
			vec![(
				0,
				"request",
				ScSpecTypeDef::Udt(ScSpecTypeUdt {
					name: StringM::<60>::from_str("Request").unwrap(),
				}),
			)],
		);
		assert_eq!(
			get_function_signature_from_spec_entry(&udt_func),
			"udt_function(Request)"
		);

		// Unknown function
		let unknown_func = ScSpecEntry::UdtEnumV0(ScSpecUdtEnumV0 {
			doc: StringM::<1024>::from_str("unknown_function").unwrap(),
			lib: StringM::<80>::from_str("unknown_function").unwrap(),
			cases: vec![].try_into().unwrap(),
			name: StringM::<60>::from_str("unknown_function").unwrap(),
		});
		assert_eq!(
			get_function_signature_from_spec_entry(&unknown_func),
			"unknown_function()"
		);

		// Test function with complex types
		let complex_func = create_test_function_entry(
			"complex_function",
			vec![
				(
					0,
					"addresses",
					ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
						element_type: Box::new(ScSpecTypeDef::Address),
					})),
				),
				(
					1,
					"data",
					ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
						key_type: Box::new(ScSpecTypeDef::String),
						value_type: Box::new(ScSpecTypeDef::U64),
					})),
				),
			],
		);
		assert_eq!(
			get_function_signature_from_spec_entry(&complex_func),
			"complex_function(Vec<Address>,Map<String,U64>)"
		);

		// Test function with nested UDT
		let nested_udt_func = create_test_function_entry(
			"nested_udt_function",
			vec![(
				0,
				"requests",
				ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
					element_type: Box::new(ScSpecTypeDef::Udt(ScSpecTypeUdt {
						name: StringM::<60>::from_str("Request").unwrap(),
					})),
				})),
			)],
		);
		assert_eq!(
			get_function_signature_from_spec_entry(&nested_udt_func),
			"nested_udt_function(Vec<Request>)"
		);
	}

	#[test]
	fn test_udt_type_matching() {
		let function_name: String = "process_request".into();
		let args = vec![
			ScVal::Address(ScAddress::Contract(Hash([0; 32]))),
			ScVal::Vec(Some(
				vec![ScVal::Map(Some(ScMap(
					vec![
						ScMapEntry {
							key: ScVal::String(ScString("type".try_into().unwrap())),
							val: ScVal::String(ScString("transfer".try_into().unwrap())),
						},
						ScMapEntry {
							key: ScVal::String(ScString("amount".try_into().unwrap())),
							val: ScVal::I128(Int128Parts { hi: 0, lo: 1000 }),
						},
					]
					.try_into()
					.unwrap(),
				)))]
				.try_into()
				.unwrap(),
			)),
		];

		let invoke_op = InvokeHostFunctionOp {
			host_function: HostFunction::InvokeContract(stellar_xdr::curr::InvokeContractArgs {
				contract_address: ScAddress::Contract(Hash([0; 32])),
				function_name: function_name.clone().try_into().unwrap(),
				args: args.try_into().unwrap(),
			}),
			auth: vec![].try_into().unwrap(),
		};

		let contract_spec = StellarFormattedContractSpec {
			functions: vec![StellarContractFunction {
				name: "process_request".to_string(),
				signature: "process_request(Address,Vec<Request>)".to_string(),
				inputs: vec![
					StellarContractInput {
						name: "validator".to_string(),
						kind: "Address".to_string(),
						index: 0,
					},
					StellarContractInput {
						name: "requests".to_string(),
						kind: "Vec<Request>".to_string(),
						index: 1,
					},
				],
			}],
		};

		// Test that the UDT signature is returned even though runtime types are different
		assert_eq!(
			get_function_signature(&invoke_op, Some(&contract_spec)),
			"process_request(Address,Vec<Request>)"
		);

		// Test without contract spec - should show concrete types
		assert_eq!(
			get_function_signature(&invoke_op, None),
			"process_request(Address,Vec<Map<String,I128,String>>)"
		);
	}

	#[test]
	fn test_from_scval() {
		// Test basic types
		assert!(matches!(
			StellarValue::from(ScVal::Bool(true)),
			StellarValue::Bool(true)
		));
		assert!(matches!(
			StellarValue::from(ScVal::Void),
			StellarValue::Void
		));
		assert!(matches!(
			StellarValue::from(ScVal::U32(42)),
			StellarValue::U32(42)
		));
		assert!(matches!(
			StellarValue::from(ScVal::I32(-42)),
			StellarValue::I32(-42)
		));
		assert!(matches!(
			StellarValue::from(ScVal::U64(42)),
			StellarValue::U64(42)
		));
		assert!(matches!(
			StellarValue::from(ScVal::I64(-42)),
			StellarValue::I64(-42)
		));

		// Test Timepoint and Duration
		assert!(matches!(
			StellarValue::from(ScVal::Timepoint(stellar_xdr::curr::TimePoint(42))),
			StellarValue::Timepoint(42)
		));
		assert!(matches!(
			StellarValue::from(ScVal::Duration(stellar_xdr::curr::Duration(42))),
			StellarValue::Duration(42)
		));

		// Test large number types
		let u128_val = UInt128Parts { hi: 1, lo: 2 };
		assert!(matches!(
			StellarValue::from(ScVal::U128(u128_val)),
			StellarValue::U128(_)
		));

		let i128_val = Int128Parts { hi: 1, lo: 2 };
		assert!(matches!(
			StellarValue::from(ScVal::I128(i128_val)),
			StellarValue::I128(_)
		));

		let u256_val = UInt256Parts {
			hi_hi: 1,
			hi_lo: 2,
			lo_hi: 3,
			lo_lo: 4,
		};
		assert!(matches!(
			StellarValue::from(ScVal::U256(u256_val)),
			StellarValue::U256(_)
		));

		let i256_val = Int256Parts {
			hi_hi: 1,
			hi_lo: 2,
			lo_hi: 3,
			lo_lo: 4,
		};
		assert!(matches!(
			StellarValue::from(ScVal::I256(i256_val)),
			StellarValue::I256(_)
		));

		// Test complex types
		let bytes = vec![1, 2, 3].try_into().unwrap();
		assert!(matches!(
			StellarValue::from(ScVal::Bytes(bytes)),
			StellarValue::Bytes(_)
		));

		let string = ScString("test".try_into().unwrap());
		assert!(matches!(
			StellarValue::from(ScVal::String(string)),
			StellarValue::String(_)
		));

		let symbol = ScSymbol("test".try_into().unwrap());
		assert!(matches!(
			StellarValue::from(ScVal::Symbol(symbol)),
			StellarValue::Symbol(_)
		));

		// Test Vec
		let vec = vec![ScVal::U32(42)].try_into().unwrap();
		assert!(matches!(
			StellarValue::from(ScVal::Vec(Some(vec))),
			StellarValue::Vec(_)
		));

		// Test Map
		let map = ScMap(vec![].try_into().unwrap());
		assert!(matches!(
			StellarValue::from(ScVal::Map(Some(map))),
			StellarValue::Map(_)
		));

		// Test Address
		let contract_addr = ScAddress::Contract(Hash([0; 32]));
		assert!(matches!(
			StellarValue::from(ScVal::Address(contract_addr)),
			StellarValue::Address(_)
		));

		// Test Other types
		assert!(matches!(
			StellarValue::from(ScVal::LedgerKeyContractInstance),
			StellarValue::Void
		));
	}

	#[test]
	fn test_from_sc_spec_def_type() {
		// Test basic types
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Bool),
			StellarType::Bool
		));
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Void),
			StellarType::Void
		));
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::U32),
			StellarType::U32
		));
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::I32),
			StellarType::I32
		));
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::U64),
			StellarType::U64
		));
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::I64),
			StellarType::I64
		));

		// Test Timepoint and Duration
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Timepoint),
			StellarType::Timepoint
		));
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Duration),
			StellarType::Duration
		));

		// Test large number types
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::U128),
			StellarType::U128
		));

		assert!(matches!(
			StellarType::from(ScSpecTypeDef::I128),
			StellarType::I128
		));

		assert!(matches!(
			StellarType::from(ScSpecTypeDef::U256),
			StellarType::U256
		));

		assert!(matches!(
			StellarType::from(ScSpecTypeDef::I256),
			StellarType::I256
		));

		// Test complex types
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Bytes),
			StellarType::Bytes(_)
		));

		assert!(matches!(
			StellarType::from(ScSpecTypeDef::String),
			StellarType::String
		));

		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Symbol),
			StellarType::Symbol
		));

		// Test Vec
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
				element_type: Box::new(ScSpecTypeDef::U32),
			}))),
			StellarType::Vec(_)
		));

		// Test Map
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
				key_type: Box::new(ScSpecTypeDef::U32),
				value_type: Box::new(ScSpecTypeDef::U32),
			}))),
			StellarType::Map(_, _)
		));

		// Test Address
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Address),
			StellarType::Address
		));

		// Test Other types
		assert!(matches!(
			StellarType::from(ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
				value_type: Box::new(ScSpecTypeDef::Void),
			}))),
			StellarType::Void
		));
	}

	#[test]
	fn test_from_json_value() {
		// Test Number types
		assert!(matches!(StellarType::from(json!(42)), StellarType::U64));
		assert!(matches!(StellarType::from(json!(-42)), StellarType::I64));

		// Test Boolean
		assert!(matches!(StellarType::from(json!(true)), StellarType::Bool));

		// Test String types
		// Regular string
		assert!(matches!(
			StellarType::from(json!("hello")),
			StellarType::String
		));

		// Address string (Stellar address)
		assert!(matches!(
			StellarType::from(json!(
				"GBZXN7PIRZGNMHGA7MUUUF4GWPY5AYPV6LY4UV2GL6VJGIQRXFDNMADI"
			)),
			StellarType::Address
		));

		// Contract address
		assert!(matches!(
			StellarType::from(json!(
				"CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4"
			)),
			StellarType::Address
		));

		// Test Array
		assert!(matches!(
			StellarType::from(json!([1, 2, 3])),
			StellarType::Vec(_)
		));

		// Test Object
		assert!(matches!(
			StellarType::from(json!({"key": "value"})),
			StellarType::Map(_, _)
		));

		// Test Null
		assert!(matches!(StellarType::from(json!(null)), StellarType::Void));

		// Test nested structures
		let nested_array = json!([{"key": "value"}, [1, 2, 3]]);
		assert!(matches!(
			StellarType::from(nested_array),
			StellarType::Vec(_)
		));

		let nested_object = json!({
			"array": [1, 2, 3],
			"object": {"key": "value"}
		});
		assert!(matches!(
			StellarType::from(nested_object),
			StellarType::Map(_, _)
		));
	}
}
