use crate::models::{
	MidnightBaseTransaction, MidnightBlock, MidnightBlockDigest, MidnightBlockHeader,
	MidnightRpcBlock, MidnightRpcTransactionEnum,
};

/// A builder for creating test Midnight blocks with default values.
#[derive(Debug)]
pub struct BlockBuilder {
	header: MidnightBlockHeader,
	body: Vec<MidnightRpcTransactionEnum>,
	transactions_index: Vec<(String, String)>,
}

impl Default for BlockBuilder {
	/// Default block builder with a testnet network
	fn default() -> Self {
		Self {
			header: MidnightBlockHeader {
				parent_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
					.to_string(),
				number: "0".to_string(),
				state_root: "0x0000000000000000000000000000000000000000000000000000000000000000"
					.to_string(),
				extrinsics_root:
					"0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
				digest: MidnightBlockDigest { logs: vec![] },
			},
			body: vec![],
			transactions_index: vec![],
		}
	}
}

impl BlockBuilder {
	/// Creates a new BlockBuilder instance.
	pub fn new() -> Self {
		Self::default()
	}

	/// Sets the header of the block.
	pub fn header(mut self, header: MidnightBlockHeader) -> Self {
		self.header = header;
		self
	}

	/// Sets the number of the block.
	pub fn number(mut self, number: u64) -> Self {
		self.header.number = format!("0x{:x}", number);
		self
	}

	/// Sets the parent hash of the block.
	pub fn parent_hash(mut self, parent_hash: String) -> Self {
		self.header.parent_hash = parent_hash;
		self
	}

	/// Sets the body of the block.
	pub fn body(mut self, body: Vec<MidnightRpcTransactionEnum>) -> Self {
		self.body = body;
		self
	}

	/// Adds a transaction to the block.
	pub fn add_transaction(mut self, transaction: MidnightRpcTransactionEnum) -> Self {
		self.body.push(transaction);
		self
	}

	/// Adds a Midnight transaction to the block.
	pub fn add_rpc_transaction(mut self, transaction: MidnightBaseTransaction) -> Self {
		let tx_hash = transaction.clone().tx_hash;
		let tx_operation = MidnightRpcTransactionEnum::MidnightTransaction {
			tx_raw: "".to_string(),
			tx: transaction,
		};

		self.body.push(tx_operation.clone());

		// TODO: Add the transaction index to the block with the serialized transaction
		// should be of type `MidnightNodeTransaction`
		self.transactions_index.push((tx_hash, "".to_string()));
		self
	}

	/// Sets the transactions index of the block.
	pub fn transactions_index(mut self, transactions_index: Vec<(String, String)>) -> Self {
		self.transactions_index = transactions_index;
		self
	}

	/// Builds the Block instance.
	pub fn build(self) -> MidnightBlock {
		let base_block = MidnightRpcBlock {
			header: self.header,
			body: self.body,
			transactions_index: self.transactions_index,
		};

		MidnightBlock::from(base_block)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::utils::tests::builders::midnight::transaction::TransactionBuilder;

	#[test]
	fn test_builder_default() {
		let block = BlockBuilder::new().build();
		assert_eq!(block.header.number, "0");
		assert_eq!(
			block.header.parent_hash,
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
		assert_eq!(
			block.header.state_root,
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
		assert_eq!(
			block.header.extrinsics_root,
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
		assert!(block.body.is_empty());
		assert!(block.transactions_index.is_empty());
	}

	#[test]
	fn test_builder_with_header() {
		let header = MidnightBlockHeader {
			number: "123".to_string(),
			parent_hash: "0xabc".to_string(),
			..Default::default()
		};
		let block = BlockBuilder::new().header(header.clone()).build();
		assert_eq!(block.header, header);
	}

	#[test]
	fn test_builder_with_number() {
		let number = 123u64;
		let block = BlockBuilder::new().number(number).build();
		assert_eq!(block.header.number, format!("0x{:x}", number));
	}

	#[test]
	fn test_builder_with_parent_hash() {
		let parent_hash = "0xabc".to_string();
		let block = BlockBuilder::new().parent_hash(parent_hash.clone()).build();
		assert_eq!(block.header.parent_hash, parent_hash);
	}

	#[test]
	fn test_builder_with_body() {
		let tx1 = TransactionBuilder::new()
			.add_call_operation("0x123".to_string(), "main".to_string())
			.build();
		let tx2 = TransactionBuilder::new()
			.add_deploy_operation("0x456".to_string())
			.build();

		let body = vec![
			MidnightRpcTransactionEnum::MidnightTransaction {
				tx_raw: "".to_string(),
				tx: tx1.into(),
			},
			MidnightRpcTransactionEnum::MidnightTransaction {
				tx_raw: "".to_string(),
				tx: tx2.into(),
			},
		];

		let block = BlockBuilder::new().body(body.clone()).build();
		assert_eq!(block.body, body);
	}

	#[test]
	fn test_builder_add_transaction() {
		let tx = TransactionBuilder::new()
			.add_call_operation("0x123".to_string(), "main".to_string())
			.build();

		let rpc_tx = MidnightRpcTransactionEnum::MidnightTransaction {
			tx_raw: "".to_string(),
			tx: tx.into(),
		};

		let block = BlockBuilder::new().add_transaction(rpc_tx.clone()).build();
		assert_eq!(block.body, vec![rpc_tx]);
	}

	#[test]
	fn test_builder_add_rpc_transaction() {
		let tx = TransactionBuilder::new()
			.add_call_operation("0x123".to_string(), "main".to_string())
			.build();

		let block = BlockBuilder::new()
			.add_rpc_transaction(tx.clone().into())
			.build();
		assert_eq!(
			block.body,
			vec![MidnightRpcTransactionEnum::MidnightTransaction {
				tx_raw: "".to_string(),
				tx: tx.into(),
			}]
		);
	}

	#[test]
	fn test_builder_with_transactions_index() {
		let transactions_index = vec![
			("0x123".to_string(), "0xabc".to_string()),
			("0x456".to_string(), "0xdef".to_string()),
		];
		let block = BlockBuilder::new()
			.transactions_index(transactions_index.clone())
			.build();
		assert_eq!(block.transactions_index, transactions_index);
	}

	#[test]
	fn test_builder_complete_block() {
		// Create test transactions
		let tx1 = TransactionBuilder::new()
			.add_call_operation("0x123".to_string(), "main".to_string())
			.build();
		let tx2 = TransactionBuilder::new()
			.add_deploy_operation("0x456".to_string())
			.build();

		// Create transactions index
		let transactions_index = vec![
			("0x123".to_string(), "0xabc".to_string()),
			("0x456".to_string(), "0xdef".to_string()),
		];

		// Build complete block
		let block = BlockBuilder::new()
			.number(123)
			.parent_hash("0xparent".to_string())
			.add_rpc_transaction(tx1.into())
			.add_rpc_transaction(tx2.into())
			.transactions_index(transactions_index.clone())
			.build();

		// Verify block contents
		assert_eq!(block.header.number, "0x7b");
		assert_eq!(block.header.parent_hash, "0xparent");
		assert_eq!(block.body.len(), 2);
		assert_eq!(block.transactions_index, transactions_index);
	}

	#[test]
	fn test_builder_multiple_transactions() {
		let tx1 = TransactionBuilder::new()
			.add_call_operation("0x123".to_string(), "main".to_string())
			.build();
		let tx2 = TransactionBuilder::new()
			.add_deploy_operation("0x456".to_string())
			.build();
		let tx3 = TransactionBuilder::new()
			.add_fallible_coins_operation()
			.build();

		let block = BlockBuilder::new()
			.add_rpc_transaction(tx1.into())
			.add_rpc_transaction(tx2.into())
			.add_rpc_transaction(tx3.into())
			.build();

		assert_eq!(block.body.len(), 3);
	}
}
