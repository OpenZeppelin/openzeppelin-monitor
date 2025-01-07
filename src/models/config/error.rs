use log::error;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum ConfigError {
    ValidationError(String),
    ParseError(String),
    FileError(String),
}

impl ConfigError {
    fn format_message(&self) -> String {
        match self {
            Self::ValidationError(msg) => format!("Validation error: {}", msg),
            Self::ParseError(msg) => format!("Parse error: {}", msg),
            Self::FileError(msg) => format!("File error: {}", msg),
        }
    }

    pub fn validation_error(msg: impl Into<String>) -> Self {
        let error = Self::ValidationError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn parse_error(msg: impl Into<String>) -> Self {
        let error = Self::ParseError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn file_error(msg: impl Into<String>) -> Self {
        let error = Self::FileError(msg.into());
        error!("{}", error.format_message());
        error
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_message())
    }
}

impl Error for ConfigError {}

impl From<std::io::Error> for ConfigError {
    fn from(err: std::io::Error) -> Self {
        Self::file_error(err.to_string())
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(err: serde_json::Error) -> Self {
        Self::parse_error(err.to_string())
    }
}
