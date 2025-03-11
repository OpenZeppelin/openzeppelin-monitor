//! Helper functions for Stellar-specific operations.
//!
//! This module provides utility functions for working with Stellar-specific data types
//! and formatting, including address normalization, XDR value parsing, and
//! operation processing.

use alloy::primitives::{I256, U256};
use hex::encode;
use log::debug;
use serde_json::{json, Value};
use stellar_strkey::{ed25519::PublicKey as StrkeyPublicKey, Contract};
use stellar_xdr::curr::{
	AccountId, HostFunction, Int128Parts, Int256Parts, InvokeHostFunctionOp, Limits, PublicKey,
	ReadXdr, ScAddress, ScMap, ScMapEntry, ScVal, ScVec, UInt128Parts, UInt256Parts,
};

use crate::models::{StellarDecodedParamEntry, StellarParsedOperationResult};

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
			log::warn!("Failed to parse XDR bytes: {}", e);
			None
		}
	}
}

/// Safely parse a string into a `serde_json::Value`.
/// Returns `Some(Value)` if successful, `None` otherwise.
pub fn parse_json_safe(input: &str) -> Option<Value> {
	match serde_json::from_str::<Value>(input) {
		Ok(val) => Some(val),
		Err(e) => {
			debug!("Failed to parse JSON: {}, error: {}", input, e);
			None
		}
	}
}

/// Recursively navigate through a JSON structure using dot notation (e.g. "user.address.street").
/// Returns `Some(&Value)` if found, `None` otherwise.
pub fn get_nested_value<'a>(json_value: &'a Value, path: &str) -> Option<&'a Value> {
	let mut current_val = json_value;

	for segment in path.split('.') {
		let obj = current_val.as_object()?;
		current_val = obj.get(segment)?;
	}

	Some(current_val)
}

/// Compare two plain strings with the given operator.
pub fn compare_strings(param_value: &str, operator: &str, compare_value: &str) -> bool {
	match operator {
		"==" => param_value.trim_matches('"') == compare_value.trim_matches('"'),
		"!=" => param_value.trim_matches('"') != compare_value.trim_matches('"'),
		_ => {
			debug!("Unsupported operator for string comparison: {operator}");
			false
		}
	}
}

/// Compare a JSON `Value` with a plain string using a specific operator.
/// This mimics the logic you had in single-key comparisons:
pub fn compare_json_values_vs_string(value: &Value, operator: &str, compare_value: &str) -> bool {
	// We can add more logic here: if `value` is a string, compare its unquoted form, etc.
	// For now, let's replicate the old `to_string().trim_matches('"') == compare_value`.
	match operator {
		"==" => value.to_string().trim_matches('"') == compare_value,
		"!=" => value.to_string().trim_matches('"') != compare_value,
		_ => {
			debug!("Unsupported operator for JSON-value vs. string comparison: {operator}");
			false
		}
	}
}

/// Compare two JSON values with the given operator.
///
/// # Arguments
/// * `param_val` - The first JSON value to compare
/// * `operator` - The operator to use for comparison
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
				debug!("Numeric comparison operator {operator} requires numeric JSON values");
				false
			}
		},
		_ => {
			debug!("Unsupported operator for JSON-to-JSON comparison: {operator}");
			false
		}
	}
}

/// Get the kind of a value from a JSON value.
///
/// This is used to determine the kind of a value for the `kind` field in the
/// `StellarMatchParamEntry` struct.
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

#[cfg(test)]
mod tests {
	use std::str::FromStr;

	use super::*;
	use serde_json::json;
	use stellar_xdr::curr::{Hash, ScString, ScSymbol, StringM};

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
			process_sc_val(&ScVal::Symbol(ScSymbol(StringM::from_str("test").unwrap()))),
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
}
