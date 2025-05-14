use crate::models::{
	EVMReceiptLog, EVMTransaction, EVMTransactionReceipt, MatchConditions, Monitor,
};
use serde::{Deserialize, Serialize};

/// Result of a successful monitor match on an EVM chain
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EVMMonitorMatch {
	/// Monitor configuration that triggered the match
	pub monitor: Monitor,

	/// Transaction that triggered the match
	pub transaction: EVMTransaction,

	/// Transaction receipt with execution results
	pub receipt: Option<EVMTransactionReceipt>,

	/// Transaction logs
	pub logs: Option<Vec<EVMReceiptLog>>,

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

	/// Raw function/event signature as bytes
	pub hex_signature: Option<String>,
}

/// Single decoded parameter from a function or event
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MatchArguments {
	/// Matched function arguments
	pub functions: Option<Vec<MatchParamsMap>>,

	/// Matched event arguments
	pub events: Option<Vec<MatchParamsMap>>,
}

/// Contract specification for an EVM smart contract
///
/// This structure represents the parsed specification of an EVM smart contract,
/// following the Ethereum Contract ABI format. It contains information about all
/// callable functions in the contract.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct ContractSpec(alloy::json_abi::JsonAbi);

/// Convert a ContractSpec to an EVMContractSpec
impl From<crate::models::ContractSpec> for ContractSpec {
	fn from(spec: crate::models::ContractSpec) -> Self {
		match spec {
			crate::models::ContractSpec::EVM(evm_spec) => Self(evm_spec.0),
			_ => Self(alloy::json_abi::JsonAbi::new()),
		}
	}
}

/// Convert a JsonAbi to a ContractSpec
impl From<alloy::json_abi::JsonAbi> for ContractSpec {
	fn from(spec: alloy::json_abi::JsonAbi) -> Self {
		Self(spec)
	}
}

/// Convert a serde_json::Value to a ContractSpec
impl From<serde_json::Value> for ContractSpec {
	fn from(spec: serde_json::Value) -> Self {
		let spec = serde_json::from_value(spec).unwrap_or_else(|e| {
			tracing::error!("Error parsing contract spec: {:?}", e);
			alloy::json_abi::JsonAbi::new()
		});
		Self(spec)
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

/// Dereference a ContractSpec
impl std::ops::Deref for ContractSpec {
	type Target = alloy::json_abi::JsonAbi;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		models::{ContractSpec as ModelsContractSpec, FunctionCondition, StellarContractSpec},
		utils::tests::evm::{
			monitor::MonitorBuilder, receipt::ReceiptBuilder, transaction::TransactionBuilder,
		},
	};

	use super::*;
	use alloy::primitives::{Address, B256, U256, U64};

	#[test]
	fn test_evm_monitor_match() {
		let monitor = MonitorBuilder::new()
			.name("TestMonitor")
			.function("transfer(address,uint256)", None)
			.build();

		let transaction = TransactionBuilder::new()
			.hash(B256::with_last_byte(1))
			.nonce(U256::from(1))
			.from(Address::ZERO)
			.to(Address::ZERO)
			.value(U256::ZERO)
			.gas_price(U256::from(20))
			.gas_limit(U256::from(21000))
			.build();

		let receipt = ReceiptBuilder::new()
			.transaction_hash(B256::with_last_byte(1))
			.transaction_index(0)
			.from(Address::ZERO)
			.to(Address::ZERO)
			.gas_used(U256::from(21000))
			.status(true)
			.build();

		let match_params = MatchParamsMap {
			signature: "transfer(address,uint256)".to_string(),
			args: Some(vec![
				MatchParamEntry {
					name: "to".to_string(),
					value: "0x0000000000000000000000000000000000000000".to_string(),
					kind: "address".to_string(),
					indexed: false,
				},
				MatchParamEntry {
					name: "amount".to_string(),
					value: "1000000000000000000".to_string(),
					kind: "uint256".to_string(),
					indexed: false,
				},
			]),
			hex_signature: Some("0xa9059cbb".to_string()),
		};

		let monitor_match = EVMMonitorMatch {
			monitor: monitor.clone(),
			transaction: transaction.clone(),
			receipt: Some(receipt.clone()),
			logs: Some(receipt.logs.clone()),
			network_slug: "ethereum_mainnet".to_string(),
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
		assert_eq!(monitor_match.transaction.hash, B256::with_last_byte(1));
		assert_eq!(
			monitor_match.receipt.as_ref().unwrap().status,
			Some(U64::from(1))
		);
		assert_eq!(monitor_match.network_slug, "ethereum_mainnet");
		assert_eq!(monitor_match.matched_on.functions.len(), 1);
		assert_eq!(
			monitor_match.matched_on.functions[0].signature,
			"transfer(address,uint256)"
		);

		let matched_args = monitor_match.matched_on_args.unwrap();
		let function_args = matched_args.functions.unwrap();
		assert_eq!(function_args.len(), 1);
		assert_eq!(function_args[0].signature, "transfer(address,uint256)");
		assert_eq!(
			function_args[0].hex_signature,
			Some("0xa9059cbb".to_string())
		);

		let args = function_args[0].args.as_ref().unwrap();
		assert_eq!(args.len(), 2);
		assert_eq!(args[0].name, "to");
		assert_eq!(args[0].kind, "address");
		assert_eq!(args[1].name, "amount");
		assert_eq!(args[1].kind, "uint256");
	}

	#[test]
	fn test_match_arguments() {
		let from_addr = Address::ZERO;
		let to_addr = Address::with_last_byte(1);
		let amount = U256::from(1000000000000000000u64);

		let match_args = MatchArguments {
			functions: Some(vec![MatchParamsMap {
				signature: "transfer(address,uint256)".to_string(),
				args: Some(vec![
					MatchParamEntry {
						name: "to".to_string(),
						value: format!("{:#x}", to_addr),
						kind: "address".to_string(),
						indexed: false,
					},
					MatchParamEntry {
						name: "amount".to_string(),
						value: amount.to_string(),
						kind: "uint256".to_string(),
						indexed: false,
					},
				]),
				hex_signature: Some("0xa9059cbb".to_string()),
			}]),
			events: Some(vec![MatchParamsMap {
				signature: "Transfer(address,address,uint256)".to_string(),
				args: Some(vec![
					MatchParamEntry {
						name: "from".to_string(),
						value: format!("{:#x}", from_addr),
						kind: "address".to_string(),
						indexed: true,
					},
					MatchParamEntry {
						name: "to".to_string(),
						value: format!("{:#x}", to_addr),
						kind: "address".to_string(),
						indexed: true,
					},
					MatchParamEntry {
						name: "amount".to_string(),
						value: amount.to_string(),
						kind: "uint256".to_string(),
						indexed: false,
					},
				]),
				hex_signature: Some(
					"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"
						.to_string(),
				),
			}]),
		};

		assert!(match_args.functions.is_some());
		let functions = match_args.functions.unwrap();
		assert_eq!(functions.len(), 1);
		assert_eq!(functions[0].signature, "transfer(address,uint256)");
		assert_eq!(functions[0].hex_signature, Some("0xa9059cbb".to_string()));

		let function_args = functions[0].args.as_ref().unwrap();
		assert_eq!(function_args.len(), 2);
		assert_eq!(function_args[0].name, "to");
		assert_eq!(function_args[0].kind, "address");
		assert_eq!(function_args[1].name, "amount");
		assert_eq!(function_args[1].kind, "uint256");

		assert!(match_args.events.is_some());
		let events = match_args.events.unwrap();
		assert_eq!(events.len(), 1);
		assert_eq!(events[0].signature, "Transfer(address,address,uint256)");
		assert_eq!(
			events[0].hex_signature,
			Some("0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef".to_string())
		);

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
	fn test_contract_spec_from_json() {
		let json_value = serde_json::json!([{
			"type": "function",
			"name": "transfer",
			"inputs": [
				{
					"name": "to",
					"type": "address",
					"internalType": "address"
				},
				{
					"name": "amount",
					"type": "uint256",
					"internalType": "uint256"
				}
			],
			"outputs": [],
			"stateMutability": "nonpayable"
		}]);

		let contract_spec = ContractSpec::from(json_value);
		let functions: Vec<_> = contract_spec.0.functions().collect();
		assert!(!functions.is_empty());

		let function = &functions[0];
		assert_eq!(function.name, "transfer");
		assert_eq!(function.inputs.len(), 2);
		assert_eq!(function.inputs[0].name, "to");
		assert_eq!(function.inputs[0].ty, "address");
		assert_eq!(function.inputs[1].name, "amount");
		assert_eq!(function.inputs[1].ty, "uint256");
	}

	#[test]
	fn test_contract_spec_from_invalid_json() {
		let invalid_json = serde_json::json!({
			"invalid": "data"
		});

		let contract_spec = ContractSpec::from(invalid_json);
		assert!(contract_spec.0.functions.is_empty());
	}

	#[test]
	fn test_contract_spec_display() {
		let json_value = serde_json::json!([{
			"type": "function",
			"name": "transfer",
			"inputs": [
				{
					"name": "to",
					"type": "address",
					"internalType": "address"
				}
			],
			"outputs": [],
			"stateMutability": "nonpayable"
		}]);

		let contract_spec = ContractSpec::from(json_value);
		let display_str = format!("{}", contract_spec);
		assert!(!display_str.is_empty());
		assert!(display_str.contains("transfer"));
		assert!(display_str.contains("address"));
	}

	#[test]
	fn test_contract_spec_deref() {
		let json_value = serde_json::json!([{
			"type": "function",
			"name": "transfer",
			"inputs": [
				{
					"name": "to",
					"type": "address",
					"internalType": "address"
				}
			],
			"outputs": [],
			"stateMutability": "nonpayable"
		}]);

		let contract_spec = ContractSpec::from(json_value);
		let functions: Vec<_> = contract_spec.functions().collect();
		assert!(!functions.is_empty());
		assert_eq!(functions[0].name, "transfer");
	}

	#[test]
	fn test_contract_spec_from_models() {
		let json_value = serde_json::json!([{
			"type": "function",
			"name": "transfer",
			"inputs": [
				{
					"name": "to",
					"type": "address",
					"internalType": "address"
				}
			],
			"outputs": [],
			"stateMutability": "nonpayable"
		}]);

		let evm_spec = ContractSpec::from(json_value.clone());
		let models_spec = ModelsContractSpec::EVM(evm_spec);
		let converted_spec = ContractSpec::from(models_spec);

		let functions: Vec<_> = converted_spec.functions().collect();
		assert!(!functions.is_empty());
		assert_eq!(functions[0].name, "transfer");
		assert_eq!(functions[0].inputs.len(), 1);
		assert_eq!(functions[0].inputs[0].name, "to");
		assert_eq!(functions[0].inputs[0].ty, "address");

		let stellar_spec = StellarContractSpec::from(vec![]);
		let models_spec = ModelsContractSpec::Stellar(stellar_spec);
		let converted_spec = ContractSpec::from(models_spec);
		assert!(converted_spec.is_empty());
	}
}
