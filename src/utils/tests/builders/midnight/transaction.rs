use crate::models::{MidnightBaseTransaction, MidnightOperation, MidnightTransaction};

/// A builder for creating test Midnight transactions with default values.
#[derive(Debug)]
pub struct TransactionBuilder {
	tx_hash: String,
	operations: Vec<MidnightOperation>,
	identifiers: Vec<String>,
}

impl Default for TransactionBuilder {
	/// Default transaction builder with a testnet transaction hash
	fn default() -> Self {
		Self {
			tx_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
				.to_string(),
			operations: vec![],
			identifiers: vec![],
		}
	}
}

impl TransactionBuilder {
	/// Creates a new TransactionBuilder instance.
	pub fn new() -> Self {
		Self::default()
	}

	/// Sets the hash of the transaction.
	pub fn hash(mut self, hash: String) -> Self {
		self.tx_hash = hash;
		self
	}

	/// Sets the operations of the transaction.
	pub fn operations(mut self, operations: Vec<MidnightOperation>) -> Self {
		self.operations = operations;
		self
	}

	/// Adds an operation to the transaction.
	pub fn add_operation(mut self, operation: MidnightOperation) -> Self {
		self.operations.push(operation);
		self
	}

	/// Adds a call operation to the transaction.
	pub fn add_call_operation(mut self, address: String, entry_point: String) -> Self {
		self.operations.push(MidnightOperation::Call {
			address,
			entry_point: hex::encode(entry_point.as_bytes()),
		});
		self
	}

	/// Adds a deploy operation to the transaction.
	pub fn add_deploy_operation(mut self, address: String) -> Self {
		self.operations.push(MidnightOperation::Deploy { address });
		self
	}

	/// Adds a fallible coins operation to the transaction.
	pub fn add_fallible_coins_operation(mut self) -> Self {
		self.operations.push(MidnightOperation::FallibleCoins);
		self
	}

	/// Adds a guaranteed coins operation to the transaction.
	pub fn add_guaranteed_coins_operation(mut self) -> Self {
		self.operations.push(MidnightOperation::GuaranteedCoins);
		self
	}

	/// Adds a maintain operation to the transaction.
	pub fn add_maintain_operation(mut self, address: String) -> Self {
		self.operations
			.push(MidnightOperation::Maintain { address });
		self
	}

	/// Adds a claim mint operation to the transaction.
	pub fn add_claim_mint_operation(mut self, value: u128, coin_type: String) -> Self {
		self.operations
			.push(MidnightOperation::ClaimMint { value, coin_type });
		self
	}

	/// Sets the identifiers of the transaction.
	pub fn identifiers(mut self, identifiers: Vec<String>) -> Self {
		self.identifiers = identifiers;
		self
	}

	/// Builds the Transaction instance.
	pub fn build(self) -> MidnightTransaction {
		let base_tx = MidnightBaseTransaction {
			tx_hash: self.tx_hash,
			operations: self.operations,
			identifiers: self.identifiers,
		};

		MidnightTransaction::from(base_tx)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_builder_default() {
		let tx = TransactionBuilder::new().build();
		assert_eq!(
			tx.tx_hash,
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
		assert!(tx.operations.is_empty());
		assert!(tx.identifiers.is_empty());
	}

	#[test]
	fn test_builder_with_hash() {
		let hash = "0x123abc".to_string();
		let tx = TransactionBuilder::new().hash(hash.clone()).build();
		assert_eq!(tx.tx_hash, hash);
	}

	#[test]
	fn test_builder_with_operations() {
		let operations = vec![
			MidnightOperation::Call {
				address: "0x123".to_string(),
				entry_point: "main".to_string(),
			},
			MidnightOperation::Deploy {
				address: "0x456".to_string(),
			},
		];
		let tx = TransactionBuilder::new()
			.operations(operations.clone())
			.build();
		assert_eq!(tx.operations, operations);
	}

	#[test]
	fn test_builder_with_identifiers() {
		let identifiers = vec!["id1".to_string(), "id2".to_string()];
		let tx = TransactionBuilder::new()
			.identifiers(identifiers.clone())
			.build();
		assert_eq!(tx.identifiers, identifiers);
	}

	#[test]
	fn test_builder_add_operation() {
		let operation = MidnightOperation::Call {
			address: "0x123".to_string(),
			entry_point: "main".to_string(),
		};
		let tx = TransactionBuilder::new()
			.add_operation(operation.clone())
			.build();
		assert_eq!(tx.operations, vec![operation]);
	}

	#[test]
	fn test_builder_add_call_operation() {
		let address = "0x123".to_string();
		let entry_point = "main".to_string();
		let tx = TransactionBuilder::new()
			.add_call_operation(address.clone(), entry_point.clone())
			.build();
		assert_eq!(
			tx.operations,
			vec![MidnightOperation::Call {
				address,
				entry_point: hex::encode(entry_point.as_bytes()),
			}]
		);
	}

	#[test]
	fn test_builder_add_deploy_operation() {
		let address = "0x123".to_string();
		let tx = TransactionBuilder::new()
			.add_deploy_operation(address.clone())
			.build();
		assert_eq!(tx.operations, vec![MidnightOperation::Deploy { address }]);
	}

	#[test]
	fn test_builder_add_fallible_coins_operation() {
		let tx = TransactionBuilder::new()
			.add_fallible_coins_operation()
			.build();
		assert_eq!(tx.operations, vec![MidnightOperation::FallibleCoins]);
	}

	#[test]
	fn test_builder_add_guaranteed_coins_operation() {
		let tx = TransactionBuilder::new()
			.add_guaranteed_coins_operation()
			.build();
		assert_eq!(tx.operations, vec![MidnightOperation::GuaranteedCoins]);
	}

	#[test]
	fn test_builder_add_maintain_operation() {
		let address = "0x123".to_string();
		let tx = TransactionBuilder::new()
			.add_maintain_operation(address.clone())
			.build();
		assert_eq!(tx.operations, vec![MidnightOperation::Maintain { address }]);
	}

	#[test]
	fn test_builder_add_claim_mint_operation() {
		let value = 100u128;
		let coin_type = "ETH".to_string();
		let tx = TransactionBuilder::new()
			.add_claim_mint_operation(value, coin_type.clone())
			.build();
		assert_eq!(
			tx.operations,
			vec![MidnightOperation::ClaimMint { value, coin_type }]
		);
	}

	#[test]
	fn test_builder_complete_transaction() {
		let tx = TransactionBuilder::new()
			.hash("0x123abc".to_string())
			.add_call_operation("0x123".to_string(), "main".to_string())
			.add_deploy_operation("0x456".to_string())
			.add_fallible_coins_operation()
			.add_guaranteed_coins_operation()
			.add_maintain_operation("0x789".to_string())
			.add_claim_mint_operation(100u128, "ETH".to_string())
			.identifiers(vec!["id1".to_string(), "id2".to_string()])
			.build();

		assert_eq!(tx.tx_hash, "0x123abc");
		assert_eq!(tx.operations.len(), 6);
		assert_eq!(tx.identifiers.len(), 2);
	}
}
