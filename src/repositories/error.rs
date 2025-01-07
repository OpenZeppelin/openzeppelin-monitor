//! Error types for repository operations.
//!
//! This module defines the error types that can occur during repository operations,
//! including validation errors, loading errors, and internal errors. It provides
//! a consistent error handling interface across all repository implementations.

use log::error;
use std::error::Error;
use std::fmt;

/// Errors that can occur during repository operations
#[derive(Debug)]
pub enum RepositoryError {
    /// Error that occurs when configuration validation fails
    ValidationError(String),

    /// Error that occurs when loading configurations from files
    LoadError(String),

    /// Error that occurs due to internal repository operations
    InternalError(String),
}

impl RepositoryError {
    /// Format an error message for display
    ///
    /// Creates a human-readable error message based on the error type.
    fn format_message(&self) -> String {
        match self {
            Self::ValidationError(msg) => format!("Validation error: {}", msg),
            Self::LoadError(msg) => format!("Load error: {}", msg),
            Self::InternalError(msg) => format!("Internal error: {}", msg),
        }
    }

    /// Create a new validation error with the given message
    ///
    /// Also logs the error message at the error level.
    pub fn validation_error(msg: impl Into<String>) -> Self {
        let error = Self::ValidationError(msg.into());
        error!("{}", error.format_message());
        error
    }

    /// Create a new load error with the given message
    ///
    /// Also logs the error message at the error level.
    pub fn load_error(msg: impl Into<String>) -> Self {
        let error = Self::LoadError(msg.into());
        error!("{}", error.format_message());
        error
    }

    /// Create a new internal error with the given message
    ///
    /// Also logs the error message at the error level.
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
