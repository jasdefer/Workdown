//! Drift guard for `crates/core/defaults/views.schema.json`.
//!
//! ADR-005 keeps the JSON Schema editor-only — the CLI never loads it. That
//! means the schema and the Rust parser (`crates/core/src/parser/views.rs`)
//! are two independent representations of the same shape. This test compiles
//! the schema and runs it against the default `views.yaml`, an example
//! carrying every view kind, and a battery of bad shapes to confirm the
//! schema agrees with the parser on what is and is not legal.
//!
//! Shapes are only half of it. The kind-list section at the bottom checks
//! that the *set* of kinds the schema accepts is the set the `ViewType`
//! enum has — drift a shape test cannot see, since a kind missing from the
//! schema is a kind no fixture exercises.
//!
//! The schema is intentionally stricter than the parser in a few places —
//! view `id` must match a kebab-style pattern (parser accepts any string)
//! and per-type slots are exclusive (parser silently ignores wrong slots).
//! These tests cover the overlap; the asymmetric gap is intentional and
//! not exercised here.

mod json_schema;

use std::collections::BTreeSet;

use strum::VariantArray;

use json_schema::SchemaGuard;
use workdown_core::model::views::ViewType;

const SCHEMA_JSON: &str = include_str!("../defaults/views.schema.json");
const DEFAULT_VIEWS_YAML: &str = include_str!("../defaults/views.yaml");

/// The compiled schema under test.
fn guard() -> SchemaGuard {
    SchemaGuard::compile("views.schema.json", SCHEMA_JSON)
}

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
    display:
      fields: [id, title, type, status, start_date, end_date]
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
  - id: roadmap-by-depth
    type: gantt_by_depth
    start: start_date
    end: end_date
    depth_link: parent
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
    metrics:
      - aggregate: count
        label: Open items
    where: ["status in to_do,in_progress"]
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

// ── Positive cases ───────────────────────────────────────────────────────

#[test]
fn default_views_yaml_validates() {
    let schema = guard();
    schema.assert_valid(DEFAULT_VIEWS_YAML);
}

#[test]
fn full_example_with_all_view_types_validates() {
    let schema = guard();
    schema.assert_valid(FULL_EXAMPLE_YAML);
}

#[test]
fn empty_views_list_validates() {
    let schema = guard();
    schema.assert_valid("views: []\n");
}

// ── Negative cases ───────────────────────────────────────────────────────

#[test]
fn board_without_field_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: missing-field
    type: board
",
    );
}

#[test]
fn metric_without_metrics_slot_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: missing-metrics
    type: metric
",
    );
}

#[test]
fn metric_row_without_aggregate_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: bad-row
    type: metric
    metrics:
      - label: oops
",
    );
}

#[test]
fn unknown_slot_on_view_rejected() {
    let schema = guard();
    schema.assert_invalid(
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
    // `size` is valid for `treemap` but not for `board`. The Rust parser
    // silently ignores it; the schema must catch it for editor warnings.
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: board-with-size
    type: board
    field: status
    size: effort
",
    );
}

#[test]
fn unknown_view_type_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: bogus-type
    type: pie_chart
",
    );
}

#[test]
fn missing_id_slot_rejected() {
    let schema = guard();
    schema.assert_invalid(
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
    let schema = guard();
    schema.assert_invalid(
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
    // no value field. Mirrors the cross-file validator's check, which
    // applies the rule at every aggregate locus from one function; the
    // three tests here are the schema side of that same rule.
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: bad-count
    type: metric
    metrics:
      - aggregate: count
        value: effort
",
    );
}

#[test]
fn bar_chart_count_with_value_rejected() {
    // Same rule, view-level locus. `check_aggregate_value_slot` rejects
    // this in Rust; the schema has to agree or the editor stays silent
    // about a config the CLI refuses.
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: bad-count-bar
    type: bar_chart
    group_by: status
    aggregate: count
    value: effort
",
    );
}

#[test]
fn bar_chart_count_without_value_validates() {
    let schema = guard();
    schema.assert_valid(
        "\
views:
  - id: items-by-status
    type: bar_chart
    group_by: status
    aggregate: count
",
    );
}

#[test]
fn heatmap_count_with_value_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: bad-count-heatmap
    type: heatmap
    x: status
    y: assignee
    aggregate: count
    value: effort
",
    );
}

#[test]
fn bar_chart_sum_with_value_validates() {
    let schema = guard();
    schema.assert_valid(
        "\
views:
  - id: effort-by-status
    type: bar_chart
    group_by: status
    aggregate: sum
    value: effort
",
    );
}

#[test]
fn metric_sum_with_value_validates() {
    let schema = guard();
    schema.assert_valid(
        "\
views:
  - id: total-effort
    type: metric
    metrics:
      - aggregate: sum
        value: effort
",
    );
}

#[test]
fn metric_empty_metrics_array_validates() {
    let schema = guard();
    schema.assert_valid(
        "\
views:
  - id: empty
    type: metric
    metrics: []
",
    );
}

#[test]
fn metric_multiple_rows_validates() {
    let schema = guard();
    schema.assert_valid(
        "\
views:
  - id: stats
    type: metric
    metrics:
      - label: Total
        aggregate: count
      - label: In progress
        aggregate: count
        where: [\"status=in_progress\"]
      - label: Story points
        aggregate: sum
        value: points
",
    );
}

#[test]
fn invalid_id_format_rejected() {
    let schema = guard();
    schema.assert_invalid(
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
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: bad-aggregate
    type: metric
    metrics:
      - aggregate: median
",
    );
}

#[test]
fn legacy_top_level_columns_rejected() {
    // `columns:` migrated into the `fields` display role — the old
    // top-level key must be rejected so editors flag stale files.
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: legacy-cols
    type: table
    columns: [id, status]
",
    );
}

#[test]
fn bare_table_validates() {
    // A table needs no slots: columns come from the `fields` display
    // role, config defaults, or the all-schema-fields fallback.
    let schema = guard();
    schema.assert_valid("views:\n  - id: bare\n    type: table\n");
}

#[test]
fn unknown_top_level_key_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
views: []
extra: nope
",
    );
}

// ── Display block (cross-cutting) ────────────────────────────────────────

#[test]
fn display_block_on_every_view_type_validates() {
    // Every per-type branch must accept the shared display block. The
    // fixture is derived from `FULL_EXAMPLE_YAML` rather than written out
    // again: a second hand-maintained copy of the kind list is exactly the
    // drift this file exists to catch.
    let mut document = json_schema::yaml_to_json(FULL_EXAMPLE_YAML);
    let display = serde_json::json!({
        "title": "title",
        "subtitle": "status",
        "fields": ["type", "effort"],
        "color": "team_color",
    });
    for view in document["views"]
        .as_array_mut()
        .expect("the fixture's `views` key holds a list")
    {
        view["display"] = display.clone();
    }
    let yaml = serde_yaml::to_string(&document).expect("derived fixture must serialize");
    guard().assert_valid(&yaml);
}

#[test]
fn display_color_none_sentinel_validates() {
    // `none` is the no-tint sentinel — a reserved word rather than a field
    // name, so it needs its own positive case alongside the pattern.
    guard().assert_valid(
        "\
views:
  - id: untinted
    type: board
    field: status
    display: { color: none }
",
    );
}

#[test]
fn display_color_rejects_non_field_shapes() {
    // The color role takes a field name or the sentinel `none` — a
    // value that is neither (here: an uppercase non-field-name string)
    // must fail the pattern for editor warnings.
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: bad-color-role
    type: board
    field: status
    display: { color: Not-A-Field }
",
    );
}

#[test]
fn legacy_top_level_title_rejected() {
    // `title:` migrated into the display block — the old top-level key
    // must be rejected so editors flag stale files.
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: legacy-title
    type: board
    field: status
    title: title
",
    );
}

#[test]
fn display_title_with_wrong_yaml_type_rejected() {
    // `display.title` must be a field-name string. A number is not a
    // valid identifier.
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: bad-title
    type: board
    field: status
    display:
      title: 42
",
    );
}

#[test]
fn unknown_display_role_rejected() {
    // Roles outside the closed vocabulary (title, subtitle, fields,
    // color) are rejected.
    let schema = guard();
    schema.assert_invalid(
        "\
views:
  - id: bad-role
    type: board
    field: status
    display:
      badge: severity
",
    );
}

// ── Kind-list drift ──────────────────────────────────────────────────────
//
// The tests above all check *shapes*. These two check the *list of kinds*:
// a fourteenth `ViewType` variant that never reached the JSON schema — or
// reached the schema but not the all-kinds fixture — passes every shape
// test in this file and ships silently. See the adding-a-view-kind
// checklist in `docs/architecture.md`.

/// Every view kind, under the `type:` name it is written as in
/// `views.yaml`. `Display` is that name — a unit test in `model::views`
/// pins it to what serde parses.
fn every_view_type() -> BTreeSet<String> {
    ViewType::VARIANTS.iter().map(ToString::to_string).collect()
}

/// The `type:` values in a `views.yaml` document.
fn view_types_in(yaml: &str) -> BTreeSet<String> {
    json_schema::yaml_to_json(yaml)["views"]
        .as_array()
        .expect("a `views` list")
        .iter()
        .map(|view| {
            view["type"]
                .as_str()
                .expect("every view names its type")
                .to_owned()
        })
        .collect()
}

#[test]
fn schema_accepts_exactly_the_enum_view_types() {
    // The schema discriminates on `type` through one `$defs` branch per
    // kind, listed in `view.oneOf`. Those branches are the set of kinds an
    // editor will accept; it must be the set the CLI has.
    let document: serde_json::Value =
        serde_json::from_str(SCHEMA_JSON).expect("views.schema.json must be valid JSON");
    let definitions = &document["$defs"];
    let in_schema: BTreeSet<String> = definitions["view"]["oneOf"]
        .as_array()
        .expect("`view` is a oneOf over the per-kind branches")
        .iter()
        .map(|branch| {
            let reference = branch["$ref"]
                .as_str()
                .expect("each `view.oneOf` branch is a $ref to a per-kind definition");
            let name = reference
                .rsplit('/')
                .next()
                .expect("a $ref names a definition");
            definitions[name]["properties"]["type"]["const"]
                .as_str()
                .unwrap_or_else(|| panic!("`{name}` must pin its `type` with a const"))
                .to_owned()
        })
        .collect();

    assert_eq!(
        in_schema,
        every_view_type(),
        "views.schema.json and the ViewType enum disagree about which view kinds exist — \
         add the missing kind's `$defs` branch and list it in `view.oneOf`"
    );
}

#[test]
fn full_example_covers_every_view_type() {
    // `FULL_EXAMPLE_YAML` is what the shape tests above run every kind
    // through, so a kind missing from it is a kind nothing checks.
    assert_eq!(
        view_types_in(FULL_EXAMPLE_YAML),
        every_view_type(),
        "FULL_EXAMPLE_YAML is missing a view kind — every kind must appear once"
    );
}
