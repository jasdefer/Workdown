//! Data types for the query engine.
//!
//! These types represent query requests and results. The [`Predicate`] tree
//! is built by the CLI parser or programmatically by other commands (board,
//! tree, graph). The [`QueryRequest`] bundles a predicate with sort and
//! column specifications. The engine evaluates it and returns a [`QueryResult`].

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
    Related {
        relation: String,
        field: String,
    },
}

/// Comparison operators supported by the query engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
