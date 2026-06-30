//! Data types for the query engine.
//!
//! These types represent query requests and results. The [`Predicate`] tree
//! is built by the CLI parser or programmatically by other commands (board,
//! tree, graph). The [`QueryRequest`] bundles a predicate with sort and
//! column specifications. The engine evaluates it and returns a [`QueryResult`].

use crate::model::schema::FieldType;

// ── Predicate model ─────────────────────────────────────────────────

/// A composable filter expression.
#[derive(Debug, Clone)]
pub enum Predicate {
    /// A single field comparison.
    Comparison(Comparison),
    /// All predicates must match.
    And(Vec<Predicate>),
    /// At least one predicate must match.
    Or(Vec<Predicate>),
    /// Negate the inner predicate.
    Not(Box<Predicate>),
}

/// A comparison of a single field against a value.
#[derive(Debug, Clone)]
pub struct Comparison {
    /// Which field to compare.
    pub field: FieldReference,
    /// The comparison operator.
    pub operator: Operator,
    /// The raw value to compare against — resolved against the field's
    /// schema type at evaluation time.
    pub value: String,
}

/// A reference to a field on a work item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldReference {
    /// A field on the current item, e.g. `"status"`.
    Local(String),
    /// A field on a related item, e.g. `"parent.status"`.
    /// Defined for future use — not yet supported by the parser or evaluator.
    Related { relation: String, field: String },
}

/// Comparison operators supported by the query engine.
///
/// Serializes in `snake_case` (`"equal"`, `"not_equal"`, `"is_set"`, …) —
/// this is the wire form the editing-vocabulary endpoint reports so the UI
/// knows which comparisons a field allows. See [`operators_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterOrEqual,
    LessOrEqual,
    /// Substring match for string-like fields, membership check for list-like fields.
    Contains,
    /// Regular expression match.
    Matches,
    /// Field is present (has a value).
    IsSet,
    /// Field is absent (no value).
    IsNotSet,
}

/// The operators that produce a meaningful comparison for a given field
/// type, mirroring how [`crate::query::eval`] dispatches on type. Reported
/// by the editing-vocabulary endpoint so a filter builder never offers an
/// operator the evaluator would treat as a no-op (e.g. `>` on a boolean).
///
/// `IsSet` / `IsNotSet` test presence and so apply to every type. The rest
/// follow the evaluator:
/// - string / choice / date / link use lexicographic comparison plus
///   substring (`contains`) and regex (`matches`);
/// - integer / float / duration are ordered scalars — comparison only;
/// - boolean supports equality only;
/// - multichoice / list / links are collections — membership (`equal` /
///   `not_equal`) plus per-element `contains` / `matches`.
pub fn operators_for(field_type: FieldType) -> Vec<Operator> {
    use FieldType::*;
    use Operator::*;

    let mut operators = match field_type {
        String | Choice | Date | Link => vec![
            Equal,
            NotEqual,
            GreaterThan,
            LessThan,
            GreaterOrEqual,
            LessOrEqual,
            Contains,
            Matches,
        ],
        Integer | Float | Duration => vec![
            Equal,
            NotEqual,
            GreaterThan,
            LessThan,
            GreaterOrEqual,
            LessOrEqual,
        ],
        Boolean => vec![Equal, NotEqual],
        Multichoice | List | Links => vec![Equal, NotEqual, Contains, Matches],
    };
    // Presence checks are type-agnostic — the evaluator answers them before
    // it ever looks at the field's type.
    operators.push(IsSet);
    operators.push(IsNotSet);
    operators
}

// ── Query request ───────────────────────────────────────────────────

/// A complete query: optional filter, sort order, and column selection.
#[derive(Debug, Clone)]
pub struct QueryRequest {
    /// Filter predicate. `None` means "match all items".
    pub predicate: Option<Predicate>,
    /// Sort specifications, applied in order. Empty means "no sorting"
    /// (items come out in store iteration order).
    pub sort: Vec<SortSpec>,
    /// Column names to include in the result. Empty means "use defaults"
    /// (id + required schema fields).
    pub fields: Vec<String>,
}

/// A single sort specification: field name and direction.
#[derive(Debug, Clone)]
pub struct SortSpec {
    pub field: String,
    pub direction: SortDirection,
}

/// Sort direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

// ── Query result ────────────────────────────────────────────────────

/// The result of executing a query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    /// Column names, in display order.
    pub columns: Vec<String>,
    /// One row per matched work item, in final sorted order.
    pub items: Vec<QueryRow>,
}

/// A single result row with pre-formatted display values.
#[derive(Debug, Clone)]
pub struct QueryRow {
    /// The work item's ID.
    pub id: String,
    /// One value per column (same length and order as [`QueryResult::columns`]).
    pub values: Vec<String>,
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_serializes_snake_case() {
        let json = serde_json::to_string(&Operator::GreaterOrEqual).unwrap();
        assert_eq!(json, "\"greater_or_equal\"");
        let json = serde_json::to_string(&Operator::IsNotSet).unwrap();
        assert_eq!(json, "\"is_not_set\"");
    }

    #[test]
    fn presence_operators_apply_to_every_type() {
        for field_type in [
            FieldType::String,
            FieldType::Choice,
            FieldType::Multichoice,
            FieldType::Integer,
            FieldType::Float,
            FieldType::Date,
            FieldType::Duration,
            FieldType::Boolean,
            FieldType::List,
            FieldType::Link,
            FieldType::Links,
        ] {
            let operators = operators_for(field_type);
            assert!(operators.contains(&Operator::IsSet), "{field_type}");
            assert!(operators.contains(&Operator::IsNotSet), "{field_type}");
        }
    }

    #[test]
    fn boolean_supports_equality_only() {
        let operators = operators_for(FieldType::Boolean);
        assert_eq!(
            operators,
            vec![Operator::Equal, Operator::NotEqual, Operator::IsSet, Operator::IsNotSet]
        );
    }

    #[test]
    fn numeric_types_support_ordering_but_not_substring() {
        for field_type in [FieldType::Integer, FieldType::Float, FieldType::Duration] {
            let operators = operators_for(field_type);
            assert!(operators.contains(&Operator::GreaterThan), "{field_type}");
            assert!(!operators.contains(&Operator::Contains), "{field_type}");
            assert!(!operators.contains(&Operator::Matches), "{field_type}");
        }
    }

    #[test]
    fn collection_types_support_membership_and_element_match_not_ordering() {
        for field_type in [FieldType::Multichoice, FieldType::List, FieldType::Links] {
            let operators = operators_for(field_type);
            assert!(operators.contains(&Operator::Contains), "{field_type}");
            assert!(operators.contains(&Operator::Matches), "{field_type}");
            assert!(!operators.contains(&Operator::GreaterThan), "{field_type}");
        }
    }

    #[test]
    fn string_like_types_support_comparison_and_match() {
        for field_type in [
            FieldType::String,
            FieldType::Choice,
            FieldType::Date,
            FieldType::Link,
        ] {
            let operators = operators_for(field_type);
            assert!(operators.contains(&Operator::LessThan), "{field_type}");
            assert!(operators.contains(&Operator::Contains), "{field_type}");
            assert!(operators.contains(&Operator::Matches), "{field_type}");
        }
    }
}
