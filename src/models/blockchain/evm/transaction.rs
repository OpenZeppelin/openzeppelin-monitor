use std::ops::Deref;

use serde::{Deserialize, Serialize};
use web3::types::{Transaction as Web3Transaction, H160, H256, U256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction(Web3Transaction);

impl Transaction {
    pub fn value(&self) -> &U256 {
        &self.0.value
    }
    pub fn sender(&self) -> &H160 {
        self.0.from.as_ref().unwrap()
    }

    pub fn to(&self) -> Option<&H160> {
        self.0.to.as_ref()
    }

    pub fn gas(&self) -> &U256 {
        &self.0.gas
    }

    pub fn gas_price(&self) -> Option<&U256> {
        self.0.gas_price.as_ref()
    }

    pub fn nonce(&self) -> &U256 {
        &self.0.nonce
    }

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
