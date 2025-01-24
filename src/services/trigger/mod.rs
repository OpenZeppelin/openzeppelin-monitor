//! Trigger service implementation.
//!
//! This module provides functionality to manage and execute triggers,
//! which are configurable actions that can be initiated based on
//! various conditions.

mod error;
mod service;

pub use error::TriggerError;
pub use service::{TriggerExecutionService, TriggerExecutionServiceTrait};
