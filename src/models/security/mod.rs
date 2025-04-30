//! Security models
//!
//! This module contains the security models for the application.
//!
//! - `error`: Error types for security operations
//! - `secret`: Secret management and zeroization

mod error;
mod secret;

pub use error::SecurityError;
pub use secret::{SecretString, SecretValue};
