//! Diagnostic types: the core model for all validation findings.
//!
//! Every source of findings (store loading, cycle detection, rule engine,
//! views check) produces [`Diagnostic`] values. The validate command
//! collects them into a single list for rendering as human-readable or
//! JSON output.
//!
//! Diagnostics are scope-typed at the structural level (see ADR-007).
//! Each [`Diagnostic`] carries a [`DiagnosticBody`] tagged by scope —
//! one of `File`, `Item`, `Files`, `Collection`, or `Config` — and each
//! body variant wraps a struct holding the source data invariant for
//! that scope plus an inner kind enum with variant-specific data.
//!
//! Adding a new diagnostic is a two-step structural decision: pick a
//! scope category (which wrapper), then add a variant to that wrapper's
//! inner kind enum.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::schema::{FieldType, Severity};
use super::views::ViewType;
use super::WorkItemId;

// ── Top-level types ──────────────────────────────────────────────────

/// A single validation finding.
///
/// Carries severity plus a scope-tagged [`DiagnosticBody`]. JSON
/// serializes flat: `{ severity, scope, ...source-data, type, ...variant-fields }`.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    /// Whether this finding is a blocking error or an informational warning.
    pub severity: Severity,
    /// Body of the diagnostic, tagged by scope category.
    #[serde(flatten)]
    pub body: DiagnosticBody,
}

/// Body of a diagnostic, tagged by scope category.
///
/// Each variant wraps a struct with the source data invariant for that
/// scope plus an inner kind enum holding the variant-specific data.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum DiagnosticBody {
    /// A diagnostic about a single file (typically I/O or parse failure).
    File(FileDiagnostic),
    /// A diagnostic about a single work item.
    Item(ItemDiagnostic),
    /// A diagnostic that intrinsically concerns multiple files.
    Files(FilesDiagnostic),
    /// A diagnostic about the collection as a whole, with no specific file.
    Collection(CollectionDiagnostic),
    /// A diagnostic about a config file (today: `views.yaml`).
    Config(ConfigDiagnostic),
}

// ── File scope ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct FileDiagnostic {
    pub source_path: PathBuf,
    #[serde(flatten)]
    pub kind: FileDiagnosticKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileDiagnosticKind {
    /// A work item or config file could not be read or parsed at all.
    ReadError { detail: String },
}

// ── Item scope ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ItemDiagnostic {
    pub source_path: PathBuf,
    pub item_id: WorkItemId,
    #[serde(flatten)]
    pub kind: ItemDiagnosticKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemDiagnosticKind {
    /// A field value doesn't match the schema's type or constraints.
    InvalidFieldValue { field: String, detail: FieldValueError },

    /// A required field is missing from the frontmatter.
    MissingRequired { field: String },

    /// A field in the frontmatter is not defined in the schema.
    UnknownField { field: String },

    /// A link/links field references an ID that doesn't exist.
    BrokenLink { field: String, target_id: WorkItemId },

    /// A schema rule was violated by this item.
    RuleViolation { rule: String, detail: String },

    /// An aggregate-configured field is set manually on this item and
    /// also on an ancestor in its rollup chain.
    AggregateChainConflict {
        field: String,
        conflicting_ancestor_id: WorkItemId,
    },

    /// An aggregate-configured field with `error_on_missing: true` has
    /// no value at this leaf (manual or inherited from an ancestor).
    AggregateMissingValue { field: String },
}

// ── Files scope ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct FilesDiagnostic {
    pub paths: Vec<PathBuf>,
    #[serde(flatten)]
    pub kind: FilesDiagnosticKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FilesDiagnosticKind {
    /// Two or more files resolved to the same ID.
    DuplicateId { id: WorkItemId },
}

// ── Collection scope ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CollectionDiagnostic {
    #[serde(flatten)]
    pub kind: CollectionDiagnosticKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CollectionDiagnosticKind {
    /// A circular reference chain was detected in a non-cyclic link field.
    Cycle {
        field: String,
        chain: Vec<WorkItemId>,
    },

    /// A collection-wide count constraint was violated.
    CountViolation {
        rule: String,
        count: usize,
        max: Option<u32>,
        min: Option<u32>,
    },
}

// ── Config scope ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ConfigDiagnostic {
    pub source_path: PathBuf,
    #[serde(flatten)]
    pub kind: ConfigDiagnosticKind,
}

/// Cross-file failures against `views.yaml`.
///
/// Today every variant carries a `view_id`. When a future `Schema*`
/// family lands for cross-file `schema.yaml` validation, those variants
/// will share `ConfigDiagnosticKind` but will not have a `view_id`.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigDiagnosticKind {
    /// Two or more view entries share the same `id`.
    ViewDuplicateId { view_id: String },

    /// A view is missing a required slot for its type (e.g. `board` without `field`).
    ViewMissingSlot {
        view_id: String,
        view_type: ViewType,
        slot: &'static str,
    },

    /// A view references a field name that isn't defined in `schema.yaml`.
    ViewUnknownField {
        view_id: String,
        slot: &'static str,
        field_name: String,
    },

    /// A view references a field whose schema type is incompatible with the slot.
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

    /// An aggregate view's `value` slot points at a field whose type is
    /// incompatible with the chosen aggregate.
    ViewAggregateTypeMismatch {
        view_id: String,
        slot: &'static str,
        aggregate: super::views::Aggregate,
        actual_type: FieldType,
    },

    /// A view slot that requires a non-cyclic link/links field
    /// (`group_by`, `after`, `root_link`, `depth_link`) points at a field
    /// whose `allow_cycles` isn't explicitly `false`.
    ViewSlotCyclic {
        view_id: String,
        slot: &'static str,
        field_name: String,
    },

    /// A view slot that requires a link/links field references an inverse
    /// relation name instead.
    ViewSlotInverseNotAllowed {
        view_id: String,
        slot: &'static str,
        field_name: String,
    },

    /// A gantt view sets neither `end` nor `duration`.
    ViewGanttEndOrDurationRequired { view_id: String },

    /// A gantt view sets both `end` and `duration`.
    ViewGanttEndAndDurationConflict { view_id: String },

    /// A gantt view sets `after` without `duration`.
    ViewGanttAfterRequiresDuration { view_id: String },

    /// A gantt view sets both `after` and `end`.
    ViewGanttAfterWithEndConflict { view_id: String },

    /// A metric row references a schema field that doesn't exist.
    /// `slot` is `"value"` or `"where"`.
    ViewMetricRowUnknownField {
        view_id: String,
        metric_index: usize,
        slot: &'static str,
        field_name: String,
    },

    /// A metric row's `value` field's type isn't compatible with the
    /// chosen aggregate.
    ViewMetricRowAggregateTypeMismatch {
        view_id: String,
        metric_index: usize,
        aggregate: super::views::Aggregate,
        actual_type: FieldType,
    },

    /// A metric row uses `aggregate: count` together with `value`.
    ViewMetricRowCountWithValue {
        view_id: String,
        metric_index: usize,
    },

    /// A metric row's per-row `where:` expression failed to parse.
    ViewMetricRowWhereParseError {
        view_id: String,
        metric_index: usize,
        raw: String,
        detail: String,
    },
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

    /// Duration value is outside the allowed range.
    OutOfRangeDuration {
        value: String,
        min: Option<String>,
        max: Option<String>,
    },

    /// Duration string failed to parse.
    InvalidDuration { value: String, reason: String },

    /// Date string is not valid YYYY-MM-DD.
    InvalidDate { value: String },

    /// String doesn't match the required regex pattern.
    PatternMismatch { value: String, pattern: String },

    /// The regex pattern itself is invalid.
    InvalidPattern { pattern: String, error: String },
}

// ── Constructors ─────────────────────────────────────────────────────

impl Diagnostic {
    /// Construct a file-scope diagnostic.
    pub fn file(severity: Severity, source_path: PathBuf, kind: FileDiagnosticKind) -> Self {
        Self {
            severity,
            body: DiagnosticBody::File(FileDiagnostic { source_path, kind }),
        }
    }

    /// Construct an item-scope diagnostic.
    pub fn item(
        severity: Severity,
        source_path: PathBuf,
        item_id: WorkItemId,
        kind: ItemDiagnosticKind,
    ) -> Self {
        Self {
            severity,
            body: DiagnosticBody::Item(ItemDiagnostic {
                source_path,
                item_id,
                kind,
            }),
        }
    }

    /// Construct a multi-file-scope diagnostic.
    pub fn files(severity: Severity, paths: Vec<PathBuf>, kind: FilesDiagnosticKind) -> Self {
        Self {
            severity,
            body: DiagnosticBody::Files(FilesDiagnostic { paths, kind }),
        }
    }

    /// Construct a collection-wide diagnostic with no specific source.
    pub fn collection(severity: Severity, kind: CollectionDiagnosticKind) -> Self {
        Self {
            severity,
            body: DiagnosticBody::Collection(CollectionDiagnostic { kind }),
        }
    }

    /// Construct a config-file-scope diagnostic.
    pub fn config(severity: Severity, source_path: PathBuf, kind: ConfigDiagnosticKind) -> Self {
        Self {
            severity,
            body: DiagnosticBody::Config(ConfigDiagnostic { source_path, kind }),
        }
    }
}

// ── Accessors ────────────────────────────────────────────────────────

impl Diagnostic {
    /// The source file this diagnostic belongs to, if any.
    ///
    /// Returns `None` for `Files` (multiple paths — read them from
    /// `body` directly) and `Collection` (no file) scopes.
    pub fn source_path(&self) -> Option<&Path> {
        match &self.body {
            DiagnosticBody::File(d) => Some(&d.source_path),
            DiagnosticBody::Item(d) => Some(&d.source_path),
            DiagnosticBody::Config(d) => Some(&d.source_path),
            DiagnosticBody::Files(_) | DiagnosticBody::Collection(_) => None,
        }
    }

    /// The view this diagnostic concerns, if any.
    ///
    /// Returns `Some(_)` for every `Config` diagnostic today; future
    /// non-view config families (e.g. schema-level) will return `None`.
    pub fn view_id(&self) -> Option<&str> {
        if let DiagnosticBody::Config(d) = &self.body {
            view_id_of(&d.kind)
        } else {
            None
        }
    }
}

/// The single place in the codebase where every view-variant is enumerated.
fn view_id_of(kind: &ConfigDiagnosticKind) -> Option<&str> {
    match kind {
        ConfigDiagnosticKind::ViewDuplicateId { view_id }
        | ConfigDiagnosticKind::ViewMissingSlot { view_id, .. }
        | ConfigDiagnosticKind::ViewUnknownField { view_id, .. }
        | ConfigDiagnosticKind::ViewFieldTypeMismatch { view_id, .. }
        | ConfigDiagnosticKind::ViewWhereParseError { view_id, .. }
        | ConfigDiagnosticKind::ViewBucketWithoutDateAxis { view_id }
        | ConfigDiagnosticKind::ViewCountAggregateWithValue { view_id }
        | ConfigDiagnosticKind::ViewAggregateTypeMismatch { view_id, .. }
        | ConfigDiagnosticKind::ViewSlotCyclic { view_id, .. }
        | ConfigDiagnosticKind::ViewSlotInverseNotAllowed { view_id, .. }
        | ConfigDiagnosticKind::ViewGanttEndOrDurationRequired { view_id }
        | ConfigDiagnosticKind::ViewGanttEndAndDurationConflict { view_id }
        | ConfigDiagnosticKind::ViewGanttAfterRequiresDuration { view_id }
        | ConfigDiagnosticKind::ViewGanttAfterWithEndConflict { view_id }
        | ConfigDiagnosticKind::ViewMetricRowUnknownField { view_id, .. }
        | ConfigDiagnosticKind::ViewMetricRowAggregateTypeMismatch { view_id, .. }
        | ConfigDiagnosticKind::ViewMetricRowCountWithValue { view_id, .. }
        | ConfigDiagnosticKind::ViewMetricRowWhereParseError { view_id, .. } => Some(view_id),
    }
}

// ── Display ──────────────────────────────────────────────────────────
//
// Layered: each inner kind enum renders its own variant data only
// (compact form). The outer `Diagnostic` orchestrates wrapper-level
// context — for `Item`, prefixes with `item 'X', `; for `File`, the
// path is shown by the file header in `render_human` so the outer
// Display also prepends the path when full context is wanted.
//
// `format_diagnostic_line` uses the inner kind Display directly for
// `Item` scope (compact under file headers) and the outer Display for
// everything else.

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.body {
            DiagnosticBody::File(d) => write!(f, "{}: {}", d.source_path.display(), d.kind),
            DiagnosticBody::Item(d) => write!(f, "item '{}', {}", d.item_id, d.kind),
            DiagnosticBody::Files(d) => write!(f, "{}: {}", d.kind, format_paths(&d.paths)),
            DiagnosticBody::Collection(d) => write!(f, "{}", d.kind),
            DiagnosticBody::Config(d) => write!(f, "{}", d.kind),
        }
    }
}

fn format_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

impl std::fmt::Display for FileDiagnosticKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileDiagnosticKind::ReadError { detail } => write!(f, "{detail}"),
        }
    }
}

impl std::fmt::Display for ItemDiagnosticKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ItemDiagnosticKind::InvalidFieldValue { field, detail } => {
                write!(f, "field '{field}': {detail}")
            }
            ItemDiagnosticKind::MissingRequired { field } => {
                write!(f, "required field '{field}' is missing")
            }
            ItemDiagnosticKind::UnknownField { field } => {
                write!(f, "unknown field '{field}'")
            }
            ItemDiagnosticKind::BrokenLink { field, target_id } => {
                write!(f, "field '{field}': broken link to '{target_id}'")
            }
            ItemDiagnosticKind::RuleViolation { rule, detail } => {
                write!(f, "rule '{rule}': {detail}")
            }
            ItemDiagnosticKind::AggregateChainConflict {
                field,
                conflicting_ancestor_id,
            } => {
                write!(
                    f,
                    "field '{field}': aggregate conflict — ancestor '{conflicting_ancestor_id}' also sets this field manually"
                )
            }
            ItemDiagnosticKind::AggregateMissingValue { field } => {
                write!(
                    f,
                    "aggregate field '{field}' is missing (no value here or in any ancestor)"
                )
            }
        }
    }
}

impl std::fmt::Display for FilesDiagnosticKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilesDiagnosticKind::DuplicateId { id } => write!(f, "duplicate ID '{id}'"),
        }
    }
}

impl std::fmt::Display for CollectionDiagnosticKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectionDiagnosticKind::Cycle { field, chain } => {
                let ids: Vec<&str> = chain.iter().map(|id| id.as_str()).collect();
                write!(f, "cycle in '{field}': {}", ids.join(" \u{2192} "))
            }
            CollectionDiagnosticKind::CountViolation {
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
        }
    }
}

impl std::fmt::Display for ConfigDiagnosticKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigDiagnosticKind::ViewDuplicateId { view_id } => {
                write!(f, "view '{view_id}' is declared more than once")
            }
            ConfigDiagnosticKind::ViewMissingSlot {
                view_id,
                view_type,
                slot,
            } => {
                write!(
                    f,
                    "view '{view_id}' (type {view_type}): missing required slot '{slot}'"
                )
            }
            ConfigDiagnosticKind::ViewUnknownField {
                view_id,
                slot,
                field_name,
            } => {
                write!(
                    f,
                    "view '{view_id}', slot '{slot}': unknown field '{field_name}'"
                )
            }
            ConfigDiagnosticKind::ViewFieldTypeMismatch {
                view_id,
                slot,
                field_name,
                actual_type,
                expected,
            } => write!(
                f,
                "view '{view_id}', slot '{slot}': field '{field_name}' has type {actual_type}, expected {expected}"
            ),
            ConfigDiagnosticKind::ViewWhereParseError {
                view_id,
                raw,
                detail,
            } => {
                write!(f, "view '{view_id}', where clause '{raw}': {detail}")
            }
            ConfigDiagnosticKind::ViewBucketWithoutDateAxis { view_id } => {
                write!(
                    f,
                    "view '{view_id}': bucket set but neither x nor y is a date field"
                )
            }
            ConfigDiagnosticKind::ViewCountAggregateWithValue { view_id } => {
                write!(
                    f,
                    "view '{view_id}': aggregate 'count' takes no 'value' slot"
                )
            }
            ConfigDiagnosticKind::ViewAggregateTypeMismatch {
                view_id,
                slot,
                aggregate,
                actual_type,
            } => write!(
                f,
                "view '{view_id}', slot '{slot}': aggregate '{aggregate}' not allowed on {actual_type} field"
            ),
            ConfigDiagnosticKind::ViewSlotCyclic {
                view_id,
                slot,
                field_name,
            } => write!(
                f,
                "view '{view_id}', slot '{slot}': field '{field_name}' must set `allow_cycles: false`"
            ),
            ConfigDiagnosticKind::ViewSlotInverseNotAllowed {
                view_id,
                slot,
                field_name,
            } => write!(
                f,
                "view '{view_id}', slot '{slot}': inverse relation '{field_name}' cannot be used (point at the original link field instead)"
            ),
            ConfigDiagnosticKind::ViewGanttEndOrDurationRequired { view_id } => {
                write!(
                    f,
                    "view '{view_id}': gantt requires exactly one of 'end' or 'duration'"
                )
            }
            ConfigDiagnosticKind::ViewGanttEndAndDurationConflict { view_id } => {
                write!(
                    f,
                    "view '{view_id}': gantt has both 'end' and 'duration' set; pick one"
                )
            }
            ConfigDiagnosticKind::ViewGanttAfterRequiresDuration { view_id } => write!(
                f,
                "view '{view_id}': gantt 'after' requires 'duration' (predecessor mode computes end as start + duration)"
            ),
            ConfigDiagnosticKind::ViewGanttAfterWithEndConflict { view_id } => write!(
                f,
                "view '{view_id}': gantt 'after' is incompatible with 'end' (use 'duration' instead)"
            ),
            ConfigDiagnosticKind::ViewMetricRowUnknownField {
                view_id,
                metric_index,
                slot,
                field_name,
            } => write!(
                f,
                "view '{view_id}', metrics[{metric_index}].{slot}: unknown field '{field_name}'"
            ),
            ConfigDiagnosticKind::ViewMetricRowAggregateTypeMismatch {
                view_id,
                metric_index,
                aggregate,
                actual_type,
            } => write!(
                f,
                "view '{view_id}', metrics[{metric_index}].value: aggregate '{aggregate}' not allowed on {actual_type} field"
            ),
            ConfigDiagnosticKind::ViewMetricRowCountWithValue {
                view_id,
                metric_index,
            } => write!(
                f,
                "view '{view_id}', metrics[{metric_index}]: aggregate 'count' takes no 'value' slot"
            ),
            ConfigDiagnosticKind::ViewMetricRowWhereParseError {
                view_id,
                metric_index,
                raw,
                detail,
            } => write!(
                f,
                "view '{view_id}', metrics[{metric_index}].where clause '{raw}': {detail}"
            ),
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
            Self::OutOfRangeDuration { value, min, max } => {
                write!(
                    f,
                    "duration '{value}' is out of range (min: {min:?}, max: {max:?})"
                )
            }
            Self::InvalidDuration { value, reason } => {
                write!(f, "'{value}' is not a valid duration: {reason}")
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
