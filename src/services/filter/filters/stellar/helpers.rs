//! Helper functions for Stellar-specific operations.
//!
//! This module provides utility functions for working with Stellar-specific data types
//! and formatting, including address normalization, XDR value parsing, and
//! operation processing.

use alloy::primitives::{I256, U256};
use hex::encode;
use serde_json::{json, Value};
// NOTE: this may be moved to stellar_xdr in the future
use soroban_spec::read;
use stellar_strkey::{ed25519::PublicKey as StrkeyPublicKey, Contract};
use stellar_xdr::curr::{
	AccountId, ContractExecutable, Hash, HostFunction, Int128Parts, Int256Parts,
	InvokeHostFunctionOp, LedgerEntryData, LedgerKey, LedgerKeyContractCode, Limits, PublicKey,
	ReadXdr, ScAddress, ScMap, ScMapEntry, ScSpecEntry, ScSpecTypeDef, ScVal, ScVec, UInt128Parts,
	UInt256Parts,
};

use crate::models::{
	StellarContractFunction, StellarContractInput, StellarDecodedParamEntry,
	StellarParsedOperationResult,
};

/// Combines the parts of a UInt256 into a single string representation.
///
/// # Arguments
/// * `n` - The UInt256Parts containing the 4 64-bit components
///
/// # Returns
/// A string representation of the combined 256-bit unsigned integer
fn combine_u256(n: &UInt256Parts) -> String {
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
fn combine_i256(n: &Int256Parts) -> String {
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
fn combine_u128(n: &UInt128Parts) -> String {
	(((n.hi as u128) << 64) | (n.lo as u128)).to_string()
}

/// Combines the parts of an Int128 into a single string representation.
///
/// # Arguments
/// * `n` - The Int128Parts containing the 2 64-bit components
///
/// # Returns
/// A string representation of the combined 128-bit signed integer
fn combine_i128(n: &Int128Parts) -> String {
	(((n.hi as i128) << 64) | (n.lo as i128)).to_string()
}

/// Processes a Stellar Contract Value (ScVal) into a JSON representation.
///
/// # Arguments
/// * `val` - The ScVal to process
///
/// # Returns
/// A JSON Value representing the processed ScVal with appropriate type information
fn process_sc_val(val: &ScVal) -> Value {
	match val {
		ScVal::Bool(b) => json!(b),
		ScVal::Void => json!(null),
		ScVal::U32(n) => json!(n),
		ScVal::I32(n) => json!(n),
		ScVal::U64(n) => json!(n),
		ScVal::I64(n) => json!(n),
		ScVal::Timepoint(t) => json!(t),
		ScVal::Duration(d) => json!(d),
		ScVal::U128(n) => json!({ "type": "U128", "value": combine_u128(n) }),
		ScVal::I128(n) => json!({ "type": "I128", "value": combine_i128(n) }),
		ScVal::U256(n) => json!({ "type": "U256", "value": combine_u256(n) }),
		ScVal::I256(n) => json!({ "type": "I256", "value": combine_i256(n) }),
		ScVal::Bytes(b) => json!(hex::encode(b)),
		ScVal::String(s) => json!(s.to_string()),
		ScVal::Symbol(s) => json!(s.to_string()),
		ScVal::Vec(Some(vec)) => process_sc_vec(vec),
		ScVal::Map(Some(map)) => process_sc_map(map),
		ScVal::Address(addr) => json!(match addr {
			ScAddress::Contract(hash) => Contract(hash.0).to_string(),
			ScAddress::Account(account_id) => match account_id {
				AccountId(PublicKey::PublicKeyTypeEd25519(key)) =>
					StrkeyPublicKey(key.0).to_string(),
			},
		}),
		_ => json!("unsupported_type"),
	}
}

/// Processes a Stellar Contract Vector into a JSON array.
///
/// # Arguments
/// * `vec` - The ScVec to process
///
/// # Returns
/// A JSON Value containing an array of processed ScVal elements
fn process_sc_vec(vec: &ScVec) -> Value {
	let values: Vec<Value> = vec.0.iter().map(process_sc_val).collect();
	json!(values)
}

/// Processes a Stellar Contract Map into a JSON object.
///
/// # Arguments
/// * `map` - The ScMap to process
///
/// # Returns
/// A JSON Value containing key-value pairs of processed ScVal elements
fn process_sc_map(map: &ScMap) -> Value {
	let entries: serde_json::Map<String, Value> = map
		.0
		.iter()
		.map(|ScMapEntry { key, val }| {
			let key_str = process_sc_val(key)
				.to_string()
				.trim_matches('"')
				.to_string();
			(key_str, process_sc_val(val))
		})
		.collect();
	json!(entries)
}

/// Gets the type of a Stellar Contract Value as a string.
///
/// # Arguments
/// * `val` - The ScVal to get the type for
///
/// # Returns
/// A string representing the type of the ScVal
fn get_sc_val_type(val: &ScVal) -> String {
	match val {
		ScVal::Bool(_) => "Bool".to_string(),
		ScVal::Void => "Void".to_string(),
		ScVal::U32(_) => "U32".to_string(),
		ScVal::I32(_) => "I32".to_string(),
		ScVal::U64(_) => "U64".to_string(),
		ScVal::I64(_) => "I64".to_string(),
		ScVal::Timepoint(_) => "Timepoint".to_string(),
		ScVal::Duration(_) => "Duration".to_string(),
		ScVal::U128(_) => "U128".to_string(),
		ScVal::I128(_) => "I128".to_string(),
		ScVal::U256(_) => "U256".to_string(),
		ScVal::I256(_) => "I256".to_string(),
		ScVal::Bytes(_) => "Bytes".to_string(),
		ScVal::String(_) => "String".to_string(),
		ScVal::Symbol(_) => "Symbol".to_string(),
		ScVal::Vec(_) => "Vec".to_string(),
		ScVal::Map(_) => "Map".to_string(),
		ScVal::Address(_) => "Address".to_string(),
		_ => "Unknown".to_string(),
	}
}

/// Gets the function signature for a Stellar host function operation.
///
/// # Arguments
/// * `invoke_op` - The InvokeHostFunctionOp to get the signature for
///
/// # Returns
/// A string representing the function signature in the format "function_name(type1,type2,...)"
pub fn get_function_signature(invoke_op: &InvokeHostFunctionOp) -> String {
	match &invoke_op.host_function {
		HostFunction::InvokeContract(args) => {
			let function_name = args.function_name.to_string();
			let arg_types: Vec<String> = args.args.iter().map(get_sc_val_type).collect();

			format!("{}({})", function_name, arg_types.join(","))
		}
		_ => "unknown_function()".to_string(),
	}
}

/// Processes a Stellar host function operation into a parsed result.
///
/// # Arguments
/// * `invoke_op` - The InvokeHostFunctionOp to process
///
/// # Returns
/// A StellarParsedOperationResult containing the processed operation details
pub fn process_invoke_host_function(
	invoke_op: &InvokeHostFunctionOp,
) -> StellarParsedOperationResult {
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

			let arguments = args.args.iter().map(process_sc_val).collect::<Vec<Value>>();

			StellarParsedOperationResult {
				contract_address,
				function_name,
				function_signature: get_function_signature(invoke_op),
				arguments,
			}
		}
		_ => StellarParsedOperationResult {
			contract_address: "".to_string(),
			function_name: "".to_string(),
			function_signature: "".to_string(),
			arguments: vec![],
		},
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
/// Attempts to decode XDR-formatted bytes into a Stellar Contract Value (ScVal) and then
/// converts it into a decoded parameter entry. This is commonly used for processing
/// contract events and function parameters.
///
/// # Arguments
/// * `bytes` - The XDR-encoded bytes to parse
/// * `indexed` - Whether this parameter is indexed in the event/function
///
/// # Returns
/// An Option containing the decoded parameter entry if successful, None if parsing fails
pub fn parse_xdr_value(bytes: &[u8], indexed: bool) -> Option<StellarDecodedParamEntry> {
	match ReadXdr::from_xdr(bytes, Limits::none()) {
		Ok(scval) => parse_sc_val(&scval, indexed),
		Err(e) => {
			tracing::debug!("Failed to parse XDR bytes: {}", e);
			None
		}
	}
}

/// Safely parse a string into a `serde_json::Value`.
///
/// # Arguments
/// * `input` - The string to parse as JSON
///
/// # Returns
/// `Some(Value)` if successful, `None` otherwise
pub fn parse_json_safe(input: &str) -> Option<Value> {
	match serde_json::from_str::<Value>(input) {
		Ok(val) => Some(val),
		Err(e) => {
			tracing::debug!("Failed to parse JSON: {}, error: {}", input, e);
			None
		}
	}
}

/// Recursively navigate through a JSON structure using dot notation (e.g. "user.address.street").
///
/// # Arguments
/// * `json_value` - The JSON value to navigate
/// * `path` - The dot-notation path to follow
///
/// # Returns
/// `Some(&Value)` if found, `None` otherwise
pub fn get_nested_value<'a>(json_value: &'a Value, path: &str) -> Option<&'a Value> {
	let mut current_val = json_value;

	for segment in path.split('.') {
		let obj = current_val.as_object()?;
		current_val = obj.get(segment)?;
	}

	Some(current_val)
}

/// Compare two plain strings with the given operator.
///
/// # Arguments
/// * `param_value` - The first string to compare
/// * `operator` - The comparison operator to use ("==" or "!=")
/// * `compare_value` - The second string to compare
///
/// # Returns
/// `true` if the comparison is true, `false` otherwise
pub fn compare_strings(param_value: &str, operator: &str, compare_value: &str) -> bool {
	match operator {
		"==" => param_value.trim_matches('"') == compare_value.trim_matches('"'),
		"!=" => param_value.trim_matches('"') != compare_value.trim_matches('"'),
		_ => {
			tracing::debug!("Unsupported operator for string comparison: {operator}");
			false
		}
	}
}

/// Compare a JSON `Value` with a plain string using a specific operator.
///
/// This function handles various string formats including:
/// - Plain strings
/// - JSON strings with quotes
/// - JSON strings with escaped quotes
///
/// # Arguments
/// * `value` - The JSON value to compare. Can be a string or other JSON value type
/// * `operator` - The comparison operator to use ("==" or "!=")
/// * `compare_value` - The string to compare against
///
/// # Returns
/// `true` if the comparison is true, `false` otherwise
pub fn compare_json_values_vs_string(value: &Value, operator: &str, compare_value: &str) -> bool {
	let value_str = match value {
		Value::String(s) => s.to_string(),
		_ => value.to_string(),
	};
	let value_str = value_str.trim_matches('"').replace("\\\"", "\"");
	let compare_str = compare_value.trim_matches('"').to_string();

	match operator {
		"==" => value_str == compare_str,
		"!=" => value_str != compare_str,
		_ => {
			tracing::debug!(
				"Unsupported operator for JSON-value vs. string comparison: {operator}"
			);
			false
		}
	}
}

/// Compare two JSON values with the given operator.
///
/// # Arguments
/// * `param_val` - The first JSON value to compare
/// * `operator` - The operator to use for comparison ("==", "!=", ">", ">=", "<", "<=")
/// * `compare_val` - The second JSON value to compare
///
/// # Returns
/// A boolean indicating if the comparison is true
pub fn compare_json_values(param_val: &Value, operator: &str, compare_val: &Value) -> bool {
	match operator {
		"==" => param_val == compare_val,
		"!=" => param_val != compare_val,
		">" | ">=" | "<" | "<=" => match (param_val.as_f64(), compare_val.as_f64()) {
			(Some(param_num), Some(compare_num)) => match operator {
				">" => param_num > compare_num,
				">=" => param_num >= compare_num,
				"<" => param_num < compare_num,
				"<=" => param_num <= compare_num,
				_ => unreachable!(),
			},
			_ => {
				tracing::debug!(
					"Numeric comparison operator {operator} requires numeric JSON values"
				);
				false
			}
		},
		_ => {
			tracing::debug!("Unsupported operator for JSON-to-JSON comparison: {operator}");
			false
		}
	}
}

/// Get the kind of a value from a JSON value.
///
/// This is used to determine the kind of a value for the `kind` field in the
/// `StellarMatchParamEntry` struct.
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
/// A LedgerKey for the deployed contract instance
///
/// # Example
/// When calling `getLedgerEntries` with the output of this function, the result might look like:
/// ```json
/// {
///   "contract_data": {
///     "ext": "v0",
///     "contract": "CDMZ6LU66KEMLKI3EJBIGXTZ4KZ2CRTSHZETMY3QQZBWRKVKB5EIOHTX",
///     "key": "ledger_key_contract_instance",
///     "durability": "persistent",
///     "val": {
///       "contract_instance": {
///         "executable": {
///           "wasm": "0adabe438e539cf5a77afd8197f8e25c822ca2d27ba99d8e0e31b80b7400c903"
///         },
///         "storage": [
///           {
///             "key": {
///               "symbol": "COUNTER"
///             },
///             "val": {
///               "u32": 17
///             }
///           }
///         ]
///       }
///     }
///   }
/// }
/// ```
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
/// * `wasm_hash` - WASM hash
///
/// # Returns
/// A LedgerKey for the contract code if successfully extracted
///
/// # Example
/// When calling `getLedgerEntries` with the output of this function, the result might look like:
/// ```json
/// {
///   "contract_code": {
///     "ext":  {...},
///     "hash": "b54ba37b7bb7dd69a7759caa9eec70e9e13615ba3b009fc23c4626ae9dffa27f"
///     "code": "0061736d0100000001e3023060027e7e017e60017e017e6000017e60037e7e7e..."
///   }
/// }
/// ```
pub fn get_contract_code_ledger_key(wasm_hash: &str) -> Result<LedgerKey, anyhow::Error> {
	Ok(LedgerKey::ContractCode(LedgerKeyContractCode {
		hash: wasm_hash.parse::<Hash>()?,
	}))
}

/// Get wasm code from a contract's XDR-encoded executable.
///
/// # Arguments
/// * `ledger_entry_data` - XDR-encoded contract data
///
/// # Returns
/// The WASM code as a hex string if successfully extracted
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

/// Get wasm hash from a contract's XDR-encoded executable.
///
/// # Arguments
/// * `ledger_entry_data` - XDR-encoded contract data
///
/// # Returns
/// The WASM hash as a hex string if successfully extracted
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

/// Format a ScSpecTypeDef into a clean string representation.
///
/// # Arguments
/// * `type_def` - The type definition to format
///
/// # Returns
/// A string representation of the type definition
fn format_type_def(type_def: &ScSpecTypeDef) -> String {
	match type_def {
		ScSpecTypeDef::Map(t) => {
			format!(
				"Map<{},{}>",
				format_type_def(&t.key_type),
				format_type_def(&t.value_type)
			)
		}
		ScSpecTypeDef::Vec(t) => format!("Vec<{}>", format_type_def(&t.element_type)),
		ScSpecTypeDef::Tuple(t) => {
			let types = t
				.value_types
				.iter()
				.map(format_type_def)
				.collect::<Vec<_>>()
				.join(",");
			format!("Tuple<{}>", types)
		}
		ScSpecTypeDef::BytesN(bytes_n) => format!("Bytes{}", bytes_n.n),
		ScSpecTypeDef::U128 => "U128".to_string(),
		ScSpecTypeDef::I128 => "I128".to_string(),
		ScSpecTypeDef::U256 => "U256".to_string(),
		ScSpecTypeDef::I256 => "I256".to_string(),
		ScSpecTypeDef::Address => "Address".to_string(),
		ScSpecTypeDef::Bool => "Bool".to_string(),
		ScSpecTypeDef::Symbol => "Symbol".to_string(),
		ScSpecTypeDef::String => "String".to_string(),
		ScSpecTypeDef::Bytes => "Bytes".to_string(),
		ScSpecTypeDef::U32 => "U32".to_string(),
		ScSpecTypeDef::I32 => "I32".to_string(),
		ScSpecTypeDef::U64 => "U64".to_string(),
		ScSpecTypeDef::I64 => "I64".to_string(),
		ScSpecTypeDef::Timepoint => "Timepoint".to_string(),
		ScSpecTypeDef::Duration => "Duration".to_string(),
		_ => "Unknown".to_string(),
	}
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
		.filter_map(|entry| match entry {
			ScSpecEntry::FunctionV0(func) => Some(StellarContractFunction {
				name: func.name.to_string(),
				inputs: func
					.inputs
					.iter()
					.enumerate()
					.map(|(index, input)| StellarContractInput {
						index: index as u32,
						name: input.name.to_string(),
						kind: format_type_def(&input.type_),
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
		ContractDataEntry, Hash, LedgerEntryData, ScContractInstance, ScSpecFunctionInputV0,
		ScSpecFunctionV0, ScSpecTypeMap, ScSpecTypeVec, ScString, ScSymbol, ScVal, SequenceNumber,
		String32, StringM, Uint256, WriteXdr,
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
	fn test_process_sc_val() {
		// Test basic types
		assert_eq!(process_sc_val(&ScVal::Bool(true)), json!(true));
		assert_eq!(process_sc_val(&ScVal::Void), json!(null));
		assert_eq!(process_sc_val(&ScVal::U32(42)), json!(42));
		assert_eq!(process_sc_val(&ScVal::I32(-42)), json!(-42));
		assert_eq!(process_sc_val(&ScVal::U64(42)), json!(42));
		assert_eq!(process_sc_val(&ScVal::I64(-42)), json!(-42));

		// Test string and symbol
		assert_eq!(
			process_sc_val(&ScVal::String(ScString("test".try_into().unwrap()))),
			json!("test")
		);
		assert_eq!(
			process_sc_val(&ScVal::Symbol(ScSymbol(
				StringM::<32>::from_str("test").unwrap()
			))),
			json!("test")
		);

		// Test bytes
		assert_eq!(
			process_sc_val(&ScVal::Bytes(vec![1, 2, 3].try_into().unwrap())),
			json!("010203")
		);

		// Test complex types (Vec and Map)
		let vec_val = ScVal::Vec(Some(ScVec(
			vec![ScVal::I32(1), ScVal::I32(2), ScVal::I32(3)]
				.try_into()
				.unwrap(),
		)));
		assert_eq!(process_sc_val(&vec_val), json!([1, 2, 3]));

		let map_entry = ScMapEntry {
			key: ScVal::String(ScString("key".try_into().unwrap())),
			val: ScVal::I32(42),
		};
		let map_val = ScVal::Map(Some(ScMap(vec![map_entry].try_into().unwrap())));
		assert_eq!(process_sc_val(&map_val), json!({"key": 42}));
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
			get_function_signature(&invoke_op),
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
	fn test_json_helper_functions() {
		// Test parse_json_safe
		assert!(parse_json_safe("invalid json").is_none());
		assert_eq!(
			parse_json_safe(r#"{"key": "value"}"#).unwrap(),
			json!({"key": "value"})
		);

		// Test get_nested_value
		let json_obj = json!({
			"user": {
				"address": {
					"street": "123 Main St"
				}
			}
		});
		assert_eq!(
			get_nested_value(&json_obj, "user.address.street").unwrap(),
			&json!("123 Main St")
		);
		assert!(get_nested_value(&json_obj, "invalid.path").is_none());

		// Test string comparison functions
		assert!(compare_strings("test", "==", "test"));
		assert!(compare_strings("test", "!=", "other"));
		assert!(!compare_strings("test", "invalid", "test"));

		// Test JSON value comparison functions
		assert!(compare_json_values(&json!(42), "==", &json!(42)));
		assert!(compare_json_values(&json!(42), ">", &json!(30)));
		assert!(!compare_json_values(&json!(42), "<", &json!(30)));
		assert!(!compare_json_values(
			&json!("test"),
			"invalid",
			&json!("test")
		));
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
		let contract_data_xdr = "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";
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
	}

	#[test]
	fn test_compare_json_values_vs_string() {
		// Test string comparison
		assert!(compare_json_values_vs_string(&json!("test"), "==", "test"));
		assert!(!compare_json_values_vs_string(
			&json!("test"),
			"==",
			"other"
		));
		assert!(compare_json_values_vs_string(&json!("test"), "!=", "other"));
		assert!(!compare_json_values_vs_string(&json!("test"), "!=", "test"));

		// Test with quoted strings
		assert!(compare_json_values_vs_string(
			&json!("\"test\""),
			"==",
			"test"
		));
		assert!(compare_json_values_vs_string(
			&json!("test"),
			"==",
			"\"test\""
		));

		// Test unsupported operator
		assert!(!compare_json_values_vs_string(&json!("test"), ">", "test"));
	}

	#[test]
	fn test_format_type_def() {
		// Test basic types
		assert_eq!(format_type_def(&ScSpecTypeDef::Bool), "Bool");
		assert_eq!(format_type_def(&ScSpecTypeDef::String), "String");
		assert_eq!(format_type_def(&ScSpecTypeDef::U64), "U64");
		assert_eq!(format_type_def(&ScSpecTypeDef::I64), "I64");
		assert_eq!(format_type_def(&ScSpecTypeDef::Address), "Address");

		// Test complex types
		let vec_type = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
			element_type: Box::new(ScSpecTypeDef::U64),
		}));
		assert_eq!(format_type_def(&vec_type), "Vec<U64>");

		let map_type = ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
			key_type: Box::new(ScSpecTypeDef::String),
			value_type: Box::new(ScSpecTypeDef::U64),
		}));
		assert_eq!(format_type_def(&map_type), "Map<String,U64>");

		// Test nested complex types
		let nested_vec = ScSpecTypeDef::Vec(Box::new(ScSpecTypeVec {
			element_type: Box::new(ScSpecTypeDef::Map(Box::new(ScSpecTypeMap {
				key_type: Box::new(ScSpecTypeDef::String),
				value_type: Box::new(ScSpecTypeDef::U64),
			}))),
		}));
		assert_eq!(format_type_def(&nested_vec), "Vec<Map<String,U64>>");

		// Test BytesN
		let bytes_n = ScSpecTypeDef::BytesN(stellar_xdr::curr::ScSpecTypeBytesN { n: 32 });
		assert_eq!(format_type_def(&bytes_n), "Bytes32");
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
}
