//! Cross-file validation for `views.yaml`.
//!
//! Catches bad view configs at `workdown validate` time rather than at
//! render time: field references that don't resolve against `schema.yaml`,
//! slot/type mismatches (e.g. `tree.field` pointing at a `choice`),
//! malformed `where:` expressions, and a handful of cross-slot constraints.
//!
//! After [`evaluate`] returns no errors, every field name referenced by
//! `views.yaml` is either present in `schema.fields`, is a recognized
//! relation name (forward link/links field name, or an inverse name from
//! `schema.inverse_table`), or is the virtual `"id"` field. Renderers and
//! extractors can rely on that invariant without re-checking.
//!
//! The companion helper [`parse_errors_to_diagnostics`] converts load-time
//! errors from [`crate::parser::views`] into the same diagnostic stream,
//! so `workdown validate` can report them instead of aborting.

use std::path::Path;

use crate::model::diagnostic::{Diagnostic, DiagnosticKind};
use crate::model::schema::{FieldDefinition, FieldType, Schema, Severity};
use crate::model::views::{Aggregate, View, ViewKind, Views};
use crate::parser::schema::is_relation_anchor;
use crate::parser::views::{ViewsLoadError, ViewsValidationError};
use crate::query::parse::parse_where;
use crate::query::types::{FieldReference, Predicate};

// ── Public API ──────────────────────────────────────────────────────

/// Run all cross-file checks on a parsed `views.yaml` against a schema.
///
/// Returns one [`Diagnostic`] per problem found; does not stop at the first.
/// All diagnostics produced here have [`Severity::Error`] — there are no
/// warnings in v1.
pub fn evaluate(views: &Views, schema: &Schema) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for view in &views.views {
        check_view(view, schema, &mut out);
        check_title(view, schema, &mut out);
        check_where_clauses(view, schema, &mut out);
    }
    out
}

/// Load `views.yaml` from disk and run cross-file checks, routing any
/// load-time error through [`parse_errors_to_diagnostics`].
///
/// Returns an empty `Vec` when the file is absent — `views.yaml` is
/// optional. All other failures (I/O, YAML parse, semantic validation)
/// are reported as diagnostics rather than propagating.
pub fn load_and_check(views_path: &Path, schema: &Schema) -> Vec<Diagnostic> {
    if !views_path.exists() {
        return Vec::new();
    }
    match crate::parser::views::load_views(views_path) {
        Ok(views) => evaluate(&views, schema),
        Err(err) => parse_errors_to_diagnostics(err, views_path),
    }
}

/// Convert a [`ViewsLoadError`] from the views parser into a list of
/// diagnostics pointed at `views_path`.
///
/// `ReadFailed` and `InvalidYaml` become a single [`DiagnosticKind::FileError`]
/// (the detail carries the serde line/column or I/O message). `Validation`
/// expands into one structured diagnostic per semantic error:
/// [`DiagnosticKind::ViewDuplicateId`] or [`DiagnosticKind::ViewMissingSlot`].
pub fn parse_errors_to_diagnostics(err: ViewsLoadError, views_path: &Path) -> Vec<Diagnostic> {
    match err {
        ViewsLoadError::ReadFailed(io) => vec![error(DiagnosticKind::FileError {
            path: views_path.to_path_buf(),
            detail: io.to_string(),
        })],
        ViewsLoadError::InvalidYaml(yaml) => vec![error(DiagnosticKind::FileError {
            path: views_path.to_path_buf(),
            detail: yaml.to_string(),
        })],
        ViewsLoadError::Validation(errors) => errors
            .into_iter()
            .map(|err| error(validation_error_to_kind(err)))
            .collect(),
    }
}

// ── Validation-error → DiagnosticKind ────────────────────────────────

fn validation_error_to_kind(err: ViewsValidationError) -> DiagnosticKind {
    match err {
        ViewsValidationError::DuplicateId { id } => DiagnosticKind::ViewDuplicateId { view_id: id },
        ViewsValidationError::MissingSlot {
            id,
            view_type,
            slot,
        } => DiagnosticKind::ViewMissingSlot {
            view_id: id,
            view_type,
            slot,
        },
    }
}

// ── Per-view checks ──────────────────────────────────────────────────

fn check_view(view: &View, schema: &Schema, out: &mut Vec<Diagnostic>) {
    let view_id = view.id.as_str();

    match &view.kind {
        ViewKind::Board { field } => check_slot(
            schema,
            view_id,
            "field",
            field,
            &[FieldType::Choice, FieldType::Multichoice, FieldType::String],
            "choice, multichoice, or string",
            out,
        ),
        ViewKind::Tree { field } => check_slot(
            schema,
            view_id,
            "field",
            field,
            &[FieldType::Link],
            "link",
            out,
        ),
        ViewKind::Graph { field } => check_graph_field(schema, view_id, field, out),
        ViewKind::Table { columns } => {
            for column in columns {
                check_slot(schema, view_id, "columns", column, &[], "", out);
            }
        }
        ViewKind::Gantt { start, end, group } => {
            check_slot(
                schema,
                view_id,
                "start",
                start,
                &[FieldType::Date],
                "date",
                out,
            );
            check_slot(schema, view_id, "end", end, &[FieldType::Date], "date", out);
            if let Some(group) = group {
                check_slot(
                    schema,
                    view_id,
                    "group",
                    group,
                    &[
                        FieldType::Choice,
                        FieldType::Multichoice,
                        FieldType::String,
                        FieldType::Link,
                    ],
                    "choice, multichoice, string, or link",
                    out,
                );
            }
        }
        ViewKind::BarChart {
            group_by, value, ..
        } => {
            check_slot(
                schema,
                view_id,
                "group_by",
                group_by,
                &[FieldType::Choice, FieldType::Multichoice, FieldType::String],
                "choice, multichoice, or string",
                out,
            );
            if let Some(value) = value {
                check_slot(
                    schema,
                    view_id,
                    "value",
                    value,
                    &[FieldType::Integer, FieldType::Float],
                    "integer or float",
                    out,
                );
            }
        }
        ViewKind::LineChart { x, y } => {
            let allowed = &[FieldType::Integer, FieldType::Float, FieldType::Date];
            check_slot(
                schema,
                view_id,
                "x",
                x,
                allowed,
                "integer, float, or date",
                out,
            );
            check_slot(
                schema,
                view_id,
                "y",
                y,
                allowed,
                "integer, float, or date",
                out,
            );
        }
        ViewKind::Workload { start, end, effort } => {
            check_slot(
                schema,
                view_id,
                "start",
                start,
                &[FieldType::Date],
                "date",
                out,
            );
            check_slot(schema, view_id, "end", end, &[FieldType::Date], "date", out);
            check_slot(
                schema,
                view_id,
                "effort",
                effort,
                &[FieldType::Integer, FieldType::Float],
                "integer or float",
                out,
            );
        }
        ViewKind::Metric {
            value, aggregate, ..
        } => {
            if let Some(value) = value {
                check_slot(
                    schema,
                    view_id,
                    "value",
                    value,
                    &[FieldType::Integer, FieldType::Float],
                    "integer or float",
                    out,
                );
            }
            if *aggregate == Aggregate::Count && value.is_some() {
                out.push(error(DiagnosticKind::ViewCountAggregateWithValue {
                    view_id: view_id.to_owned(),
                }));
            }
        }
        ViewKind::Treemap { group, size } => {
            check_slot(
                schema,
                view_id,
                "group",
                group,
                &[
                    FieldType::Choice,
                    FieldType::Multichoice,
                    FieldType::String,
                    FieldType::Link,
                ],
                "choice, multichoice, string, or link",
                out,
            );
            check_slot(
                schema,
                view_id,
                "size",
                size,
                &[FieldType::Integer, FieldType::Float],
                "integer or float",
                out,
            );
        }
        ViewKind::Heatmap {
            x,
            y,
            value,
            bucket,
            ..
        } => {
            let axis_allowed = &[
                FieldType::Choice,
                FieldType::Multichoice,
                FieldType::String,
                FieldType::Date,
            ];
            check_slot(
                schema,
                view_id,
                "x",
                x,
                axis_allowed,
                "choice, multichoice, string, or date",
                out,
            );
            check_slot(
                schema,
                view_id,
                "y",
                y,
                axis_allowed,
                "choice, multichoice, string, or date",
                out,
            );
            if let Some(value) = value {
                check_slot(
                    schema,
                    view_id,
                    "value",
                    value,
                    &[FieldType::Integer, FieldType::Float],
                    "integer or float",
                    out,
                );
            }
            if bucket.is_some() && !has_date_axis(schema, x, y) {
                out.push(error(DiagnosticKind::ViewBucketWithoutDateAxis {
                    view_id: view_id.to_owned(),
                }));
            }
        }
    }
}

// ── Title slot (cross-cutting) ───────────────────────────────────────

fn check_title(view: &View, schema: &Schema, out: &mut Vec<Diagnostic>) {
    let Some(field_name) = view.title.as_deref() else {
        return;
    };
    check_slot(
        schema,
        view.id.as_str(),
        "title",
        field_name,
        &[FieldType::String, FieldType::Choice],
        "string or choice",
        out,
    );
}

// ── Slot helper ──────────────────────────────────────────────────────

/// Check one slot's field reference. Emits:
/// - [`DiagnosticKind::ViewUnknownField`] if `field_name` isn't defined in
///   `schema.fields` and isn't the virtual `"id"`,
/// - [`DiagnosticKind::ViewFieldTypeMismatch`] if `allowed` is non-empty and
///   the field's type isn't in the list.
///
/// Passing an empty `allowed` performs an existence-only check (used by
/// `table.columns[*]`).
fn check_slot(
    schema: &Schema,
    view_id: &str,
    slot: &'static str,
    field_name: &str,
    allowed: &[FieldType],
    expected_label: &'static str,
    out: &mut Vec<Diagnostic>,
) {
    if field_name == "id" {
        return;
    }

    let Some(def) = schema.fields.get(field_name) else {
        out.push(error(DiagnosticKind::ViewUnknownField {
            view_id: view_id.to_owned(),
            slot,
            field_name: field_name.to_owned(),
        }));
        return;
    };

    if allowed.is_empty() {
        return;
    }

    let actual = def.field_type();
    if !allowed.contains(&actual) {
        out.push(error(DiagnosticKind::ViewFieldTypeMismatch {
            view_id: view_id.to_owned(),
            slot,
            field_name: field_name.to_owned(),
            actual_type: actual,
            expected: expected_label.to_owned(),
        }));
    }
}

// ── Graph field helper ───────────────────────────────────────────────

/// Graph-specific slot check: accepts a direct Link/Links field, or an
/// inverse name (declared via `inverse:` on a link/links field and thus
/// present in `schema.inverse_table`). Inverse names resolve to their
/// original field at extraction time; the underlying data is the same.
fn check_graph_field(
    schema: &Schema,
    view_id: &str,
    field_name: &str,
    out: &mut Vec<Diagnostic>,
) {
    if let Some(def) = schema.fields.get(field_name) {
        match def.field_type() {
            FieldType::Link | FieldType::Links => {}
            actual => out.push(error(DiagnosticKind::ViewFieldTypeMismatch {
                view_id: view_id.to_owned(),
                slot: "field",
                field_name: field_name.to_owned(),
                actual_type: actual,
                expected: "link or links".to_owned(),
            })),
        }
        return;
    }

    if schema.inverse_table.contains_key(field_name) {
        return;
    }

    out.push(error(DiagnosticKind::ViewUnknownField {
        view_id: view_id.to_owned(),
        slot: "field",
        field_name: field_name.to_owned(),
    }));
}

// ── Heatmap bucket-coupling helper ───────────────────────────────────

/// Does at least one of the two axis fields resolve to a `date` field in the schema?
fn has_date_axis(schema: &Schema, x: &str, y: &str) -> bool {
    is_date_field(schema.fields.get(x)) || is_date_field(schema.fields.get(y))
}

fn is_date_field(def: Option<&FieldDefinition>) -> bool {
    matches!(def.map(|d| d.field_type()), Some(FieldType::Date))
}

// ── Where-clause checks ──────────────────────────────────────────────

fn check_where_clauses(view: &View, schema: &Schema, out: &mut Vec<Diagnostic>) {
    let view_id = view.id.as_str();
    for raw in &view.where_clauses {
        match parse_where(raw) {
            Ok(predicate) => walk_predicate(&predicate, view_id, schema, out),
            Err(err) => out.push(error(DiagnosticKind::ViewWhereParseError {
                view_id: view_id.to_owned(),
                raw: raw.clone(),
                detail: err.to_string(),
            })),
        }
    }
}

fn walk_predicate(
    predicate: &Predicate,
    view_id: &str,
    schema: &Schema,
    out: &mut Vec<Diagnostic>,
) {
    match predicate {
        Predicate::Comparison(comparison) => {
            check_where_field_ref(&comparison.field, view_id, schema, out)
        }
        Predicate::And(inner) | Predicate::Or(inner) => {
            for p in inner {
                walk_predicate(p, view_id, schema, out);
            }
        }
        Predicate::Not(inner) => walk_predicate(inner, view_id, schema, out),
    }
}

fn check_where_field_ref(
    field_ref: &FieldReference,
    view_id: &str,
    schema: &Schema,
    out: &mut Vec<Diagnostic>,
) {
    match field_ref {
        FieldReference::Local(name) => {
            if name == "id" {
                return;
            }
            if !schema.fields.contains_key(name) {
                out.push(error(DiagnosticKind::ViewUnknownField {
                    view_id: view_id.to_owned(),
                    slot: "where",
                    field_name: name.clone(),
                }));
            }
        }
        FieldReference::Related { relation, .. } => {
            if is_relation_anchor(relation, &schema.fields) {
                return;
            }
            out.push(error(DiagnosticKind::ViewUnknownField {
                view_id: view_id.to_owned(),
                slot: "where",
                field_name: relation.clone(),
            }));
        }
    }
}

// ── Tiny helper: every diagnostic this module emits is an error in v1. ──

fn error(kind: DiagnosticKind) -> Diagnostic {
    Diagnostic {
        severity: Severity::Error,
        kind,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig, Schema};
    use crate::model::views::{Aggregate, Bucket, View, ViewKind, Views};
    use crate::parser::views::parse_views;
    use indexmap::IndexMap;
    use std::path::PathBuf;

    // ── Fixture helpers ────────────────────────────────────────

    /// Build a schema from `(field_name, FieldTypeConfig)` pairs. Link/Links
    /// fields' `inverse` is honored to populate `inverse_table`.
    fn build_schema(fields: Vec<(&str, FieldTypeConfig)>) -> Schema {
        let mut map = IndexMap::new();
        for (name, config) in fields {
            map.insert(name.to_owned(), FieldDefinition::new(config));
        }
        let inverse_table = Schema::build_inverse_table(&map);
        Schema {
            fields: map,
            rules: vec![],
            inverse_table,
        }
    }

    fn simple_schema() -> Schema {
        build_schema(vec![
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into(), "done".into()],
                },
            ),
            ("title", FieldTypeConfig::String { pattern: None }),
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: Some(false),
                    inverse: Some("children".into()),
                },
            ),
            (
                "depends_on",
                FieldTypeConfig::Links {
                    allow_cycles: Some(false),
                    inverse: Some("dependents".into()),
                },
            ),
            ("start_date", FieldTypeConfig::Date),
            ("end_date", FieldTypeConfig::Date),
            (
                "effort",
                FieldTypeConfig::Integer {
                    min: None,
                    max: None,
                },
            ),
            ("assignee", FieldTypeConfig::String { pattern: None }),
        ])
    }

    fn one_view(kind: ViewKind) -> Views {
        Views {
            views: vec![View {
                id: "v".into(),
                where_clauses: vec![],
                title: None,
                kind,
            }],
        }
    }

    fn view_with_where(kind: ViewKind, where_clauses: Vec<String>) -> Views {
        Views {
            views: vec![View {
                id: "v".into(),
                where_clauses,
                title: None,
                kind,
            }],
        }
    }

    fn view_with_title(kind: ViewKind, title: &str) -> Views {
        Views {
            views: vec![View {
                id: "v".into(),
                where_clauses: vec![],
                title: Some(title.into()),
                kind,
            }],
        }
    }

    // ── Reference resolution ───────────────────────────────────

    #[test]
    fn unknown_field_in_board() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Board {
                field: "nonexistent".into(),
            }),
            &simple_schema(),
        );
        assert!(matches!(
            diagnostics.as_slice(),
            [d] if matches!(
                &d.kind,
                DiagnosticKind::ViewUnknownField { slot, field_name, .. }
                if *slot == "field" && field_name == "nonexistent"
            )
        ));
    }

    #[test]
    fn unknown_column_in_table_errors() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Table {
                columns: vec!["status".into(), "nonexistent".into()],
            }),
            &simple_schema(),
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewUnknownField { slot, field_name, .. }
                if *slot == "columns" && field_name == "nonexistent"
        ));
    }

    #[test]
    fn id_accepted_as_table_column_without_schema_entry() {
        // `id` is the virtual always-present field — schema.fields doesn't
        // have to declare it.
        let schema = build_schema(vec![(
            "status",
            FieldTypeConfig::Choice {
                values: vec!["open".into()],
            },
        )]);
        let diagnostics = evaluate(
            &one_view(ViewKind::Table {
                columns: vec!["id".into(), "status".into()],
            }),
            &schema,
        );
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    // ── Type compatibility (one representative per row) ────────

    #[test]
    fn tree_field_must_be_link() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Tree {
                field: "status".into(), // choice, not link
            }),
            &simple_schema(),
        );
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewFieldTypeMismatch { slot, actual_type, .. }
                if *slot == "field" && *actual_type == FieldType::Choice
        ));
    }

    #[test]
    fn graph_field_rejects_non_link_types() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Graph {
                field: "status".into(), // choice, not link/links
            }),
            &simple_schema(),
        );
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewFieldTypeMismatch { actual_type, .. }
                if *actual_type == FieldType::Choice
        ));
    }

    #[test]
    fn graph_field_accepts_single_link() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Graph {
                field: "parent".into(),
            }),
            &simple_schema(),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn graph_field_accepts_inverse_name() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Graph {
                field: "children".into(), // inverse of parent
            }),
            &simple_schema(),
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn graph_field_rejects_unknown_name() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Graph {
                field: "nonexistent".into(),
            }),
            &simple_schema(),
        );
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewUnknownField { field_name, .. }
                if field_name == "nonexistent"
        ));
    }

    #[test]
    fn gantt_start_must_be_date() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Gantt {
                start: "effort".into(), // integer
                end: "end_date".into(),
                group: None,
            }),
            &simple_schema(),
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewFieldTypeMismatch { slot, actual_type, .. }
                if *slot == "start" && *actual_type == FieldType::Integer
        ));
    }

    #[test]
    fn workload_effort_must_be_numeric() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Workload {
                start: "start_date".into(),
                end: "end_date".into(),
                effort: "title".into(), // string
            }),
            &simple_schema(),
        );
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewFieldTypeMismatch { slot, .. } if *slot == "effort"
        ));
    }

    #[test]
    fn bar_chart_value_must_be_numeric_when_present() {
        let diagnostics = evaluate(
            &one_view(ViewKind::BarChart {
                group_by: "status".into(),
                value: Some("title".into()), // string
                aggregate: Aggregate::Sum,
            }),
            &simple_schema(),
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewFieldTypeMismatch { slot, .. } if *slot == "value"
        ));
    }

    #[test]
    fn line_chart_accepts_numeric_and_date() {
        let diagnostics = evaluate(
            &one_view(ViewKind::LineChart {
                x: "effort".into(),
                y: "start_date".into(),
            }),
            &simple_schema(),
        );
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    // ── Heatmap bucket coupling ────────────────────────────────

    #[test]
    fn heatmap_bucket_without_date_axis_errors() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Heatmap {
                x: "status".into(),   // choice
                y: "assignee".into(), // string
                value: None,
                aggregate: Aggregate::Count,
                bucket: Some(Bucket::Week),
            }),
            &simple_schema(),
        );
        assert!(diagnostics
            .iter()
            .any(|d| matches!(&d.kind, DiagnosticKind::ViewBucketWithoutDateAxis { .. })));
    }

    #[test]
    fn heatmap_bucket_with_date_axis_passes() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Heatmap {
                x: "end_date".into(),
                y: "assignee".into(),
                value: None,
                aggregate: Aggregate::Count,
                bucket: Some(Bucket::Week),
            }),
            &simple_schema(),
        );
        assert!(
            !diagnostics
                .iter()
                .any(|d| matches!(&d.kind, DiagnosticKind::ViewBucketWithoutDateAxis { .. })),
            "got: {diagnostics:?}"
        );
    }

    // ── Metric: count-with-value ───────────────────────────────

    #[test]
    fn metric_count_with_value_errors() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Metric {
                label: None,
                value: Some("effort".into()),
                aggregate: Aggregate::Count,
            }),
            &simple_schema(),
        );
        assert!(diagnostics
            .iter()
            .any(|d| matches!(&d.kind, DiagnosticKind::ViewCountAggregateWithValue { .. })));
    }

    #[test]
    fn metric_count_with_unknown_value_emits_both_diagnostics() {
        // Existence check runs regardless of the count-with-value error —
        // they're orthogonal problems.
        let diagnostics = evaluate(
            &one_view(ViewKind::Metric {
                label: None,
                value: Some("nonexistent".into()),
                aggregate: Aggregate::Count,
            }),
            &simple_schema(),
        );
        assert!(diagnostics
            .iter()
            .any(|d| matches!(&d.kind, DiagnosticKind::ViewUnknownField { .. })));
        assert!(diagnostics
            .iter()
            .any(|d| matches!(&d.kind, DiagnosticKind::ViewCountAggregateWithValue { .. })));
    }

    #[test]
    fn metric_sum_with_value_passes() {
        let diagnostics = evaluate(
            &one_view(ViewKind::Metric {
                label: None,
                value: Some("effort".into()),
                aggregate: Aggregate::Sum,
            }),
            &simple_schema(),
        );
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    // ── Where-clause checks ────────────────────────────────────

    #[test]
    fn where_parse_error() {
        let diagnostics = evaluate(
            &view_with_where(
                ViewKind::Board {
                    field: "status".into(),
                },
                vec!["justtext".into()],
            ),
            &simple_schema(),
        );
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewWhereParseError { raw, .. } if raw == "justtext"
        ));
    }

    #[test]
    fn where_unknown_local_field() {
        let diagnostics = evaluate(
            &view_with_where(
                ViewKind::Board {
                    field: "status".into(),
                },
                vec!["typo_field=x".into()],
            ),
            &simple_schema(),
        );
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewUnknownField { slot, field_name, .. }
                if *slot == "where" && field_name == "typo_field"
        ));
    }

    #[test]
    fn where_forward_relation_accepted() {
        let diagnostics = evaluate(
            &view_with_where(
                ViewKind::Board {
                    field: "status".into(),
                },
                vec!["parent.status=open".into()],
            ),
            &simple_schema(),
        );
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn where_inverse_relation_accepted() {
        let diagnostics = evaluate(
            &view_with_where(
                ViewKind::Board {
                    field: "status".into(),
                },
                vec!["children.status=done".into()],
            ),
            &simple_schema(),
        );
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn where_unknown_relation_emits_diagnostic() {
        let diagnostics = evaluate(
            &view_with_where(
                ViewKind::Board {
                    field: "status".into(),
                },
                vec!["typo.status=open".into()],
            ),
            &simple_schema(),
        );
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewUnknownField { slot, field_name, .. }
                if *slot == "where" && field_name == "typo"
        ));
    }

    #[test]
    fn where_string_field_not_valid_as_relation() {
        // `assignee` is a string — can't be traversed.
        let diagnostics = evaluate(
            &view_with_where(
                ViewKind::Board {
                    field: "status".into(),
                },
                vec!["assignee.status=open".into()],
            ),
            &simple_schema(),
        );
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::ViewUnknownField { field_name, .. }
                if field_name == "assignee"
        ));
    }

    // ── Title slot (cross-cutting) ─────────────────────────────

    #[test]
    fn title_string_field_accepted() {
        let diagnostics = evaluate(
            &view_with_title(
                ViewKind::Board {
                    field: "status".into(),
                },
                "title",
            ),
            &simple_schema(),
        );
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn title_choice_field_accepted() {
        let diagnostics = evaluate(
            &view_with_title(
                ViewKind::Board {
                    field: "status".into(),
                },
                "status",
            ),
            &simple_schema(),
        );
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn title_id_accepted_though_redundant() {
        // `id` is the fallback when title is unset — setting it explicitly
        // is harmless and must not trip existence / type checks.
        let diagnostics = evaluate(
            &view_with_title(
                ViewKind::Board {
                    field: "status".into(),
                },
                "id",
            ),
            &simple_schema(),
        );
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn title_unknown_field_rejected() {
        let diagnostics = evaluate(
            &view_with_title(
                ViewKind::Board {
                    field: "status".into(),
                },
                "nonexistent",
            ),
            &simple_schema(),
        );
        assert!(matches!(
            diagnostics.as_slice(),
            [d] if matches!(
                &d.kind,
                DiagnosticKind::ViewUnknownField { slot, field_name, .. }
                if *slot == "title" && field_name == "nonexistent"
            )
        ));
    }

    #[test]
    fn title_wrong_type_rejected() {
        // `effort` is integer — not allowed as a display title.
        let diagnostics = evaluate(
            &view_with_title(
                ViewKind::Board {
                    field: "status".into(),
                },
                "effort",
            ),
            &simple_schema(),
        );
        assert!(matches!(
            diagnostics.as_slice(),
            [d] if matches!(
                &d.kind,
                DiagnosticKind::ViewFieldTypeMismatch { slot, field_name, actual_type, .. }
                if *slot == "title" && field_name == "effort" && *actual_type == FieldType::Integer
            )
        ));
    }

    #[test]
    fn title_link_field_rejected() {
        // Relation fields can resolve to multiple values — not a title.
        let diagnostics = evaluate(
            &view_with_title(
                ViewKind::Board {
                    field: "status".into(),
                },
                "parent",
            ),
            &simple_schema(),
        );
        assert!(matches!(
            diagnostics.as_slice(),
            [d] if matches!(
                &d.kind,
                DiagnosticKind::ViewFieldTypeMismatch { slot, actual_type, .. }
                if *slot == "title" && *actual_type == FieldType::Link
            )
        ));
    }

    // ── parse_errors_to_diagnostics ────────────────────────────

    fn view_path() -> PathBuf {
        PathBuf::from(".workdown/views.yaml")
    }

    #[test]
    fn parse_invalid_yaml_becomes_file_error() {
        // Unknown slot — serde's `deny_unknown_fields` triggers InvalidYaml.
        let yaml = "views:\n  - id: c\n    type: board\n    field: status\n    color: red\n";
        let err = parse_views(yaml).unwrap_err();
        let diagnostics = parse_errors_to_diagnostics(err, &view_path());
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::FileError { path, .. } if path == &view_path()
        ));
    }

    #[test]
    fn parse_read_failed_becomes_file_error() {
        let err = ViewsLoadError::ReadFailed(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no such file",
        ));
        let diagnostics = parse_errors_to_diagnostics(err, &view_path());
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            &diagnostics[0].kind,
            DiagnosticKind::FileError { .. }
        ));
    }

    #[test]
    fn parse_duplicate_id_becomes_view_duplicate_id() {
        let yaml = "views:\n  - id: a\n    type: board\n    field: status\n  - id: a\n    type: tree\n    field: parent\n";
        let err = parse_views(yaml).unwrap_err();
        let diagnostics = parse_errors_to_diagnostics(err, &view_path());
        assert!(matches!(
            diagnostics.as_slice(),
            [d] if matches!(&d.kind, DiagnosticKind::ViewDuplicateId { view_id } if view_id == "a")
        ));
    }

    #[test]
    fn parse_missing_slot_becomes_view_missing_slot() {
        let yaml = "views:\n  - id: b\n    type: board\n";
        let err = parse_views(yaml).unwrap_err();
        let diagnostics = parse_errors_to_diagnostics(err, &view_path());
        assert!(matches!(
            diagnostics.as_slice(),
            [d] if matches!(
                &d.kind,
                DiagnosticKind::ViewMissingSlot { view_id, slot, .. }
                if view_id == "b" && *slot == "field"
            )
        ));
    }

    #[test]
    fn parse_multiple_validation_errors_produce_multiple_diagnostics() {
        let yaml = "views:\n  - id: x\n    type: gantt\n    start: start_date\n  - id: y\n    type: bar_chart\n    group_by: status\n";
        let err = parse_views(yaml).unwrap_err();
        let diagnostics = parse_errors_to_diagnostics(err, &view_path());
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics
            .iter()
            .all(|d| matches!(&d.kind, DiagnosticKind::ViewMissingSlot { .. })));
    }
}
