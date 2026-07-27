//! Evaluation: compute an expression's value for one item.
//!
//! The runtime mirror of the type checker behind [`super::check_types`]:
//! every operand-type pairing the algebra defines is implemented here
//! with checked arithmetic, and everything else returns
//! [`EvaluateError::InvalidOperation`] — which the caller treats as a
//! silent skip, because an expression that reaches evaluation with an
//! undefined pairing was already reported by the schema-level type
//! check.
//!
//! Values are the evaluator's own [`Value`] enum, resolved through a
//! caller-supplied [`ValueContext`] — the module stays free of schema
//! and item knowledge, like the type checker. Dates travel as
//! [`Value::Timestamp`], seconds since the Unix epoch, so sub-day
//! precision survives intermediate steps (`start + 4h + 4h` must equal
//! `start + 8h`); the *caller* rounds the final timestamp onto a
//! calendar day with its configured rounding mode.

use super::ast::{BinaryOperator, Expression};

/// A value during evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Integer(i64),
    Float(f64),
    /// Canonical seconds, like the duration field type.
    Duration(i64),
    /// A point in time as seconds since the Unix epoch. Dates enter
    /// evaluation as midnight timestamps and leave with whatever
    /// sub-day remainder the arithmetic produced.
    Timestamp(i64),
}

/// Resolves field and constant references to their current values for
/// the item being evaluated. `None` means the reference has no usable
/// value here (absent, or a type outside the algebra).
pub trait ValueContext {
    fn field(&self, name: &str) -> Option<Value>;
    fn constant(&self, name: &str) -> Option<Value>;
}

/// Ways evaluation can fail for a specific item.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvaluateError {
    /// A referenced field or constant has no usable value on this item.
    #[error("input '{name}' has no value")]
    MissingInput { name: String },

    /// Division by zero (integer, float, or zero-length duration).
    #[error("division by zero")]
    DivisionByZero,

    /// Integer or duration arithmetic overflowed, or a result left the
    /// representable range.
    #[error("arithmetic overflow")]
    Overflow,

    /// A float result came out non-finite (infinity or NaN).
    #[error("result is not a finite number")]
    NotFinite,

    /// Operator applied to operand types outside the algebra. Already
    /// reported by the schema-level type check; callers skip silently.
    #[error("operation is not defined for these operand types")]
    InvalidOperation,
}

/// Evaluate `expression`, resolving references through `context`.
pub fn evaluate(
    expression: &Expression,
    context: &impl ValueContext,
) -> Result<Value, EvaluateError> {
    match expression {
        Expression::IntegerLiteral { value, .. } => Ok(Value::Integer(*value)),
        Expression::FloatLiteral { value, .. } => Ok(Value::Float(*value)),

        Expression::FieldReference { name, .. } => context
            .field(name)
            .ok_or_else(|| EvaluateError::MissingInput { name: name.clone() }),
        Expression::ConstantReference { name, .. } => context
            .constant(name)
            .ok_or_else(|| EvaluateError::MissingInput { name: name.clone() }),

        Expression::Negate { operand, .. } => match evaluate(operand, context)? {
            Value::Integer(value) => value
                .checked_neg()
                .map(Value::Integer)
                .ok_or(EvaluateError::Overflow),
            Value::Float(value) => finite(-value),
            Value::Duration(seconds) => seconds
                .checked_neg()
                .map(Value::Duration)
                .ok_or(EvaluateError::Overflow),
            Value::Timestamp(_) => Err(EvaluateError::InvalidOperation),
        },

        Expression::Binary {
            operator,
            left,
            right,
            ..
        } => {
            let left = evaluate(left, context)?;
            let right = evaluate(right, context)?;
            apply(*operator, left, right)
        }
    }
}

/// Apply one operator to two values — the runtime half of the algebra.
/// Arm order matches `typecheck::binary_result_type`.
fn apply(operator: BinaryOperator, left: Value, right: Value) -> Result<Value, EvaluateError> {
    use Value::{Duration, Integer, Timestamp};

    match operator {
        BinaryOperator::Add => match (left, right) {
            (Integer(a), Integer(b)) => {
                a.checked_add(b).map(Integer).ok_or(EvaluateError::Overflow)
            }
            (a, b) if both_numbers(a, b) => finite(as_float(a) + as_float(b)),
            (Timestamp(t), Duration(d)) | (Duration(d), Timestamp(t)) => t
                .checked_add(d)
                .map(Timestamp)
                .ok_or(EvaluateError::Overflow),
            (Duration(a), Duration(b)) => a
                .checked_add(b)
                .map(Duration)
                .ok_or(EvaluateError::Overflow),
            _ => Err(EvaluateError::InvalidOperation),
        },
        BinaryOperator::Subtract => match (left, right) {
            (Integer(a), Integer(b)) => {
                a.checked_sub(b).map(Integer).ok_or(EvaluateError::Overflow)
            }
            (a, b) if both_numbers(a, b) => finite(as_float(a) - as_float(b)),
            (Timestamp(t), Duration(d)) => t
                .checked_sub(d)
                .map(Timestamp)
                .ok_or(EvaluateError::Overflow),
            (Timestamp(a), Timestamp(b)) => a
                .checked_sub(b)
                .map(Duration)
                .ok_or(EvaluateError::Overflow),
            (Duration(a), Duration(b)) => a
                .checked_sub(b)
                .map(Duration)
                .ok_or(EvaluateError::Overflow),
            _ => Err(EvaluateError::InvalidOperation),
        },
        BinaryOperator::Multiply => match (left, right) {
            (Integer(a), Integer(b)) => {
                a.checked_mul(b).map(Integer).ok_or(EvaluateError::Overflow)
            }
            (a, b) if both_numbers(a, b) => finite(as_float(a) * as_float(b)),
            (Duration(d), scale) | (scale, Duration(d)) if is_number(scale) => {
                scale_duration(d, as_float(scale))
            }
            _ => Err(EvaluateError::InvalidOperation),
        },
        BinaryOperator::Divide => match (left, right) {
            // Division never truncates: integer / integer is a float.
            (a, b) if both_numbers(a, b) => {
                let divisor = as_float(b);
                if divisor == 0.0 {
                    return Err(EvaluateError::DivisionByZero);
                }
                finite(as_float(a) / divisor)
            }
            (Duration(d), scale) if is_number(scale) => {
                let divisor = as_float(scale);
                if divisor == 0.0 {
                    return Err(EvaluateError::DivisionByZero);
                }
                scale_duration(d, 1.0 / divisor)
            }
            (Duration(a), Duration(b)) => {
                if b == 0 {
                    return Err(EvaluateError::DivisionByZero);
                }
                finite(a as f64 / b as f64)
            }
            _ => Err(EvaluateError::InvalidOperation),
        },
    }
}

fn is_number(value: Value) -> bool {
    matches!(value, Value::Integer(_) | Value::Float(_))
}

fn both_numbers(left: Value, right: Value) -> bool {
    is_number(left) && is_number(right)
}

/// Numeric value as f64. Only called on values `is_number` accepted.
fn as_float(value: Value) -> f64 {
    match value {
        Value::Integer(value) => value as f64,
        Value::Float(value) => value,
        Value::Duration(_) | Value::Timestamp(_) => {
            unreachable!("as_float is only called on numbers")
        }
    }
}

/// Wrap a float result, rejecting infinity and NaN.
fn finite(value: f64) -> Result<Value, EvaluateError> {
    if value.is_finite() {
        Ok(Value::Float(value))
    } else {
        Err(EvaluateError::NotFinite)
    }
}

/// Scale a duration by a factor, rounding to the nearest whole second.
fn scale_duration(seconds: i64, factor: f64) -> Result<Value, EvaluateError> {
    let scaled = (seconds as f64 * factor).round();
    if !scaled.is_finite() {
        return Err(EvaluateError::NotFinite);
    }
    // The i64 range check must be strict: `as` would silently saturate.
    if scaled < i64::MIN as f64 || scaled > i64::MAX as f64 {
        return Err(EvaluateError::Overflow);
    }
    Ok(Value::Duration(scaled as i64))
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::parser::parse_expression;
    use super::*;
    use std::collections::HashMap;

    struct MapContext {
        fields: HashMap<&'static str, Value>,
        constants: HashMap<&'static str, Value>,
    }

    impl ValueContext for MapContext {
        fn field(&self, name: &str) -> Option<Value> {
            self.fields.get(name).copied()
        }

        fn constant(&self, name: &str) -> Option<Value> {
            self.constants.get(name).copied()
        }
    }

    const DAY: i64 = 86_400;
    const HOUR: i64 = 3_600;

    fn context() -> MapContext {
        MapContext {
            fields: HashMap::from([
                ("count", Value::Integer(4)),
                ("weight", Value::Float(2.5)),
                // 2026-01-05 as a midnight timestamp: 20_458 days.
                ("start_date", Value::Timestamp(20_458 * DAY)),
                ("end_date", Value::Timestamp(20_465 * DAY)),
                ("duration", Value::Duration(7 * DAY)),
                ("effort", Value::Duration(6 * HOUR)),
                ("zero_duration", Value::Duration(0)),
            ]),
            constants: HashMap::from([("daily_rate", Value::Float(800.0))]),
        }
    }

    fn evaluated(source: &str) -> Result<Value, EvaluateError> {
        evaluate(&parse_expression(source).unwrap(), &context())
    }

    #[test]
    fn evaluates_the_motivating_expressions() {
        assert_eq!(
            evaluated("start_date + duration"),
            Ok(Value::Timestamp(20_465 * DAY))
        );
        assert_eq!(
            evaluated("end_date - start_date"),
            Ok(Value::Duration(7 * DAY))
        );
        assert_eq!(
            evaluated("effort / duration"),
            Ok(Value::Float(6.0 * 3_600.0 / (7.0 * 86_400.0)))
        );
        assert_eq!(
            evaluated("effort * $constants.daily_rate"),
            Ok(Value::Duration(6 * HOUR * 800))
        );
    }

    #[test]
    fn numeric_arithmetic_and_promotion() {
        assert_eq!(evaluated("count * 2"), Ok(Value::Integer(8)));
        assert_eq!(evaluated("count * 1.5"), Ok(Value::Float(6.0)));
        assert_eq!(evaluated("count / count"), Ok(Value::Float(1.0)));
        assert_eq!(evaluated("weight + count"), Ok(Value::Float(6.5)));
        assert_eq!(evaluated("-count"), Ok(Value::Integer(-4)));
    }

    #[test]
    fn sub_day_precision_survives_intermediate_steps() {
        // (start + 4h) + 4h must land exactly on start + 8h — no
        // intermediate rounding onto calendar days.
        let context = MapContext {
            fields: HashMap::from([
                ("start_date", Value::Timestamp(100 * DAY)),
                ("four_hours", Value::Duration(4 * HOUR)),
            ]),
            constants: HashMap::new(),
        };
        let expression = parse_expression("start_date + four_hours + four_hours").unwrap();
        assert_eq!(
            evaluate(&expression, &context),
            Ok(Value::Timestamp(100 * DAY + 8 * HOUR))
        );
    }

    #[test]
    fn duration_scaling_rounds_to_whole_seconds() {
        assert_eq!(evaluated("effort * 1.5"), Ok(Value::Duration(9 * HOUR)));
        assert_eq!(evaluated("duration / 2"), Ok(Value::Duration(84 * HOUR)));
        assert_eq!(evaluated("2 * effort"), Ok(Value::Duration(12 * HOUR)));
    }

    #[test]
    fn missing_field_reports_the_name() {
        assert_eq!(
            evaluated("count + absent"),
            Err(EvaluateError::MissingInput {
                name: "absent".to_owned()
            })
        );
        assert_eq!(
            evaluated("count * $constants.absent"),
            Err(EvaluateError::MissingInput {
                name: "absent".to_owned()
            })
        );
    }

    #[test]
    fn division_by_zero_in_every_shape() {
        assert_eq!(evaluated("count / 0"), Err(EvaluateError::DivisionByZero));
        assert_eq!(evaluated("count / 0.0"), Err(EvaluateError::DivisionByZero));
        assert_eq!(
            evaluated("duration / 0"),
            Err(EvaluateError::DivisionByZero)
        );
        assert_eq!(
            evaluated("effort / zero_duration"),
            Err(EvaluateError::DivisionByZero)
        );
    }

    #[test]
    fn integer_overflow_is_reported() {
        let context = MapContext {
            fields: HashMap::from([("big", Value::Integer(i64::MAX))]),
            constants: HashMap::new(),
        };
        let expression = parse_expression("big + 1").unwrap();
        assert_eq!(
            evaluate(&expression, &context),
            Err(EvaluateError::Overflow)
        );
    }

    #[test]
    fn undefined_pairings_are_invalid_operations() {
        assert_eq!(
            evaluated("start_date + end_date"),
            Err(EvaluateError::InvalidOperation)
        );
        assert_eq!(
            evaluated("duration * effort"),
            Err(EvaluateError::InvalidOperation)
        );
        assert_eq!(
            evaluated("-start_date"),
            Err(EvaluateError::InvalidOperation)
        );
    }
}
