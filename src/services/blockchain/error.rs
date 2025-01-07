use crate::services::blockwatcher::BlockWatcherError;
use log::error;

#[derive(Debug)]
pub enum BlockChainError {
    ConnectionError(String),
    RequestError(String),
    BlockNotFound(u64),
    TransactionError(String),
    InternalError(String),
}

impl BlockChainError {
    fn format_message(&self) -> String {
        match self {
            Self::ConnectionError(msg) => format!("Connection error: {}", msg),
            Self::RequestError(msg) => format!("Request error: {}", msg),
            Self::BlockNotFound(number) => format!("Block not found: {}", number),
            Self::TransactionError(msg) => format!("Transaction error: {}", msg),
            Self::InternalError(msg) => format!("Internal error: {}", msg),
        }
    }

    pub fn connection_error(msg: impl Into<String>) -> Self {
        let error = Self::ConnectionError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn request_error(msg: impl Into<String>) -> Self {
        let error = Self::RequestError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn block_not_found(number: u64) -> Self {
        let error = Self::BlockNotFound(number);
        error!("{}", error.format_message());
        error
    }

    pub fn transaction_error(msg: impl Into<String>) -> Self {
        let error = Self::TransactionError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        let error = Self::InternalError(msg.into());
        error!("{}", error.format_message());
        error
    }
}

impl std::fmt::Display for BlockChainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_message())
    }
}

impl std::error::Error for BlockChainError {}

// Update From implementation to use constructor
impl From<web3::Error> for BlockChainError {
    fn from(err: web3::Error) -> Self {
        Self::request_error(err.to_string())
    }
}

// Conversion to BlockWatcherError
impl From<BlockChainError> for BlockWatcherError {
    fn from(err: BlockChainError) -> Self {
        BlockWatcherError::network_error(err.to_string())
    }
}
