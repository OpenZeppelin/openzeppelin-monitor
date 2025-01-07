use log::error;
use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum RepositoryError {
    ValidationError(String),
    LoadError(String),
    InternalError(String),
}

impl RepositoryError {
    fn format_message(&self) -> String {
        match self {
            Self::ValidationError(msg) => format!("Validation error: {}", msg),
            Self::LoadError(msg) => format!("Load error: {}", msg),
            Self::InternalError(msg) => format!("Internal error: {}", msg),
        }
    }

    pub fn validation_error(msg: impl Into<String>) -> Self {
        let error = Self::ValidationError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn load_error(msg: impl Into<String>) -> Self {
        let error = Self::LoadError(msg.into());
        error!("{}", error.format_message());
        error
    }

    pub fn internal_error(msg: impl Into<String>) -> Self {
        let error = Self::InternalError(msg.into());
        error!("{}", error.format_message());
        error
    }
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_message())
    }
}

impl Error for RepositoryError {}

impl From<std::io::Error> for RepositoryError {
    fn from(err: std::io::Error) -> Self {
        Self::load_error(err.to_string())
    }
}
