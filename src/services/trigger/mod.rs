//! Trigger service implementation.
//!
//! This module provides functionality to manage and execute triggers,
//! which are configurable actions that can be initiated based on
//! various conditions.

mod error;
mod script;
mod service;

pub use error::TriggerError;
pub use script::{
	process_script_output, validate_script_config, ScriptError, ScriptExecutor,
	ScriptExecutorFactory,
};
pub use service::{TriggerExecutionService, TriggerExecutionServiceTrait};
