//! Drift guard for `crates/core/defaults/views.schema.json`.
//!
//! ADR-005 keeps the JSON Schema editor-only — the CLI never loads it. That
//! means the schema and the Rust parser (`crates/core/src/parser/views.rs`)
//! are two independent representations of the same shape. This test compiles
//! the schema and runs it against the default `views.yaml`, the full 11-view
//! example from `docs/views.md`, and a battery of bad shapes to confirm the
//! schema agrees with the parser on what is and is not legal.
//!
//! The schema is intentionally stricter than the parser in a few places —
//! view `id` must match a kebab-style pattern (parser accepts any string),
//! per-type slots are exclusive (parser silently ignores wrong slots), and
//! `columns: []` is rejected. These tests cover the overlap; the asymmetric
//! gap is intentional and not exercised here.

use jsonschema::{Draft, JSONSchema};

const SCHEMA_JSON: &str = include_str!("../defaults/views.schema.json");
const DEFAULT_VIEWS_YAML: &str = include_str!("../defaults/views.yaml");

const FULL_EXAMPLE_YAML: &str = r#"
views:
  - id: status-board
    type: board
    field: status
    where:
      - "type=issue"
      - "status!=removed"
  - id: hierarchy
    type: tree
    field: parent
  - id: deps
    type: graph
    field: depends_on
  - id: all-items
    type: table
    columns: [id, title, type, status, start_date, end_date]
  - id: roadmap
    type: gantt
    start: start_date
    end: end_date
    group: parent
  - id: roadmap-by-initiative
    type: gantt_by_initiative
    start: start_date
    end: end_date
    root_link: parent
  - id: effort-by-status
    type: bar_chart
    group_by: status
    value: effort
    aggregate: sum
  - id: estimate-vs-actual
    type: line_chart
    x: estimate
    y: actual_effort
  - id: capacity
    type: workload
    start: start_date
    end: end_date
    effort: effort
  - id: open-count
    type: metric
    aggregate: count
    label: Open items
    where: ["status=to_do,in_progress"]
  - id: effort-by-milestone
    type: treemap
    group: parent
    size: effort
  - id: activity
    type: heatmap
    x: end_date
    y: assignee
    aggregate: count
    bucket: week
"#;

// ── Helpers ──────────────────────────────────────────────────────────────

fn compile_schema() -> JSONSchema {
    let schema_value: serde_json::Value =
        serde_json::from_str(SCHEMA_JSON).expect("views.schema.json must be valid JSON");
    JSONSchema::options()
        .with_draft(Draft::Draft202012)
        .compile(&schema_value)
        .expect("views.schema.json must be a valid JSON Schema")
}

fn yaml_to_json(yaml: &str) -> serde_json::Value {
    serde_yaml::from_str(yaml).expect("YAML fixture must parse")
}

fn assert_valid(schema: &JSONSchema, yaml: &str) {
    let value = yaml_to_json(yaml);
    let messages: Vec<String> = match schema.validate(&value) {
        Ok(()) => return,
        Err(errors) => errors
            .map(|error| format!("  at {}: {}", error.instance_path, error))
            .collect(),
    };
    panic!(
        "expected YAML to validate against views.schema.json, but got errors:\n{}\nYAML:\n{}",
        messages.join("\n"),
        yaml
    );
}

fn assert_invalid(schema: &JSONSchema, yaml: &str) {
    let value = yaml_to_json(yaml);
    assert!(
        schema.validate(&value).is_err(),
        "expected YAML to be rejected by views.schema.json, but it validated:\n{yaml}"
    );
}

// ── Positive cases ───────────────────────────────────────────────────────

#[test]
fn default_views_yaml_validates() {
    let schema = compile_schema();
    assert_valid(&schema, DEFAULT_VIEWS_YAML);
}

#[test]
fn full_example_with_all_view_types_validates() {
    let schema = compile_schema();
    assert_valid(&schema, FULL_EXAMPLE_YAML);
}

#[test]
fn empty_views_list_validates() {
    let schema = compile_schema();
    assert_valid(&schema, "views: []\n");
}

// ── Negative cases ───────────────────────────────────────────────────────

#[test]
fn board_without_field_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: missing-field
    type: board
",
    );
}

#[test]
fn metric_without_aggregate_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: missing-aggregate
    type: metric
    label: oops
",
    );
}

#[test]
fn unknown_slot_on_view_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: garbage-slot
    type: board
    field: status
    color: red
",
    );
}

#[test]
fn known_slot_on_wrong_view_type_rejected() {
    // `columns` is valid for `table` but not for `board`. The Rust parser
    // silently ignores it; the schema must catch it for editor warnings.
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: board-with-columns
    type: board
    field: status
    columns: [id, title]
",
    );
}

#[test]
fn unknown_view_type_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: bogus-type
    type: pie_chart
",
    );
}

#[test]
fn missing_id_slot_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - type: board
    field: status
",
    );
}

#[test]
fn wrong_yaml_type_for_slot_rejected() {
    // `field` must be a string. Numbers, lists, etc. are rejected.
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: typed-wrong
    type: board
    field: 123
",
    );
}

#[test]
fn metric_count_with_value_rejected() {
    // `aggregate: count` combined with `value:` is forbidden — count takes
    // no value field. Mirrors the cross-file validator's check.
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: bad-count
    type: metric
    aggregate: count
    value: effort
",
    );
}

#[test]
fn metric_sum_with_value_validates() {
    let schema = compile_schema();
    assert_valid(
        &schema,
        "\
views:
  - id: total-effort
    type: metric
    aggregate: sum
    value: effort
",
    );
}

#[test]
fn invalid_id_format_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: Has Spaces!
    type: board
    field: status
",
    );
}

#[test]
fn bad_aggregate_value_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: bad-aggregate
    type: metric
    aggregate: median
",
    );
}

#[test]
fn empty_table_columns_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: empty-cols
    type: table
    columns: []
",
    );
}

#[test]
fn unknown_top_level_key_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views: []
extra: nope
",
    );
}

// ── Title slot (cross-cutting) ───────────────────────────────────────────

#[test]
fn title_slot_on_every_view_type_validates() {
    // Same fixture as `full_example_with_all_view_types_validates` but
    // every entry carries `title: title`. Ensures each per-type branch
    // accepts the shared slot.
    let schema = compile_schema();
    let yaml = r#"
views:
  - id: status-board
    type: board
    field: status
    title: title
  - id: hierarchy
    type: tree
    field: parent
    title: title
  - id: deps
    type: graph
    field: depends_on
    title: title
  - id: all-items
    type: table
    columns: [id, title]
    title: title
  - id: roadmap
    type: gantt
    start: start_date
    end: end_date
    title: title
  - id: roadmap-by-initiative
    type: gantt_by_initiative
    start: start_date
    end: end_date
    root_link: parent
    title: title
  - id: effort-by-status
    type: bar_chart
    group_by: status
    aggregate: count
    title: title
  - id: estimate-vs-actual
    type: line_chart
    x: estimate
    y: actual_effort
    title: title
  - id: capacity
    type: workload
    start: start_date
    end: end_date
    effort: effort
    title: title
  - id: open-count
    type: metric
    aggregate: count
    title: title
  - id: effort-by-milestone
    type: treemap
    group: parent
    size: effort
    title: title
  - id: activity
    type: heatmap
    x: end_date
    y: assignee
    aggregate: count
    title: title
"#;
    assert_valid(&schema, yaml);
}

#[test]
fn title_with_wrong_yaml_type_rejected() {
    // `title` must be a field-name string. A number is not a valid identifier.
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
views:
  - id: bad-title
    type: board
    field: status
    title: 42
",
    );
}
