//! Typed field values plus their human-readable formatters.

use serde::Serialize;

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
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
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
    /// JSON-typed as `string` to match the custom serializer; raw `i64`
    /// in Rust would mismatch the wire shape.
    Duration(
        // `with` (not `serialize_with`) so ts-rs's serde-compat parser
        // recognizes the attribute instead of warning on it; the
        // `#[ts(type)]` override below is then required by ts-rs, not
        // merely advisory. See the `duration_seconds` module.
        #[serde(with = "duration_seconds")]
        #[ts(type = "string")]
        i64,
    ),
    /// A color in canonical authoring form: a lowercase palette name
    /// (`"red"`) or lowercase expanded hex (`"#aabbcc"`). Produced by
    /// [`crate::model::color::parse_color`]; resolved to display hex via
    /// [`crate::model::color::resolve_color_to_hex`] at use sites.
    Color(String),
    /// A boolean flag.
    Boolean(bool),
    /// A list of free-form strings.
    List(Vec<String>),
    /// A reference to a single work item by ID.
    Link(WorkItemId),
    /// References to multiple work items by ID.
    Links(Vec<WorkItemId>),
}

/// Serde adapter for `FieldValue::Duration`: emits the formatted string
/// (`"5d"`) rather than the raw i64, matching how `Date` emits
/// `"YYYY-MM-DD"` rather than its internal representation. Only
/// `serialize` is defined — `FieldValue` derives `Serialize` but not
/// `Deserialize`, so the `with` module's `deserialize` half is never
/// referenced.
mod duration_seconds {
    use serde::Serializer;

    use super::format_duration_seconds;

    pub fn serialize<S>(seconds: &i64, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format_duration_seconds(*seconds))
    }
}

impl FieldValue {
    /// The value as `schema.yaml` or an item's frontmatter spells it: a
    /// date as `YYYY-MM-DD`, a duration as its shorthand, a reference
    /// as its id, a collection as a sequence. The inverse of the
    /// coercion that produced the value, for writing one back into a
    /// file — a `when` branch's `then`, a fallback default.
    pub fn to_yaml(&self) -> serde_yaml::Value {
        use serde_yaml::Value;
        match self {
            FieldValue::String(text) | FieldValue::Choice(text) | FieldValue::Color(text) => {
                Value::String(text.clone())
            }
            FieldValue::Multichoice(values) | FieldValue::List(values) => {
                Value::Sequence(values.iter().cloned().map(Value::String).collect())
            }
            FieldValue::Integer(integer) => Value::Number((*integer).into()),
            FieldValue::Float(float) => Value::Number((*float).into()),
            FieldValue::Date(date) => Value::String(date.format("%Y-%m-%d").to_string()),
            FieldValue::Duration(seconds) => Value::String(format_duration_seconds(*seconds)),
            FieldValue::Boolean(boolean) => Value::Bool(*boolean),
            FieldValue::Link(id) => Value::String(id.as_str().to_owned()),
            FieldValue::Links(ids) => Value::Sequence(
                ids.iter()
                    .map(|id| Value::String(id.as_str().to_owned()))
                    .collect(),
            ),
        }
    }
}

// ── Display formatting ──────────────────────────────────────────────

/// Format a field value as a human-readable display string.
///
/// Multi-valued variants (Multichoice, List, Links) join with `", "`.
/// For embedding inside diagnostic message prose where the value needs
/// visual delimiters, see [`format_field_value_bracketed`].
///
/// This output also defines the query engine's text semantics: the filter
/// evaluator's string coercion, the sorter's text fallback, and the scalar
/// cells of delimited output all delegate here. Changing a variant's format
/// changes what filters match and how string-like fields sort.
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
        FieldValue::Color(color) => color.clone(),
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
    fn to_yaml_spells_each_value_as_a_file_does() {
        use serde_yaml::Value;
        let date = chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap();
        assert_eq!(
            FieldValue::Date(date).to_yaml(),
            Value::String("2026-03-01".to_owned())
        );
        assert_eq!(
            FieldValue::Duration(86_400).to_yaml(),
            Value::String("1d".to_owned())
        );
        assert_eq!(FieldValue::Integer(3).to_yaml(), Value::Number(3.into()));
        assert_eq!(FieldValue::Boolean(true).to_yaml(), Value::Bool(true));
        assert_eq!(
            FieldValue::Links(vec![WorkItemId::from("a".to_owned())]).to_yaml(),
            Value::Sequence(vec![Value::String("a".to_owned())])
        );
    }

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
