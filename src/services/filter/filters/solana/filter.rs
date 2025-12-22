//! Solana blockchain filter implementation for processing and matching blockchain events.
//!
//! This module provides functionality to:
//! - Filter and match Solana blockchain transactions against monitor conditions
//! - Process and decode Solana instructions
//! - Match program logs (events)
//! - Evaluate complex matching expressions

use std::marker::PhantomData;

use async_trait::async_trait;

use crate::{
	models::{
		BlockType, ContractSpec, EventCondition, FunctionCondition, Monitor, MonitorMatch, Network,
		SolanaContractSpec, SolanaMatchArguments, SolanaMatchParamEntry, SolanaMatchParamsMap,
		SolanaMonitorMatch, SolanaTransaction, TransactionCondition, TransactionStatus,
	},
	services::{
		blockchain::BlockChainClient,
		filter::{
			expression::{self, EvaluationError},
			BlockFilter, FilterError,
		},
	},
};

use super::{evaluator::SolanaConditionEvaluator, helpers::are_same_signature};

/// Implementation of the block filter for Solana blockchain
pub struct SolanaBlockFilter<T> {
	pub _client: PhantomData<T>,
}

impl<T> SolanaBlockFilter<T> {
	/// Finds matching transactions based on monitor conditions
	pub fn find_matching_transaction(
		&self,
		transaction: &SolanaTransaction,
		monitor: &Monitor,
		matched_transactions: &mut Vec<TransactionCondition>,
	) {
		let tx_status: TransactionStatus = if transaction.is_success() {
			TransactionStatus::Success
		} else {
			TransactionStatus::Failure
		};

		if monitor.match_conditions.transactions.is_empty() {
			matched_transactions.push(TransactionCondition {
				expression: None,
				status: TransactionStatus::Any,
			});
		} else {
			for condition in &monitor.match_conditions.transactions {
				let status_matches = match &condition.status {
					TransactionStatus::Any => true,
					required_status => *required_status == tx_status,
				};

				if status_matches {
					if let Some(expr) = &condition.expression {
						let tx_params = self.build_transaction_params(transaction);
						match self.evaluate_expression(expr, &tx_params) {
							Ok(true) => {
								matched_transactions.push(TransactionCondition {
									expression: Some(expr.to_string()),
									status: tx_status,
								});
								break;
							}
							Ok(false) => continue,
							Err(e) => {
								tracing::error!(
									"Failed to evaluate transaction expression '{}': {}",
									expr,
									e
								);
								continue;
							}
						}
					} else {
						matched_transactions.push(condition.clone());
					}
				}
			}
		}
	}

	/// Finds matching instructions (functions) in a transaction
	/// Note: Full IDL decoding is not yet implemented - matches on parsed instructions only
	pub fn find_matching_instructions(
		&self,
		transaction: &SolanaTransaction,
		monitor: &Monitor,
		_contract_spec: Option<&SolanaContractSpec>,
		matched_functions: &mut Vec<FunctionCondition>,
		matched_on_args: &mut SolanaMatchArguments,
	) {
		if monitor.match_conditions.functions.is_empty() {
			return;
		}

		// Check parsed instructions (for known programs like SPL Token)
		for instruction in &transaction.0.transaction.instructions {
			if let Some(parsed) = &instruction.parsed {
				let parsed_signature = format!("{}()", parsed.instruction_type);
				for condition in &monitor.match_conditions.functions {
					if are_same_signature(&condition.signature, &parsed_signature)
						|| condition.signature == parsed.instruction_type
					{
						let params = self.build_params_from_parsed(&parsed.info);

						if let Some(expr) = &condition.expression {
							match self.evaluate_expression(expr, &params) {
								Ok(true) => {
									matched_functions.push(FunctionCondition {
										signature: parsed_signature.clone(),
										expression: Some(expr.to_string()),
									});

									if let Some(functions) = &mut matched_on_args.functions {
										functions.push(SolanaMatchParamsMap {
											signature: parsed_signature.clone(),
											args: Some(params.clone()),
										});
									}
									break;
								}
								Ok(false) => continue,
								Err(e) => {
									tracing::error!(
										"Failed to evaluate parsed instruction expression '{}': {}",
										expr,
										e
									);
									continue;
								}
							}
						} else {
							matched_functions.push(FunctionCondition {
								signature: parsed_signature.clone(),
								expression: None,
							});

							if let Some(functions) = &mut matched_on_args.functions {
								functions.push(SolanaMatchParamsMap {
									signature: parsed_signature.clone(),
									args: Some(params),
								});
							}
							break;
						}
					}
				}
			}
		}
	}

	/// Finds matching events (logs) in a transaction
	pub fn find_matching_events(
		&self,
		transaction: &SolanaTransaction,
		monitor: &Monitor,
		_contract_spec: Option<&SolanaContractSpec>,
		matched_events: &mut Vec<EventCondition>,
		matched_on_args: &mut SolanaMatchArguments,
	) {
		if monitor.match_conditions.events.is_empty() {
			return;
		}

		let logs = transaction.logs();
		if logs.is_empty() {
			return;
		}

		// Match on raw log messages (for programs without IDL)
		// Strip parentheses from signature for matching (e.g., "MintTo()" -> "MintTo")
		for condition in &monitor.match_conditions.events {
			let search_pattern = condition
				.signature
				.split('(')
				.next()
				.unwrap_or(&condition.signature);

			for log in logs {
				if log.contains(search_pattern) {
					matched_events.push(EventCondition {
						signature: condition.signature.clone(),
						expression: None,
					});

					if let Some(events) = &mut matched_on_args.events {
						events.push(SolanaMatchParamsMap {
							signature: condition.signature.clone(),
							args: Some(vec![SolanaMatchParamEntry {
								name: "log".to_string(),
								value: log.clone(),
								kind: "string".to_string(),
								indexed: false,
							}]),
						});
					}
					break;
				}
			}
		}
	}

	/// Evaluates an expression against provided parameters
	fn evaluate_expression(
		&self,
		expression: &str,
		args: &[SolanaMatchParamEntry],
	) -> Result<bool, EvaluationError> {
		if expression.trim().is_empty() {
			return Err(EvaluationError::parse_error(
				"Expression cannot be empty".to_string(),
				None,
				None,
			));
		}

		let evaluator = SolanaConditionEvaluator::new(args);

		let parsed_ast = expression::parse(expression).map_err(|e| {
			EvaluationError::parse_error(format!("Failed to parse expression: {}", e), None, None)
		})?;

		expression::evaluate(&parsed_ast, &evaluator)
	}

	/// Builds transaction parameters for expression evaluation
	fn build_transaction_params(
		&self,
		transaction: &SolanaTransaction,
	) -> Vec<SolanaMatchParamEntry> {
		vec![
			SolanaMatchParamEntry {
				name: "signature".to_string(),
				value: transaction.signature().to_string(),
				kind: "string".to_string(),
				indexed: false,
			},
			SolanaMatchParamEntry {
				name: "slot".to_string(),
				value: transaction.slot().to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			SolanaMatchParamEntry {
				name: "fee".to_string(),
				value: transaction.fee().to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			SolanaMatchParamEntry {
				name: "is_success".to_string(),
				value: transaction.is_success().to_string(),
				kind: "bool".to_string(),
				indexed: false,
			},
		]
	}

	/// Builds parameters from parsed instruction info
	fn build_params_from_parsed(&self, info: &serde_json::Value) -> Vec<SolanaMatchParamEntry> {
		let mut params = Vec::new();

		if let serde_json::Value::Object(map) = info {
			for (key, value) in map {
				let (val_str, kind) = match value {
					serde_json::Value::String(s) => (s.clone(), "string".to_string()),
					serde_json::Value::Number(n) => {
						if n.is_u64() {
							(n.to_string(), "u64".to_string())
						} else if n.is_i64() {
							(n.to_string(), "i64".to_string())
						} else {
							(n.to_string(), "f64".to_string())
						}
					}
					serde_json::Value::Bool(b) => (b.to_string(), "bool".to_string()),
					_ => (value.to_string(), "json".to_string()),
				};

				params.push(SolanaMatchParamEntry {
					name: key.clone(),
					value: val_str,
					kind,
					indexed: false,
				});
			}
		}

		params
	}

	/// Gets the Solana contract spec from the generic contract specs
	fn get_solana_spec<'a>(
		contract_specs: Option<&'a [(String, ContractSpec)]>,
		address: &str,
	) -> Option<&'a SolanaContractSpec> {
		contract_specs
			.and_then(|specs| {
				specs
					.iter()
					.find(|(addr, _)| addr.eq_ignore_ascii_case(address))
			})
			.and_then(|(_, spec)| {
				if let ContractSpec::Solana(solana_spec) = spec {
					Some(solana_spec)
				} else {
					None
				}
			})
	}
}

#[async_trait]
impl<T: Send + Sync + Clone + BlockChainClient> BlockFilter for SolanaBlockFilter<T> {
	type Client = T;

	async fn filter_block(
		&self,
		_client: &Self::Client,
		network: &Network,
		block: &BlockType,
		monitors: &[Monitor],
		contract_specs: Option<&[(String, ContractSpec)]>,
	) -> Result<Vec<MonitorMatch>, FilterError> {
		let solana_block = match block {
			BlockType::Solana(block) => block,
			_ => {
				return Err(FilterError::internal_error(
					"Expected Solana block type".to_string(),
					None,
					None,
				));
			}
		};

		let mut all_matches = Vec::new();

		for monitor in monitors {
			let monitored_addresses: Vec<&str> = monitor
				.addresses
				.iter()
				.map(|addr| addr.address.as_str())
				.collect();

			for transaction in &solana_block.transactions {
				let program_ids = transaction.program_ids();

				let involves_monitored = program_ids.iter().any(|program_id| {
					monitored_addresses
						.iter()
						.any(|addr| addr.eq_ignore_ascii_case(program_id))
				});

				if !involves_monitored {
					continue;
				}

				let mut matched_transactions = Vec::new();
				let mut matched_functions = Vec::new();
				let mut matched_events = Vec::new();
				let mut matched_on_args = SolanaMatchArguments {
					functions: Some(Vec::new()),
					events: Some(Vec::new()),
				};

				let matching_address = monitor.addresses.iter().find(|addr| {
					program_ids
						.iter()
						.any(|pid| pid.eq_ignore_ascii_case(&addr.address))
				});

				let contract_spec = matching_address
					.and_then(|addr| Self::get_solana_spec(contract_specs, &addr.address));

				self.find_matching_transaction(transaction, monitor, &mut matched_transactions);

				self.find_matching_instructions(
					transaction,
					monitor,
					contract_spec,
					&mut matched_functions,
					&mut matched_on_args,
				);

				self.find_matching_events(
					transaction,
					monitor,
					contract_spec,
					&mut matched_events,
					&mut matched_on_args,
				);

				let has_transaction_match = !matched_transactions.is_empty();
				let has_function_match = !matched_functions.is_empty();
				let has_event_match = !matched_events.is_empty();

				let should_match = if monitor.match_conditions.functions.is_empty()
					&& monitor.match_conditions.events.is_empty()
				{
					has_transaction_match
				} else {
					(has_function_match || has_event_match) && has_transaction_match
				};

				if !should_match {
					continue;
				}

				tracing::info!(
					slot = solana_block.slot,
					signature = %transaction.signature(),
					monitor_name = %monitor.name,
					program_ids = ?program_ids,
					is_success = transaction.is_success(),
					fee = transaction.fee(),
					matched_transactions = matched_transactions.len(),
					matched_functions = matched_functions.len(),
					matched_events = matched_events.len(),
					"Solana filter: MATCH FOUND!"
				);

				let monitor_match = SolanaMonitorMatch {
					monitor: monitor.clone(),
					transaction: transaction.clone(),
					block: (**solana_block).clone(),
					network_slug: network.slug.clone(),
					matched_on: crate::models::MatchConditions {
						functions: matched_functions,
						events: matched_events,
						transactions: matched_transactions,
					},
					matched_on_args: Some(matched_on_args),
				};

				all_matches.push(MonitorMatch::Solana(Box::new(monitor_match)));
			}
		}

		if !all_matches.is_empty() {
			tracing::info!(
				slot = solana_block.slot,
				total_matches = all_matches.len(),
				"Solana filter: block processing complete with matches"
			);
		}

		Ok(all_matches)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_solana_block_filter_creation() {
		let _filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};
	}

	#[test]
	fn test_build_transaction_params() {
		use crate::models::SolanaTransaction;

		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let tx = SolanaTransaction::default();
		let params = filter.build_transaction_params(&tx);

		assert!(params.iter().any(|p| p.name == "signature"));
		assert!(params.iter().any(|p| p.name == "slot"));
		assert!(params.iter().any(|p| p.name == "fee"));
		assert!(params.iter().any(|p| p.name == "is_success"));
	}
}
