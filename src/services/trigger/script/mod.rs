//! Trigger script service implementation.
//!
//! This module provides functionality to manage and execute triggers,
//! which are configurable actions that can be initiated based on
//! various conditions.

mod error;
mod executor;
mod factory;
mod validation;
pub use error::ScriptError;
pub use executor::{process_script_output, ScriptExecutor};
pub use factory::ScriptExecutorFactory;
pub use validation::validate_script_config;
