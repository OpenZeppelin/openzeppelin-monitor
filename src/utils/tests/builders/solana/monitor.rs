//! Test helper utilities for Solana Monitor
//!
//! - `MonitorBuilder`: Builder for creating test Monitor instances with Solana configuration

use crate::models::{
	AddressWithSpec, ChainConfiguration, ContractSpec, EventCondition, FunctionCondition,
	MatchConditions, Monitor, ScriptLanguage, SolanaMonitorConfig, TransactionCondition,
	TransactionStatus, TriggerConditions,
};

/// Builder for creating test Monitor instances with Solana configuration
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
	fn default() -> Self {
		Self {
			name: "TestSolanaMonitor".to_string(),
			networks: vec!["solana_mainnet".to_string()],
			paused: false,
			addresses: vec![AddressWithSpec {
				address: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
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
				solana: Some(SolanaMonitorConfig::default()),
				..Default::default()
			}],
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
				address: addr,
				contract_spec: spec,
			})
			.collect();
		self
	}

	/// Adds a function (instruction) condition for Solana
	pub fn function(mut self, signature: &str, expression: Option<String>) -> Self {
		self.match_conditions.functions.push(FunctionCondition {
			signature: signature.to_string(),
			expression,
		});
		self
	}

	/// Adds an event (log) condition for Solana
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
			chain_configurations: self.chain_configurations,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_solana_monitor() {
		let monitor = MonitorBuilder::new().build();

		assert_eq!(monitor.name, "TestSolanaMonitor");
		assert_eq!(monitor.networks, vec!["solana_mainnet"]);
		assert!(!monitor.paused);
		assert_eq!(monitor.addresses.len(), 1);
		assert_eq!(
			monitor.addresses[0].address,
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
		);
		assert!(monitor.addresses[0].contract_spec.is_none());
		assert!(monitor.match_conditions.functions.is_empty());
		assert!(monitor.match_conditions.events.is_empty());
		assert!(monitor.match_conditions.transactions.is_empty());
		assert!(monitor.trigger_conditions.is_empty());
		assert!(monitor.triggers.is_empty());
		// Verify Solana chain configuration is set
		assert!(monitor.chain_configurations[0].solana.is_some());
		assert!(monitor.chain_configurations[0].evm.is_none());
	}

	#[test]
	fn test_basic_builder_methods() {
		let monitor = MonitorBuilder::new()
			.name("MySolanaMonitor")
			.networks(vec!["solana_devnet".to_string()])
			.paused(true)
			.address("11111111111111111111111111111111")
			.build();

		assert_eq!(monitor.name, "MySolanaMonitor");
		assert_eq!(monitor.networks, vec!["solana_devnet"]);
		assert!(monitor.paused);
		assert_eq!(monitor.addresses.len(), 1);
		assert_eq!(
			monitor.addresses[0].address,
			"11111111111111111111111111111111"
		);
	}

	#[test]
	fn test_address_methods() {
		let monitor = MonitorBuilder::new()
			.addresses(vec![
				"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				"11111111111111111111111111111111".to_string(),
			])
			.add_address("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
			.build();

		assert_eq!(monitor.addresses.len(), 3);
		assert_eq!(
			monitor.addresses[0].address,
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
		);
		assert_eq!(
			monitor.addresses[1].address,
			"11111111111111111111111111111111"
		);
		assert_eq!(
			monitor.addresses[2].address,
			"ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"
		);
	}

	#[test]
	fn test_match_conditions() {
		let monitor = MonitorBuilder::new()
			.function("transfer", Some("amount > 1000000".to_string()))
			.event("TransferEvent", None)
			.transaction(TransactionStatus::Success, None)
			.build();

		assert_eq!(monitor.match_conditions.functions.len(), 1);
		assert_eq!(monitor.match_conditions.functions[0].signature, "transfer");
		assert_eq!(
			monitor.match_conditions.functions[0].expression,
			Some("amount > 1000000".to_string())
		);
		assert_eq!(monitor.match_conditions.events.len(), 1);
		assert_eq!(
			monitor.match_conditions.events[0].signature,
			"TransferEvent"
		);
		assert_eq!(monitor.match_conditions.transactions.len(), 1);
		assert_eq!(
			monitor.match_conditions.transactions[0].status,
			TransactionStatus::Success
		);
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
}
