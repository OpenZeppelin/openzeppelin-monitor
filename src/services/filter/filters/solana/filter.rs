//! Solana blockchain filter implementation for processing and matching blockchain events.
//!
//! This module provides functionality to:
//! - Filter and match Solana blockchain transactions against monitor conditions
//! - Match program logs (events)
//! - Evaluate complex matching expressions

use std::marker::PhantomData;

use async_trait::async_trait;

use crate::{
	models::{
		BlockType, ContractSpec, EventCondition, Monitor, MonitorMatch, Network,
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

use super::evaluator::SolanaConditionEvaluator;

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

	// Note: Instruction matching functionality is not implemented.
	// This feature requires full IDL (Interface Definition Language) parsing
	// and decoding of arbitrary program instructions, which is not yet implemented.
	// Currently, only event matching (via program logs) is supported.

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
		let mut params = vec![
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
		];

		// Only include fee_payer if it's defined
		if let Some(fee_payer) = transaction.fee_payer() {
			params.push(SolanaMatchParamEntry {
				name: "fee_payer".to_string(),
				value: fee_payer.to_string(),
				kind: "pubkey".to_string(),
				indexed: false,
			});
		}

		// Include accounts with pipe delimiters for exact matching
		let accounts = transaction.accounts();
		if !accounts.is_empty() {
			params.push(SolanaMatchParamEntry {
				name: "accounts".to_string(),
				value: format!("|{}|", accounts.join("|")),
				kind: "string".to_string(),
				indexed: false,
			});
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

				self.find_matching_events(
					transaction,
					monitor,
					contract_spec,
					&mut matched_events,
					&mut matched_on_args,
				);

				let has_transaction_match = !matched_transactions.is_empty();
				let has_event_match = !matched_events.is_empty();

				let should_match = if monitor.match_conditions.events.is_empty() {
					has_transaction_match
				} else {
					has_event_match && has_transaction_match
				};

				if !should_match {
					continue;
				}

				tracing::debug!(
					slot = solana_block.slot,
					signature = %transaction.signature(),
					monitor_name = %monitor.name,
					program_ids = ?program_ids,
					is_success = transaction.is_success(),
					fee = transaction.fee(),
					matched_transactions = matched_transactions.len(),
					matched_events = matched_events.len(),
					"Solana filter: MATCH FOUND!"
				);

				let monitor_match = SolanaMonitorMatch {
					monitor: monitor.clone(),
					transaction: transaction.clone(),
					block: (**solana_block).clone(),
					network_slug: network.slug.clone(),
					matched_on: crate::models::MatchConditions {
						functions: Vec::new(),
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
		// fee_payer and accounts are omitted when empty
		assert!(!params.iter().any(|p| p.name == "fee_payer"));
		assert!(!params.iter().any(|p| p.name == "accounts"));
	}

	#[test]
	fn test_build_transaction_params_with_accounts() {
		use crate::models::{
			SolanaInstruction, SolanaTransactionInfo, SolanaTransactionMessage,
			SolanaTransactionMeta,
		};

		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let fee_payer = "WaLLeT123abcdefghijklmnopqrstuvwxyz12345678";
		let token_account = "TokenAccount456abcdefghijklmnopqrstuvwxy";
		let program_id = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

		let tx_info = SolanaTransactionInfo {
			signature: "test_sig".to_string(),
			slot: 123456789,
			block_time: Some(1234567890),
			transaction: SolanaTransactionMessage {
				account_keys: vec![
					fee_payer.to_string(),
					token_account.to_string(),
					program_id.to_string(),
				],
				recent_blockhash: "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
				instructions: vec![SolanaInstruction {
					program_id_index: 2,
					accounts: vec![0, 1],
					data: "".to_string(),
					parsed: None,
					program: None,
					program_id: None,
				}],
				address_table_lookups: vec![],
			},
			meta: Some(SolanaTransactionMeta {
				fee: 5000,
				pre_balances: vec![],
				post_balances: vec![],
				log_messages: vec![],
				err: None,
				inner_instructions: vec![],
				pre_token_balances: vec![],
				post_token_balances: vec![],
				compute_units_consumed: Some(1000),
				loaded_addresses: None,
			}),
		};

		let tx = SolanaTransaction::from(tx_info);
		let params = filter.build_transaction_params(&tx);

		// Verify fee_payer is set correctly
		let fee_payer_param = params.iter().find(|p| p.name == "fee_payer").unwrap();
		assert_eq!(fee_payer_param.value, fee_payer);
		assert_eq!(fee_payer_param.kind, "pubkey");

		// Verify accounts contains all account keys with pipe delimiters for exact matching
		let accounts_param = params.iter().find(|p| p.name == "accounts").unwrap();
		// Format is |account1|account2|account3| to prevent substring false positives
		assert!(accounts_param.value.contains(&format!("|{}|", fee_payer)));
		assert!(accounts_param
			.value
			.contains(&format!("|{}|", token_account)));
		assert!(accounts_param.value.contains(&format!("|{}|", program_id)));
	}

	#[test]
	fn test_fee_payer_expression_matching() {
		use crate::models::{
			AddressWithSpec, MatchConditions, SolanaInstruction, SolanaTransactionInfo,
			SolanaTransactionMessage, SolanaTransactionMeta,
		};

		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let fee_payer = "WaLLeT123abcdefghijklmnopqrstuvwxyz12345678";

		let monitor = Monitor {
			name: "test_monitor".to_string(),
			paused: false,
			networks: vec!["solana_mainnet".to_string()],
			addresses: vec![AddressWithSpec {
				address: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				contract_spec: None,
			}],
			match_conditions: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![TransactionCondition {
					status: TransactionStatus::Any,
					expression: Some(format!("fee_payer == \"{}\"", fee_payer)),
				}],
			},
			trigger_conditions: vec![],
			triggers: vec![],
			chain_configurations: vec![],
		};

		let tx_info = SolanaTransactionInfo {
			signature: "test_sig".to_string(),
			slot: 123456789,
			block_time: Some(1234567890),
			transaction: SolanaTransactionMessage {
				account_keys: vec![
					fee_payer.to_string(),
					"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				],
				recent_blockhash: "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
				instructions: vec![SolanaInstruction {
					program_id_index: 1,
					accounts: vec![0],
					data: "".to_string(),
					parsed: None,
					program: None,
					program_id: None,
				}],
				address_table_lookups: vec![],
			},
			meta: Some(SolanaTransactionMeta {
				fee: 5000,
				pre_balances: vec![],
				post_balances: vec![],
				log_messages: vec![],
				err: None,
				inner_instructions: vec![],
				pre_token_balances: vec![],
				post_token_balances: vec![],
				compute_units_consumed: Some(1000),
				loaded_addresses: None,
			}),
		};

		let tx = SolanaTransaction::from(tx_info);
		let mut matched_transactions = Vec::new();

		filter.find_matching_transaction(&tx, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 1);
		assert!(matched_transactions[0]
			.expression
			.as_ref()
			.unwrap()
			.contains("fee_payer"));
	}

	#[test]
	fn test_accounts_contains_expression_matching() {
		use crate::models::{
			AddressWithSpec, MatchConditions, SolanaInstruction, SolanaTransactionInfo,
			SolanaTransactionMessage, SolanaTransactionMeta,
		};

		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let token_account = "TokenAccount456abcdefghijklmnopqrstuvwxy";

		let monitor = Monitor {
			name: "test_monitor".to_string(),
			paused: false,
			networks: vec!["solana_mainnet".to_string()],
			addresses: vec![AddressWithSpec {
				address: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				contract_spec: None,
			}],
			match_conditions: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![TransactionCondition {
					status: TransactionStatus::Any,
					expression: Some(format!("accounts contains \"|{}|\"", token_account)),
				}],
			},
			trigger_conditions: vec![],
			triggers: vec![],
			chain_configurations: vec![],
		};

		let tx_info = SolanaTransactionInfo {
			signature: "test_sig".to_string(),
			slot: 123456789,
			block_time: Some(1234567890),
			transaction: SolanaTransactionMessage {
				account_keys: vec![
					"WaLLeT123abcdefghijklmnopqrstuvwxyz12345678".to_string(),
					token_account.to_string(),
					"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				],
				recent_blockhash: "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
				instructions: vec![SolanaInstruction {
					program_id_index: 2,
					accounts: vec![0, 1],
					data: "".to_string(),
					parsed: None,
					program: None,
					program_id: None,
				}],
				address_table_lookups: vec![],
			},
			meta: Some(SolanaTransactionMeta {
				fee: 5000,
				pre_balances: vec![],
				post_balances: vec![],
				log_messages: vec![],
				err: None,
				inner_instructions: vec![],
				pre_token_balances: vec![],
				post_token_balances: vec![],
				compute_units_consumed: Some(1000),
				loaded_addresses: None,
			}),
		};

		let tx = SolanaTransaction::from(tx_info);
		let mut matched_transactions = Vec::new();

		filter.find_matching_transaction(&tx, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 1);
		assert!(matched_transactions[0]
			.expression
			.as_ref()
			.unwrap()
			.contains("accounts"));
	}

	#[test]
	fn test_fee_payer_expression_no_match() {
		use crate::models::{
			AddressWithSpec, MatchConditions, SolanaInstruction, SolanaTransactionInfo,
			SolanaTransactionMessage, SolanaTransactionMeta,
		};

		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = Monitor {
			name: "test_monitor".to_string(),
			paused: false,
			networks: vec!["solana_mainnet".to_string()],
			addresses: vec![AddressWithSpec {
				address: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				contract_spec: None,
			}],
			match_conditions: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: vec![TransactionCondition {
					status: TransactionStatus::Any,
					expression: Some(
						"fee_payer == \"DifferentWallet999999999999999999999999\"".to_string(),
					),
				}],
			},
			trigger_conditions: vec![],
			triggers: vec![],
			chain_configurations: vec![],
		};

		let tx_info = SolanaTransactionInfo {
			signature: "test_sig".to_string(),
			slot: 123456789,
			block_time: Some(1234567890),
			transaction: SolanaTransactionMessage {
				account_keys: vec![
					"WaLLeT123abcdefghijklmnopqrstuvwxyz12345678".to_string(),
					"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				],
				recent_blockhash: "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
				instructions: vec![SolanaInstruction {
					program_id_index: 1,
					accounts: vec![0],
					data: "".to_string(),
					parsed: None,
					program: None,
					program_id: None,
				}],
				address_table_lookups: vec![],
			},
			meta: Some(SolanaTransactionMeta {
				fee: 5000,
				pre_balances: vec![],
				post_balances: vec![],
				log_messages: vec![],
				err: None,
				inner_instructions: vec![],
				pre_token_balances: vec![],
				post_token_balances: vec![],
				compute_units_consumed: Some(1000),
				loaded_addresses: None,
			}),
		};

		let tx = SolanaTransaction::from(tx_info);
		let mut matched_transactions = Vec::new();

		filter.find_matching_transaction(&tx, &monitor, &mut matched_transactions);

		// Should not match because fee_payer is different
		assert!(matched_transactions.is_empty());
	}

	// ============================================================================
	// Expression evaluation tests
	// ============================================================================

	#[test]
	fn test_evaluate_expression_simple_equality() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "amount".to_string(),
			value: "1000000".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];

		let result = filter.evaluate_expression("amount == 1000000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[test]
	fn test_evaluate_expression_simple_inequality() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "amount".to_string(),
			value: "1000000".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];

		let result = filter.evaluate_expression("amount != 500000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[test]
	fn test_evaluate_expression_greater_than() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "amount".to_string(),
			value: "1000000".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];

		// Greater than should be true
		let result = filter.evaluate_expression("amount > 500000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());

		// Greater than should be false
		let result = filter.evaluate_expression("amount > 2000000", &args);
		assert!(result.is_ok());
		assert!(!result.unwrap());
	}

	#[test]
	fn test_evaluate_expression_less_than() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "amount".to_string(),
			value: "1000000".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];

		// Less than should be true
		let result = filter.evaluate_expression("amount < 2000000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());

		// Less than should be false
		let result = filter.evaluate_expression("amount < 500000", &args);
		assert!(result.is_ok());
		assert!(!result.unwrap());
	}

	#[test]
	fn test_evaluate_expression_gte_lte() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "amount".to_string(),
			value: "1000000".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];

		// Gte (equal case)
		let result = filter.evaluate_expression("amount >= 1000000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());

		// Gte (greater case)
		let result = filter.evaluate_expression("amount >= 500000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());

		// Lte (equal case)
		let result = filter.evaluate_expression("amount <= 1000000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());

		// Lte (less case)
		let result = filter.evaluate_expression("amount <= 2000000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[test]
	fn test_evaluate_expression_string_equality() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "recipient".to_string(),
			value: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
			kind: "pubkey".to_string(),
			indexed: false,
		}];

		let result = filter.evaluate_expression(
			"recipient == \"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA\"",
			&args,
		);
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[test]
	fn test_evaluate_expression_boolean() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "is_initialized".to_string(),
			value: "true".to_string(),
			kind: "bool".to_string(),
			indexed: false,
		}];

		let result = filter.evaluate_expression("is_initialized == true", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());

		let result = filter.evaluate_expression("is_initialized != false", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	#[test]
	fn test_evaluate_expression_and_condition() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![
			SolanaMatchParamEntry {
				name: "amount".to_string(),
				value: "1000000".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			SolanaMatchParamEntry {
				name: "fee".to_string(),
				value: "5000".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
		];

		// Both conditions true (use AND keyword, not &&)
		let result = filter.evaluate_expression("amount > 500000 AND fee < 10000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());

		// One condition false
		let result = filter.evaluate_expression("amount > 500000 AND fee > 10000", &args);
		assert!(result.is_ok());
		assert!(!result.unwrap());
	}

	#[test]
	fn test_evaluate_expression_or_condition() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![
			SolanaMatchParamEntry {
				name: "amount".to_string(),
				value: "1000000".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
			SolanaMatchParamEntry {
				name: "fee".to_string(),
				value: "5000".to_string(),
				kind: "u64".to_string(),
				indexed: false,
			},
		];

		// One condition true (use OR keyword, not ||)
		let result = filter.evaluate_expression("amount > 2000000 OR fee < 10000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());

		// Both conditions false
		let result = filter.evaluate_expression("amount > 2000000 OR fee > 10000", &args);
		assert!(result.is_ok());
		assert!(!result.unwrap());
	}

	#[test]
	fn test_evaluate_expression_empty() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![];

		let result = filter.evaluate_expression("", &args);
		assert!(result.is_err());
		let err_str = format!("{:?}", result.unwrap_err());
		assert!(err_str.contains("empty") || err_str.contains("Expression"));
	}

	#[test]
	fn test_evaluate_expression_whitespace_only() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![];

		let result = filter.evaluate_expression("   ", &args);
		assert!(result.is_err());
	}

	#[test]
	fn test_evaluate_expression_unknown_param() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "amount".to_string(),
			value: "1000000".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];

		// Unknown parameter should fail
		let result = filter.evaluate_expression("unknown_param == 100", &args);
		assert!(result.is_err());
	}

	#[test]
	fn test_evaluate_expression_invalid_syntax() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "amount".to_string(),
			value: "1000000".to_string(),
			kind: "u64".to_string(),
			indexed: false,
		}];

		// Invalid expression syntax
		let result = filter.evaluate_expression("amount ===", &args);
		assert!(result.is_err());
	}

	#[test]
	fn test_evaluate_expression_signed_integer() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let args = vec![SolanaMatchParamEntry {
			name: "balance".to_string(),
			value: "-500".to_string(),
			kind: "i64".to_string(),
			indexed: false,
		}];

		// Negative comparison
		let result = filter.evaluate_expression("balance < 0", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());

		// Compare negative to negative
		let result = filter.evaluate_expression("balance > -1000", &args);
		assert!(result.is_ok());
		assert!(result.unwrap());
	}

	// ============================================================================
	// Event matching tests
	// ============================================================================

	fn create_test_monitor_with_events(event_signatures: Vec<&str>) -> Monitor {
		use crate::models::{AddressWithSpec, MatchConditions, TransactionCondition};

		Monitor {
			name: "test_monitor".to_string(),
			paused: false,
			networks: vec!["solana_mainnet".to_string()],
			addresses: vec![AddressWithSpec {
				address: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				contract_spec: None,
			}],
			match_conditions: MatchConditions {
				functions: vec![],
				events: event_signatures
					.into_iter()
					.map(|sig| EventCondition {
						signature: sig.to_string(),
						expression: None,
					})
					.collect(),
				transactions: vec![TransactionCondition {
					status: TransactionStatus::Any,
					expression: None,
				}],
			},
			trigger_conditions: vec![],
			triggers: vec![],
			chain_configurations: vec![],
		}
	}

	fn create_test_transaction_with_logs(logs: Vec<&str>) -> SolanaTransaction {
		use crate::models::{
			SolanaInstruction, SolanaTransactionInfo, SolanaTransactionMessage,
			SolanaTransactionMeta,
		};

		let meta = SolanaTransactionMeta {
			fee: 5000,
			pre_balances: vec![],
			post_balances: vec![],
			log_messages: logs.iter().map(|s| s.to_string()).collect(),
			err: None,
			inner_instructions: vec![],
			pre_token_balances: vec![],
			post_token_balances: vec![],
			compute_units_consumed: Some(1000),
			loaded_addresses: None,
		};

		let tx_info = SolanaTransactionInfo {
			signature: "test_sig".to_string(),
			slot: 123456789,
			block_time: Some(1234567890),
			transaction: SolanaTransactionMessage {
				account_keys: vec!["TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string()],
				recent_blockhash: "4sGjMW1sUnHzSxGspuhpqLDx6wiyjNtZAMdL4VZHirAn".to_string(),
				instructions: vec![SolanaInstruction {
					program_id_index: 0,
					accounts: vec![],
					data: "".to_string(),
					parsed: None,
					program: None,
					program_id: None,
				}],
				address_table_lookups: vec![],
			},
			meta: Some(meta),
		};

		SolanaTransaction::from(tx_info)
	}

	#[test]
	fn test_find_matching_events_single_match() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_events(vec!["Transfer"]);
		let transaction = create_test_transaction_with_logs(vec![
			"Program log: Instruction: Transfer",
			"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
		]);

		let mut matched_events = Vec::new();
		let mut matched_on_args = SolanaMatchArguments {
			functions: Some(Vec::new()),
			events: Some(Vec::new()),
		};

		filter.find_matching_events(
			&transaction,
			&monitor,
			None,
			&mut matched_events,
			&mut matched_on_args,
		);

		assert_eq!(matched_events.len(), 1);
		assert_eq!(matched_events[0].signature, "Transfer");
	}

	#[test]
	fn test_find_matching_events_multiple_patterns() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_events(vec!["Transfer", "MintTo"]);
		let transaction = create_test_transaction_with_logs(vec![
			"Program log: Instruction: Transfer",
			"Program log: Instruction: MintTo",
			"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
		]);

		let mut matched_events = Vec::new();
		let mut matched_on_args = SolanaMatchArguments {
			functions: Some(Vec::new()),
			events: Some(Vec::new()),
		};

		filter.find_matching_events(
			&transaction,
			&monitor,
			None,
			&mut matched_events,
			&mut matched_on_args,
		);

		assert_eq!(matched_events.len(), 2);
	}

	#[test]
	fn test_find_matching_events_no_match() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_events(vec!["Burn"]);
		let transaction = create_test_transaction_with_logs(vec![
			"Program log: Instruction: Transfer",
			"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
		]);

		let mut matched_events = Vec::new();
		let mut matched_on_args = SolanaMatchArguments {
			functions: Some(Vec::new()),
			events: Some(Vec::new()),
		};

		filter.find_matching_events(
			&transaction,
			&monitor,
			None,
			&mut matched_events,
			&mut matched_on_args,
		);

		assert!(matched_events.is_empty());
	}

	#[test]
	fn test_find_matching_events_empty_logs() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_events(vec!["Transfer"]);
		let transaction = create_test_transaction_with_logs(vec![]);

		let mut matched_events = Vec::new();
		let mut matched_on_args = SolanaMatchArguments {
			functions: Some(Vec::new()),
			events: Some(Vec::new()),
		};

		filter.find_matching_events(
			&transaction,
			&monitor,
			None,
			&mut matched_events,
			&mut matched_on_args,
		);

		assert!(matched_events.is_empty());
	}

	#[test]
	fn test_find_matching_events_with_parentheses_signature() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		// Signature with parentheses should still match
		let monitor = create_test_monitor_with_events(vec!["MintTo()"]);
		let transaction = create_test_transaction_with_logs(vec![
			"Program log: Instruction: MintTo",
			"Program TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA success",
		]);

		let mut matched_events = Vec::new();
		let mut matched_on_args = SolanaMatchArguments {
			functions: Some(Vec::new()),
			events: Some(Vec::new()),
		};

		filter.find_matching_events(
			&transaction,
			&monitor,
			None,
			&mut matched_events,
			&mut matched_on_args,
		);

		assert_eq!(matched_events.len(), 1);
	}

	#[test]
	fn test_find_matching_events_captures_log_in_args() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_events(vec!["Transfer"]);
		let transaction = create_test_transaction_with_logs(vec![
			"Program log: Instruction: Transfer amount=1000000",
		]);

		let mut matched_events = Vec::new();
		let mut matched_on_args = SolanaMatchArguments {
			functions: Some(Vec::new()),
			events: Some(Vec::new()),
		};

		filter.find_matching_events(
			&transaction,
			&monitor,
			None,
			&mut matched_events,
			&mut matched_on_args,
		);

		assert!(!matched_events.is_empty());

		// Check that the log was captured in matched_on_args
		let events = matched_on_args.events.unwrap();
		assert!(!events.is_empty());
		let args = events[0].args.as_ref().unwrap();
		assert!(args
			.iter()
			.any(|a| a.name == "log" && a.value.contains("Transfer")));
	}

	// ============================================================================
	// Transaction matching tests
	// ============================================================================

	fn create_test_monitor_with_transactions(
		conditions: Vec<(TransactionStatus, Option<&str>)>,
	) -> Monitor {
		use crate::models::{AddressWithSpec, MatchConditions};

		Monitor {
			name: "test_monitor".to_string(),
			paused: false,
			networks: vec!["solana_mainnet".to_string()],
			addresses: vec![AddressWithSpec {
				address: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
				contract_spec: None,
			}],
			match_conditions: MatchConditions {
				functions: vec![],
				events: vec![],
				transactions: conditions
					.into_iter()
					.map(|(status, expr)| TransactionCondition {
						status,
						expression: expr.map(|e| e.to_string()),
					})
					.collect(),
			},
			trigger_conditions: vec![],
			triggers: vec![],
			chain_configurations: vec![],
		}
	}

	#[test]
	fn test_find_matching_transaction_empty_conditions() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_transactions(vec![]);
		let transaction = create_test_transaction_with_logs(vec![]);

		let mut matched_transactions = Vec::new();
		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		// Empty conditions should match any transaction
		assert_eq!(matched_transactions.len(), 1);
		assert_eq!(matched_transactions[0].status, TransactionStatus::Any);
	}

	#[test]
	fn test_find_matching_transaction_status_any() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_transactions(vec![(TransactionStatus::Any, None)]);
		let transaction = create_test_transaction_with_logs(vec![]);

		let mut matched_transactions = Vec::new();
		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 1);
	}

	#[test]
	fn test_find_matching_transaction_status_success() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		// Transaction with no error is success
		let monitor =
			create_test_monitor_with_transactions(vec![(TransactionStatus::Success, None)]);
		let transaction = create_test_transaction_with_logs(vec![]);

		let mut matched_transactions = Vec::new();
		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		assert_eq!(matched_transactions.len(), 1);
		assert_eq!(matched_transactions[0].status, TransactionStatus::Success);
	}

	#[test]
	fn test_find_matching_transaction_with_expression() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_transactions(vec![(
			TransactionStatus::Any,
			Some("slot > 100000000"),
		)]);
		let transaction = create_test_transaction_with_logs(vec![]);

		let mut matched_transactions = Vec::new();
		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		// Slot is 123456789 which is > 100000000
		assert_eq!(matched_transactions.len(), 1);
	}

	#[test]
	fn test_find_matching_transaction_expression_false() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_transactions(vec![(
			TransactionStatus::Any,
			Some("slot > 200000000"),
		)]);
		let transaction = create_test_transaction_with_logs(vec![]);

		let mut matched_transactions = Vec::new();
		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		// Slot is 123456789 which is < 200000000
		assert!(matched_transactions.is_empty());
	}

	#[test]
	fn test_find_matching_transaction_expression_fee() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_transactions(vec![(
			TransactionStatus::Any,
			Some("fee <= 10000"),
		)]);
		let transaction = create_test_transaction_with_logs(vec![]);

		let mut matched_transactions = Vec::new();
		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		// Fee is 5000 which is <= 10000
		assert_eq!(matched_transactions.len(), 1);
	}

	#[test]
	fn test_find_matching_transaction_is_success_expression() {
		let filter: SolanaBlockFilter<()> = SolanaBlockFilter {
			_client: PhantomData,
		};

		let monitor = create_test_monitor_with_transactions(vec![(
			TransactionStatus::Any,
			Some("is_success == true"),
		)]);
		let transaction = create_test_transaction_with_logs(vec![]);

		let mut matched_transactions = Vec::new();
		filter.find_matching_transaction(&transaction, &monitor, &mut matched_transactions);

		// Transaction has no error so is_success is true
		assert_eq!(matched_transactions.len(), 1);
	}

	// ============================================================================
	// get_solana_spec tests
	// ============================================================================

	#[test]
	fn test_get_solana_spec_found() {
		let solana_spec = SolanaContractSpec::default();

		let specs: Vec<(String, ContractSpec)> = vec![(
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
			ContractSpec::Solana(solana_spec),
		)];

		let result = SolanaBlockFilter::<()>::get_solana_spec(
			Some(&specs),
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
		);

		assert!(result.is_some());
	}

	#[test]
	fn test_get_solana_spec_not_found() {
		let solana_spec = SolanaContractSpec::default();

		let specs: Vec<(String, ContractSpec)> = vec![(
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
			ContractSpec::Solana(solana_spec),
		)];

		let result = SolanaBlockFilter::<()>::get_solana_spec(
			Some(&specs),
			"11111111111111111111111111111111",
		);

		assert!(result.is_none());
	}

	#[test]
	fn test_get_solana_spec_none_specs() {
		let result = SolanaBlockFilter::<()>::get_solana_spec(
			None,
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
		);

		assert!(result.is_none());
	}

	#[test]
	fn test_get_solana_spec_case_insensitive() {
		let solana_spec = SolanaContractSpec::default();

		let specs: Vec<(String, ContractSpec)> = vec![(
			"tokenkegqfezyinwajbnbgkpfxcwubvf9ss623vq5da".to_string(),
			ContractSpec::Solana(solana_spec),
		)];

		let result = SolanaBlockFilter::<()>::get_solana_spec(
			Some(&specs),
			"TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
		);

		assert!(result.is_some());
	}
}
