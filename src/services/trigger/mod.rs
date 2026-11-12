//! Trigger service implementation.
//!
//! This module provides functionality to manage and execute triggers,
//! which are configurable actions that can be initiated based on
//! various conditions.

mod error;
mod script;
mod service;

pub use error::TriggerError;
#[cfg(unix)]
pub(crate) use script::interpreter_is_writable_by_others;
pub use script::{
	process_script_output, validate_script_config, validate_script_interpreter,
	InterpreterExecutor, ScriptError, ScriptExecutor, ScriptExecutorFactory,
};
pub use service::{TriggerExecutionService, TriggerExecutionServiceTrait};
