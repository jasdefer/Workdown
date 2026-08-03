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
//! and item knowledge, like the type checker (the one exception is the
//! color palette, a value-domain table needed to compare a color against
//! a text literal naming one). Dates travel as [`Value::Timestamp`],
//! seconds since the Unix epoch, so sub-day precision survives
//! intermediate steps (`start + 4h + 4h` must equal `start + 8h`); the
//! *caller* rounds the final timestamp onto a calendar day with its
//! configured rounding mode.

use super::ast::{BinaryOperator, ComparisonOperator, Expression};
use crate::model::color::{parse_color, resolve_color_to_hex};

/// A value during evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i64),
    Float(f64),
    /// Canonical seconds, like the duration field type.
    Duration(i64),
    /// A point in time as seconds since the Unix epoch. Dates enter
    /// evaluation as midnight timestamps and leave with whatever
    /// sub-day remainder the arithmetic produced.
    Timestamp(i64),
    /// A boolean, produced by comparisons and boolean literals.
    Boolean(bool),
    /// Text: a string or choice field value, or a quoted literal.
    Text(String),
    /// A color as its resolved `#rrggbb` hex — the caller resolves on
    /// the way in, so equality here is plain string equality.
    Color(String),
}

/// Resolves field and constant references to their current values for
/// the item being evaluated. `None` means the reference has no usable
/// value here (absent, or a type outside the algebra).
pub trait ValueContext {
    fn field(&self, name: &str) -> Option<Value>;
    fn constant(&self, name: &str) -> Option<Value>;
    /// The evaluation date (`$today`) as a midnight timestamp. Resolved
    /// once per run by the caller and handed in — evaluation never
    /// reads the clock, so a pinned date pins every expression.
    fn today(&self) -> Value;
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
        Expression::TodayReference { .. } => Ok(context.today()),
        Expression::StringLiteral { value, .. } => Ok(Value::Text(value.clone())),
        Expression::BooleanLiteral { value, .. } => Ok(Value::Boolean(*value)),

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
            Value::Timestamp(_) | Value::Boolean(_) | Value::Text(_) | Value::Color(_) => {
                Err(EvaluateError::InvalidOperation)
            }
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

        Expression::Comparison {
            operator,
            left,
            right,
            ..
        } => {
            let left = evaluate(left, context)?;
            let right = evaluate(right, context)?;
            apply_comparison(*operator, left, right)
        }
    }
}

/// Apply one comparison to two values — the runtime half of
/// `typecheck::comparison_is_defined`, with the same pairings.
fn apply_comparison(
    operator: ComparisonOperator,
    left: Value,
    right: Value,
) -> Result<Value, EvaluateError> {
    use std::cmp::Ordering;

    // Orderable pairings answer every operator.
    let ordering: Option<Ordering> = match (&left, &right) {
        (a, b) if both_numbers(a, b) => as_float(a).partial_cmp(&as_float(b)),
        (Value::Timestamp(a), Value::Timestamp(b)) => Some(a.cmp(b)),
        (Value::Duration(a), Value::Duration(b)) => Some(a.cmp(b)),
        _ => None,
    };
    if let Some(ordering) = ordering {
        let holds = match operator {
            ComparisonOperator::Equal => ordering == Ordering::Equal,
            ComparisonOperator::NotEqual => ordering != Ordering::Equal,
            ComparisonOperator::LessThan => ordering == Ordering::Less,
            ComparisonOperator::LessOrEqual => ordering != Ordering::Greater,
            ComparisonOperator::GreaterThan => ordering == Ordering::Greater,
            ComparisonOperator::GreaterOrEqual => ordering != Ordering::Less,
        };
        return Ok(Value::Boolean(holds));
    }

    // Equality-only pairings.
    if operator.is_equality() {
        let equal: Option<bool> = match (&left, &right) {
            (Value::Text(a), Value::Text(b)) => Some(a == b),
            (Value::Boolean(a), Value::Boolean(b)) => Some(a == b),
            // Colors are already resolved hex on both sides.
            (Value::Color(a), Value::Color(b)) => Some(a == b),
            (Value::Color(hex), Value::Text(text)) | (Value::Text(text), Value::Color(hex)) => {
                Some(text_names_color(text, hex))
            }
            _ => None,
        };
        if let Some(equal) = equal {
            let holds = if operator == ComparisonOperator::NotEqual {
                !equal
            } else {
                equal
            };
            return Ok(Value::Boolean(holds));
        }
    }

    // Undefined pairing — already reported by the schema-level check.
    Err(EvaluateError::InvalidOperation)
}

/// Whether a text value names the given resolved color hex: parsed as a
/// color (palette name or hex) and resolved, does it match? Text that
/// parses as no color names no color — the comparison is simply false.
fn text_names_color(text: &str, hex: &str) -> bool {
    parse_color(text)
        .ok()
        .and_then(|canonical| resolve_color_to_hex(&canonical))
        .is_some_and(|resolved| resolved == hex)
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
            (a, b) if both_numbers(&a, &b) => finite(as_float(&a) + as_float(&b)),
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
            (a, b) if both_numbers(&a, &b) => finite(as_float(&a) - as_float(&b)),
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
            (a, b) if both_numbers(&a, &b) => finite(as_float(&a) * as_float(&b)),
            (Duration(d), scale) | (scale, Duration(d)) if is_number(&scale) => {
                scale_duration(d, as_float(&scale))
            }
            _ => Err(EvaluateError::InvalidOperation),
        },
        BinaryOperator::Divide => match (left, right) {
            // Division never truncates: integer / integer is a float.
            (a, b) if both_numbers(&a, &b) => {
                let divisor = as_float(&b);
                if divisor == 0.0 {
                    return Err(EvaluateError::DivisionByZero);
                }
                finite(as_float(&a) / divisor)
            }
            (Duration(d), scale) if is_number(&scale) => {
                let divisor = as_float(&scale);
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

fn is_number(value: &Value) -> bool {
    matches!(value, Value::Integer(_) | Value::Float(_))
}

fn both_numbers(left: &Value, right: &Value) -> bool {
    is_number(left) && is_number(right)
}

/// Numeric value as f64. Only called on values `is_number` accepted.
fn as_float(value: &Value) -> f64 {
    match value {
        Value::Integer(value) => *value as f64,
        Value::Float(value) => *value,
        _ => unreachable!("as_float is only called on numbers"),
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
            self.fields.get(name).cloned()
        }

        fn constant(&self, name: &str) -> Option<Value> {
            self.constants.get(name).cloned()
        }

        fn today(&self) -> Value {
            // 2026-01-08 as a midnight timestamp: 20_461 days.
            Value::Timestamp(20_461 * DAY)
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
                ("status", Value::Text("done".to_owned())),
                ("flag", Value::Boolean(true)),
                // Red's pinned hex, as the conversion layer resolves it.
                ("tint", Value::Color("#ef4444".to_owned())),
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
    fn today_resolves_through_the_context() {
        // end_date is 20_465 days, the context's today is 20_461 days.
        assert_eq!(evaluated("end_date - $today"), Ok(Value::Duration(4 * DAY)));
        assert_eq!(evaluated("$today"), Ok(Value::Timestamp(20_461 * DAY)));
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

    // ── Comparisons ────────────────────────────────────────────────────

    #[test]
    fn ordering_comparisons_on_dates_durations_and_numbers() {
        assert_eq!(evaluated("end_date > start_date"), Ok(Value::Boolean(true)));
        assert_eq!(evaluated("end_date > $today"), Ok(Value::Boolean(true)));
        assert_eq!(evaluated("effort <= duration"), Ok(Value::Boolean(true)));
        assert_eq!(evaluated("count < weight"), Ok(Value::Boolean(false)));
        assert_eq!(evaluated("count >= 4"), Ok(Value::Boolean(true)));
        assert_eq!(
            evaluated("end_date - start_date >= duration"),
            Ok(Value::Boolean(true))
        );
    }

    #[test]
    fn equality_on_text_boolean_and_numbers() {
        assert_eq!(evaluated("status == \"done\""), Ok(Value::Boolean(true)));
        assert_eq!(evaluated("status != \"done\""), Ok(Value::Boolean(false)));
        assert_eq!(evaluated("status == \"open\""), Ok(Value::Boolean(false)));
        assert_eq!(evaluated("flag == true"), Ok(Value::Boolean(true)));
        assert_eq!(evaluated("flag != false"), Ok(Value::Boolean(true)));
        assert_eq!(evaluated("count == 4"), Ok(Value::Boolean(true)));
        assert_eq!(evaluated("count == weight"), Ok(Value::Boolean(false)));
    }

    #[test]
    fn color_equality_resolves_names_to_hex() {
        // The tint field holds red's resolved hex; both the palette
        // name and the hex literal must match it.
        assert_eq!(evaluated("tint == \"red\""), Ok(Value::Boolean(true)));
        assert_eq!(evaluated("tint == \"#ef4444\""), Ok(Value::Boolean(true)));
        assert_eq!(evaluated("tint == \"green\""), Ok(Value::Boolean(false)));
        assert_eq!(evaluated("tint != \"green\""), Ok(Value::Boolean(true)));
        // Text naming no color names nothing — unequal, not an error.
        assert_eq!(
            evaluated("tint == \"not-a-color\""),
            Ok(Value::Boolean(false))
        );
    }

    #[test]
    fn undefined_comparison_pairings_are_invalid_operations() {
        // Already rejected by the schema-level check; the runtime
        // mirror skips defensively.
        assert_eq!(
            evaluated("duration < 5"),
            Err(EvaluateError::InvalidOperation)
        );
        assert_eq!(
            evaluated("status < \"done\""),
            Err(EvaluateError::InvalidOperation)
        );
        assert_eq!(
            evaluated("flag == status"),
            Err(EvaluateError::InvalidOperation)
        );
    }
}
