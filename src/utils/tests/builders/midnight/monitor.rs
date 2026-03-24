//! Test helper utilities for the Midnight Monitor
//!
//! - `MonitorBuilder`: Builder for creating test Monitor instances

use crate::models::{
	AddressWithSpec, ChainConfiguration, EventCondition, FunctionCondition, MatchConditions,
	MidnightMonitorConfig, Monitor, ScriptLanguage, TransactionCondition, TransactionStatus,
	TriggerConditions,
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
	chain_configurations: Vec<ChainConfiguration>,
}

impl Default for MonitorBuilder {
	/// Default monitor builder with a testnet network
	fn default() -> Self {
		Self {
			name: "TestMonitor".to_string(),
			networks: vec!["midnight_testnet".to_string()],
			paused: false,
			addresses: vec![AddressWithSpec {
				address: "0202000000000000000000000000000000000000000000000000000000000000000000"
					.to_string(),
				contract_spec: None,
			}],
			match_conditions: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![],
			},
			trigger_conditions: vec![],
			triggers: vec![],
			chain_configurations: vec![ChainConfiguration {
				midnight: Some(MidnightMonitorConfig::default()),
				..Default::default()
			}],
		}
	}
}

impl MonitorBuilder {
	/// Create a new monitor builder
	pub fn new() -> Self {
		Self::default()
	}

	/// Set the name of the monitor
	pub fn name(mut self, name: &str) -> Self {
		self.name = name.to_string();
		self
	}

	/// Set the networks of the monitor
	pub fn networks(mut self, networks: Vec<String>) -> Self {
		self.networks = networks;
		self
	}

	/// Set the paused state of the monitor
	pub fn paused(mut self, paused: bool) -> Self {
		self.paused = paused;
		self
	}

	/// Add an address to the monitor
	pub fn address(mut self, address: &str) -> Self {
		self.addresses = vec![AddressWithSpec {
			address: address.to_string(),
			contract_spec: None,
		}];
		self
	}

	/// Set the addresses of the monitor
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

	/// Add an address to the monitor
	pub fn add_address(mut self, address: &str) -> Self {
		self.addresses.push(AddressWithSpec {
			address: address.to_string(),
			contract_spec: None,
		});
		self
	}

	/// Add a function to the monitor
	pub fn function(mut self, signature: &str, expression: Option<String>) -> Self {
		self.match_conditions.functions.push(FunctionCondition {
			signature: signature.to_string(),
			expression,
		});
		self
	}

	/// Add an event to the monitor
	pub fn event(mut self, signature: &str, expression: Option<String>) -> Self {
		self.match_conditions.events.push(EventCondition {
			signature: signature.to_string(),
			expression,
		});
		self
	}

	/// Add a transaction to the monitor
	pub fn transaction(mut self, status: TransactionStatus, expression: Option<String>) -> Self {
		self.match_conditions
			.transactions
			.push(TransactionCondition { status, expression });
		self
	}

	/// Add a trigger condition to the monitor
	pub fn trigger_condition(
		mut self,
		script_path: &str,
		timeout_ms: u32,
		language: ScriptLanguage,
		arguments: Option<Vec<String>>,
		runtime_flags: Option<Vec<String>>,
	) -> Self {
		self.trigger_conditions.push(TriggerConditions {
			script_path: script_path.to_string(),
			timeout_ms,
			arguments,
			runtime_flags,
			language,
		});
		self
	}

	/// Add a trigger to the monitor
	pub fn triggers(mut self, triggers: Vec<String>) -> Self {
		self.triggers = triggers;
		self
	}

	/// Set the match conditions of the monitor
	pub fn match_conditions(mut self, match_conditions: MatchConditions) -> Self {
		self.match_conditions = match_conditions;
		self
	}

	/// Build the monitor
	pub fn build(self) -> Monitor {
		Monitor {
			name: self.name,
			networks: self.networks,
			paused: self.paused,
			addresses: self.addresses,
			match_conditions: self.match_conditions,
			trigger_conditions: self.trigger_conditions,
			triggers: self.triggers,
			chain_configurations: self.chain_configurations,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_monitor() {
		let monitor = MonitorBuilder::new().build();

		assert_eq!(monitor.name, "TestMonitor");
		assert_eq!(monitor.networks, vec!["midnight_testnet"]);
		assert!(!monitor.paused);
		assert_eq!(monitor.addresses.len(), 1);
		assert_eq!(
			monitor.addresses[0].address,
			"0202000000000000000000000000000000000000000000000000000000000000000000"
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
			.networks(vec!["midnight_testnet".to_string()])
			.paused(true)
			.address("0202000000000000000000000000000000000000000000000000000000000000000000")
			.build();

		assert_eq!(monitor.name, "MyMonitor");
		assert_eq!(monitor.networks, vec!["midnight_testnet"]);
		assert!(monitor.paused);
		assert_eq!(monitor.addresses.len(), 1);
		assert_eq!(
			monitor.addresses[0].address,
			"0202000000000000000000000000000000000000000000000000000000000000000000"
		);
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
	fn test_match_conditions() {
		let monitor = MonitorBuilder::new()
			.function("transfer(address,uint256)", Some("value >= 0".to_string()))
			.event("Transfer(address,address,uint256)", None)
			.transaction(TransactionStatus::Success, None)
			.build();

		assert_eq!(monitor.match_conditions.functions.len(), 1);
		assert_eq!(
			monitor.match_conditions.functions[0].signature,
			"transfer(address,uint256)".to_string()
		);
		assert_eq!(
			monitor.match_conditions.functions[0].expression,
			Some("value >= 0".to_string())
		);
		assert_eq!(monitor.match_conditions.events.len(), 1);
		assert_eq!(
			monitor.match_conditions.events[0].signature,
			"Transfer(address,address,uint256)".to_string()
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
			.trigger_condition("script.py", 1000, ScriptLanguage::Python, None, None)
			.trigger_condition(
				"script.js",
				2000,
				ScriptLanguage::JavaScript,
				Some(vec!["-verbose".to_string()]),
				None,
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
		assert_eq!(monitor.trigger_conditions[1].runtime_flags, None);
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
		let monitor = MonitorBuilder::new()
			.name("ComplexMonitor")
			.networks(vec!["ethereum".to_string(), "midnight_testnet".to_string()])
			.paused(true)
			.addresses(vec![
				"0x123".to_string(),
				"0202000000000000000000000000000000000000000000000000000000000000000000"
					.to_string(),
			])
			.add_address("0x789")
			.function("transfer(address,uint256)", Some("value >= 0".to_string()))
			.event("Transfer(address,address,uint256)", None)
			.transaction(TransactionStatus::Success, None)
			.trigger_condition("script.py", 1000, ScriptLanguage::Python, None, None)
			.triggers(vec!["trigger1".to_string(), "trigger2".to_string()])
			.build();

		// Verify final state
		assert_eq!(monitor.name, "ComplexMonitor");
		assert_eq!(monitor.networks, vec!["ethereum", "midnight_testnet"]);
		assert!(monitor.paused);
		assert_eq!(monitor.addresses.len(), 3);
		assert_eq!(monitor.addresses[0].address, "0x123");
		assert_eq!(
			monitor.addresses[1].address,
			"0202000000000000000000000000000000000000000000000000000000000000000000"
		);
		assert_eq!(monitor.addresses[2].address, "0x789");
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
