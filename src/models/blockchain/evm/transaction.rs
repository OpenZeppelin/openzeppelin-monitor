//! EVM transaction data structures.

use std::ops::Deref;

use serde::{Deserialize, Serialize};
use web3::types::{Transaction as Web3Transaction, H160, H256, U256};

/// Wrapper around Web3 Transaction that implements additional functionality
///
/// This type provides a convenient interface for working with EVM transactions
/// while maintaining compatibility with the web3 types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction(pub Web3Transaction);

impl Transaction {
    /// Get the transaction value (amount of ETH transferred)
    pub fn value(&self) -> &U256 {
        &self.0.value
    }

    /// Get the transaction sender address
    pub fn sender(&self) -> &H160 {
        self.0.from.as_ref().unwrap()
    }

    /// Get the transaction recipient address (None for contract creation)
    pub fn to(&self) -> Option<&H160> {
        self.0.to.as_ref()
    }

    /// Get the gas limit for the transaction
    pub fn gas(&self) -> &U256 {
        &self.0.gas
    }

    /// Get the gas price (None for EIP-1559 transactions)
    pub fn gas_price(&self) -> Option<&U256> {
        self.0.gas_price.as_ref()
    }

    /// Get the transaction nonce
    pub fn nonce(&self) -> &U256 {
        &self.0.nonce
    }

    /// Get the transaction hash
    pub fn hash(&self) -> &H256 {
        &self.0.hash
    }
}

impl From<Web3Transaction> for Transaction {
    fn from(tx: Web3Transaction) -> Self {
        Self(tx)
    }
}

impl Deref for Transaction {
    type Target = Web3Transaction;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
