//! Transaction and event filtering functionality.
//!
//! Implements the core filtering logic for monitoring blockchain activity:
//! - Block filtering for different chain types
//! - Match handling and processing
//! - Chain-specific helper functions

mod error;
mod expression;
mod filter_match;
mod filters;

pub use error::FilterError;
pub use filter_match::handle_match;

pub use filters::{
	evm::helpers as evm_helpers, stellar::helpers as stellar_helpers, BlockFilter, EVMArgs,
	EVMBlockFilter, EVMConditionEvaluator, EventMap, FilterService, StellarArgs,
	StellarBlockFilter, StellarConditionEvaluator,
};

pub use expression::{ComparisonOperator, ConditionEvaluator, EvaluationError, LiteralValue};
