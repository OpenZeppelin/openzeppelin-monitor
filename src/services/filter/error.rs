use std::error::Error;
use std::fmt;

use log::error;

#[derive(Debug)]
pub enum FilterError {
    BlockTypeMismatch(String),
    NetworkError(String),
    InternalError(String),
}

impl FilterError {
    fn format_message(&self) -> String {
        match self {
            FilterError::BlockTypeMismatch(msg) => format!("Block type mismatch error: {}", msg),
            FilterError::NetworkError(msg) => format!("Network error: {}", msg),
            FilterError::InternalError(msg) => format!("Internal error: {}", msg),
        }
    }

    pub fn block_type_mismatch(msg: impl Into<String>) -> Self {
        let error = FilterError::BlockTypeMismatch(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn network_error(msg: impl Into<String>) -> Self {
        let error = FilterError::NetworkError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        let error = FilterError::InternalError(msg.into());
        error!("{}", error.format_message());
        error
    }
}

impl fmt::Display for FilterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_message())
    }
}

impl Error for FilterError {}
