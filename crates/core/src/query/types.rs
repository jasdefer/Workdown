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

/// A comparison of a single field against an operand.
#[derive(Debug, Clone)]
pub struct Comparison {
    /// Which field to compare.
    pub field: FieldReference,
    /// The comparison operator.
    pub operator: Operator,
    /// What to compare against.
    pub operand: Operand,
}

/// The right-hand side of a comparison.
#[derive(Debug, Clone)]
pub enum Operand {
    /// A literal value — resolved against the field's schema type at
    /// evaluation time.
    Value(String),
    /// A regular expression, compiled once at parse time. Carried by
    /// [`Operator::Matches`] comparisons.
    Regex(QueryRegex),
}

impl Operand {
    /// The operand as clause text: the literal value, or the regex's
    /// original `/pattern/flags` form.
    pub fn text(&self) -> &str {
        match self {
            Operand::Value(value) => value,
            Operand::Regex(regex) => regex.source(),
        }
    }

    /// The compiled regex, when this operand is one.
    pub fn regex(&self) -> Option<&QueryRegex> {
        match self {
            Operand::Regex(regex) => Some(regex),
            Operand::Value(_) => None,
        }
    }
}

/// A regex operand: compiled once at parse time, carrying its original
/// `/pattern/flags` clause form for serialization. The convention lives
/// here and nowhere else — no other code re-encodes or re-splits it.
#[derive(Debug, Clone)]
pub struct QueryRegex {
    source: String,
    compiled: regex::Regex,
}

impl QueryRegex {
    /// Compile a pattern with its flags. `i` (case-insensitive) is the only
    /// flag; anything else the grammar rejects before reaching here.
    pub fn new(pattern: &str, flags: &str) -> Result<Self, regex::Error> {
        let full_pattern = if flags.contains('i') {
            format!("(?i){pattern}")
        } else {
            pattern.to_owned()
        };
        Ok(Self {
            source: format!("/{pattern}/{flags}"),
            compiled: regex::Regex::new(&full_pattern)?,
        })
    }

    /// Whether the pattern matches anywhere in `haystack`.
    pub fn is_match(&self, haystack: &str) -> bool {
        self.compiled.is_match(haystack)
    }

    /// The original `/pattern/flags` clause form.
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// A reference to a field on a work item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldReference {
    /// A field on the current item, e.g. `"status"`.
    Local(String),
    /// A field on a related item, e.g. `"parent.status"` — the relation
    /// segment is a link/links field or a derived inverse.
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
    /// Field equals any of several values. Never reaches the evaluator —
    /// [`crate::query::parse`] rewrites it into an [`Predicate::Or`] of
    /// [`Operator::Equal`] comparisons before evaluation, and
    /// [`crate::query::clause`] folds that shape back. It exists as an
    /// operator so the wire format and the guided builder can name it.
    In,
    /// Field equals none of several values. Rewritten into an
    /// [`Predicate::And`] of [`Operator::NotEqual`] comparisons, on the same
    /// terms as [`Operator::In`].
    NotIn,
}

impl Operator {
    /// Whether this operator asserts the *absence* of a match rather than a
    /// match. An item whose field has no value satisfies these and fails
    /// every other value comparison — see [`crate::query::eval`].
    ///
    /// [`Operator::IsNotSet`] is deliberately not here: presence checks are
    /// answered before the absent-value rule applies.
    pub fn is_negative(self) -> bool {
        matches!(self, Operator::NotEqual | Operator::NotIn)
    }

    /// Whether this operator takes a list of values (`values`) rather than a
    /// single one (`value`).
    pub fn is_list_valued(self) -> bool {
        matches!(self, Operator::In | Operator::NotIn)
    }

    /// How this operator is spelled in the clause grammar. Used in diagnostics
    /// and error messages so they quote what a user would type; the clause
    /// *serializer* still formats each operator itself, because the operand's
    /// position differs (`field?`, `!field?`, `field/pattern/`).
    pub fn token(self) -> &'static str {
        match self {
            Operator::Equal => "=",
            Operator::NotEqual => "!=",
            Operator::GreaterThan => ">",
            Operator::LessThan => "<",
            Operator::GreaterOrEqual => ">=",
            Operator::LessOrEqual => "<=",
            Operator::Contains => "~",
            Operator::Matches => "/…/",
            Operator::IsSet => "?",
            Operator::IsNotSet => "!?",
            Operator::In => "in",
            Operator::NotIn => "not in",
        }
    }
}

/// The operators the filter builder should *offer* for a field type — a
/// curated subset chosen for what reads meaningfully to a user, not the
/// full set the evaluator can compute. [`crate::query::eval`] is more
/// permissive (it will lexicographically compare any string-like field for
/// `>` / `<`, etc.); that path stays reachable via a hand-written clause or
/// the raw escape hatch, it just isn't surfaced in the guided builder.
///
/// `IsSet` / `IsNotSet` test presence and apply to every type. Otherwise:
/// - `string` — equality plus substring (`contains`) and regex (`matches`).
///   Ordering is omitted: byte-wise string comparison surprises users
///   (case-sensitive, and `"10" < "9"`).
/// - `choice` — equality and list membership (`in` / `not in`). Categories
///   are matched whole, and lexicographic ordering of category names is
///   meaningless.
/// - `date` — equality and ordering (ISO dates sort chronologically as
///   text); substring / regex omitted.
/// - `link` — equality and list membership; a link is an id reference.
/// - `integer` / `float` / `duration` — ordered scalars: equality and
///   comparison.
/// - `boolean` — equality only.
/// - `color` — equality only, compared on the *resolved hex* (so
///   `color == red` matches an item storing red's pinned hex literally).
///   Ordering and substring matching are meaningless, as for `choice`.
/// - `multichoice` / `list` / `links` — collections: membership (`equal` /
///   `not_equal`) plus per-element `contains` / `matches`, and `in` /
///   `not in` against several members at once.
///
/// `In` / `NotIn` are offered only for the choice-like and link-like types —
/// the ones where "any of these known values" is the natural question and the
/// builder can render a picker. They stay reachable elsewhere through a
/// hand-written clause, like the operators the evaluator supports but this
/// list omits.
pub fn operators_for(field_type: FieldType) -> Vec<Operator> {
    use FieldType::*;
    use Operator::*;

    let mut operators = match field_type {
        String => vec![Equal, NotEqual, Contains, Matches],
        Choice | Link => vec![Equal, NotEqual, In, NotIn],
        Date => vec![
            Equal,
            NotEqual,
            GreaterThan,
            LessThan,
            GreaterOrEqual,
            LessOrEqual,
        ],
        Integer | Float | Duration => vec![
            Equal,
            NotEqual,
            GreaterThan,
            LessThan,
            GreaterOrEqual,
            LessOrEqual,
        ],
        Boolean | Color => vec![Equal, NotEqual],
        Multichoice | Links => vec![Equal, NotEqual, In, NotIn, Contains, Matches],
        // A `list` holds free-form strings, so there is no known value set to
        // pick members from — `in` stays in the raw hatch here.
        List => vec![Equal, NotEqual, Contains, Matches],
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
            FieldType::Color,
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
    fn color_supports_equality_only() {
        let operators = operators_for(FieldType::Color);
        assert_eq!(
            operators,
            vec![
                Operator::Equal,
                Operator::NotEqual,
                Operator::IsSet,
                Operator::IsNotSet
            ]
        );
    }

    #[test]
    fn boolean_supports_equality_only() {
        let operators = operators_for(FieldType::Boolean);
        assert_eq!(
            operators,
            vec![
                Operator::Equal,
                Operator::NotEqual,
                Operator::IsSet,
                Operator::IsNotSet
            ]
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
    fn string_supports_substring_and_regex_but_not_ordering() {
        let operators = operators_for(FieldType::String);
        assert!(operators.contains(&Operator::Contains));
        assert!(operators.contains(&Operator::Matches));
        assert!(operators.contains(&Operator::Equal));
        // Ordering is a byte-wise footgun for free text — not offered.
        assert!(!operators.contains(&Operator::GreaterThan));
        assert!(!operators.contains(&Operator::LessThan));
    }

    #[test]
    fn choice_and_link_offer_equality_and_membership() {
        for field_type in [FieldType::Choice, FieldType::Link] {
            let operators = operators_for(field_type);
            assert_eq!(
                operators,
                vec![
                    Operator::Equal,
                    Operator::NotEqual,
                    Operator::In,
                    Operator::NotIn,
                    Operator::IsSet,
                    Operator::IsNotSet
                ],
                "{field_type}"
            );
        }
    }

    #[test]
    fn membership_offered_for_choice_like_and_link_like_only() {
        for field_type in [
            FieldType::Choice,
            FieldType::Multichoice,
            FieldType::Link,
            FieldType::Links,
        ] {
            let operators = operators_for(field_type);
            assert!(operators.contains(&Operator::In), "{field_type}");
            assert!(operators.contains(&Operator::NotIn), "{field_type}");
        }
        // No known value set to pick members from.
        for field_type in [
            FieldType::String,
            FieldType::List,
            FieldType::Integer,
            FieldType::Float,
            FieldType::Date,
            FieldType::Duration,
            FieldType::Boolean,
            FieldType::Color,
        ] {
            let operators = operators_for(field_type);
            assert!(!operators.contains(&Operator::In), "{field_type}");
            assert!(!operators.contains(&Operator::NotIn), "{field_type}");
        }
    }

    #[test]
    fn negative_operators_are_not_equal_and_not_in() {
        assert!(Operator::NotEqual.is_negative());
        assert!(Operator::NotIn.is_negative());
        assert!(!Operator::Equal.is_negative());
        assert!(!Operator::In.is_negative());
        assert!(!Operator::Contains.is_negative());
        // Presence checks are answered before the absent-value rule.
        assert!(!Operator::IsNotSet.is_negative());
    }

    #[test]
    fn date_supports_ordering_but_not_substring() {
        let operators = operators_for(FieldType::Date);
        assert!(operators.contains(&Operator::GreaterThan));
        assert!(operators.contains(&Operator::LessOrEqual));
        // Chronological ordering is meaningful; substring/regex on a date
        // is not offered.
        assert!(!operators.contains(&Operator::Contains));
        assert!(!operators.contains(&Operator::Matches));
    }
}
