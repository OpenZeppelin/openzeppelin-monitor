use log::error;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum BlockWatcherError {
    SchedulerError(String),
    NetworkError(String),
    ProcessingError(String),
    StorageError(String),
}

impl BlockWatcherError {
    fn format_message(&self) -> String {
        match self {
            Self::SchedulerError(msg) => format!("Scheduler error: {}", msg),
            Self::NetworkError(msg) => format!("Network error: {}", msg),
            Self::ProcessingError(msg) => format!("Processing error: {}", msg),
            Self::StorageError(msg) => format!("Storage error: {}", msg),
        }
    }

    pub fn scheduler_error(msg: impl Into<String>) -> Self {
        let error = Self::SchedulerError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn network_error(msg: impl Into<String>) -> Self {
        let error = Self::NetworkError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn processing_error(msg: impl Into<String>) -> Self {
        let error = Self::ProcessingError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn storage_error(msg: impl Into<String>) -> Self {
        let error = Self::StorageError(msg.into());
        error!("{}", error.format_message());
        error
    }
}

impl fmt::Display for BlockWatcherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_message())
    }
}

impl Error for BlockWatcherError {}
