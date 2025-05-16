//! This module defines the abstract syntax tree (AST) for the filter expressions

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralValue<'a> {
	Bool(bool),
	Str(&'a str),
	// Store as str, conversion to specific type is done within chain context
	// Holds both integers and fixed-point numbers
	// e.g., "123", "123.456", "0x123" or hexadecimal
	Number(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOperator {
	Eq,
	Ne,
	Gt,
	Gte,
	Lt,
	Lte,
	StartsWith,
	EndsWith,
	Contains,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogicalOperator {
	And,
	Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accessor<'a> {
	Index(usize),
	Key(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariablePath<'a> {
	pub base: &'a str,
	pub accessors: Vec<Accessor<'a>>,
}

// The left side of a condition can be either a simple variable name or a path
// (e.g., "a.b.c" or "a[0].b")
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionLeft<'a> {
	Simple(&'a str),
	Path(VariablePath<'a>),
}

impl<'a> ConditionLeft<'a> {
	pub fn base_name(&self) -> &'a str {
		match self {
			ConditionLeft::Simple(name) => name,
			ConditionLeft::Path(path) => path.base,
		}
	}

	pub fn accessors(&self) -> &[Accessor] {
		match self {
			ConditionLeft::Simple(_) => &[],
			ConditionLeft::Path(path) => &path.accessors,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Condition<'a> {
	pub left: ConditionLeft<'a>,
	pub operator: ComparisonOperator,
	pub right: LiteralValue<'a>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression<'a> {
	Condition(Condition<'a>),
	Logical {
		left: Box<Expression<'a>>,
		operator: LogicalOperator,
		right: Box<Expression<'a>>,
	},
}
