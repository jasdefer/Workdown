//! Compute pass: evaluate one field's `compute:` expression per item.
//!
//! The same-item counterpart to [`super::rollup`]. For each item that
//! doesn't already carry the field (a hand-written frontmatter value
//! always wins), the expression evaluates over the item's current field
//! values and the project constants, and the result is written into
//! `WorkItem.fields` — indistinguishable from a manually-set value for
//! everything downstream.
//!
//! When the field *also* declares `aggregate:`, the pass is restricted
//! to leaves of the aggregate's `over` hierarchy: compute fills leaves,
//! the rollup fills everything above. That keeps a milestone's
//! `end_date` the `max` of its children instead of the gap-blind
//! `start + duration` of its rolled-up inputs.
//!
//! Failure handling per item: missing inputs skip silently (or emit an
//! error when the config sets `error_on_missing`); runtime failures on
//! actual values (division by zero, overflow, non-finite results) emit
//! a warning; type-level impossibilities skip silently because
//! `compute_check` already reported them against the schema.

use std::collections::HashMap;

use chrono::NaiveDate;
use indexmap::IndexMap;

use crate::expression::{evaluate, EvaluateError, Value, ValueContext};
use crate::model::diagnostic::{Diagnostic, ItemDiagnosticKind};
use crate::model::schema::{ComputeConfig, FieldType, RoundMode, Severity};
use crate::model::{FieldValue, WorkItem, WorkItemId};

const SECONDS_PER_DAY: i64 = 86_400;

/// Evaluate `config` for every eligible item, writing results into
/// `items`. `leaves_only_over` carries the aggregate's resolved `over`
/// link when the field also aggregates — restricting the pass to leaves
/// of that hierarchy.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_for_field(
    items: &mut HashMap<WorkItemId, WorkItem>,
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    constants: &IndexMap<String, FieldValue>,
    field_name: &str,
    declared_type: FieldType,
    config: &ComputeConfig,
    leaves_only_over: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Sorted for deterministic diagnostic order, like the rollup.
    let mut item_ids: Vec<WorkItemId> = items.keys().cloned().collect();
    item_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    for item_id in item_ids {
        let Some(item) = items.get(&item_id) else {
            continue;
        };
        if item.fields.contains_key(field_name) {
            continue; // manual value wins; compute fills only absence
        }
        if let Some(over) = leaves_only_over {
            if !is_leaf(reverse_links, &item_id, over) {
                continue; // non-leaf of a compute+aggregate field: rollup's job
            }
        }

        let missing = missing_inputs(item, config);
        if !missing.is_empty() {
            if config.error_on_missing {
                diagnostics.push(Diagnostic::item(
                    Severity::Error,
                    item.source_path.clone(),
                    item_id.clone(),
                    ItemDiagnosticKind::ComputeMissingInputs {
                        field: field_name.to_owned(),
                        missing_inputs: missing,
                    },
                ));
            }
            continue;
        }

        let context = ItemValueContext {
            fields: &item.fields,
            constants,
        };
        let outcome = match evaluate(&config.expression, &context) {
            Ok(value) => match field_value_from(value, declared_type, config.round) {
                Some(field_value) => Outcome::Value(field_value),
                // A result that doesn't fit the declared type is
                // schema-level and already reported by compute_check;
                // a date outside chrono's calendar range also lands
                // here and is accepted as a silent skip.
                None => Outcome::Skip,
            },
            // Schema-level impossibilities — compute_check reported them.
            Err(EvaluateError::MissingInput { .. }) | Err(EvaluateError::InvalidOperation) => {
                Outcome::Skip
            }
            // Real runtime failures on this item's actual values.
            Err(runtime_failure) => Outcome::Failed(runtime_failure.to_string()),
        };

        match outcome {
            Outcome::Skip => {}
            Outcome::Failed(detail) => diagnostics.push(Diagnostic::item(
                Severity::Warning,
                item.source_path.clone(),
                item_id.clone(),
                ItemDiagnosticKind::ComputeFailed {
                    field: field_name.to_owned(),
                    detail,
                },
            )),
            Outcome::Value(field_value) => {
                if let Some(item) = items.get_mut(&item_id) {
                    item.fields.insert(field_name.to_owned(), field_value);
                }
            }
        }
    }
}

enum Outcome {
    Value(FieldValue),
    Failed(String),
    Skip,
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

/// True if no item references `item_id` via `over_field` — nothing has
/// it as their parent in the aggregate hierarchy.
fn is_leaf(
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    item_id: &WorkItemId,
    over_field: &str,
) -> bool {
    reverse_links
        .get(over_field)
        .and_then(|by_target| by_target.get(item_id))
        .is_none_or(|sources| sources.is_empty())
}

// ── Value conversion ──────────────────────────────────────────────────

struct ItemValueContext<'a> {
    fields: &'a HashMap<String, FieldValue>,
    constants: &'a IndexMap<String, FieldValue>,
}

impl ValueContext for ItemValueContext<'_> {
    fn field(&self, name: &str) -> Option<Value> {
        self.fields.get(name).and_then(value_of)
    }

    fn constant(&self, name: &str) -> Option<Value> {
        self.constants.get(name).and_then(value_of)
    }
}

fn epoch() -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch is a valid date")
}

/// A field value as an evaluation [`Value`]. `None` for types outside
/// the algebra.
fn value_of(field_value: &FieldValue) -> Option<Value> {
    match field_value {
        FieldValue::Integer(value) => Some(Value::Integer(*value)),
        FieldValue::Float(value) => Some(Value::Float(*value)),
        FieldValue::Duration(seconds) => Some(Value::Duration(*seconds)),
        FieldValue::Date(date) => {
            let days = date.signed_duration_since(epoch()).num_days();
            Some(Value::Timestamp(days * SECONDS_PER_DAY))
        }
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
        (Value::Timestamp(seconds), FieldType::Date) => {
            let days = match round {
                RoundMode::Nearest => (seconds + SECONDS_PER_DAY / 2).div_euclid(SECONDS_PER_DAY),
                RoundMode::Floor => seconds.div_euclid(SECONDS_PER_DAY),
                RoundMode::Ceil => (seconds + SECONDS_PER_DAY - 1).div_euclid(SECONDS_PER_DAY),
            };
            let date = epoch().checked_add_signed(chrono::Duration::days(days))?;
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
                field_value_from(four_hours_in, FieldType::Date, round),
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
    fn non_scalar_field_values_do_not_convert() {
        assert_eq!(value_of(&FieldValue::String("x".to_owned())), None);
        assert_eq!(value_of(&FieldValue::Boolean(true)), None);
    }
}
