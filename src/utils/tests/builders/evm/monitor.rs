//! Test helper utilities for the EVM Monitor
//!
//! - `MonitorBuilder`: Builder for creating test Monitor instances

use crate::models::{
	AddressWithSpec, ContractSpec, EventCondition, FunctionCondition, MatchConditions, Monitor,
	ScriptLanguage, TransactionCondition, TransactionStatus, TriggerConditions,
};

/// Builder for creating test Monitor instances
pub struct MonitorBuilder {
	name: String,
	networks: Vec<String>,
	paused: bool,
	addresses: Vec<AddressWithSpec>,
	match_conditions: MatchConditions,
	trigger_conditions: Vec<TriggerConditions>,
	triggers: Vec<String>,
}

impl Default for MonitorBuilder {
	fn default() -> Self {
		Self {
			name: "TestMonitor".to_string(),
			networks: vec!["ethereum_mainnet".to_string()],
			paused: false,
			addresses: vec![AddressWithSpec {
				address: "0x0000000000000000000000000000000000000000".to_string(),
				contract_spec: None,
			}],
			match_conditions: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![],
			},
			trigger_conditions: vec![],
			triggers: vec![],
		}
	}
}

impl MonitorBuilder {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn name(mut self, name: &str) -> Self {
		self.name = name.to_string();
		self
	}

	pub fn networks(mut self, networks: Vec<String>) -> Self {
		self.networks = networks;
		self
	}

	pub fn paused(mut self, paused: bool) -> Self {
		self.paused = paused;
		self
	}

	pub fn address(mut self, address: &str) -> Self {
		self.addresses = vec![AddressWithSpec {
			address: address.to_string(),
			contract_spec: None,
		}];
		self
	}

	pub fn addresses(mut self, addresses: Vec<String>) -> Self {
		self.addresses = addresses
			.into_iter()
			.map(|addr| AddressWithSpec {
				address: addr,
				contract_spec: None,
			})
			.collect();
		self
	}

	pub fn add_address(mut self, address: &str) -> Self {
		self.addresses.push(AddressWithSpec {
			address: address.to_string(),
			contract_spec: None,
		});
		self
	}

	pub fn address_with_spec(mut self, address: &str, spec: Option<ContractSpec>) -> Self {
		self.addresses = vec![AddressWithSpec {
			address: address.to_string(),
			contract_spec: spec,
		}];
		self
	}

	pub fn addresses_with_spec(mut self, addresses: Vec<(String, Option<ContractSpec>)>) -> Self {
		self.addresses = addresses
			.into_iter()
			.map(|(addr, spec)| AddressWithSpec {
				address: addr.to_string(),
				contract_spec: spec,
			})
			.collect();
		self
	}

	pub fn function(mut self, signature: &str, expression: Option<String>) -> Self {
		self.match_conditions.functions.push(FunctionCondition {
			signature: signature.to_string(),
			expression,
		});
		self
	}

	pub fn event(mut self, signature: &str, expression: Option<String>) -> Self {
		self.match_conditions.events.push(EventCondition {
			signature: signature.to_string(),
			expression,
		});
		self
	}

	pub fn transaction(mut self, status: TransactionStatus, expression: Option<String>) -> Self {
		self.match_conditions
			.transactions
			.push(TransactionCondition { status, expression });
		self
	}

	pub fn trigger_condition(
		mut self,
		script_path: &str,
		timeout_ms: u32,
		language: ScriptLanguage,
		arguments: Option<Vec<String>>,
	) -> Self {
		self.trigger_conditions.push(TriggerConditions {
			script_path: script_path.to_string(),
			timeout_ms,
			arguments,
			language,
		});
		self
	}

	pub fn triggers(mut self, triggers: Vec<String>) -> Self {
		self.triggers = triggers;
		self
	}

	pub fn match_conditions(mut self, match_conditions: MatchConditions) -> Self {
		self.match_conditions = match_conditions;
		self
	}

	pub fn build(self) -> Monitor {
		Monitor {
			name: self.name,
			networks: self.networks,
			paused: self.paused,
			addresses: self.addresses,
			match_conditions: self.match_conditions,
			trigger_conditions: self.trigger_conditions,
			triggers: self.triggers,
		}
	}
}

#[cfg(test)]
mod tests {
	use crate::models::EVMContractSpec;

	use super::*;
	use serde_json::json;

	#[test]
	fn test_default_monitor() {
		let monitor = MonitorBuilder::new().build();

		assert_eq!(monitor.name, "TestMonitor");
		assert_eq!(monitor.networks, vec!["ethereum_mainnet"]);
		assert!(!monitor.paused);
		assert_eq!(monitor.addresses.len(), 1);
		assert_eq!(
			monitor.addresses[0].address,
			"0x0000000000000000000000000000000000000000"
		);
		assert!(monitor.addresses[0].contract_spec.is_none());
		assert!(monitor.match_conditions.functions.is_empty());
		assert!(monitor.match_conditions.events.is_empty());
		assert!(monitor.match_conditions.transactions.is_empty());
		assert!(monitor.trigger_conditions.is_empty());
		assert!(monitor.triggers.is_empty());
	}

	#[test]
	fn test_basic_builder_methods() {
		let monitor = MonitorBuilder::new()
			.name("MyMonitor")
			.networks(vec!["polygon".to_string()])
			.paused(true)
			.address("0x123")
			.build();

		assert_eq!(monitor.name, "MyMonitor");
		assert_eq!(monitor.networks, vec!["polygon"]);
		assert!(monitor.paused);
		assert_eq!(monitor.addresses.len(), 1);
		assert_eq!(monitor.addresses[0].address, "0x123");
	}

	#[test]
	fn test_address_methods() {
		let monitor = MonitorBuilder::new()
			.addresses(vec!["0x123".to_string(), "0x456".to_string()])
			.add_address("0x789")
			.build();

		assert_eq!(monitor.addresses.len(), 3);
		assert_eq!(monitor.addresses[0].address, "0x123");
		assert_eq!(monitor.addresses[1].address, "0x456");
		assert_eq!(monitor.addresses[2].address, "0x789");
	}

	#[test]
	fn test_address_with_abi() {
		let abi = json!({"some": "abi"});
		let monitor = MonitorBuilder::new()
			.address_with_spec(
				"0x123",
				Some(ContractSpec::EVM(EVMContractSpec::from(abi.clone()))),
			)
			.build();

		assert_eq!(monitor.addresses.len(), 1);
		assert_eq!(monitor.addresses[0].address, "0x123");
		assert_eq!(
			monitor.addresses[0].contract_spec,
			Some(ContractSpec::EVM(EVMContractSpec::from(abi)))
		);
	}

	#[test]
	fn test_addresses_with_abi() {
		let abi1 = json!({"contract_spec": "1"});
		let abi2 = json!({"contract_spec": "2"});
		let monitor = MonitorBuilder::new()
			.addresses_with_spec(vec![
				(
					"0x123".to_string(),
					Some(ContractSpec::EVM(EVMContractSpec::from(abi1.clone()))),
				),
				("0x456".to_string(), None),
				(
					"0x789".to_string(),
					Some(ContractSpec::EVM(EVMContractSpec::from(abi2.clone()))),
				),
			])
			.build();

		assert_eq!(monitor.addresses.len(), 3);
		assert_eq!(monitor.addresses[0].address, "0x123");
		assert_eq!(
			monitor.addresses[0].contract_spec,
			Some(ContractSpec::EVM(EVMContractSpec::from(abi1)))
		);
		assert_eq!(monitor.addresses[1].address, "0x456");
		assert_eq!(monitor.addresses[1].contract_spec, None);
		assert_eq!(monitor.addresses[2].address, "0x789");
		assert_eq!(
			monitor.addresses[2].contract_spec,
			Some(ContractSpec::EVM(EVMContractSpec::from(abi2)))
		);
	}

	#[test]
	fn test_match_conditions() {
		let monitor = MonitorBuilder::new()
			.function("transfer(address,uint256)", Some("value >= 0".to_string()))
			.event("Transfer(address,address,uint256)", None)
			.transaction(TransactionStatus::Success, None)
			.build();

		assert_eq!(monitor.match_conditions.functions.len(), 1);
		assert_eq!(
			monitor.match_conditions.functions[0].signature,
			"transfer(address,uint256)"
		);
		assert_eq!(
			monitor.match_conditions.functions[0].expression,
			Some("value >= 0".to_string())
		);
		assert_eq!(monitor.match_conditions.events.len(), 1);
		assert_eq!(
			monitor.match_conditions.events[0].signature,
			"Transfer(address,address,uint256)"
		);
		assert_eq!(monitor.match_conditions.transactions.len(), 1);
		assert_eq!(
			monitor.match_conditions.transactions[0].status,
			TransactionStatus::Success
		);
	}

	#[test]
	fn test_match_condition() {
		let monitor = MonitorBuilder::new()
			.match_conditions(MatchConditions {
				functions: vec![FunctionCondition {
					signature: "transfer(address,uint256)".to_string(),
					expression: None,
				}],
				events: vec![],
				transactions: vec![],
			})
			.build();
		assert_eq!(monitor.match_conditions.functions.len(), 1);
		assert_eq!(
			monitor.match_conditions.functions[0].signature,
			"transfer(address,uint256)"
		);
		assert!(monitor.match_conditions.events.is_empty());
		assert!(monitor.match_conditions.transactions.is_empty());
	}

	#[test]
	fn test_trigger_conditions() {
		let monitor = MonitorBuilder::new()
			.trigger_condition("script.py", 1000, ScriptLanguage::Python, None)
			.trigger_condition(
				"script.js",
				2000,
				ScriptLanguage::JavaScript,
				Some(vec!["-verbose".to_string()]),
			)
			.build();

		assert_eq!(monitor.trigger_conditions.len(), 2);
		assert_eq!(monitor.trigger_conditions[0].script_path, "script.py");
		assert_eq!(monitor.trigger_conditions[0].timeout_ms, 1000);
		assert_eq!(
			monitor.trigger_conditions[0].language,
			ScriptLanguage::Python
		);
		assert_eq!(monitor.trigger_conditions[1].script_path, "script.js");
		assert_eq!(monitor.trigger_conditions[1].timeout_ms, 2000);
		assert_eq!(
			monitor.trigger_conditions[1].language,
			ScriptLanguage::JavaScript
		);
		assert_eq!(
			monitor.trigger_conditions[1].arguments,
			Some(vec!["-verbose".to_string()])
		);
	}

	#[test]
	fn test_triggers() {
		let monitor = MonitorBuilder::new()
			.triggers(vec!["trigger1".to_string(), "trigger2".to_string()])
			.build();

		assert_eq!(monitor.triggers.len(), 2);
		assert_eq!(monitor.triggers[0], "trigger1");
		assert_eq!(monitor.triggers[1], "trigger2");
	}

	#[test]
	fn test_complex_monitor_build() {
		let abi = json!({"some": "abi"});
		let monitor = MonitorBuilder::new()
			.name("ComplexMonitor")
			.networks(vec!["ethereum".to_string(), "polygon".to_string()])
			.paused(true)
			.addresses(vec!["0x123".to_string(), "0x456".to_string()])
			.add_address("0x789")
			.address_with_spec(
				"0xabc",
				Some(ContractSpec::EVM(EVMContractSpec::from(abi.clone()))),
			)
			.function("transfer(address,uint256)", Some("value >= 0".to_string()))
			.event("Transfer(address,address,uint256)", None)
			.transaction(TransactionStatus::Success, None)
			.trigger_condition("script.py", 1000, ScriptLanguage::Python, None)
			.triggers(vec!["trigger1".to_string(), "trigger2".to_string()])
			.build();

		// Verify final state
		assert_eq!(monitor.name, "ComplexMonitor");
		assert_eq!(monitor.networks, vec!["ethereum", "polygon"]);
		assert!(monitor.paused);
		assert_eq!(monitor.addresses.len(), 1); // address_with_abi overwrites previous addresses
		assert_eq!(monitor.addresses[0].address, "0xabc");
		assert_eq!(
			monitor.addresses[0].contract_spec,
			Some(ContractSpec::EVM(EVMContractSpec::from(abi)))
		);
		assert_eq!(monitor.match_conditions.functions.len(), 1);
		assert_eq!(
			monitor.match_conditions.functions[0].expression,
			Some("value >= 0".to_string())
		);
		assert_eq!(monitor.match_conditions.events.len(), 1);
		assert_eq!(monitor.match_conditions.transactions.len(), 1);
		assert_eq!(monitor.trigger_conditions.len(), 1);
		assert_eq!(monitor.triggers.len(), 2);
	}
}
