//! Monitor implementation for Stellar blockchain.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use stellar_xdr::curr::ScSpecEntry;

use crate::{
	models::{MatchConditions, Monitor, StellarBlock, StellarTransaction},
	services::filter::stellar_helpers::{
		get_contract_spec_functions, get_contract_spec_with_function_input_parameters,
	},
};

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

/// Raw contract specification for a Stellar smart contract
///
/// This structure represents the native Stellar contract specification format, derived directly
/// from ScSpecEntry. It contains the raw contract interface data as provided by the Stellar
/// blockchain, including all function definitions, types, and other contract metadata in their
/// original format.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ContractSpec(Vec<ScSpecEntry>);

impl From<Vec<ScSpecEntry>> for ContractSpec {
	fn from(spec: Vec<ScSpecEntry>) -> Self {
		ContractSpec(spec)
	}
}

/// Convert a ContractSpec to a StellarContractSpec
impl From<crate::models::ContractSpec> for ContractSpec {
	fn from(spec: crate::models::ContractSpec) -> Self {
		match spec {
			crate::models::ContractSpec::Stellar(stellar_spec) => Self(stellar_spec.0),
			_ => Self(Vec::new()),
		}
	}
}

/// Convert a serde_json::Value to a StellarContractSpec
impl From<serde_json::Value> for ContractSpec {
	fn from(spec: serde_json::Value) -> Self {
		let spec = serde_json::from_value(spec).unwrap_or_else(|e| {
			tracing::error!("Error parsing contract spec: {:?}", e);
			Vec::new()
		});
		Self(spec)
	}
}

/// Display a StellarContractSpec
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

/// Dereference a StellarContractSpec
impl std::ops::Deref for ContractSpec {
	type Target = Vec<ScSpecEntry>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

/// Human-readable contract specification for a Stellar smart contract
///
/// This structure provides a simplified, application-specific view of a Stellar contract's
/// interface. It transforms the raw ContractSpec into a more accessible format that's easier
/// to work with in our monitoring system. The main differences are:
/// - Focuses on callable functions with their input parameters
/// - Provides a cleaner, more structured representation
/// - Optimized for our specific use case of monitoring contract interactions
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct FormattedContractSpec {
	/// List of callable functions defined in the contract
	pub functions: Vec<ContractFunction>,
}

impl From<ContractSpec> for FormattedContractSpec {
	fn from(spec: ContractSpec) -> Self {
		let functions =
			get_contract_spec_with_function_input_parameters(get_contract_spec_functions(spec.0));

		FormattedContractSpec { functions }
	}
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
///     ],
///     "signature": "transfer(Address,U64)"
/// }
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ContractFunction {
	/// Name of the function as defined in the contract
	pub name: String,

	/// Ordered list of input parameters accepted by the function
	pub inputs: Vec<ContractInput>,

	/// Signature of the function
	pub signature: String,
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
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ContractInput {
	/// Zero-based index of the parameter in the function signature
	pub index: u32,

	/// Parameter name as defined in the contract
	pub name: String,

	/// Parameter type in Stellar's type system format
	pub kind: String,
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::models::EVMContractSpec;
	use crate::models::{
		blockchain::stellar::block::LedgerInfo as StellarLedgerInfo,
		blockchain::stellar::transaction::TransactionInfo as StellarTransactionInfo,
		ContractSpec as ModelsContractSpec, FunctionCondition, MatchConditions,
	};
	use crate::utils::tests::builders::stellar::monitor::MonitorBuilder;
	use serde_json::json;
	use stellar_xdr::curr::{ScSpecEntry, ScSpecFunctionInputV0, ScSpecFunctionV0, ScSpecTypeDef};

	#[test]
	fn test_contract_spec_from_vec() {
		let spec_entries = vec![ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
			name: "test_function".try_into().unwrap(),
			inputs: vec![].try_into().unwrap(),
			outputs: vec![].try_into().unwrap(),
			doc: "Test function documentation".try_into().unwrap(),
		})];

		let contract_spec = ContractSpec::from(spec_entries.clone());
		assert_eq!(contract_spec.0, spec_entries);
	}

	#[test]
	fn test_contract_spec_from_json() {
		let json_value = serde_json::json!([
			{
				"function_v0": {
					"doc": "Test function documentation",
					"name": "test_function",
					"inputs": [
						{
							"doc": "",
							"name": "from",
							"type_": "address"
						},
						{
							"doc": "",
							"name": "to",
							"type_": "address"
						},
						{
							"doc": "",
							"name": "amount",
							"type_": "i128"
						}
					],
					"outputs": []
				}
			},
		]);

		let contract_spec = ContractSpec::from(json_value);
		assert!(!contract_spec.0.is_empty());
		if let ScSpecEntry::FunctionV0(func) = &contract_spec.0[0] {
			assert_eq!(func.name.to_string(), "test_function");
			assert_eq!(func.doc.to_string(), "Test function documentation");
		} else {
			panic!("Expected FunctionV0 entry");
		}
	}

	#[test]
	fn test_contract_spec_from_invalid_json() {
		let invalid_json = serde_json::json!({
			"invalid": "data"
		});

		let contract_spec = ContractSpec::from(invalid_json);
		assert!(contract_spec.0.is_empty());
	}

	#[test]
	fn test_formatted_contract_spec_from_contract_spec() {
		let spec_entries = vec![ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
			name: "transfer".try_into().unwrap(),
			inputs: vec![
				ScSpecFunctionInputV0 {
					name: "to".try_into().unwrap(),
					type_: ScSpecTypeDef::Address,
					doc: "Recipient address".try_into().unwrap(),
				},
				ScSpecFunctionInputV0 {
					name: "amount".try_into().unwrap(),
					type_: ScSpecTypeDef::U64,
					doc: "Amount to transfer".try_into().unwrap(),
				},
			]
			.try_into()
			.unwrap(),
			outputs: vec![].try_into().unwrap(),
			doc: "Transfer function documentation".try_into().unwrap(),
		})];

		let contract_spec = ContractSpec(spec_entries);
		let formatted_spec = FormattedContractSpec::from(contract_spec);

		assert_eq!(formatted_spec.functions.len(), 1);
		let function = &formatted_spec.functions[0];
		assert_eq!(function.name, "transfer");
		assert_eq!(function.inputs.len(), 2);
		assert_eq!(function.inputs[0].name, "to");
		assert_eq!(function.inputs[0].kind, "Address");
		assert_eq!(function.inputs[1].name, "amount");
		assert_eq!(function.inputs[1].kind, "U64");
		assert_eq!(function.signature, "transfer(Address,U64)");
	}

	#[test]
	fn test_contract_spec_display() {
		let spec_entries = vec![ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
			name: "test_function".try_into().unwrap(),
			inputs: vec![].try_into().unwrap(),
			outputs: vec![].try_into().unwrap(),
			doc: "Test function documentation".try_into().unwrap(),
		})];

		let contract_spec = ContractSpec(spec_entries);
		let display_str = format!("{}", contract_spec);
		assert!(!display_str.is_empty());
		assert!(display_str.contains("test_function"));
	}

	#[test]
	fn test_contract_spec_with_multiple_functions() {
		let spec_entries = vec![
			ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
				name: "transfer".try_into().unwrap(),
				inputs: vec![
					ScSpecFunctionInputV0 {
						name: "to".try_into().unwrap(),
						type_: ScSpecTypeDef::Address,
						doc: "Recipient address".try_into().unwrap(),
					},
					ScSpecFunctionInputV0 {
						name: "amount".try_into().unwrap(),
						type_: ScSpecTypeDef::U64,
						doc: "Amount to transfer".try_into().unwrap(),
					},
				]
				.try_into()
				.unwrap(),
				outputs: vec![].try_into().unwrap(),
				doc: "Transfer function".try_into().unwrap(),
			}),
			ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
				name: "balance".try_into().unwrap(),
				inputs: vec![ScSpecFunctionInputV0 {
					name: "account".try_into().unwrap(),
					type_: ScSpecTypeDef::Address,
					doc: "Account to check balance for".try_into().unwrap(),
				}]
				.try_into()
				.unwrap(),
				outputs: vec![ScSpecTypeDef::U64].try_into().unwrap(),
				doc: "Balance function".try_into().unwrap(),
			}),
		];

		let contract_spec = ContractSpec(spec_entries);
		let formatted_spec = FormattedContractSpec::from(contract_spec);

		assert_eq!(formatted_spec.functions.len(), 2);

		let transfer_fn = formatted_spec
			.functions
			.iter()
			.find(|f| f.name == "transfer")
			.expect("Transfer function not found");
		assert_eq!(transfer_fn.signature, "transfer(Address,U64)");

		let balance_fn = formatted_spec
			.functions
			.iter()
			.find(|f| f.name == "balance")
			.expect("Balance function not found");
		assert_eq!(balance_fn.signature, "balance(Address)");
	}

	#[test]
	fn test_monitor_match() {
		let monitor = MonitorBuilder::new()
			.name("TestMonitor")
			.function("transfer(address,uint256)", None)
			.build();

		let transaction = StellarTransaction(StellarTransactionInfo {
			status: "SUCCESS".to_string(),
			transaction_hash: "test_hash".to_string(),
			application_order: 1,
			fee_bump: false,
			envelope_xdr: Some("mock_xdr".to_string()),
			envelope_json: Some(serde_json::json!({
				"type": "ENVELOPE_TYPE_TX",
				"tx": {
					"sourceAccount": "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
					"operations": [{
						"type": "invokeHostFunction",
						"function": "transfer",
						"parameters": ["GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF", "1000000"]
					}]
				}
			})),
			result_xdr: Some("mock_result".to_string()),
			result_json: None,
			result_meta_xdr: Some("mock_meta".to_string()),
			result_meta_json: None,
			diagnostic_events_xdr: None,
			diagnostic_events_json: None,
			ledger: 123,
			ledger_close_time: 1234567890,
			decoded: None,
		});

		let ledger = StellarBlock(StellarLedgerInfo {
			hash: "test_ledger_hash".to_string(),
			sequence: 123,
			ledger_close_time: "2024-03-20T12:00:00Z".to_string(),
			ledger_header: "mock_header".to_string(),
			ledger_header_json: None,
			ledger_metadata: "mock_metadata".to_string(),
			ledger_metadata_json: None,
		});

		let match_params = MatchParamsMap {
			signature: "transfer(address,uint256)".to_string(),
			args: Some(vec![
				MatchParamEntry {
					name: "to".to_string(),
					value: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_string(),
					kind: "Address".to_string(),
					indexed: false,
				},
				MatchParamEntry {
					name: "amount".to_string(),
					value: "1000000".to_string(),
					kind: "U64".to_string(),
					indexed: false,
				},
			]),
		};

		let monitor_match = MonitorMatch {
			monitor: monitor.clone(),
			transaction: transaction.clone(),
			ledger: ledger.clone(),
			network_slug: "stellar_mainnet".to_string(),
			matched_on: MatchConditions {
				functions: vec![FunctionCondition {
					signature: "transfer(address,uint256)".to_string(),
					expression: None,
				}],
				events: vec![],
				transactions: vec![],
			},
			matched_on_args: Some(MatchArguments {
				functions: Some(vec![match_params]),
				events: None,
			}),
		};

		assert_eq!(monitor_match.monitor.name, "TestMonitor");
		assert_eq!(monitor_match.transaction.transaction_hash, "test_hash");
		assert_eq!(monitor_match.ledger.sequence, 123);
		assert_eq!(monitor_match.network_slug, "stellar_mainnet");
		assert_eq!(monitor_match.matched_on.functions.len(), 1);
		assert_eq!(
			monitor_match.matched_on.functions[0].signature,
			"transfer(address,uint256)"
		);

		let matched_args = monitor_match.matched_on_args.unwrap();
		let function_args = matched_args.functions.unwrap();
		assert_eq!(function_args.len(), 1);
		assert_eq!(function_args[0].signature, "transfer(address,uint256)");

		let args = function_args[0].args.as_ref().unwrap();
		assert_eq!(args.len(), 2);
		assert_eq!(args[0].name, "to");
		assert_eq!(args[0].kind, "Address");
		assert_eq!(args[1].name, "amount");
		assert_eq!(args[1].kind, "U64");
	}

	#[test]
	fn test_parsed_operation_result() {
		let result = ParsedOperationResult {
			contract_address: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
				.to_string(),
			function_name: "transfer".to_string(),
			function_signature: "transfer(address,uint256)".to_string(),
			arguments: vec![
				serde_json::json!("GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"),
				serde_json::json!("1000000"),
			],
		};

		assert_eq!(
			result.contract_address,
			"GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
		);
		assert_eq!(result.function_name, "transfer");
		assert_eq!(result.function_signature, "transfer(address,uint256)");
		assert_eq!(result.arguments.len(), 2);
		assert_eq!(
			result.arguments[0],
			"GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
		);
		assert_eq!(result.arguments[1], "1000000");
	}

	#[test]
	fn test_decoded_param_entry() {
		let param = DecodedParamEntry {
			value: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_string(),
			kind: "Address".to_string(),
			indexed: false,
		};

		assert_eq!(
			param.value,
			"GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
		);
		assert_eq!(param.kind, "Address");
		assert!(!param.indexed);
	}

	#[test]
	fn test_match_arguments() {
		let match_args = MatchArguments {
			functions: Some(vec![MatchParamsMap {
				signature: "transfer(address,uint256)".to_string(),
				args: Some(vec![
					MatchParamEntry {
						name: "to".to_string(),
						value: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
							.to_string(),
						kind: "Address".to_string(),
						indexed: false,
					},
					MatchParamEntry {
						name: "amount".to_string(),
						value: "1000000".to_string(),
						kind: "U64".to_string(),
						indexed: false,
					},
				]),
			}]),
			events: Some(vec![MatchParamsMap {
				signature: "Transfer(address,address,uint256)".to_string(),
				args: Some(vec![
					MatchParamEntry {
						name: "from".to_string(),
						value: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
							.to_string(),
						kind: "Address".to_string(),
						indexed: true,
					},
					MatchParamEntry {
						name: "to".to_string(),
						value: "GBXGQJWVLWOYHFLVTKWV5FGHA3LNYY2JQKM7OAJAUEQFU6LPCSEFVXON"
							.to_string(),
						kind: "Address".to_string(),
						indexed: true,
					},
					MatchParamEntry {
						name: "amount".to_string(),
						value: "1000000".to_string(),
						kind: "U64".to_string(),
						indexed: false,
					},
				]),
			}]),
		};

		assert!(match_args.functions.is_some());
		let functions = match_args.functions.unwrap();
		assert_eq!(functions.len(), 1);
		assert_eq!(functions[0].signature, "transfer(address,uint256)");

		let function_args = functions[0].args.as_ref().unwrap();
		assert_eq!(function_args.len(), 2);
		assert_eq!(function_args[0].name, "to");
		assert_eq!(function_args[0].kind, "Address");
		assert_eq!(function_args[1].name, "amount");
		assert_eq!(function_args[1].kind, "U64");

		assert!(match_args.events.is_some());
		let events = match_args.events.unwrap();
		assert_eq!(events.len(), 1);
		assert_eq!(events[0].signature, "Transfer(address,address,uint256)");

		let event_args = events[0].args.as_ref().unwrap();
		assert_eq!(event_args.len(), 3);
		assert_eq!(event_args[0].name, "from");
		assert!(event_args[0].indexed);
		assert_eq!(event_args[1].name, "to");
		assert!(event_args[1].indexed);
		assert_eq!(event_args[2].name, "amount");
		assert!(!event_args[2].indexed);
	}

	#[test]
	fn test_contract_spec_deref() {
		let spec_entries = vec![ScSpecEntry::FunctionV0(ScSpecFunctionV0 {
			name: "transfer".try_into().unwrap(),
			inputs: vec![].try_into().unwrap(),
			outputs: vec![].try_into().unwrap(),
			doc: "Test function documentation".try_into().unwrap(),
		})];

		let contract_spec = ContractSpec(spec_entries.clone());
		assert_eq!(contract_spec.len(), 1);
		if let ScSpecEntry::FunctionV0(func) = &contract_spec[0] {
			assert_eq!(func.name.to_string(), "transfer");
		} else {
			panic!("Expected FunctionV0 entry");
		}
	}

	#[test]
	fn test_contract_spec_from_models() {
		let json_value = serde_json::json!([
				{
					"function_v0": {
						"doc": "",
						"name": "transfer",
						"inputs": [
							{
								"doc": "",
								"name": "from",
								"type_": "address"
							},
							{
								"doc": "",
								"name": "to",
								"type_": "address"
							},
							{
								"doc": "",
								"name": "amount",
								"type_": "i128"
							}
						],
						"outputs": []
					}
				},
			]
		);

		let stellar_spec = ContractSpec::from(json_value.clone());
		let models_spec = ModelsContractSpec::Stellar(stellar_spec);
		let converted_spec = ContractSpec::from(models_spec);
		let formatted_spec = FormattedContractSpec::from(converted_spec);

		assert!(!formatted_spec.functions.is_empty());
		assert_eq!(formatted_spec.functions[0].name, "transfer");
		assert_eq!(formatted_spec.functions[0].inputs.len(), 3);
		assert_eq!(formatted_spec.functions[0].inputs[0].name, "from");
		assert_eq!(formatted_spec.functions[0].inputs[0].kind, "Address");
		assert_eq!(formatted_spec.functions[0].inputs[1].name, "to");
		assert_eq!(formatted_spec.functions[0].inputs[1].kind, "Address");
		assert_eq!(formatted_spec.functions[0].inputs[2].name, "amount");
		assert_eq!(formatted_spec.functions[0].inputs[2].kind, "I128");

		let evm_spec = EVMContractSpec::from(json!({}));
		let models_spec = ModelsContractSpec::EVM(evm_spec);
		let converted_spec = ContractSpec::from(models_spec);
		assert!(converted_spec.is_empty());
	}
}
