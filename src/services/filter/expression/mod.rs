//! Shared logic for parsing and evaluating expressions

mod ast;
mod error;
mod evaluation;
mod helpers;
mod parsing;

pub use ast::{ComparisonOperator, LiteralValue};
pub use error::EvaluationError;
pub use evaluation::ConditionEvaluator;
pub use helpers::{compare_ordered_values, evaluate};
pub use parsing::parse;
