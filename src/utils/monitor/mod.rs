//! Monitor utilities for executing and managing blockchain monitors.
//!
//! This module provides functionality for executing monitors against a specific block
//!
//! - execution: Monitor execution logic against a specific block
//! - error: Error types for monitor execution

mod error;
pub use error::MonitorExecutionError;
pub mod execution;
