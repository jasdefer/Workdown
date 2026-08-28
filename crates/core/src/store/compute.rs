//! Compute evaluation: one field's `compute:` expression on one item.
//!
//! The same-item mechanism next to [`super::rollup`]: the expression
//! evaluates over the item's current field values and the project
//! constants, and the result — converted to the field's declared type —
//! becomes the item's value, indistinguishable from a manually-set one
//! for everything downstream.
//!
//! Scheduling — which items evaluate, that a hand-written frontmatter
//! value always wins, and the leaves-only restriction when the field
//! also aggregates — is the derive orchestrator's job
//! ([`super::derive`]); this module only answers "what does the
//! expression yield for this item".
//!
//! Failure handling per item: missing inputs skip silently (or report
//! an error when the config sets `error_on_missing`); runtime failures
//! on actual values (division by zero, overflow, non-finite results)
//! report a warning. Configs that failed the schema-level check never
//! reach this pass — the derive orchestrator skips them — so the
//! remaining type-level error paths below are defensive mappings to
//! silent skips.

use std::collections::HashMap;

use chrono::NaiveDate;
use indexmap::IndexMap;

use crate::expression::{evaluate, EvaluateError, Value, ValueContext};
use crate::model::diagnostic::{Diagnostic, ItemDiagnosticKind};
use crate::model::schema::{ComputeConfig, FieldType, RoundMode, Severity};
use crate::model::{FieldValue, WorkItem};

const SECONDS_PER_DAY: i64 = 86_400;

/// What one item's compute evaluation produced: a value to write, a
/// diagnostic to report (the value stays absent), or a silent skip.
pub(super) enum SameItemOutcome {
    Value(FieldValue),
    Report(Diagnostic),
    Skip,
}

/// Evaluate `config`'s expression on one item. `today` is what
/// `$today` resolves to — passed in, never read from the clock here
/// (see ADR-010).
pub(super) fn evaluate_for_item(
    item: &WorkItem,
    field_name: &str,
    declared_type: FieldType,
    config: &ComputeConfig,
    constants: &IndexMap<String, FieldValue>,
    today: &Value,
) -> SameItemOutcome {
    let missing = missing_inputs(item, config);
    if !missing.is_empty() {
        if config.error_on_missing {
            return SameItemOutcome::Report(Diagnostic::item(
                Severity::Error,
                item.source_path.clone(),
                item.id.clone(),
                ItemDiagnosticKind::ComputeMissingInputs {
                    field: field_name.to_owned(),
                    missing_inputs: missing,
                },
            ));
        }
        return SameItemOutcome::Skip;
    }

    let context = ItemValueContext {
        fields: &item.fields,
        constants,
        today: today.clone(),
    };
    match evaluate(&config.expression, &context) {
        Ok(value) => match field_value_from(value, declared_type, config.round) {
            Some(field_value) => SameItemOutcome::Value(field_value),
            // A result that doesn't fit the declared type is
            // schema-level and already reported by compute_check; a
            // date outside chrono's calendar range also lands here and
            // is accepted as a silent skip.
            None => SameItemOutcome::Skip,
        },
        // Schema-level impossibilities — compute_check reported them.
        Err(EvaluateError::MissingInput { .. }) | Err(EvaluateError::InvalidOperation) => {
            SameItemOutcome::Skip
        }
        // Real runtime failures on this item's actual values.
        Err(runtime_failure) => SameItemOutcome::Report(Diagnostic::item(
            Severity::Warning,
            item.source_path.clone(),
            item.id.clone(),
            ItemDiagnosticKind::ComputeFailed {
                field: field_name.to_owned(),
                detail: runtime_failure.to_string(),
            },
        )),
    }
}

/// The expression's field references that have no value on `item`,
/// deduplicated, in source order.
pub(super) fn missing_inputs(item: &WorkItem, config: &ComputeConfig) -> Vec<String> {
    let mut missing = Vec::new();
    for reference in config.expression.field_references() {
        if !item.fields.contains_key(reference) && !missing.iter().any(|seen| seen == reference) {
            missing.push(reference.to_owned());
        }
    }
    missing
}

// ── Value conversion ──────────────────────────────────────────────────

/// [`ValueContext`] over one item's fields plus the project constants
/// and the evaluation date. Shared with the conditional pass.
pub(super) struct ItemValueContext<'a> {
    pub(super) fields: &'a HashMap<String, FieldValue>,
    pub(super) constants: &'a IndexMap<String, FieldValue>,
    /// The evaluation date as a midnight timestamp — what `$today`
    /// resolves to for every item in this pass.
    pub(super) today: Value,
}

impl ValueContext for ItemValueContext<'_> {
    fn field(&self, name: &str) -> Option<Value> {
        self.fields.get(name).and_then(value_of)
    }

    fn constant(&self, name: &str) -> Option<Value> {
        self.constants.get(name).and_then(value_of)
    }

    fn today(&self) -> Value {
        self.today.clone()
    }
}

fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch is a valid date")
}

/// A calendar date as a midnight [`Value::Timestamp`] — the same
/// conversion [`value_of`] applies to date field values.
pub(super) fn timestamp_of(date: NaiveDate) -> Value {
    let days = date.signed_duration_since(epoch()).num_days();
    Value::Timestamp(days * SECONDS_PER_DAY)
}

/// A field value as an evaluation [`Value`]. `None` for types outside
/// the algebra.
fn value_of(field_value: &FieldValue) -> Option<Value> {
    match field_value {
        FieldValue::Integer(value) => Some(Value::Integer(*value)),
        FieldValue::Float(value) => Some(Value::Float(*value)),
        FieldValue::Duration(seconds) => Some(Value::Duration(*seconds)),
        FieldValue::Date(date) => Some(timestamp_of(*date)),
        FieldValue::Boolean(flag) => Some(Value::Boolean(*flag)),
        FieldValue::String(text) | FieldValue::Choice(text) => Some(Value::Text(text.clone())),
        // Canonical color (palette name or hex) resolves to display hex
        // on the way in, so equality inside evaluation is hex equality.
        FieldValue::Color(canonical) => Some(Value::Color(
            crate::model::color::resolve_color_to_hex(canonical)
                .unwrap_or_else(|| canonical.clone()),
        )),
        _ => None,
    }
}

/// An evaluation result as a [`FieldValue`] fitting the declared field
/// type — including the one `integer → float` widening the algebra
/// allows, and rounding a timestamp onto a calendar day. `None` when
/// the value doesn't fit the type (schema-level, reported by
/// `compute_check`) or the date leaves the calendar range.
fn field_value_from(
    value: Value,
    declared_type: FieldType,
    round: RoundMode,
) -> Option<FieldValue> {
    match (value, declared_type) {
        (Value::Integer(value), FieldType::Integer) => Some(FieldValue::Integer(value)),
        (Value::Integer(value), FieldType::Float) => Some(FieldValue::Float(value as f64)),
        (Value::Float(value), FieldType::Float) => Some(FieldValue::Float(value)),
        (Value::Duration(seconds), FieldType::Duration) => Some(FieldValue::Duration(seconds)),
        (Value::Boolean(flag), FieldType::Boolean) => Some(FieldValue::Boolean(flag)),
        (Value::Timestamp(seconds), FieldType::Date) => {
            // The rounding shift must not wrap: a timestamp near
            // `i64::MAX` (reachable through duration arithmetic) skips
            // like any date outside the calendar range.
            let shifted = match round {
                RoundMode::Nearest => seconds.checked_add(SECONDS_PER_DAY / 2)?,
                RoundMode::Floor => seconds,
                RoundMode::Ceil => seconds.checked_add(SECONDS_PER_DAY - 1)?,
            };
            let days = shifted.div_euclid(SECONDS_PER_DAY);
            // `try_days`, not `days` — the panicking constructor rejects
            // day counts this arithmetic can legitimately produce.
            let date = epoch().checked_add_signed(chrono::Duration::try_days(days)?)?;
            Some(FieldValue::Date(date))
        }
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn date(y: i32, m: u32, d: u32) -> FieldValue {
        FieldValue::Date(NaiveDate::from_ymd_opt(y, m, d).unwrap())
    }

    const HOUR: i64 = 3_600;

    #[test]
    fn date_and_timestamp_conversion_round_trips() {
        let original = date(2026, 1, 5);
        let Some(Value::Timestamp(seconds)) = value_of(&original) else {
            panic!("expected a timestamp");
        };
        assert_eq!(
            field_value_from(
                Value::Timestamp(seconds),
                FieldType::Date,
                RoundMode::Nearest
            ),
            Some(original)
        );
    }

    #[test]
    fn rounding_modes_on_a_sub_day_remainder() {
        // 2026-01-05 midnight plus 4 hours.
        let Some(Value::Timestamp(midnight)) = value_of(&date(2026, 1, 5)) else {
            panic!("expected a timestamp");
        };
        let four_hours_in = Value::Timestamp(midnight + 4 * HOUR);

        for (round, expected_day) in [
            (RoundMode::Nearest, 5), // 4h < half a day: rounds down
            (RoundMode::Floor, 5),
            (RoundMode::Ceil, 6),
        ] {
            assert_eq!(
                field_value_from(four_hours_in.clone(), FieldType::Date, round),
                Some(date(2026, 1, expected_day)),
                "{round:?}"
            );
        }

        let twenty_hours_in = Value::Timestamp(midnight + 20 * HOUR);
        assert_eq!(
            field_value_from(twenty_hours_in, FieldType::Date, RoundMode::Nearest),
            Some(date(2026, 1, 6)),
            "20h > half a day: nearest rounds up"
        );
    }

    #[test]
    fn integer_result_widens_into_a_float_field() {
        assert_eq!(
            field_value_from(Value::Integer(4), FieldType::Float, RoundMode::Nearest),
            Some(FieldValue::Float(4.0))
        );
    }

    #[test]
    fn value_that_does_not_fit_the_declared_type_converts_to_none() {
        assert_eq!(
            field_value_from(Value::Float(1.5), FieldType::Integer, RoundMode::Nearest),
            None
        );
        assert_eq!(
            field_value_from(Value::Duration(60), FieldType::Date, RoundMode::Nearest),
            None
        );
    }

    #[test]
    fn collection_field_values_do_not_convert() {
        assert_eq!(value_of(&FieldValue::List(vec!["x".to_owned()])), None);
        assert_eq!(
            value_of(&FieldValue::Multichoice(vec!["a".to_owned()])),
            None
        );
    }

    #[test]
    fn equality_participating_values_convert() {
        assert_eq!(
            value_of(&FieldValue::String("x".to_owned())),
            Some(Value::Text("x".to_owned()))
        );
        assert_eq!(
            value_of(&FieldValue::Choice("done".to_owned())),
            Some(Value::Text("done".to_owned()))
        );
        assert_eq!(
            value_of(&FieldValue::Boolean(true)),
            Some(Value::Boolean(true))
        );
        // A palette name resolves to its pinned hex on the way in.
        assert_eq!(
            value_of(&FieldValue::Color("red".to_owned())),
            Some(Value::Color("#ef4444".to_owned()))
        );
    }

    #[test]
    fn boolean_result_fits_a_boolean_field() {
        assert_eq!(
            field_value_from(Value::Boolean(true), FieldType::Boolean, RoundMode::Nearest),
            Some(FieldValue::Boolean(true))
        );
        // A boolean result does not fit any other type, and text
        // results fit nothing (no computable text fields).
        assert_eq!(
            field_value_from(Value::Boolean(true), FieldType::Integer, RoundMode::Nearest),
            None
        );
        assert_eq!(
            field_value_from(
                Value::Text("done".to_owned()),
                FieldType::Boolean,
                RoundMode::Nearest
            ),
            None
        );
    }

    #[test]
    fn timestamp_at_the_integer_limit_skips_instead_of_wrapping() {
        // Nearest and Ceil shift before dividing; Floor fails on the
        // calendar range instead. All three must skip, never wrap.
        for round in [RoundMode::Nearest, RoundMode::Floor, RoundMode::Ceil] {
            assert_eq!(
                field_value_from(Value::Timestamp(i64::MAX), FieldType::Date, round),
                None,
                "{round:?}"
            );
        }
    }
}
