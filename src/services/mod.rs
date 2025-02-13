//! Core services implementing the business logic.
//!
//! This module contains the main service implementations:
//! - `blockchain`: Blockchain client interfaces and implementations
//! - `blockwatcher`: Block monitoring and processing
//! - `filter`: Transaction and event filtering logic
//! - `notification`: Alert and notification handling
//! - `trigger`: Trigger evaluation and execution

pub mod blockchain;
pub mod blockwatcher;
pub mod filter;
pub mod notification;
pub mod trigger;
