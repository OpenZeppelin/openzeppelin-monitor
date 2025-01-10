//! Transaction and event filtering functionality.
//!
//! Implements the core filtering logic for monitoring blockchain activity:
//! - Block filtering for different chain types
//! - Match handling and processing
//! - Chain-specific helper functions

mod error;
mod filter_match;
mod filters;

pub mod helpers;

pub use error::FilterError;
pub use filter_match::handle_match;
pub use filters::{BlockFilter, EVMBlockFilter, FilterService, StellarBlockFilter};
