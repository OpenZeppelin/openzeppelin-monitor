//! Test helper utilities for EVM monitor matches
//!
//! - `MonitorMatchBuilder`: Builder for creating test `MonitorMatch::EVM` instances

use crate::{
	models::{
		EVMMatchArguments, EVMMonitorMatch, EVMReceiptLog, EVMTransaction, EVMTransactionReceipt,
		MatchConditions, Monitor, MonitorMatch,
	},
	utils::tests::builders::evm::{monitor::MonitorBuilder, transaction::TransactionBuilder},
};

/// Builder for creating test EVM `MonitorMatch` instances
///
/// Defaults to a match on `ethereum_mainnet` for a monitor built with
/// [`MonitorBuilder`], a transaction built with [`TransactionBuilder`], a
/// default receipt, empty logs and no matched conditions.
pub struct MonitorMatchBuilder {
	monitor: Monitor,
	transaction: EVMTransaction,
	receipt: Option<EVMTransactionReceipt>,
	logs: Option<Vec<EVMReceiptLog>>,
	network_slug: String,
	matched_on: MatchConditions,
	matched_on_args: Option<EVMMatchArguments>,
}

impl Default for MonitorMatchBuilder {
	fn default() -> Self {
		Self {
			monitor: MonitorBuilder::new().build(),
			transaction: TransactionBuilder::new().build(),
			receipt: Some(EVMTransactionReceipt::default()),
			logs: Some(vec![]),
			network_slug: "ethereum_mainnet".to_string(),
			matched_on: MatchConditions::default(),
			matched_on_args: None,
		}
	}
}

impl MonitorMatchBuilder {
	/// Creates a new MonitorMatchBuilder instance
	pub fn new() -> Self {
		Self::default()
	}

	/// Sets the monitor that triggered the match
	pub fn monitor(mut self, monitor: Monitor) -> Self {
		self.monitor = monitor;
		self
	}

	/// Sets the matched transaction
	pub fn transaction(mut self, transaction: EVMTransaction) -> Self {
		self.transaction = transaction;
		self
	}

	/// Sets the transaction receipt
	pub fn receipt(mut self, receipt: Option<EVMTransactionReceipt>) -> Self {
		self.receipt = receipt;
		self
	}

	/// Sets the transaction logs
	pub fn logs(mut self, logs: Option<Vec<EVMReceiptLog>>) -> Self {
		self.logs = logs;
		self
	}

	/// Sets the network slug
	pub fn network_slug(mut self, network_slug: &str) -> Self {
		self.network_slug = network_slug.to_string();
		self
	}

	/// Sets the matched conditions
	pub fn matched_on(mut self, matched_on: MatchConditions) -> Self {
		self.matched_on = matched_on;
		self
	}

	/// Sets the decoded arguments of the matched conditions
	pub fn matched_on_args(mut self, matched_on_args: Option<EVMMatchArguments>) -> Self {
		self.matched_on_args = matched_on_args;
		self
	}

	/// Builds the `EVMMonitorMatch`
	pub fn build_evm(self) -> EVMMonitorMatch {
		EVMMonitorMatch {
			monitor: self.monitor,
			transaction: self.transaction,
			receipt: self.receipt,
			logs: self.logs,
			network_slug: self.network_slug,
			matched_on: self.matched_on,
			matched_on_args: self.matched_on_args,
		}
	}

	/// Builds the `MonitorMatch::EVM`
	pub fn build(self) -> MonitorMatch {
		MonitorMatch::EVM(Box::new(self.build_evm()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_monitor_match() {
		let MonitorMatch::EVM(evm_match) = MonitorMatchBuilder::new().build() else {
			panic!("expected an EVM monitor match");
		};

		assert_eq!(evm_match.monitor.name, MonitorBuilder::new().build().name);
		assert_eq!(evm_match.network_slug, "ethereum_mainnet");
		assert!(evm_match.receipt.is_some());
		assert_eq!(evm_match.logs.as_ref().map(Vec::len), Some(0));
		assert!(evm_match.matched_on.functions.is_empty());
		assert!(evm_match.matched_on.events.is_empty());
		assert!(evm_match.matched_on.transactions.is_empty());
		assert!(evm_match.matched_on_args.is_none());
	}

	#[test]
	fn test_custom_monitor_match() {
		let monitor = MonitorBuilder::new().name("custom").build();
		let evm_match = MonitorMatchBuilder::new()
			.monitor(monitor)
			.network_slug("evm_mainnet")
			.receipt(None)
			.logs(None)
			.build_evm();

		assert_eq!(evm_match.monitor.name, "custom");
		assert_eq!(evm_match.network_slug, "evm_mainnet");
		assert!(evm_match.receipt.is_none());
		assert!(evm_match.logs.is_none());
	}
}
