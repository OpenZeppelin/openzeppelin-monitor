//! Shared logic for parsing and evaluating expressions

mod ast;
mod evaluation;
mod helpers;
mod parsing;

pub use ast::{ComparisonOperator, LiteralValue};
pub use evaluation::{ConditionEvaluator, EvaluationError};
pub use helpers::{compare_ordered_values, evaluate};
pub use parsing::parse;
