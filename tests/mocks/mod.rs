//! Mock implementations for testing purposes.
//!
//! This module contains mock implementations of various traits used throughout
//! the application, primarily for testing. It includes mocks for:
//! - Blockchain clients (EVM and Stellar)
//! - Repository interfaces
//!
//! The mocks are implemented using the `mockall` crate.

mod clients;
mod repositories;

#[allow(unused_imports)]
pub use clients::*;
#[allow(unused_imports)]
pub use repositories::*;
