//! Diagnostic types: the core model for all validation findings.
//!
//! Every source of findings (store loading, cycle detection, rule engine)
//! produces [`Diagnostic`] values. The validate command collects them into
//! a single list for rendering as human-readable or JSON output.

use std::path::PathBuf;

use serde::Serialize;

use super::schema::{FieldType, Severity};
use super::views::ViewType;
use super::WorkItemId;

// ── Core types ───────────────────────────────────────────────────────

/// A single validation finding.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    /// Whether this finding is a blocking error or an informational warning.
    pub severity: Severity,
    /// What kind of finding this is, with structured context data.
    pub kind: DiagnosticKind,
}

/// The specific kind of finding, with all relevant context.
///
/// Each variant carries exactly the data needed for that finding type —
/// no optional fields for data that doesn't apply.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DiagnosticKind {
    // ── File-level ────────────────────────────────────────────────
    /// A work item or config file could not be read or parsed at all.
    FileError { path: PathBuf, detail: String },

    // ── Field-level ───────────────────────────────────────────────
    /// A field value doesn't match the schema's type or constraints.
    InvalidFieldValue {
        item_id: WorkItemId,
        field: String,
        detail: FieldValueError,
    },

    /// A required field is missing from the frontmatter.
    MissingRequired { item_id: WorkItemId, field: String },

    /// A field in the frontmatter is not defined in the schema.
    UnknownField { item_id: WorkItemId, field: String },

    // ── Reference-level ───────────────────────────────────────────
    /// A link/links field references an ID that doesn't exist.
    BrokenLink {
        item_id: WorkItemId,
        field: String,
        target_id: WorkItemId,
    },

    /// Two or more files resolved to the same ID.
    DuplicateId { id: WorkItemId, paths: Vec<PathBuf> },

    /// A circular reference chain was detected in a non-cyclic link field.
    Cycle {
        field: String,
        chain: Vec<WorkItemId>,
    },

    // ── Rule-level ────────────────────────────────────────────────
    /// A schema rule was violated by a specific item.
    RuleViolation {
        item_id: WorkItemId,
        rule: String,
        detail: String,
    },

    /// A collection-wide count constraint was violated.
    CountViolation {
        rule: String,
        count: usize,
        max: Option<u32>,
        min: Option<u32>,
    },

    // ── View-level ────────────────────────────────────────────────
    // Structured parse-time failures (produced by `views_check::parse_errors_to_diagnostics`
    // from a `ViewsLoadError`) and cross-file check failures (produced by
    // `views_check::evaluate` against `views.yaml` + `schema.yaml`). Read/YAML
    // parse failures surface as the generic `FileError` with `views.yaml`'s path.
    /// Two or more view entries share the same `id`.
    ViewDuplicateId { view_id: String },

    /// A view is missing a required slot for its type (e.g. `board` without `field`).
    ViewMissingSlot {
        view_id: String,
        view_type: ViewType,
        slot: &'static str,
    },

    /// A view references a field name that isn't defined in `schema.yaml`.
    /// Slot `"where"` covers field references inside filter expressions.
    ViewUnknownField {
        view_id: String,
        slot: &'static str,
        field_name: String,
    },

    /// A view references a field whose schema type is incompatible with
    /// the slot (e.g. `tree.field` points at a `choice` field).
    ViewFieldTypeMismatch {
        view_id: String,
        slot: &'static str,
        field_name: String,
        actual_type: FieldType,
        /// Human-readable list of allowed types, e.g. `"choice, multichoice, or string"`.
        expected: String,
    },

    /// A `where:` expression string failed to parse.
    ViewWhereParseError {
        view_id: String,
        raw: String,
        detail: String,
    },

    /// A heatmap view has a `bucket` set but neither `x` nor `y` resolves
    /// to a `date` field.
    ViewBucketWithoutDateAxis { view_id: String },

    /// A metric view with `aggregate: count` also sets `value`, which is
    /// meaningless (count takes no value field).
    ViewCountAggregateWithValue { view_id: String },
}

// ── Field value errors ───────────────────────────────────────────────

/// Specific reason a field value is invalid.
///
/// Produced by the coercion layer when converting raw YAML values to
/// typed [`FieldValue`](super::FieldValue)s.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldValueError {
    /// Expected one type, got another.
    TypeMismatch { expected: FieldType, got: String },

    /// Value is not in the allowed list (choice field).
    InvalidChoice { value: String, allowed: Vec<String> },

    /// One or more values are not in the allowed list (multichoice field).
    InvalidMultichoice {
        values: Vec<String>,
        allowed: Vec<String>,
    },

    /// Numeric value is outside the allowed range.
    OutOfRange {
        value: f64,
        min: Option<f64>,
        max: Option<f64>,
    },

    /// Date string is not valid YYYY-MM-DD.
    InvalidDate { value: String },

    /// String doesn't match the required regex pattern.
    PatternMismatch { value: String, pattern: String },

    /// The regex pattern itself is invalid.
    InvalidPattern { pattern: String, error: String },
}

// ── Display ──────────────────────────────────────────────────────────

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            DiagnosticKind::FileError { path, detail } => {
                write!(f, "{}: {detail}", path.display())
            }
            DiagnosticKind::InvalidFieldValue {
                item_id,
                field,
                detail,
            } => {
                write!(f, "item '{item_id}', field '{field}': {detail}")
            }
            DiagnosticKind::MissingRequired { item_id, field } => {
                write!(f, "item '{item_id}': required field '{field}' is missing")
            }
            DiagnosticKind::UnknownField { item_id, field } => {
                write!(f, "item '{item_id}': unknown field '{field}'")
            }
            DiagnosticKind::BrokenLink {
                item_id,
                field,
                target_id,
            } => {
                write!(
                    f,
                    "item '{item_id}', field '{field}': broken link to '{target_id}'"
                )
            }
            DiagnosticKind::DuplicateId { id, paths } => {
                let files: Vec<_> = paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
                write!(f, "duplicate ID '{id}': {}", files.join(", "))
            }
            DiagnosticKind::Cycle { field, chain } => {
                let ids: Vec<&str> = chain.iter().map(|id| id.as_str()).collect();
                write!(f, "cycle in '{field}': {}", ids.join(" \u{2192} "))
            }
            DiagnosticKind::RuleViolation {
                item_id,
                rule,
                detail,
            } => {
                write!(f, "item '{item_id}': rule '{rule}': {detail}")
            }
            DiagnosticKind::CountViolation {
                rule,
                count,
                max,
                min,
            } => {
                write!(f, "rule '{rule}': {count} matching items")?;
                if let Some(max) = max {
                    write!(f, " (max {max})")?;
                }
                if let Some(min) = min {
                    write!(f, " (min {min})")?;
                }
                Ok(())
            }

            // View-level: terse under a grouped file header.
            DiagnosticKind::ViewDuplicateId { view_id } => {
                write!(f, "view '{view_id}' is declared more than once")
            }
            DiagnosticKind::ViewMissingSlot {
                view_id,
                view_type,
                slot,
            } => {
                write!(
                    f,
                    "view '{view_id}' (type {view_type}): missing required slot '{slot}'"
                )
            }
            DiagnosticKind::ViewUnknownField {
                view_id,
                slot,
                field_name,
            } => {
                write!(
                    f,
                    "view '{view_id}', slot '{slot}': unknown field '{field_name}'"
                )
            }
            DiagnosticKind::ViewFieldTypeMismatch {
                view_id,
                slot,
                field_name,
                actual_type,
                expected,
            } => {
                write!(
                    f,
                    "view '{view_id}', slot '{slot}': field '{field_name}' has type {actual_type}, expected {expected}"
                )
            }
            DiagnosticKind::ViewWhereParseError {
                view_id,
                raw,
                detail,
            } => {
                write!(f, "view '{view_id}', where clause '{raw}': {detail}")
            }
            DiagnosticKind::ViewBucketWithoutDateAxis { view_id } => {
                write!(
                    f,
                    "view '{view_id}': bucket set but neither x nor y is a date field"
                )
            }
            DiagnosticKind::ViewCountAggregateWithValue { view_id } => {
                write!(
                    f,
                    "view '{view_id}': aggregate 'count' takes no 'value' slot"
                )
            }
        }
    }
}

impl std::fmt::Display for FieldValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TypeMismatch { expected, got } => {
                write!(f, "expected {expected}, got {got}")
            }
            Self::InvalidChoice { value, allowed } => {
                write!(f, "'{value}' is not one of the allowed values: {allowed:?}")
            }
            Self::InvalidMultichoice { values, allowed } => {
                write!(f, "invalid values {values:?}, allowed: {allowed:?}")
            }
            Self::OutOfRange { value, min, max } => {
                write!(f, "{value} is out of range (min: {min:?}, max: {max:?})")
            }
            Self::InvalidDate { value } => {
                write!(f, "'{value}' is not a valid date (expected YYYY-MM-DD)")
            }
            Self::PatternMismatch { value, pattern } => {
                write!(f, "'{value}' does not match pattern '{pattern}'")
            }
            Self::InvalidPattern { pattern, error } => {
                write!(f, "invalid regex pattern '{pattern}': {error}")
            }
        }
    }
}
