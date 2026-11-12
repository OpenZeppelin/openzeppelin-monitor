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
pub use executor::{process_script_output, InterpreterExecutor, ScriptExecutor};
pub use factory::ScriptExecutorFactory;
#[cfg(unix)]
pub(crate) use validation::interpreter_is_writable_by_others;
pub use validation::{validate_script_config, validate_script_interpreter};
