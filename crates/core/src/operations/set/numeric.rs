//! `--delta` on `integer` / `float` fields.

use std::collections::HashMap;

use super::{current_value, ComputedMutation, SetError};

/// Reject `--delta` when the field is absent or the current value isn't
/// a number we can parse. Hard error — the file is not written.
///
/// Unlike a duration, a count is left strict: "absent" for a count can be
/// a deliberate statement, and nothing needs the zero-start yet.
pub(super) fn require_existing(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match current_value(frontmatter, field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "delta",
            field: field.to_owned(),
        }),
        Some(value) if value.as_i64().is_some() || value.as_f64().is_some() => Ok(()),
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "number",
        }),
    }
}

pub(super) fn compute_delta(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    delta: serde_yaml::Number,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    // Preconditions guarantee `previous_value` is `Some(Number)`.
    let new_value = apply_numeric_delta(
        previous_value
            .as_ref()
            .expect("precondition ensures existing numeric value"),
        &delta,
    );
    new_frontmatter.insert(field.to_owned(), new_value.clone());
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: Some(new_value),
        write_needed: true,
        info_messages: Vec::new(),
    }
}

/// Add a signed delta to a numeric field's current value, preserving
/// the field's int/float typing.
///
/// Float arithmetic kicks in only when either operand is itself a
/// float; pure-integer adds stay as i64 so the on-disk YAML reads as
/// `points: 8` and not `points: 8.0`.
fn apply_numeric_delta(
    current: &serde_yaml::Value,
    delta: &serde_yaml::Number,
) -> serde_yaml::Value {
    let current_number = match current {
        serde_yaml::Value::Number(number) => number,
        _ => unreachable!("preconditions ensure numeric current value"),
    };

    let use_float = delta.is_f64() || current_number.is_f64();
    if use_float {
        let a = current_number
            .as_f64()
            .expect("number coerces to f64 unless infinitely large");
        let b = delta.as_f64().expect("delta coerces to f64");
        serde_yaml::Value::Number(serde_yaml::Number::from(a + b))
    } else {
        let a = current_number.as_i64().expect("integer number stays i64");
        let b = delta.as_i64().expect("integer delta stays i64");
        serde_yaml::Value::Number(serde_yaml::Number::from(a.saturating_add(b)))
    }
}
