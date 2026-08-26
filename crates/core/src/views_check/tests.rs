//! Tests for [`super`] — the `views.yaml` cross-file validation pass.

use super::*;
use crate::model::diagnostic::DiagnosticBody;
use crate::model::schema::{FieldDefinition, FieldTypeConfig, Schema};
use crate::model::views::{
    Aggregate, Bucket, ColorRole, DisplayConfig, MetricRow, View, ViewKind, Views,
};
use crate::parser::views::parse_views;
use indexmap::IndexMap;
use std::path::{Path, PathBuf};

/// Standard `views.yaml` path used across tests.
fn test_views_path() -> &'static Path {
    Path::new("views.yaml")
}

/// Run [`evaluate`] against an empty project: no `resources.yaml`, no
/// work items. Most cases here check slot and field-reference rules,
/// which read only the schema; the operand checks in
/// [`crate::where_check`] have nothing to match against and stay
/// quiet, except where a clause names an item id — those tests use
/// [`check_views_with_items`] instead.
fn check_views(views: &Views, schema: &Schema, views_path: &Path) -> Vec<Diagnostic> {
    check_views_with_items(views, schema, views_path, &[])
}

/// As [`check_views`], with a store built from `item_ids` so that
/// clauses referencing items have something to resolve against.
fn check_views_with_items(
    views: &Views,
    schema: &Schema,
    views_path: &Path,
    item_ids: &[&str],
) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("tempdir");
    for id in item_ids {
        std::fs::write(dir.path().join(format!("{id}.md")), "---\n---\n").expect("write item");
    }
    let store = crate::store::Store::load(dir.path(), schema).expect("load store");
    evaluate(
        views,
        schema,
        &crate::model::resources::Resources::default(),
        &store,
        views_path,
    )
}

/// Extract the inner `ConfigDiagnosticKind` from a Config-scope diagnostic,
/// panicking otherwise. All view diagnostics are Config-scope.
fn view_kind(diagnostic: &Diagnostic) -> &ConfigDiagnosticKind {
    match &diagnostic.body {
        DiagnosticBody::Config(c) => &c.kind,
        other => panic!("expected Config body, got {other:?}"),
    }
}

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
        (
            "estimate",
            FieldTypeConfig::Duration {
                min: None,
                max: None,
            },
        ),
        ("assignee", FieldTypeConfig::String { pattern: None }),
    ])
}

fn one_view(kind: ViewKind) -> Views {
    Views {
        output_dir: PathBuf::from("views"),
        views: vec![View {
            id: "v".into(),
            where_clauses: vec![],
            display: DisplayConfig::default(),
            kind,
        }],
    }
}

fn view_with_where(kind: ViewKind, where_clauses: Vec<String>) -> Views {
    Views {
        output_dir: PathBuf::from("views"),
        views: vec![View {
            id: "v".into(),
            where_clauses,
            display: DisplayConfig::default(),
            kind,
        }],
    }
}

fn view_with_display(kind: ViewKind, display: DisplayConfig) -> Views {
    Views {
        output_dir: PathBuf::from("views"),
        views: vec![View {
            id: "v".into(),
            where_clauses: vec![],
            display,
            kind,
        }],
    }
}

fn view_with_title(kind: ViewKind, title: &str) -> Views {
    view_with_display(
        kind,
        DisplayConfig {
            title: Some(title.into()),
            ..DisplayConfig::default()
        },
    )
}

// ── Reference resolution ───────────────────────────────────

#[test]
fn unknown_field_in_board() {
    let diagnostics = check_views(
        &one_view(ViewKind::Board {
            field: "nonexistent".into(),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        diagnostics.as_slice(),
        [d] if matches!(
            view_kind(d),
            ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "field" && field_name == "nonexistent"
        )
    ));
}

#[test]
fn unknown_display_field_in_table_errors() {
    let diagnostics = check_views(
        &view_with_display(
            ViewKind::Table,
            DisplayConfig {
                fields: Some(vec!["status".into(), "nonexistent".into()]),
                ..DisplayConfig::default()
            },
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "display.fields" && field_name == "nonexistent"
    ));
}

#[test]
fn id_accepted_as_display_field_without_schema_entry() {
    // `id` is the virtual always-present field — schema.fields doesn't
    // have to declare it.
    let schema = build_schema(vec![(
        "status",
        FieldTypeConfig::Choice {
            values: vec!["open".into()],
        },
    )]);
    let diagnostics = check_views(
        &view_with_display(
            ViewKind::Table,
            DisplayConfig {
                fields: Some(vec!["id".into(), "status".into()]),
                ..DisplayConfig::default()
            },
        ),
        &schema,
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn id_rejected_in_board_field() {
    // `field: id` would put every item in a column of its own — a
    // unique key groups nothing.
    let diagnostics = check_views(
        &one_view(ViewKind::Board { field: "id".into() }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewVirtualIdNotAllowed { location, .. } if location.slot == "field"
    ));
}

#[test]
fn id_rejected_in_existence_only_slot() {
    // Heatmap axes pass an empty `allowed` list (any type goes) —
    // the virtual-id rejection must fire before that shortcut.
    let diagnostics = check_views(
        &one_view(ViewKind::Heatmap {
            x: "id".into(),
            y: "status".into(),
            value: None,
            aggregate: Aggregate::Count,
            bucket: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewVirtualIdNotAllowed { location, .. } if location.slot == "x"
    ));
}

#[test]
fn id_rejected_in_link_slot() {
    // Link-walk slots used to report `id` as an unknown field —
    // misleading for a field that exists; the dedicated rejection
    // names the real problem (a unique key groups nothing).
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "depends_on".into(),
            group_by: Some("id".into()),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewVirtualIdNotAllowed { location, .. } if location.slot == "group_by"
    ));
}

#[test]
fn id_rejected_in_graph_field() {
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "id".into(),
            group_by: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewVirtualIdNotAllowed { location, .. } if location.slot == "field"
    ));
}

#[test]
fn id_rejected_in_aggregate_value_slot() {
    let diagnostics = check_views(
        &one_view(ViewKind::BarChart {
            group_by: "status".into(),
            value: Some("id".into()),
            aggregate: Aggregate::Sum,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewVirtualIdNotAllowed { location, .. } if location.slot == "value"
    ));
}

#[test]
fn id_rejected_in_metric_row_value() {
    let diagnostics = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![MetricRow {
                label: None,
                aggregate: Aggregate::Sum,
                value: Some("id".into()),
                where_clauses: vec![],
            }],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewVirtualIdNotAllowed { location }
            if location.metric_index == Some(0) && location.slot == "value"
    ));
}

#[test]
fn id_accepted_in_where_clause() {
    // Filtering by id is legitimate — provided the id exists. The
    // virtual `id` has no schema entry but the tightest option set
    // there is, so the operand is checked against the item set.
    let diagnostics = check_views_with_items(
        &view_with_where(ViewKind::Table, vec!["id=some-item".into()]),
        &simple_schema(),
        test_views_path(),
        &["some-item"],
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn id_where_clause_naming_a_missing_item_warns() {
    let diagnostics = check_views_with_items(
        &view_with_where(ViewKind::Table, vec!["id=no-such-item".into()]),
        &simple_schema(),
        test_views_path(),
        &["some-item"],
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewWhereUnknownValue { field_name, detail, .. }
            if field_name == "id" && detail.contains("existing work item id")
    ));
}

// ── Where-clause operands ──────────────────────────────────
//
// The rules live in `where_check` and are tested there; these cover
// the wrapping — severity, which diagnostic kind, and that a metric
// row's parallel path gets the same treatment.

#[test]
fn where_clause_with_unknown_choice_value_warns() {
    let diagnostics = check_views(
        &view_with_where(ViewKind::Table, vec!["status=nonsense".into()]),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    // A warning, not an error: the view still renders. `render` and
    // the server both filter on severity for exactly this case.
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewWhereUnknownValue { raw, field_name, detail, .. }
            if raw == "status=nonsense"
                && field_name == "status"
                && detail.contains("open")
    ));
}

/// The regression that made this check urgent: a filter written when
/// `type=a,b` still meant membership now compares against the literal
/// string, matching nothing.
#[test]
fn stale_implicit_membership_filter_warns_with_a_hint() {
    let diagnostics = check_views(
        &view_with_where(ViewKind::Table, vec!["status=open,done".into()]),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewWhereUnknownValue { detail, .. }
            if detail.contains("did you mean 'status in open,done'?")
    ));
}

#[test]
fn matches_clause_produces_no_value_warning() {
    let diagnostics = check_views(
        &view_with_where(ViewKind::Table, vec!["status/^nonsense$/".into()]),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

/// An operand judged against a field that doesn't exist would stack a
/// second complaint on one cause — the unknown field is the finding.
#[test]
fn unknown_field_in_where_reports_only_the_field() {
    let diagnostics = check_views(
        &view_with_where(ViewKind::Table, vec!["nonexistent=whatever".into()]),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { location, .. } if location.slot == "where"
    ));
}

#[test]
fn metric_row_where_operand_is_checked_too() {
    let diagnostics = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![MetricRow {
                label: None,
                aggregate: Aggregate::Count,
                value: None,
                where_clauses: vec!["status=nonsense".into()],
            }],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewWhereUnknownValue { location, field_name, .. }
            if location.metric_index == Some(0) && field_name == "status"
    ));
}

#[test]
fn unknown_subtitle_field_errors() {
    let diagnostics = check_views(
        &view_with_display(
            ViewKind::Table,
            DisplayConfig {
                subtitle: Some("nonexistent".into()),
                ..DisplayConfig::default()
            },
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "display.subtitle" && field_name == "nonexistent"
    ));
}

#[test]
fn color_role_accepts_color_typed_field() {
    let schema = build_schema(vec![
        ("team_color", FieldTypeConfig::Color),
        ("risk_color", FieldTypeConfig::Color),
    ]);
    let diagnostics = check_views(
        &view_with_display(
            ViewKind::Table,
            DisplayConfig {
                color: Some(ColorRole::Field("risk_color".into())),
                ..DisplayConfig::default()
            },
        ),
        &schema,
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn color_role_none_sentinel_is_always_valid() {
    // `color: none` disables tinting; it references no field, so
    // there is nothing to check — even in a schema with no color
    // fields at all.
    let diagnostics = check_views(
        &view_with_display(
            ViewKind::Table,
            DisplayConfig {
                color: Some(ColorRole::None),
                ..DisplayConfig::default()
            },
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn unknown_color_role_field_errors() {
    let diagnostics = check_views(
        &view_with_display(
            ViewKind::Table,
            DisplayConfig {
                color: Some(ColorRole::Field("nonexistent".into())),
                ..DisplayConfig::default()
            },
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "display.color" && field_name == "nonexistent"
    ));
}

#[test]
fn color_role_field_must_be_color_typed() {
    let diagnostics = check_views(
        &view_with_display(
            ViewKind::Table,
            DisplayConfig {
                color: Some(ColorRole::Field("status".into())),
                ..DisplayConfig::default()
            },
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, field_name, expected, .. }
            if location.slot == "display.color" && field_name == "status" && expected == "color"
    ));
}

#[test]
fn id_rejected_as_color_role() {
    // The virtual `id` is accepted by every text role, but it can
    // never feed a tint — silently accepting it would just be a
    // dead config.
    let diagnostics = check_views(
        &view_with_display(
            ViewKind::Table,
            DisplayConfig {
                color: Some(ColorRole::Field("id".into())),
                ..DisplayConfig::default()
            },
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, field_name, expected, .. }
            if location.slot == "display.color" && field_name == "id" && expected == "color"
    ));
}

// ── Type compatibility (one representative per row) ────────

#[test]
fn tree_field_must_be_link() {
    let diagnostics = check_views(
        &one_view(ViewKind::Tree {
            field: "status".into(), // choice, not link
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, actual_type, .. }
            if location.slot == "field" && *actual_type == FieldType::Choice
    ));
}

#[test]
fn unknown_display_field_in_tree_errors() {
    let diagnostics = check_views(
        &view_with_display(
            ViewKind::Tree {
                field: "parent".into(),
            },
            DisplayConfig {
                fields: Some(vec!["status".into(), "nonexistent".into()]),
                ..DisplayConfig::default()
            },
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "display.fields" && field_name == "nonexistent"
    ));
}

#[test]
fn graph_field_rejects_non_link_types() {
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "status".into(), // choice, not link/links
            group_by: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { actual_type, .. }
            if *actual_type == FieldType::Choice
    ));
}

#[test]
fn graph_field_accepts_single_link() {
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "parent".into(),
            group_by: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn graph_field_accepts_inverse_name() {
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "children".into(), // inverse of parent
            group_by: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty());
}

#[test]
fn graph_field_rejects_unknown_name() {
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "nonexistent".into(),
            group_by: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { field_name, .. }
            if field_name == "nonexistent"
    ));
}

// ── Graph group_by ─────────────────────────────────────────

#[test]
fn graph_group_by_accepts_link_with_cycles_disabled() {
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "depends_on".into(),
            group_by: Some("parent".into()),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn graph_group_by_rejects_links_field() {
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "parent".into(),
            group_by: Some("depends_on".into()), // links, not link
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, actual_type, .. }
            if location.slot == "group_by" && *actual_type == FieldType::Links
    ));
}

#[test]
fn graph_group_by_rejects_unknown_field() {
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "depends_on".into(),
            group_by: Some("nonexistent".into()),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "group_by" && field_name == "nonexistent"
    ));
}

#[test]
fn graph_group_by_rejects_inverse_name() {
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "depends_on".into(),
            group_by: Some("children".into()), // inverse of parent
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewSlotInverseNotAllowed { location, field_name, .. }
            if location.slot == "group_by"
                && field_name == "children"
    ));
}

#[test]
fn graph_group_by_rejects_link_with_cycles_allowed() {
    let schema = build_schema(vec![
        (
            "depends_on",
            FieldTypeConfig::Links {
                allow_cycles: Some(false),
                inverse: None,
            },
        ),
        (
            "topic",
            FieldTypeConfig::Link {
                allow_cycles: Some(true),
                inverse: None,
            },
        ),
    ]);
    let diagnostics = check_views(
        &one_view(ViewKind::Graph {
            field: "depends_on".into(),
            group_by: Some("topic".into()),
        }),
        &schema,
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewSlotCyclic { location, field_name, .. }
            if location.slot == "group_by"
                && field_name == "topic"
    ));
}

#[test]
fn gantt_start_must_be_date() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "effort".into(), // integer
            end: Some("end_date".into()),
            duration: None,
            after: None,
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, actual_type, .. }
            if location.slot == "start" && *actual_type == FieldType::Integer
    ));
}

#[test]
fn gantt_group_accepts_choice_string_link_and_links() {
    for field in ["status", "title", "parent", "depends_on"] {
        let diagnostics = check_views(
            &one_view(ViewKind::Gantt {
                start: "start_date".into(),
                end: Some("end_date".into()),
                duration: None,
                after: None,
                group: Some(field.into()),
            }),
            &simple_schema(),
            test_views_path(),
        );
        assert!(
            diagnostics.is_empty(),
            "field '{field}' should be accepted as gantt group, got: {diagnostics:?}"
        );
    }
}

#[test]
fn gantt_group_rejects_non_value_field_types() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            group: Some("effort".into()), // integer
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, actual_type, .. }
            if location.slot == "group" && *actual_type == FieldType::Integer
    ));
}

#[test]
fn gantt_neither_end_nor_duration_errors() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: None,
            after: None,
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewGanttEndOrDurationRequired { .. }
    ));
}

#[test]
fn gantt_both_end_and_duration_errors() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: Some("estimate".into()),
            after: None,
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewGanttEndAndDurationConflict { .. }
    ));
}

#[test]
fn gantt_duration_must_be_duration_field() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("end_date".into()), // date, not duration
            after: None,
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, actual_type, .. }
            if location.slot == "duration" && *actual_type == FieldType::Date
    ));
}

#[test]
fn gantt_duration_with_correct_type_passes() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("estimate".into()),
            after: None,
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got {diagnostics:?}"
    );
}

// ── Gantt after-mode (predecessor) ─────────────────────────

#[test]
fn gantt_after_with_duration_passes() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("estimate".into()),
            after: Some("depends_on".into()),
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got {diagnostics:?}"
    );
}

#[test]
fn gantt_after_accepts_single_link() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("estimate".into()),
            after: Some("parent".into()),
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(
        diagnostics.is_empty(),
        "expected no diagnostics, got {diagnostics:?}"
    );
}

#[test]
fn gantt_after_without_duration_errors() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: None,
            after: Some("depends_on".into()),
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewGanttAfterRequiresDuration { .. }
    )));
}

#[test]
fn gantt_after_with_end_errors() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: Some("estimate".into()),
            after: Some("depends_on".into()),
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewGanttAfterWithEndConflict { .. }
    )));
}

#[test]
fn gantt_after_must_be_link_or_links() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("estimate".into()),
            after: Some("status".into()), // choice, not link
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, expected, .. }
            if location.slot == "after" && expected == "link or links"
    ));
}

#[test]
fn gantt_after_rejects_unknown_field() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("estimate".into()),
            after: Some("nonexistent".into()),
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "after" && field_name == "nonexistent"
    ));
}

#[test]
fn gantt_after_rejects_inverse_name() {
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("estimate".into()),
            after: Some("dependents".into()), // inverse of depends_on
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewSlotInverseNotAllowed { location, field_name, .. }
            if location.slot == "after"
                && field_name == "dependents"
    ));
}

#[test]
fn gantt_after_rejects_link_with_cycles_allowed() {
    let schema = build_schema(vec![
        ("start_date", FieldTypeConfig::Date),
        (
            "estimate",
            FieldTypeConfig::Duration {
                min: None,
                max: None,
            },
        ),
        (
            "blocks",
            FieldTypeConfig::Links {
                allow_cycles: Some(true),
                inverse: None,
            },
        ),
    ]);
    let diagnostics = check_views(
        &one_view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("estimate".into()),
            after: Some("blocks".into()),
            group: None,
        }),
        &schema,
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewSlotCyclic { location, field_name, .. }
            if location.slot == "after"
                && field_name == "blocks"
    ));
}

// ── gantt_by_initiative root_link ──────────────────────────────

#[test]
fn gantt_by_initiative_accepts_link_with_cycles_disabled() {
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByInitiative {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            root_link: "parent".into(),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got {diagnostics:?}");
}

#[test]
fn gantt_by_initiative_root_link_rejects_unknown_field() {
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByInitiative {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            root_link: "nonexistent".into(),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "root_link" && field_name == "nonexistent"
    )));
}

#[test]
fn gantt_by_initiative_root_link_rejects_links_field() {
    // Links is rejected — initiative partition requires single-target.
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByInitiative {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            root_link: "depends_on".into(), // Links, not Link
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, expected, .. }
            if location.slot == "root_link" && expected == "link"
    )));
}

#[test]
fn gantt_by_initiative_root_link_rejects_inverse_name() {
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByInitiative {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            root_link: "children".into(), // inverse of parent
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewSlotInverseNotAllowed { location, field_name, .. }
            if location.slot == "root_link"
                && field_name == "children"
    )));
}

#[test]
fn gantt_by_initiative_root_link_rejects_link_with_cycles_allowed() {
    let schema = build_schema(vec![
        ("start_date", FieldTypeConfig::Date),
        ("end_date", FieldTypeConfig::Date),
        (
            "topic",
            FieldTypeConfig::Link {
                allow_cycles: Some(true),
                inverse: None,
            },
        ),
    ]);
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByInitiative {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            root_link: "topic".into(),
        }),
        &schema,
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewSlotCyclic { location, field_name, .. }
            if location.slot == "root_link"
                && field_name == "topic"
    )));
}

#[test]
fn gantt_by_initiative_input_mode_rules_mirror_basic_gantt() {
    // Both end and duration set → conflict (same as basic gantt).
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByInitiative {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: Some("estimate".into()),
            after: None,
            root_link: "parent".into(),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewGanttEndAndDurationConflict { .. }
    )));
}

// ── gantt_by_depth depth_link ──────────────────────────────────

#[test]
fn gantt_by_depth_accepts_link_with_cycles_disabled() {
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByDepth {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            depth_link: "parent".into(),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got {diagnostics:?}");
}

#[test]
fn gantt_by_depth_depth_link_rejects_unknown_field() {
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByDepth {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            depth_link: "nonexistent".into(),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "depth_link" && field_name == "nonexistent"
    )));
}

#[test]
fn gantt_by_depth_depth_link_rejects_links_field() {
    // Links is rejected — depth requires single-target.
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByDepth {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            depth_link: "depends_on".into(), // Links, not Link
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, expected, .. }
            if location.slot == "depth_link" && expected == "link"
    )));
}

#[test]
fn gantt_by_depth_depth_link_rejects_inverse_name() {
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByDepth {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            depth_link: "children".into(), // inverse of parent
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewSlotInverseNotAllowed { location, field_name, .. }
            if location.slot == "depth_link"
                && field_name == "children"
    )));
}

#[test]
fn gantt_by_depth_depth_link_rejects_link_with_cycles_allowed() {
    let schema = build_schema(vec![
        ("start_date", FieldTypeConfig::Date),
        ("end_date", FieldTypeConfig::Date),
        (
            "topic",
            FieldTypeConfig::Link {
                allow_cycles: Some(true),
                inverse: None,
            },
        ),
    ]);
    let diagnostics = check_views(
        &one_view(ViewKind::GanttByDepth {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            depth_link: "topic".into(),
        }),
        &schema,
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewSlotCyclic { location, field_name, .. }
            if location.slot == "depth_link"
                && field_name == "topic"
    )));
}

#[test]
fn workload_effort_must_be_numeric_or_duration() {
    let diagnostics = check_views(
        &one_view(ViewKind::Workload {
            start: "start_date".into(),
            end: "end_date".into(),
            effort: "title".into(), // string
            working_days: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, .. } if location.slot == "effort"
    ));
}

#[test]
fn workload_effort_accepts_duration() {
    let diagnostics = check_views(
        &one_view(ViewKind::Workload {
            start: "start_date".into(),
            end: "end_date".into(),
            effort: "estimate".into(), // duration
            working_days: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn bar_chart_sum_rejects_non_numeric_value() {
    let diagnostics = check_views(
        &one_view(ViewKind::BarChart {
            group_by: "status".into(),
            value: Some("title".into()), // string
            aggregate: Aggregate::Sum,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1);
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewAggregateTypeMismatch { location, aggregate, actual_type, .. }
            if location.slot == "value" && *aggregate == Aggregate::Sum && *actual_type == FieldType::String
    ));
}

#[test]
fn bar_chart_sum_rejects_date_value() {
    let diagnostics = check_views(
        &one_view(ViewKind::BarChart {
            group_by: "status".into(),
            value: Some("end_date".into()),
            aggregate: Aggregate::Sum,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewAggregateTypeMismatch { aggregate, actual_type, .. }
            if *aggregate == Aggregate::Sum && *actual_type == FieldType::Date
    ));
}

#[test]
fn bar_chart_avg_accepts_date_value() {
    let diagnostics = check_views(
        &one_view(ViewKind::BarChart {
            group_by: "status".into(),
            value: Some("end_date".into()),
            aggregate: Aggregate::Avg,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn bar_chart_group_by_accepts_any_field_type() {
    let diagnostics = check_views(
        &one_view(ViewKind::BarChart {
            group_by: "effort".into(), // integer — now allowed
            value: None,
            aggregate: Aggregate::Count,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn metric_avg_accepts_date_value() {
    let diagnostics = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![MetricRow {
                label: None,
                aggregate: Aggregate::Avg,
                value: Some("end_date".into()),
                where_clauses: vec![],
            }],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn heatmap_axis_accepts_any_field_type() {
    let diagnostics = check_views(
        &one_view(ViewKind::Heatmap {
            x: "effort".into(), // integer — now allowed
            y: "title".into(),  // string — still allowed
            value: None,
            aggregate: Aggregate::Count,
            bucket: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn line_chart_accepts_date_x_numeric_y() {
    let diagnostics = check_views(
        &one_view(ViewKind::LineChart {
            x: "start_date".into(),
            y: "effort".into(),
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn line_chart_rejects_date_y() {
    let diagnostics = check_views(
        &one_view(ViewKind::LineChart {
            x: "effort".into(),
            y: "start_date".into(),
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, actual_type, .. }
            if location.slot == "y" && *actual_type == FieldType::Date
    ));
}

// ── Heatmap bucket coupling ────────────────────────────────

#[test]
fn heatmap_bucket_without_date_axis_errors() {
    let diagnostics = check_views(
        &one_view(ViewKind::Heatmap {
            x: "status".into(),   // choice
            y: "assignee".into(), // string
            value: None,
            aggregate: Aggregate::Count,
            bucket: Some(Bucket::Week),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewBucketWithoutDateAxis { .. }
    )));
}

#[test]
fn heatmap_bucket_with_date_axis_passes() {
    let diagnostics = check_views(
        &one_view(ViewKind::Heatmap {
            x: "end_date".into(),
            y: "assignee".into(),
            value: None,
            aggregate: Aggregate::Count,
            bucket: Some(Bucket::Week),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(
        !diagnostics.iter().any(|d| matches!(
            view_kind(d),
            ConfigDiagnosticKind::ViewBucketWithoutDateAxis { .. }
        )),
        "got: {diagnostics:?}"
    );
}

// ── Treemap group must be a link ───────────────────────────

#[test]
fn treemap_group_rejects_non_link() {
    let diagnostics = check_views(
        &one_view(ViewKind::Treemap {
            group: "status".into(), // choice, not link
            size: "effort".into(),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, actual_type, .. }
            if location.slot == "group" && *actual_type == FieldType::Choice
    ));
}

#[test]
fn treemap_group_accepts_link() {
    let diagnostics = check_views(
        &one_view(ViewKind::Treemap {
            group: "parent".into(),
            size: "effort".into(),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn treemap_size_accepts_duration() {
    let diagnostics = check_views(
        &one_view(ViewKind::Treemap {
            group: "parent".into(),
            size: "estimate".into(),
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn line_chart_y_accepts_duration() {
    let diagnostics = check_views(
        &one_view(ViewKind::LineChart {
            x: "start_date".into(),
            y: "estimate".into(),
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn line_chart_x_accepts_duration() {
    let diagnostics = check_views(
        &one_view(ViewKind::LineChart {
            x: "estimate".into(),
            y: "effort".into(),
            group: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn line_chart_group_accepts_choice_string_link_and_links() {
    for field in ["status", "title", "parent", "depends_on"] {
        let diagnostics = check_views(
            &one_view(ViewKind::LineChart {
                x: "estimate".into(),
                y: "effort".into(),
                group: Some(field.into()),
            }),
            &simple_schema(),
            test_views_path(),
        );
        assert!(
            diagnostics.is_empty(),
            "field '{field}' should be accepted as line chart group, got: {diagnostics:?}"
        );
    }
}

#[test]
fn line_chart_group_rejects_non_value_field_types() {
    let diagnostics = check_views(
        &one_view(ViewKind::LineChart {
            x: "estimate".into(),
            y: "effort".into(),
            group: Some("effort".into()), // integer
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewFieldTypeMismatch { location, actual_type, .. }
            if location.slot == "group" && *actual_type == FieldType::Integer
    ));
}

// ── Metric: count-with-value ───────────────────────────────

#[test]
fn metric_count_with_value_errors() {
    let diagnostics = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![MetricRow {
                label: None,
                aggregate: Aggregate::Count,
                value: Some("effort".into()),
                where_clauses: vec![],
            }],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewCountAggregateWithValue { location }
            if location.metric_index == Some(0) && location.slot == "value"
    )));
}

#[test]
fn metric_count_with_unknown_value_reports_only_the_count_conflict() {
    // The slot shouldn't be there at all, so its contents aren't judged:
    // reporting a bad field name in a slot we're asking the author to
    // delete would be noise on top of the real verdict.
    let diagnostics = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![MetricRow {
                label: None,
                aggregate: Aggregate::Count,
                value: Some("nonexistent".into()),
                where_clauses: vec![],
            }],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewCountAggregateWithValue { location }
            if location.metric_index == Some(0) && location.slot == "value"
    ));
}

#[test]
fn metric_sum_with_value_passes() {
    let diagnostics = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![MetricRow {
                label: None,
                aggregate: Aggregate::Sum,
                value: Some("effort".into()),
                where_clauses: vec![],
            }],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn metric_per_row_where_parse_error_pinpoints_index() {
    let diagnostics = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![
                MetricRow {
                    label: None,
                    aggregate: Aggregate::Count,
                    value: None,
                    where_clauses: vec![],
                },
                MetricRow {
                    label: None,
                    aggregate: Aggregate::Count,
                    value: None,
                    where_clauses: vec!["justtext".into()],
                },
            ],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewWhereParseError { location, raw, .. }
            if location.metric_index == Some(1) && raw == "justtext"
    )));
}

#[test]
fn metric_per_row_where_unknown_field_pinpoints_index() {
    let diagnostics = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![MetricRow {
                label: None,
                aggregate: Aggregate::Count,
                value: None,
                where_clauses: vec!["typo_field=x".into()],
            }],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.iter().any(|d| matches!(
        view_kind(d),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.metric_index == Some(0)
                && location.slot == "where"
                && field_name == "typo_field"
    )));
}

// ── Where-clause checks ────────────────────────────────────

#[test]
fn where_parse_error() {
    let diagnostics = check_views(
        &view_with_where(
            ViewKind::Board {
                field: "status".into(),
            },
            vec!["justtext".into()],
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewWhereParseError { raw, .. } if raw == "justtext"
    ));
}

#[test]
fn where_unknown_local_field() {
    let diagnostics = check_views(
        &view_with_where(
            ViewKind::Board {
                field: "status".into(),
            },
            vec!["typo_field=x".into()],
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "where" && field_name == "typo_field"
    ));
}

#[test]
fn where_forward_relation_accepted() {
    let diagnostics = check_views(
        &view_with_where(
            ViewKind::Board {
                field: "status".into(),
            },
            vec!["parent.status=open".into()],
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn where_inverse_relation_accepted() {
    let diagnostics = check_views(
        &view_with_where(
            ViewKind::Board {
                field: "status".into(),
            },
            vec!["children.status=done".into()],
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn where_unknown_relation_emits_diagnostic() {
    let diagnostics = check_views(
        &view_with_where(
            ViewKind::Board {
                field: "status".into(),
            },
            vec!["typo.status=open".into()],
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "where" && field_name == "typo"
    ));
}

#[test]
fn where_string_field_not_valid_as_relation() {
    // `assignee` is a string — can't be traversed.
    let diagnostics = check_views(
        &view_with_where(
            ViewKind::Board {
                field: "status".into(),
            },
            vec!["assignee.status=open".into()],
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewUnknownField { field_name, .. }
            if field_name == "assignee"
    ));
}

// ── Title slot (cross-cutting) ─────────────────────────────

#[test]
fn title_string_field_accepted() {
    let diagnostics = check_views(
        &view_with_title(
            ViewKind::Board {
                field: "status".into(),
            },
            "title",
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn title_choice_field_accepted() {
    let diagnostics = check_views(
        &view_with_title(
            ViewKind::Board {
                field: "status".into(),
            },
            "status",
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn title_id_accepted_though_redundant() {
    // `id` is the fallback when title is unset — setting it explicitly
    // is harmless and must not trip existence / type checks.
    let diagnostics = check_views(
        &view_with_title(
            ViewKind::Board {
                field: "status".into(),
            },
            "id",
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

#[test]
fn title_unknown_field_rejected() {
    let diagnostics = check_views(
        &view_with_title(
            ViewKind::Board {
                field: "status".into(),
            },
            "nonexistent",
        ),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        diagnostics.as_slice(),
        [d] if matches!(
            view_kind(d),
            ConfigDiagnosticKind::ViewUnknownField { location, field_name, .. }
            if location.slot == "display.title" && field_name == "nonexistent"
        )
    ));
}

#[test]
fn title_accepts_any_field_type() {
    // Display roles are existence-only: every field value renders as
    // text, so an integer or link title is legal (if unusual).
    for title_field in ["effort", "parent"] {
        let diagnostics = check_views(
            &view_with_title(
                ViewKind::Board {
                    field: "status".into(),
                },
                title_field,
            ),
            &simple_schema(),
            test_views_path(),
        );
        assert!(
            diagnostics.is_empty(),
            "title `{title_field}` should be accepted, got: {diagnostics:?}"
        );
    }
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
        &diagnostics[0].body,
        DiagnosticBody::File(file)
            if file.source_path == view_path()
                && matches!(file.kind, FileDiagnosticKind::ReadError { .. })
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
        &diagnostics[0].body,
        DiagnosticBody::File(file)
            if matches!(file.kind, FileDiagnosticKind::ReadError { .. })
    ));
}

#[test]
fn parse_duplicate_id_becomes_view_duplicate_id() {
    let yaml = "views:\n  - id: a\n    type: board\n    field: status\n  - id: a\n    type: tree\n    field: parent\n";
    let err = parse_views(yaml).unwrap_err();
    let diagnostics = parse_errors_to_diagnostics(err, &view_path());
    assert!(matches!(
        diagnostics.as_slice(),
        [d] if matches!(view_kind(d), ConfigDiagnosticKind::ViewDuplicateId { view_id } if view_id == "a")
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
            view_kind(d),
            ConfigDiagnosticKind::ViewMissingSlot { view_id, slot, .. }
            if view_id == "b" && *slot == "field"
        )
    ));
}

#[test]
fn parse_multiple_validation_errors_produce_multiple_diagnostics() {
    // tree missing `field`, bar_chart missing `aggregate` — both
    // produce parse-stage MissingSlot diagnostics that stack.
    let yaml =
        "views:\n  - id: x\n    type: tree\n  - id: y\n    type: bar_chart\n    group_by: status\n";
    let err = parse_views(yaml).unwrap_err();
    let diagnostics = parse_errors_to_diagnostics(err, &view_path());
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics
        .iter()
        .all(|d| matches!(view_kind(d), ConfigDiagnosticKind::ViewMissingSlot { .. })));
}

#[test]
fn bar_chart_count_with_value_is_rejected() {
    // `count` counts items, so a `value` field is meaningless wherever
    // the aggregate lives — the same verdict a metric row gets, from the
    // same check.
    let diagnostics = check_views(
        &one_view(ViewKind::BarChart {
            group_by: "status".into(),
            value: Some("effort".into()),
            aggregate: Aggregate::Count,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewCountAggregateWithValue { location }
            if location.metric_index.is_none() && location.slot == "value"
    ));
}

#[test]
fn heatmap_count_with_value_is_rejected() {
    let diagnostics = check_views(
        &one_view(ViewKind::Heatmap {
            x: "status".into(),
            y: "title".into(),
            value: Some("effort".into()),
            aggregate: Aggregate::Count,
            bucket: None,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(matches!(
        view_kind(&diagnostics[0]),
        ConfigDiagnosticKind::ViewCountAggregateWithValue { location }
            if location.metric_index.is_none() && location.slot == "value"
    ));
}

#[test]
fn bar_chart_count_without_value_is_fine() {
    let diagnostics = check_views(
        &one_view(ViewKind::BarChart {
            group_by: "status".into(),
            value: None,
            aggregate: Aggregate::Count,
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
}

/// One check, two loci: the same misconfiguration in a view slot and in
/// a metric row differs only in the location the diagnostic carries.
#[test]
fn the_same_rule_reports_both_loci() {
    let view_level = check_views(
        &one_view(ViewKind::BarChart {
            group_by: "status".into(),
            value: Some("title".into()),
            aggregate: Aggregate::Sum,
        }),
        &simple_schema(),
        test_views_path(),
    );
    let row_level = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![MetricRow {
                label: None,
                aggregate: Aggregate::Sum,
                value: Some("title".into()),
                where_clauses: vec![],
            }],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(matches!(
        view_kind(&view_level[0]),
        ConfigDiagnosticKind::ViewAggregateTypeMismatch { location, .. }
            if location.metric_index.is_none() && location.slot == "value"
    ));
    assert!(matches!(
        view_kind(&row_level[0]),
        ConfigDiagnosticKind::ViewAggregateTypeMismatch { location, .. }
            if location.metric_index == Some(0) && location.slot == "value"
    ));
}

/// The location renders as a path into the YAML, so a row-level message
/// says which row.
#[test]
fn metric_row_message_names_the_row() {
    let diagnostics = check_views(
        &one_view(ViewKind::Metric {
            metrics: vec![MetricRow {
                label: None,
                aggregate: Aggregate::Sum,
                value: Some("nonexistent".into()),
                where_clauses: vec![],
            }],
        }),
        &simple_schema(),
        test_views_path(),
    );
    assert!(
        diagnostics[0]
            .to_string()
            .contains("slot 'metrics[0].value'"),
        "got: {}",
        diagnostics[0]
    );
}
