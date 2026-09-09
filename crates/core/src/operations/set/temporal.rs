//! `--delta` on `duration` and `date` fields.
//!
//! Both share the same operand shape (signed seconds, parsed from a
//! duration literal like `1w 2d`). They differ in the storage format
//! (duration string vs `YYYY-MM-DD`), the arithmetic, and what they make
//! of an absent field: a duration counts as `0s` and is created by the
//! delta, a date has no zero to count from and asks for a value first.

use std::collections::HashMap;

use super::{current_value, ComputedMutation, SetError};
use crate::model::date::parse_date;

/// Reject `--delta` on a duration field only when the current value is
/// something other than a duration.
///
/// An absent duration is allowed through — it counts as `0s` and the
/// delta creates the field, because the first time anyone records effort
/// on an item is exactly the moment the field is absent. A value that is
/// present but isn't a duration still fails: replacing a typo with a
/// measured number would destroy the evidence that something was wrong.
pub(super) fn require_absent_or_valid_duration(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match current_value(frontmatter, field) {
        None => Ok(()),
        Some(serde_yaml::Value::String(string))
            if crate::model::duration::parse_duration(string).is_ok() =>
        {
            Ok(())
        }
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "duration string (e.g. '1w 2d', '-3h')",
        }),
    }
}

/// Reject `--delta` when the date field is absent or its current value
/// isn't a parseable `YYYY-MM-DD` date.
///
/// Unlike a duration, a date has no zero to count from, so an absent
/// date stays an error — `--delta` works on dates, it just needs a value
/// to move.
pub(super) fn require_existing_date(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match current_value(frontmatter, field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "delta",
            field: field.to_owned(),
        }),
        Some(serde_yaml::Value::String(string)) if parse_date(string).is_some() => Ok(()),
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "date (YYYY-MM-DD)",
        }),
    }
}

pub(super) fn compute_duration_delta(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    delta_seconds: i64,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    // An absent field starts at zero: an absent duration and `0s` reach
    // the same answer. A derived value — rolled up, computed or pulled —
    // never appears in the frontmatter map this reads, so it starts at
    // zero too, rather than freezing the derived number into the file
    // where it would go stale the next time a child changed.
    let current_seconds = match previous_value.as_ref() {
        None => 0,
        Some(value) => {
            let current_string = value
                .as_str()
                .expect("precondition ensures a duration string when a value is present");
            crate::model::duration::parse_duration(current_string)
                .expect("precondition ensures parseable duration")
        }
    };
    let new_seconds = current_seconds.saturating_add(delta_seconds);
    let new_string = crate::model::duration::format_duration_seconds(new_seconds);
    let new_value = serde_yaml::Value::String(new_string);
    new_frontmatter.insert(field.to_owned(), new_value.clone());
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: Some(new_value),
        write_needed: true,
        info_messages: Vec::new(),
    }
}

pub(super) fn compute_date_delta(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    delta_seconds: i64,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    let current_string = previous_value
        .as_ref()
        .and_then(|value| value.as_str())
        .expect("precondition ensures existing date string");
    let current_date = parse_date(current_string).expect("precondition ensures parseable date");
    let new_date = current_date
        .checked_add_signed(chrono::Duration::seconds(delta_seconds))
        .expect("date arithmetic must fit chrono's NaiveDate range");
    let new_value = serde_yaml::Value::String(new_date.format("%Y-%m-%d").to_string());
    new_frontmatter.insert(field.to_owned(), new_value.clone());
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: Some(new_value),
        write_needed: true,
        info_messages: Vec::new(),
    }
}
