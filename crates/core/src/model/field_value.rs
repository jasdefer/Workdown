//! Typed field values plus their human-readable formatters.

use serde::{Serialize, Serializer};

use super::duration::format_duration_seconds;
use super::WorkItemId;

/// A typed field value, coerced from raw YAML according to the schema's field type.
///
/// Serialized untagged: consumers see bare JSON scalars/arrays
/// (`"open"`, `42`, `["a","b"]`, `"2026-04-23"`) and consult the schema
/// separately to interpret the type. Duration uses a custom serializer
/// that emits the formatted string (`"5d"`) — same convention `Date`
/// follows for human-readable output. Deserialize is not derived —
/// values are produced by the coercion layer, not parsed from JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FieldValue {
    /// A free-form string.
    String(String),
    /// A single value from an allowed set.
    Choice(String),
    /// Multiple values from an allowed set.
    Multichoice(Vec<String>),
    /// A signed integer.
    Integer(i64),
    /// A floating-point number.
    Float(f64),
    /// A calendar date. On disk `YYYY-MM-DD`; in memory a native `NaiveDate`.
    Date(chrono::NaiveDate),
    /// A signed duration in canonical seconds. Formatted as suffix
    /// shorthand (`"5d"`, `"1w 2d 3h"`) for human-readable output.
    Duration(#[serde(serialize_with = "serialize_duration_seconds")] i64),
    /// A boolean flag.
    Boolean(bool),
    /// A list of free-form strings.
    List(Vec<String>),
    /// A reference to a single work item by ID.
    Link(WorkItemId),
    /// References to multiple work items by ID.
    Links(Vec<WorkItemId>),
}

/// Serializer for `FieldValue::Duration`: emits the formatted string
/// (`"5d"`) rather than the raw i64, matching how `Date` emits
/// `"YYYY-MM-DD"` rather than its internal representation.
fn serialize_duration_seconds<S>(seconds: &i64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format_duration_seconds(*seconds))
}

// ── Display formatting ──────────────────────────────────────────────

/// Format a field value as a human-readable display string.
///
/// Multi-valued variants (Multichoice, List, Links) join with `", "`.
/// For embedding inside diagnostic message prose where the value needs
/// visual delimiters, see [`format_field_value_bracketed`].
pub fn format_field_value(value: &FieldValue) -> String {
    format_with(value, false)
}

/// Same as [`format_field_value`] but wraps multi-valued variants in
/// brackets: `[a, b, c]` instead of `a, b, c`. Used inside rule-violation
/// messages where the value is embedded in surrounding prose and the
/// brackets disambiguate a list-of-two from a single string with a comma.
pub fn format_field_value_bracketed(value: &FieldValue) -> String {
    format_with(value, true)
}

fn format_with(value: &FieldValue, bracketed: bool) -> String {
    let join_strs = |items: &[String]| -> String {
        let body = items.join(", ");
        if bracketed {
            format!("[{body}]")
        } else {
            body
        }
    };
    let join_ids = |ids: &[WorkItemId]| -> String {
        let body = ids
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if bracketed {
            format!("[{body}]")
        } else {
            body
        }
    };
    match value {
        FieldValue::String(string) => string.clone(),
        FieldValue::Choice(string) => string.clone(),
        FieldValue::Date(date) => date.format("%Y-%m-%d").to_string(),
        FieldValue::Duration(seconds) => format_duration_seconds(*seconds),
        FieldValue::Link(id) => id.as_str().to_owned(),
        FieldValue::Integer(number) => number.to_string(),
        FieldValue::Float(number) => number.to_string(),
        FieldValue::Boolean(flag) => flag.to_string(),
        FieldValue::Multichoice(values) => join_strs(values),
        FieldValue::List(values) => join_strs(values),
        FieldValue::Links(ids) => join_ids(ids),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_string_value() {
        assert_eq!(
            format_field_value(&FieldValue::String("hello".into())),
            "hello"
        );
    }

    #[test]
    fn format_integer_value() {
        assert_eq!(format_field_value(&FieldValue::Integer(42)), "42");
    }

    #[test]
    fn format_list_value() {
        assert_eq!(
            format_field_value(&FieldValue::List(vec!["a".into(), "b".into()])),
            "a, b"
        );
    }

    #[test]
    fn format_links_value() {
        assert_eq!(
            format_field_value(&FieldValue::Links(vec![
                WorkItemId::from("x".to_owned()),
                WorkItemId::from("y".to_owned()),
            ])),
            "x, y"
        );
    }

    #[test]
    fn format_bracketed_list() {
        assert_eq!(
            format_field_value_bracketed(&FieldValue::List(vec!["a".into(), "b".into()])),
            "[a, b]"
        );
    }

    #[test]
    fn format_bracketed_links() {
        assert_eq!(
            format_field_value_bracketed(&FieldValue::Links(vec![
                WorkItemId::from("x".to_owned()),
                WorkItemId::from("y".to_owned()),
            ])),
            "[x, y]"
        );
    }

    #[test]
    fn format_bracketed_scalar_is_unbracketed() {
        // brackets only apply to collection variants
        assert_eq!(
            format_field_value_bracketed(&FieldValue::String("hello".into())),
            "hello"
        );
        assert_eq!(format_field_value_bracketed(&FieldValue::Integer(42)), "42");
    }
}
