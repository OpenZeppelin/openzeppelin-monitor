use std::error::Error;
use std::fmt;

use log::error;

#[derive(Debug)]
pub enum TriggerError {
    NotFound(String),
    ExecutionError(String),
    ConfigurationError(String),
}

impl TriggerError {
    fn format_message(&self) -> String {
        match self {
            TriggerError::NotFound(msg) => format!("Trigger not found: {}", msg),
            TriggerError::ExecutionError(msg) => format!("Trigger execution error: {}", msg),
            TriggerError::ConfigurationError(msg) => {
                format!("Trigger configuration error: {}", msg)
            }
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        let error = TriggerError::NotFound(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn execution_error(msg: impl Into<String>) -> Self {
        let error = TriggerError::ExecutionError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn configuration_error(msg: impl Into<String>) -> Self {
        let error = TriggerError::ConfigurationError(msg.into());
        error!("{}", error.format_message());
        error
    }
}

impl fmt::Display for TriggerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_message())
    }
}

impl Error for TriggerError {}
