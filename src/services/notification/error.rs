use log::error;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum NotificationError {
    NetworkError(String),
    ConfigError(String),
}

use reqwest;

impl NotificationError {
    fn format_message(&self) -> String {
        match self {
            Self::NetworkError(msg) => format!("Network error: {}", msg),
            Self::ConfigError(msg) => format!("Config error: {}", msg),
        }
    }

    pub fn network_error(msg: impl Into<String>) -> Self {
        let error = Self::NetworkError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn config_error(msg: impl Into<String>) -> Self {
        let error = Self::ConfigError(msg.into());
        error!("{}", error.format_message());
        error
    }
}

impl From<reqwest::Error> for NotificationError {
    fn from(error: reqwest::Error) -> Self {
        Self::network_error(error.to_string())
    }
}

impl fmt::Display for NotificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_message())
    }
}

impl Error for NotificationError {}
