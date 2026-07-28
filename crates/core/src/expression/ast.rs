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
    /// A whole-number literal, e.g. `2`.
    IntegerLiteral { value: i64, span: Span },
    /// A fractional literal, e.g. `1.2`.
    FloatLiteral { value: f64, span: Span },
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
}

impl Expression {
    /// The source range this (sub)expression was parsed from.
    pub fn span(&self) -> Span {
        match self {
            Expression::FieldReference { span, .. }
            | Expression::ConstantReference { span, .. }
            | Expression::IntegerLiteral { span, .. }
            | Expression::FloatLiteral { span, .. }
            | Expression::Negate { span, .. }
            | Expression::Binary { span, .. } => *span,
        }
    }

    /// Names of all fields this expression references, in source order,
    /// duplicates included.
    pub fn field_references(&self) -> Vec<&str> {
        fn collect<'a>(expression: &'a Expression, references: &mut Vec<&'a str>) {
            match expression {
                Expression::FieldReference { name, .. } => references.push(name),
                Expression::ConstantReference { .. }
                | Expression::IntegerLiteral { .. }
                | Expression::FloatLiteral { .. } => {}
                Expression::Negate { operand, .. } => collect(operand, references),
                Expression::Binary { left, right, .. } => {
                    collect(left, references);
                    collect(right, references);
                }
            }
        }

        let mut references = Vec::new();
        collect(self, &mut references);
        references
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
