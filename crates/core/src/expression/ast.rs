//! The expression tree produced by [`crate::expression::parse_expression`].

/// A half-open byte range into the expression source string. Carried by
/// every token and tree node so any later pass — type checking today,
/// evaluation tomorrow — can point its errors at the exact spot in the
/// source the user wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Byte offset of the first character.
    pub start: usize,
    /// Byte offset one past the last character.
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    /// The smallest span covering both `self` and `other`.
    pub fn merge(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// One-based column of the span's start, for error messages.
    pub fn column(&self) -> usize {
        self.start + 1
    }
}

/// A parsed compute expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    /// A reference to another field of the same item, e.g. `start_date`.
    FieldReference { name: String, span: Span },
    /// A reference to a project constant, e.g. `$constants.daily_rate`.
    ConstantReference { name: String, span: Span },
    /// The evaluation date, `$today`. Resolved once per run by the
    /// caller and injected through the evaluation context — evaluation
    /// itself never reads the clock.
    TodayReference { span: Span },
    /// A whole-number literal, e.g. `2`.
    IntegerLiteral { value: i64, span: Span },
    /// A fractional literal, e.g. `1.2`.
    FloatLiteral { value: f64, span: Span },
    /// A quoted string literal, e.g. `"done"`. Quotes keep literals
    /// visibly distinct from field references, so a typo'd field name
    /// stays an unknown-field error instead of silently becoming text.
    StringLiteral { value: String, span: Span },
    /// A boolean literal, `true` or `false` (reserved words).
    BooleanLiteral { value: bool, span: Span },
    /// A negated subexpression, e.g. `-effort`.
    Negate {
        operand: Box<Expression>,
        span: Span,
    },
    /// Two subexpressions combined with an arithmetic operator.
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// Two subexpressions combined with a comparison operator,
    /// producing a boolean. Non-associative: `a < b < c` is a parse
    /// error, not a chained comparison.
    Comparison {
        operator: ComparisonOperator,
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
}

impl Expression {
    /// The source range this (sub)expression was parsed from.
    pub fn span(&self) -> Span {
        match self {
            Expression::FieldReference { span, .. }
            | Expression::ConstantReference { span, .. }
            | Expression::TodayReference { span }
            | Expression::IntegerLiteral { span, .. }
            | Expression::FloatLiteral { span, .. }
            | Expression::StringLiteral { span, .. }
            | Expression::BooleanLiteral { span, .. }
            | Expression::Negate { span, .. }
            | Expression::Binary { span, .. }
            | Expression::Comparison { span, .. } => *span,
        }
    }

    /// Names of all fields this expression references, in source order,
    /// duplicates included.
    pub fn field_references(&self) -> Vec<&str> {
        fn collect<'a>(expression: &'a Expression, references: &mut Vec<&'a str>) {
            match expression {
                Expression::FieldReference { name, .. } => references.push(name),
                Expression::ConstantReference { .. }
                | Expression::TodayReference { .. }
                | Expression::IntegerLiteral { .. }
                | Expression::FloatLiteral { .. }
                | Expression::StringLiteral { .. }
                | Expression::BooleanLiteral { .. } => {}
                Expression::Negate { operand, .. } => collect(operand, references),
                Expression::Binary { left, right, .. }
                | Expression::Comparison { left, right, .. } => {
                    collect(left, references);
                    collect(right, references);
                }
            }
        }

        let mut references = Vec::new();
        collect(self, &mut references);
        references
    }

    /// Whether this expression references the evaluation date (`$today`)
    /// anywhere — the signal that its result depends on the clock.
    pub fn references_today(&self) -> bool {
        match self {
            Expression::TodayReference { .. } => true,
            Expression::FieldReference { .. }
            | Expression::ConstantReference { .. }
            | Expression::IntegerLiteral { .. }
            | Expression::FloatLiteral { .. }
            | Expression::StringLiteral { .. }
            | Expression::BooleanLiteral { .. } => false,
            Expression::Negate { operand, .. } => operand.references_today(),
            Expression::Binary { left, right, .. } | Expression::Comparison { left, right, .. } => {
                left.references_today() || right.references_today()
            }
        }
    }
}

/// The four arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl std::fmt::Display for BinaryOperator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let symbol = match self {
            BinaryOperator::Add => "+",
            BinaryOperator::Subtract => "-",
            BinaryOperator::Multiply => "*",
            BinaryOperator::Divide => "/",
        };
        write!(formatter, "{symbol}")
    }
}

/// The six comparison operators. All produce a boolean; the ordering
/// four apply to types with a meaningful order, equality additionally
/// to text, boolean, and color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    LessThan,
    LessOrEqual,
    GreaterThan,
    GreaterOrEqual,
}

impl ComparisonOperator {
    /// Whether this operator only asks about equality, which more types
    /// support than ordering.
    pub fn is_equality(self) -> bool {
        matches!(
            self,
            ComparisonOperator::Equal | ComparisonOperator::NotEqual
        )
    }
}

impl std::fmt::Display for ComparisonOperator {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let symbol = match self {
            ComparisonOperator::Equal => "==",
            ComparisonOperator::NotEqual => "!=",
            ComparisonOperator::LessThan => "<",
            ComparisonOperator::LessOrEqual => "<=",
            ComparisonOperator::GreaterThan => ">",
            ComparisonOperator::GreaterOrEqual => ">=",
        };
        write!(formatter, "{symbol}")
    }
}
