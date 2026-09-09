//! Integration tests for the view-write operations (`add_view`,
//! `create_view`, `set_view_filter`, `update_view`, `delete_view`).
//!
//! Each test builds a throwaway project, mutates `views.yaml` through the
//! public operations, and asserts on the outcome and the files on disk.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use workdown_core::model::config::Config;
use workdown_core::model::schema::Severity;
use workdown_core::model::views::Views;
use workdown_core::operations::view_write::{
    add_view, create_view, delete_view, set_view_filter, update_view, ViewWriteError,
};
use workdown_core::parser::config::load_config;
use workdown_core::parser::views::load_views;
use workdown_core::query::clause::{Clause, Condition};
use workdown_core::query::types::Operator;

/// A raw passthrough clause — used where a test only cares that the
/// clause string lands in the file, not how it was built.
fn raw(clause: &str) -> Clause {
    Clause::Raw {
        raw: clause.to_owned(),
    }
}

fn condition(field: &str, operator: Operator, value: Option<&str>) -> Clause {
    Clause::Comparison(Condition {
        field: field.to_owned(),
        operator,
        value: value.map(str::to_owned),
        values: Vec::new(),
    })
}

fn membership(field: &str, operator: Operator, values: &[&str]) -> Clause {
    Clause::Comparison(Condition {
        field: field.to_owned(),
        operator,
        value: None,
        values: values.iter().map(|value| (*value).to_owned()).collect(),
    })
}

const CONFIG: &str = "\
project:
  name: Test Project
  description: ''
paths:
  work_items: workdown-items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
";

const SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
  status:
    type: choice
    values: [open, in_progress, done]
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
";

fn setup() -> (TempDir, PathBuf, Config) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown")).unwrap();
    // A view write reads the work items to check filter operands
    // against the ids they may name, so the fixture scaffolds the
    // directory `workdown init` would have created. Items are added
    // per-test by `write_item` where a clause needs one.
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();
    let config = load_config(&root.join(".workdown/config.yaml")).unwrap();
    (directory, root, config)
}

fn write_item(root: &Path, id: &str) {
    fs::write(root.join(format!("workdown-items/{id}.md")), "---\n---\n").unwrap();
}

fn write_views(root: &Path, content: &str) {
    fs::write(root.join(".workdown/views.yaml"), content).unwrap();
}

fn read_views(root: &Path) -> String {
    fs::read_to_string(root.join(".workdown/views.yaml")).unwrap()
}

fn board(id: &str) -> serde_yaml::Value {
    serde_yaml::from_str(&format!("id: {id}\ntype: board\nfield: status\n")).unwrap()
}

// ── add_view ─────────────────────────────────────────────────────

#[test]
fn add_view_creates_file_when_absent() {
    let (_dir, root, config) = setup();
    assert!(!root.join(".workdown/views.yaml").exists());

    let outcome = add_view(&config, &root, board("status-board")).unwrap();

    assert_eq!(outcome.view_id, "status-board");
    assert!(!outcome.mutation_caused_warning);
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(reloaded.views.len(), 1);
    assert_eq!(reloaded.views[0].id, "status-board");
}

#[test]
fn add_view_appends_to_existing() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: first\n    type: board\n    field: status\n",
    );

    add_view(&config, &root, board("second")).unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    let ids: Vec<&str> = reloaded.views.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, vec!["first", "second"]);
}

#[test]
fn add_view_duplicate_id_errors_without_writing() {
    let (_dir, root, config) = setup();
    let original = "views:\n  - id: dup\n    type: board\n    field: status\n";
    write_views(&root, original);

    let error = add_view(&config, &root, board("dup")).unwrap_err();

    assert!(matches!(error, ViewWriteError::DuplicateId { id } if id == "dup"));
    assert_eq!(read_views(&root), original, "file must be untouched");
}

#[test]
fn add_view_missing_required_slot_errors_without_writing() {
    let (_dir, root, config) = setup();
    let definition: serde_yaml::Value = serde_yaml::from_str("id: b\ntype: board\n").unwrap();

    let error = add_view(&config, &root, definition).unwrap_err();

    assert!(matches!(error, ViewWriteError::InvalidDefinition { .. }));
    assert!(!root.join(".workdown/views.yaml").exists());
}

#[test]
fn add_view_unknown_slot_errors_without_writing() {
    let (_dir, root, config) = setup();
    let definition: serde_yaml::Value =
        serde_yaml::from_str("id: b\ntype: board\nfield: status\nbogus: x\n").unwrap();

    let error = add_view(&config, &root, definition).unwrap_err();

    assert!(matches!(error, ViewWriteError::InvalidDefinition { .. }));
    assert!(!root.join(".workdown/views.yaml").exists());
}

#[test]
fn add_view_with_bad_field_reference_writes_with_warning() {
    let (_dir, root, config) = setup();
    // `field: nope` parses fine but fails cross-file validation —
    // save-with-warning: the view is written, the problem is surfaced.
    let definition: serde_yaml::Value =
        serde_yaml::from_str("id: b\ntype: board\nfield: nope\n").unwrap();

    let outcome = add_view(&config, &root, definition).unwrap();

    assert!(outcome.mutation_caused_warning);
    assert!(!outcome.warnings.is_empty());
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(reloaded.views[0].id, "b");
}

#[test]
fn add_view_over_invalid_existing_file_errors() {
    let (_dir, root, config) = setup();
    write_views(&root, "views:\n  - id: x\n    type: not_a_real_kind\n");

    let error = add_view(&config, &root, board("new")).unwrap_err();

    assert!(matches!(error, ViewWriteError::ExistingInvalid { .. }));
}

// ── create_view (name → slug) ────────────────────────────────────

#[test]
fn create_view_slugs_name_to_id() {
    let (_dir, root, config) = setup();
    let definition: serde_yaml::Value =
        serde_yaml::from_str("type: board\nfield: status\n").unwrap();

    let outcome = create_view(&config, &root, "My Status Board", definition, &[]).unwrap();

    assert_eq!(outcome.view_id, "my-status-board");
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(reloaded.views[0].id, "my-status-board");
}

#[test]
fn create_view_injects_the_filter_clauses() {
    let (_dir, root, config) = setup();
    let definition: serde_yaml::Value =
        serde_yaml::from_str("type: board\nfield: status\n").unwrap();

    create_view(
        &config,
        &root,
        "Open Board",
        definition,
        &[raw("status=open")],
    )
    .unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(reloaded.views[0].where_clauses, vec!["status=open"]);
}

#[test]
fn create_view_overwrites_supplied_id_with_the_slug() {
    let (_dir, root, config) = setup();
    // A stray `id` in the definition is ignored — the name is authoritative.
    let definition: serde_yaml::Value =
        serde_yaml::from_str("id: ignored\ntype: board\nfield: status\n").unwrap();

    let outcome = create_view(&config, &root, "Real Name", definition, &[]).unwrap();

    assert_eq!(outcome.view_id, "real-name");
}

#[test]
fn create_view_blank_name_errors_without_writing() {
    let (_dir, root, config) = setup();
    let definition: serde_yaml::Value =
        serde_yaml::from_str("type: board\nfield: status\n").unwrap();

    let error = create_view(&config, &root, "   ", definition, &[]).unwrap_err();

    assert!(matches!(error, ViewWriteError::InvalidName { .. }));
    assert!(!root.join(".workdown/views.yaml").exists());
}

// ── set_view_filter ──────────────────────────────────────────────

#[test]
fn set_view_filter_updates_where() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    // Structured conditions, to exercise the serializer end to end.
    let outcome = set_view_filter(
        &config,
        &root,
        "board",
        &[
            condition("status", Operator::Equal, Some("open")),
            condition("title", Operator::Contains, Some("fix")),
        ],
    )
    .unwrap();

    assert_eq!(outcome.view_id, "board");
    assert!(!outcome.mutation_caused_warning);
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(
        reloaded.views[0].where_clauses,
        vec!["status=open", "title~fix"]
    );
}

/// A membership condition reaches `views.yaml` as `in` / `not in`, with the
/// comma-join happening in the serializer and nowhere else.
#[test]
fn set_view_filter_writes_membership_clauses() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    let outcome = set_view_filter(
        &config,
        &root,
        "board",
        &[
            membership("status", Operator::In, &["open", "in_progress"]),
            membership("status", Operator::NotIn, &["done"]),
        ],
    )
    .unwrap();

    assert!(!outcome.mutation_caused_warning);
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(
        reloaded.views[0].where_clauses,
        vec!["status in open,in_progress", "status not in done"]
    );
}

/// An operand that doesn't match its operator's arity fails the write
/// outright — the guided builder cannot produce one, so it is a malformed
/// request rather than a filter to save with a warning.
#[test]
fn set_view_filter_rejects_operand_arity_mismatch_without_writing() {
    let (_dir, root, config) = setup();
    let source = "views:\n  - id: board\n    type: board\n    field: status\n";
    write_views(&root, source);

    let error = set_view_filter(
        &config,
        &root,
        "board",
        &[condition("status", Operator::In, Some("open"))],
    )
    .unwrap_err();

    assert!(matches!(error, ViewWriteError::InvalidCondition(_)));
    assert_eq!(
        std::fs::read_to_string(root.join(".workdown/views.yaml")).unwrap(),
        source
    );
}

#[test]
fn set_view_filter_replaces_previous_where() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=done\"\n",
    );

    set_view_filter(&config, &root, "board", &[raw("status=open")]).unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(reloaded.views[0].where_clauses, vec!["status=open"]);
}

#[test]
fn set_view_filter_empty_clears_where() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=done\"\n",
    );

    set_view_filter(&config, &root, "board", &[]).unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert!(reloaded.views[0].where_clauses.is_empty());
    assert!(
        !read_views(&root).contains("where:"),
        "empty where should not be emitted"
    );
}

#[test]
fn set_view_filter_unknown_view_errors_without_writing() {
    let (_dir, root, config) = setup();
    let original = "views:\n  - id: board\n    type: board\n    field: status\n";
    write_views(&root, original);

    let error = set_view_filter(&config, &root, "nope", &[raw("status=open")]).unwrap_err();

    assert!(matches!(error, ViewWriteError::ViewNotFound { id } if id == "nope"));
    assert_eq!(read_views(&root), original, "file must be untouched");
}

#[test]
fn set_view_filter_with_unknown_field_writes_with_warning() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    // References a field not in the schema: parses, but fails cross-file
    // validation. Save-with-warning — written and surfaced.
    let outcome = set_view_filter(&config, &root, "board", &[raw("nonexistent=x")]).unwrap();

    assert!(outcome.mutation_caused_warning);
    assert!(!outcome.warnings.is_empty());
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(reloaded.views[0].where_clauses, vec!["nonexistent=x"]);
}

/// The write path checks operands, not just field names: the value
/// is written and the problem comes back as a warning, so a filter
/// that can never match is caught as it is authored rather than at
/// the next `validate`.
#[test]
fn set_view_filter_with_unknown_value_writes_with_warning() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    let outcome = set_view_filter(&config, &root, "board", &[raw("status=nonsense")]).unwrap();

    assert!(outcome.mutation_caused_warning);
    assert_eq!(outcome.warnings.len(), 1, "{:?}", outcome.warnings);
    assert_eq!(outcome.warnings[0].severity, Severity::Warning);
    assert!(
        outcome.warnings[0].message.contains("nonsense"),
        "{}",
        outcome.warnings[0].message
    );
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(reloaded.views[0].where_clauses, vec!["status=nonsense"]);
}

/// Item ids are the option set that only the store can supply, which
/// is why this path loads it. The same clause is clean or not
/// depending on whether the item exists.
#[test]
fn set_view_filter_checks_item_ids_against_the_store() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    let outcome = set_view_filter(&config, &root, "board", &[raw("parent=epic-1")]).unwrap();
    assert!(outcome.mutation_caused_warning, "no such item yet");

    write_item(&root, "epic-1");
    let outcome = set_view_filter(&config, &root, "board", &[raw("parent=epic-1")]).unwrap();
    assert!(!outcome.mutation_caused_warning);
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
}

// ── update_view ──────────────────────────────────────────────────

/// Two views, so replacement can be checked to stay in place.
const TWO_VIEWS: &str = "\
views:
  - id: first
    type: board
    field: status
  - id: second
    type: tree
    field: parent
";

fn definition(yaml: &str) -> serde_yaml::Value {
    serde_yaml::from_str(yaml).unwrap()
}

#[test]
fn update_view_replaces_definition_in_place() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    // Switch `first` from a board to a tree — a full kind change.
    let outcome = update_view(
        &config,
        &root,
        "first",
        None,
        definition("type: tree\nfield: parent\n"),
        &[],
    )
    .unwrap();

    assert_eq!(outcome.view_id, "first");
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    let ids: Vec<&str> = reloaded.views.iter().map(|view| view.id.as_str()).collect();
    assert_eq!(ids, vec!["first", "second"], "position must be preserved");
    assert!(matches!(
        &reloaded.views[0].kind,
        workdown_core::model::views::ViewKind::Tree { field } if field == "parent"
    ));
}

#[test]
fn update_view_replaces_the_filter() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=done\"\n",
    );

    update_view(
        &config,
        &root,
        "board",
        None,
        definition("type: board\nfield: status\n"),
        &[condition("status", Operator::Equal, Some("open"))],
    )
    .unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(reloaded.views[0].where_clauses, vec!["status=open"]);
}

#[test]
fn update_view_empty_filter_clears_where() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=done\"\n",
    );

    update_view(
        &config,
        &root,
        "board",
        None,
        definition("type: board\nfield: status\n"),
        &[],
    )
    .unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert!(reloaded.views[0].where_clauses.is_empty());
}

#[test]
fn update_view_unknown_id_errors_without_writing() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    let error = update_view(
        &config,
        &root,
        "nope",
        None,
        definition("type: board\nfield: status\n"),
        &[],
    )
    .unwrap_err();

    assert!(matches!(error, ViewWriteError::ViewNotFound { id } if id == "nope"));
    assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
}

#[test]
fn update_view_invalid_definition_errors_without_writing() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    // A board without its required `field` slot cannot be constructed.
    let error = update_view(
        &config,
        &root,
        "first",
        None,
        definition("type: board\n"),
        &[],
    )
    .unwrap_err();

    assert!(matches!(error, ViewWriteError::InvalidDefinition { .. }));
    assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
}

#[test]
fn update_view_with_bad_field_reference_writes_with_warning() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    // `field: nope` loads but fails cross-file validation —
    // save-with-warning, same as create.
    let outcome = update_view(
        &config,
        &root,
        "first",
        None,
        definition("type: board\nfield: nope\n"),
        &[],
    )
    .unwrap();

    assert!(outcome.mutation_caused_warning);
    assert!(!outcome.warnings.is_empty());
}

#[test]
fn update_view_with_new_name_renames_the_id() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    let outcome = update_view(
        &config,
        &root,
        "first",
        Some("Sprint Board"),
        definition("type: board\nfield: status\n"),
        &[],
    )
    .unwrap();

    assert_eq!(outcome.view_id, "sprint-board");
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    let ids: Vec<&str> = reloaded.views.iter().map(|view| view.id.as_str()).collect();
    assert_eq!(ids, vec!["sprint-board", "second"]);
}

#[test]
fn update_view_rename_removes_the_old_rendered_file() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);
    fs::create_dir_all(root.join("views")).unwrap();
    fs::write(root.join("views/first.md"), "# rendered\n").unwrap();

    let outcome = update_view(
        &config,
        &root,
        "first",
        Some("Sprint Board"),
        definition("type: board\nfield: status\n"),
        &[],
    )
    .unwrap();

    assert!(
        !root.join("views/first.md").exists(),
        "the old id's rendered file must be removed"
    );
    assert_eq!(
        outcome.info_messages.len(),
        1,
        "{:?}",
        outcome.info_messages
    );
    assert!(outcome.info_messages[0].contains("first.md"));
}

#[test]
fn update_view_rename_to_same_slug_is_not_a_rename() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);
    fs::create_dir_all(root.join("views")).unwrap();
    fs::write(root.join("views/first.md"), "# rendered\n").unwrap();

    // "First" slugs back to "first" — not a rename, nothing removed.
    let outcome = update_view(
        &config,
        &root,
        "first",
        Some("First"),
        definition("type: board\nfield: status\n"),
        &[],
    )
    .unwrap();

    assert_eq!(outcome.view_id, "first");
    assert!(root.join("views/first.md").exists());
    assert!(outcome.info_messages.is_empty());
}

#[test]
fn update_view_rename_to_existing_id_errors_without_writing() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    let error = update_view(
        &config,
        &root,
        "first",
        Some("Second"),
        definition("type: board\nfield: status\n"),
        &[],
    )
    .unwrap_err();

    assert!(matches!(error, ViewWriteError::DuplicateId { id } if id == "second"));
    assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
}

#[test]
fn update_view_blank_name_errors_without_writing() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    let error = update_view(
        &config,
        &root,
        "first",
        Some("   "),
        definition("type: board\nfield: status\n"),
        &[],
    )
    .unwrap_err();

    assert!(matches!(error, ViewWriteError::InvalidName { .. }));
    assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
}

#[test]
fn update_view_rename_keeps_preexisting_warning_unflagged() {
    let (_dir, root, config) = setup();
    // `field: nope` is a long-standing cross-file warning on the view.
    write_views(
        &root,
        "views:\n  - id: first\n    type: board\n    field: nope\n",
    );

    let outcome = update_view(
        &config,
        &root,
        "first",
        Some("Sprint Board"),
        definition("type: board\nfield: nope\n"),
        &[],
    )
    .unwrap();

    // The warning's diagnostic identity changes with the id, but the
    // rename didn't cause it — it must not flip the causation flag.
    assert!(!outcome.mutation_caused_warning, "{:?}", outcome.warnings);
    assert!(
        !outcome.warnings.is_empty(),
        "the pre-existing warning still rides in warnings"
    );
}

#[test]
fn update_view_rename_flags_a_warning_it_introduces() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    // Rename and break the definition in the same save.
    let outcome = update_view(
        &config,
        &root,
        "first",
        Some("Sprint Board"),
        definition("type: board\nfield: nope\n"),
        &[],
    )
    .unwrap();

    assert!(outcome.mutation_caused_warning);
}

// ── metric row filters ───────────────────────────────────────────

/// A metric view with a persisted per-row `where:` filter.
const METRIC_VIEW: &str = "\
views:
  - id: stats
    type: metric
    metrics:
      - label: Open
        aggregate: count
        where:
          - status=open
      - aggregate: count
";

fn metric_rows(views: &Views) -> &[workdown_core::model::views::MetricRow] {
    match &views.views[0].kind {
        workdown_core::model::views::ViewKind::Metric { metrics } => metrics,
        other => panic!("expected a metric view, got {other:?}"),
    }
}

#[test]
fn create_view_serializes_metric_row_filters() {
    let (_dir, root, config) = setup();

    // Rows carry their filter structured, exactly like the view level.
    create_view(
        &config,
        &root,
        "Stats",
        definition(
            "type: metric\nmetrics:\n  - aggregate: count\n    filter:\n      - kind: comparison\n        field: status\n        operator: equal\n        value: open\n",
        ),
        &[],
    )
    .unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(metric_rows(&reloaded)[0].where_clauses, vec!["status=open"]);
}

#[test]
fn update_view_replaces_metric_row_filters() {
    let (_dir, root, config) = setup();
    write_views(&root, METRIC_VIEW);

    update_view(
        &config,
        &root,
        "stats",
        None,
        definition(
            "type: metric\nmetrics:\n  - label: Open\n    aggregate: count\n    filter:\n      - kind: comparison\n        field: status\n        operator: equal\n        value: done\n  - aggregate: count\n    filter: []\n",
        ),
        &[],
    )
    .unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    let rows = metric_rows(&reloaded);
    assert_eq!(rows[0].where_clauses, vec!["status=done"]);
    assert!(rows[1].where_clauses.is_empty());
}

#[test]
fn update_view_metric_row_filter_with_bad_shape_errors_without_writing() {
    let (_dir, root, config) = setup();
    write_views(&root, METRIC_VIEW);
    let original = read_views(&root);

    let error = update_view(
        &config,
        &root,
        "stats",
        None,
        definition("type: metric\nmetrics:\n  - aggregate: count\n    filter: 5\n"),
        &[],
    )
    .unwrap_err();

    assert!(matches!(error, ViewWriteError::InvalidDefinition { .. }));
    assert_eq!(read_views(&root), original, "file must be untouched");
}

/// The edit-form contract, per row: what `ViewDefinition` hands out
/// (rows carrying structured `filter`, no `where`) feeds back through
/// `update_view` into an identical view.
#[test]
fn metric_row_filters_round_trip_through_view_definition() {
    let (_dir, root, config) = setup();
    write_views(&root, METRIC_VIEW);
    let views = load_views(&root.join(".workdown/views.yaml")).unwrap();

    let seed = workdown_core::mutation_data::ViewDefinition::from_view(&views.views[0]).unwrap();
    let serde_yaml::Value::Mapping(mapping) = &seed.definition else {
        panic!("definition must be a mapping");
    };
    let Some(serde_yaml::Value::Sequence(rows)) = mapping.get("metrics") else {
        panic!("metrics must be a sequence");
    };
    for row in rows {
        let serde_yaml::Value::Mapping(row) = row else {
            panic!("row must be a mapping");
        };
        assert!(
            row.contains_key("filter"),
            "row carries a structured filter"
        );
        assert!(
            !row.contains_key("where"),
            "raw where strings never leave core"
        );
    }

    update_view(&config, &root, "stats", None, seed.definition, &seed.filter).unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(
        reloaded.views, views.views,
        "an untouched round-trip is a no-op"
    );
}

// ── delete_view ──────────────────────────────────────────────────

#[test]
fn delete_view_removes_the_entry_and_keeps_the_rest() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    let outcome = delete_view(&config, &root, "first").unwrap();

    assert_eq!(outcome.view_id, "first");
    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    let ids: Vec<&str> = reloaded.views.iter().map(|view| view.id.as_str()).collect();
    assert_eq!(ids, vec!["second"]);
}

#[test]
fn delete_view_last_entry_leaves_a_loadable_empty_file() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: only\n    type: board\n    field: status\n",
    );

    delete_view(&config, &root, "only").unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert!(reloaded.views.is_empty());
}

#[test]
fn delete_view_unknown_id_errors_without_writing() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    let error = delete_view(&config, &root, "nope").unwrap_err();

    assert!(matches!(error, ViewWriteError::ViewNotFound { id } if id == "nope"));
    assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
}

#[test]
fn delete_view_removes_the_rendered_file() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);
    fs::create_dir_all(root.join("views")).unwrap();
    fs::write(root.join("views/first.md"), "# rendered\n").unwrap();

    let outcome = delete_view(&config, &root, "first").unwrap();

    assert!(!root.join("views/first.md").exists());
    assert_eq!(
        outcome.info_messages.len(),
        1,
        "{:?}",
        outcome.info_messages
    );
    assert!(outcome.info_messages[0].contains("removed"));
}

#[test]
fn delete_view_without_rendered_file_is_silent() {
    let (_dir, root, config) = setup();
    write_views(&root, TWO_VIEWS);

    let outcome = delete_view(&config, &root, "first").unwrap();

    assert!(
        outcome.info_messages.is_empty(),
        "{:?}",
        outcome.info_messages
    );
}

#[test]
fn delete_view_with_warning_elsewhere_reports_no_mutation_warning() {
    let (_dir, root, config) = setup();
    // `second` carries a pre-existing warning; deleting `first` can
    // only remove diagnostics, never introduce one.
    write_views(
        &root,
        "views:\n  - id: first\n    type: board\n    field: status\n  - id: second\n    type: board\n    field: nope\n",
    );

    let outcome = delete_view(&config, &root, "first").unwrap();

    assert!(!outcome.mutation_caused_warning);
    assert!(
        !outcome.warnings.is_empty(),
        "the surviving view's warning still rides in warnings"
    );
}

/// The removal honours a non-default `directory:` — the file lives
/// wherever `workdown render` would have written it.
#[test]
fn delete_view_removes_the_rendered_file_from_a_custom_directory() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "directory: rendered/views\nviews:\n  - id: only\n    type: board\n    field: status\n",
    );
    fs::create_dir_all(root.join("rendered/views")).unwrap();
    fs::write(root.join("rendered/views/only.md"), "# rendered\n").unwrap();

    delete_view(&config, &root, "only").unwrap();

    assert!(!root.join("rendered/views/only.md").exists());
}

#[test]
fn set_view_filter_preserves_other_views() {
    let (_dir, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: a\n    type: board\n    field: status\n  - id: b\n    type: tree\n    field: parent\n",
    );

    set_view_filter(&config, &root, "a", &[raw("status=open")]).unwrap();

    let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
    assert_eq!(reloaded.views.len(), 2);
    assert_eq!(reloaded.views[1].id, "b");
    assert!(matches!(
        &reloaded.views[1].kind,
        workdown_core::model::views::ViewKind::Tree { field, .. } if field == "parent"
    ));
}
