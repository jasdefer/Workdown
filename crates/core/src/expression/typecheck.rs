//! Type checking: infer an expression's result type or reject it.
//!
//! The algebra is closed and total — [`binary_result_type`] is the single
//! source of truth for what each operator does to each operand-type
//! pairing, and everything it doesn't list is an error. The interesting
//! entries are the temporal ones:
//!
//! ```text
//! date ± duration        → date
//! date - date            → duration
//! duration ± duration    → duration
//! duration * number      → duration      (and commuted)
//! duration / number      → duration
//! duration / duration    → float         (ratios: flow efficiency)
//! integer / integer      → float         (division never truncates)
//! ```
//!
//! References are resolved through a caller-supplied [`TypeContext`], so
//! this module needs no knowledge of schemas or resources — the caller
//! maps its field and constant types into [`ExpressionType`]s (or reports
//! them unknown/unsupported).

use super::ast::{BinaryOperator, Expression};

/// The types a (sub)expression can have. A deliberate subset of the field
/// type system: only types with meaningful arithmetic participate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionType {
    Integer,
    Float,
    Date,
    Duration,
}

impl ExpressionType {
    fn is_number(self) -> bool {
        matches!(self, ExpressionType::Integer | ExpressionType::Float)
    }

    /// Whether a value of this type may be assigned to a field declared as
    /// `target`. Identity plus the one widening the algebra allows:
    /// an integer result fits a float field.
    pub fn coerces_to(self, target: ExpressionType) -> bool {
        self == target || (self == ExpressionType::Integer && target == ExpressionType::Float)
    }
}

impl std::fmt::Display for ExpressionType {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ExpressionType::Integer => "integer",
            ExpressionType::Float => "float",
            ExpressionType::Date => "date",
            ExpressionType::Duration => "duration",
        };
        write!(formatter, "{name}")
    }
}

/// How a [`TypeContext`] answers a reference lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceResolution {
    /// No field/constant of this name exists.
    Unknown,
    /// The name exists but its type has no arithmetic (choice, string, …).
    /// Carries the type's display name for the error message.
    Unsupported { type_name: String },
    /// The name exists and participates in the algebra.
    Typed(ExpressionType),
}

/// Resolves field and constant references to their types. Implemented by
/// the schema-loading layer; kept abstract so the checker stays free of
/// schema and resources knowledge (and trivially testable).
pub trait TypeContext {
    fn field(&self, name: &str) -> ReferenceResolution;
    fn constant(&self, name: &str) -> ReferenceResolution;
}

/// Errors produced by type checking.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExpressionTypeError {
    #[error("unknown field '{name}' at column {column}")]
    UnknownField { name: String, column: usize },

    #[error("unknown constant '{name}' at column {column}")]
    UnknownConstant { name: String, column: usize },

    #[error("'{name}' at column {column} has type {type_name}, which cannot be used in an expression (usable: integer, float, date, duration)")]
    UnsupportedReferenceType {
        name: String,
        type_name: String,
        column: usize,
    },

    #[error("cannot apply '{operator}' to {left} and {right} at column {column}")]
    InvalidOperation {
        operator: BinaryOperator,
        left: ExpressionType,
        right: ExpressionType,
        column: usize,
    },

    #[error("cannot negate a {operand} at column {column}")]
    InvalidNegation {
        operand: ExpressionType,
        column: usize,
    },
}

/// Infer the result type of `expression`, resolving references through
/// `context`.
pub fn check_types(
    expression: &Expression,
    context: &impl TypeContext,
) -> Result<ExpressionType, ExpressionTypeError> {
    match expression {
        Expression::IntegerLiteral { .. } => Ok(ExpressionType::Integer),
        Expression::FloatLiteral { .. } => Ok(ExpressionType::Float),

        Expression::FieldReference { name, span } => match context.field(name) {
            ReferenceResolution::Typed(expression_type) => Ok(expression_type),
            ReferenceResolution::Unknown => Err(ExpressionTypeError::UnknownField {
                name: name.clone(),
                column: span.column(),
            }),
            ReferenceResolution::Unsupported { type_name } => {
                Err(ExpressionTypeError::UnsupportedReferenceType {
                    name: name.clone(),
                    type_name,
                    column: span.column(),
                })
            }
        },

        Expression::ConstantReference { name, span } => match context.constant(name) {
            ReferenceResolution::Typed(expression_type) => Ok(expression_type),
            ReferenceResolution::Unknown => Err(ExpressionTypeError::UnknownConstant {
                name: name.clone(),
                column: span.column(),
            }),
            ReferenceResolution::Unsupported { type_name } => {
                Err(ExpressionTypeError::UnsupportedReferenceType {
                    name: name.clone(),
                    type_name,
                    column: span.column(),
                })
            }
        },

        Expression::Negate { operand, span } => {
            let operand_type = check_types(operand, context)?;
            match operand_type {
                ExpressionType::Date => Err(ExpressionTypeError::InvalidNegation {
                    operand: operand_type,
                    column: span.column(),
                }),
                other => Ok(other),
            }
        }

        Expression::Binary {
            operator,
            left,
            right,
            span,
        } => {
            let left_type = check_types(left, context)?;
            let right_type = check_types(right, context)?;
            binary_result_type(*operator, left_type, right_type).ok_or(
                ExpressionTypeError::InvalidOperation {
                    operator: *operator,
                    left: left_type,
                    right: right_type,
                    column: span.column(),
                },
            )
        }
    }
}

/// The algebra: result type of `left operator right`, or `None` when the
/// pairing is meaningless (which the caller turns into an error).
fn binary_result_type(
    operator: BinaryOperator,
    left: ExpressionType,
    right: ExpressionType,
) -> Option<ExpressionType> {
    use ExpressionType::{Date, Duration, Float, Integer};

    match operator {
        BinaryOperator::Add => match (left, right) {
            (Integer, Integer) => Some(Integer),
            (left, right) if left.is_number() && right.is_number() => Some(Float),
            (Date, Duration) | (Duration, Date) => Some(Date),
            (Duration, Duration) => Some(Duration),
            _ => None,
        },
        BinaryOperator::Subtract => match (left, right) {
            (Integer, Integer) => Some(Integer),
            (left, right) if left.is_number() && right.is_number() => Some(Float),
            (Date, Duration) => Some(Date),
            (Date, Date) => Some(Duration),
            (Duration, Duration) => Some(Duration),
            _ => None,
        },
        BinaryOperator::Multiply => match (left, right) {
            (Integer, Integer) => Some(Integer),
            (left, right) if left.is_number() && right.is_number() => Some(Float),
            (Duration, right) if right.is_number() => Some(Duration),
            (left, Duration) if left.is_number() => Some(Duration),
            _ => None,
        },
        BinaryOperator::Divide => match (left, right) {
            // Division never truncates: integer / integer is a float.
            (left, right) if left.is_number() && right.is_number() => Some(Float),
            (Duration, right) if right.is_number() => Some(Duration),
            (Duration, Duration) => Some(Float),
            _ => None,
        },
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::parser::parse_expression;
    use super::*;
    use std::collections::HashMap;

    struct MapContext {
        fields: HashMap<&'static str, ReferenceResolution>,
        constants: HashMap<&'static str, ReferenceResolution>,
    }

    impl TypeContext for MapContext {
        fn field(&self, name: &str) -> ReferenceResolution {
            self.fields
                .get(name)
                .cloned()
                .unwrap_or(ReferenceResolution::Unknown)
        }

        fn constant(&self, name: &str) -> ReferenceResolution {
            self.constants
                .get(name)
                .cloned()
                .unwrap_or(ReferenceResolution::Unknown)
        }
    }

    /// A context with one field of each usable type, one unsupported
    /// field, and matching constants.
    fn context() -> MapContext {
        use ExpressionType::{Date, Duration, Float, Integer};
        let typed = |expression_type| ReferenceResolution::Typed(expression_type);
        MapContext {
            fields: HashMap::from([
                ("count", typed(Integer)),
                ("weight", typed(Float)),
                ("start_date", typed(Date)),
                ("end_date", typed(Date)),
                ("duration", typed(Duration)),
                ("effort", typed(Duration)),
                (
                    "status",
                    ReferenceResolution::Unsupported {
                        type_name: "choice".to_owned(),
                    },
                ),
            ]),
            constants: HashMap::from([
                ("daily_rate", typed(Float)),
                ("work_hours_per_day", typed(Duration)),
            ]),
        }
    }

    fn inferred(source: &str) -> Result<ExpressionType, ExpressionTypeError> {
        check_types(&parse_expression(source).unwrap(), &context())
    }

    // ── The full algebra, one operator at a time ──────────────────────
    //
    // Each table lists every (left, right) pairing with a defined result;
    // the loop then asserts every *other* pairing is rejected, so the
    // tables are exhaustive by construction.

    fn assert_algebra(
        operator: BinaryOperator,
        defined: &[(ExpressionType, ExpressionType, ExpressionType)],
    ) {
        use ExpressionType::{Date, Duration, Float, Integer};
        for left in [Integer, Float, Date, Duration] {
            for right in [Integer, Float, Date, Duration] {
                let expected = defined
                    .iter()
                    .find(|(defined_left, defined_right, _)| {
                        *defined_left == left && *defined_right == right
                    })
                    .map(|(_, _, result)| *result);
                assert_eq!(
                    binary_result_type(operator, left, right),
                    expected,
                    "{left} {operator} {right}"
                );
            }
        }
    }

    #[test]
    fn addition_algebra() {
        use ExpressionType::{Date, Duration, Float, Integer};
        assert_algebra(
            BinaryOperator::Add,
            &[
                (Integer, Integer, Integer),
                (Integer, Float, Float),
                (Float, Integer, Float),
                (Float, Float, Float),
                (Date, Duration, Date),
                (Duration, Date, Date),
                (Duration, Duration, Duration),
            ],
        );
    }

    #[test]
    fn subtraction_algebra() {
        use ExpressionType::{Date, Duration, Float, Integer};
        assert_algebra(
            BinaryOperator::Subtract,
            &[
                (Integer, Integer, Integer),
                (Integer, Float, Float),
                (Float, Integer, Float),
                (Float, Float, Float),
                (Date, Duration, Date),
                (Date, Date, Duration),
                (Duration, Duration, Duration),
            ],
        );
    }

    #[test]
    fn multiplication_algebra() {
        use ExpressionType::{Duration, Float, Integer};
        assert_algebra(
            BinaryOperator::Multiply,
            &[
                (Integer, Integer, Integer),
                (Integer, Float, Float),
                (Float, Integer, Float),
                (Float, Float, Float),
                (Duration, Integer, Duration),
                (Duration, Float, Duration),
                (Integer, Duration, Duration),
                (Float, Duration, Duration),
            ],
        );
    }

    #[test]
    fn division_algebra() {
        use ExpressionType::{Duration, Float, Integer};
        assert_algebra(
            BinaryOperator::Divide,
            &[
                (Integer, Integer, Float),
                (Integer, Float, Float),
                (Float, Integer, Float),
                (Float, Float, Float),
                (Duration, Integer, Duration),
                (Duration, Float, Duration),
                (Duration, Duration, Float),
            ],
        );
    }

    // ── End-to-end inference through the parser ───────────────────────

    #[test]
    fn infers_the_motivating_expressions() {
        assert_eq!(inferred("start_date + duration"), Ok(ExpressionType::Date));
        assert_eq!(
            inferred("end_date - start_date"),
            Ok(ExpressionType::Duration)
        );
        assert_eq!(inferred("effort / duration"), Ok(ExpressionType::Float));
        assert_eq!(
            inferred("effort * $constants.daily_rate"),
            Ok(ExpressionType::Duration)
        );
        assert_eq!(inferred("count * 2"), Ok(ExpressionType::Integer));
        assert_eq!(inferred("count * 1.5"), Ok(ExpressionType::Float));
        assert_eq!(inferred("count / count"), Ok(ExpressionType::Float));
    }

    #[test]
    fn infers_through_parentheses_and_negation() {
        assert_eq!(
            inferred("(end_date - start_date) * 2"),
            Ok(ExpressionType::Duration)
        );
        assert_eq!(inferred("-effort"), Ok(ExpressionType::Duration));
        assert_eq!(inferred("-count"), Ok(ExpressionType::Integer));
    }

    #[test]
    fn negating_a_date_is_an_error() {
        assert!(matches!(
            inferred("-start_date"),
            Err(ExpressionTypeError::InvalidNegation {
                operand: ExpressionType::Date,
                ..
            })
        ));
    }

    #[test]
    fn invalid_operation_reports_operator_types_and_column() {
        assert_eq!(
            inferred("start_date + end_date"),
            Err(ExpressionTypeError::InvalidOperation {
                operator: BinaryOperator::Add,
                left: ExpressionType::Date,
                right: ExpressionType::Date,
                column: 1,
            })
        );
    }

    #[test]
    fn unknown_field_reports_name_and_column() {
        assert_eq!(
            inferred("effort + typo"),
            Err(ExpressionTypeError::UnknownField {
                name: "typo".to_owned(),
                column: 10,
            })
        );
    }

    #[test]
    fn unknown_constant_reports_name_and_column() {
        assert_eq!(
            inferred("effort * $constants.typo"),
            Err(ExpressionTypeError::UnknownConstant {
                name: "typo".to_owned(),
                column: 10,
            })
        );
    }

    #[test]
    fn unsupported_reference_type_reports_the_type() {
        assert_eq!(
            inferred("status + 1"),
            Err(ExpressionTypeError::UnsupportedReferenceType {
                name: "status".to_owned(),
                type_name: "choice".to_owned(),
                column: 1,
            })
        );
    }

    #[test]
    fn first_error_in_evaluation_order_wins() {
        // Both operands are broken; the left one is reported.
        assert!(matches!(
            inferred("typo_a + typo_b"),
            Err(ExpressionTypeError::UnknownField { name, .. }) if name == "typo_a"
        ));
    }

    #[test]
    fn integer_widens_to_float_but_nothing_else_coerces() {
        use ExpressionType::{Date, Duration, Float, Integer};
        assert!(Integer.coerces_to(Float));
        assert!(Integer.coerces_to(Integer));
        assert!(!Float.coerces_to(Integer));
        assert!(!Duration.coerces_to(Float));
        assert!(!Date.coerces_to(Duration));
    }
}
