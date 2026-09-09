//! Integration tests for `workdown set`.
//!
//! Each test builds a throwaway project, mutates one field through the
//! public operation, and asserts on the outcome and the file on disk.
//! The per-family compute logic (`collection`, `numeric`, `temporal`,
//! `boolean`) is covered here too, one section per family.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use workdown_core::model::config::Config;
use workdown_core::model::diagnostic::{DiagnosticBody, ItemDiagnosticKind};
use workdown_core::model::schema::Severity;
use workdown_core::model::WorkItemId;
use workdown_core::operations::set::{
    run_set, BooleanMode, CollectionMode, DateMode, DurationMode, NumericMode, SetError,
    SetOperation, SetOutcome,
};
use workdown_core::parser::config::load_config;

// ── Fixture ──────────────────────────────────────────────────────────

const TEST_SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
    default: $filename_pretty
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
    default: open
  priority:
    type: choice
    values: [low, medium, high]
    required: false
  points:
    type: integer
    required: false
  tags:
    type: list
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
  assignees:
    type: links
    required: false
    allow_cycles: false
  labels:
    type: multichoice
    values: [bug, feature, chore]
    required: false
  velocity:
    type: float
    required: false
  estimate:
    type: duration
    required: false
  due_date:
    type: date
    required: false
  archived:
    type: boolean
    required: false
";

const TEST_CONFIG: &str = "\
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

const AGGREGATE_SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
    default: $filename_pretty
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
    default: open
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
  effort:
    type: integer
    required: false
    aggregate:
      function: sum
      over: parent
      error_on_missing: true
";

const RESOURCE_SCHEMA: &str = "\
fields:
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
    default: open
  assignee:
    type: string
    required: false
    resource: people
";

const RESOURCE_RESOURCES: &str = "\
people:
  - id: alice
    name: Alice Smith
";

fn setup_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), TEST_SCHEMA).unwrap();
    (directory, root)
}

fn setup_aggregate_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), AGGREGATE_SCHEMA).unwrap();
    (directory, root)
}

fn load_test_config(root: &Path) -> Config {
    load_config(&root.join(".workdown/config.yaml")).unwrap()
}

fn write_item(root: &Path, id: &str, content: &str) {
    fs::write(root.join(format!("workdown-items/{id}.md")), content).unwrap();
}

fn read_item(root: &Path, id: &str) -> String {
    fs::read_to_string(root.join(format!("workdown-items/{id}.md"))).unwrap()
}

/// Duration fields whose values come from somewhere other than the file
/// itself: rolled up from children, computed from a sibling field, or
/// pulled over a forward link. The fixture for "`--delta` starts from
/// zero regardless".
///
/// No `default:` counterpart — a `duration` field cannot declare one
/// (see `validate_default` in the schema parser), so there is no
/// stamped-in duration for a delta to pick up by accident.
const DERIVED_DURATION_SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
    default: $filename_pretty
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
    default: open
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
  depends_on:
    type: links
    required: false
    allow_cycles: false
  base_effort:
    type: duration
    required: false
  rolled_up_effort:
    type: duration
    required: false
    aggregate:
      function: sum
      over: parent
  computed_effort:
    type: duration
    required: false
    compute: base_effort * 2
  pulled_effort:
    type: duration
    required: false
    pull:
      over: depends_on
      field: base_effort
      function: sum
";

fn setup_derived_duration_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), DERIVED_DURATION_SCHEMA).unwrap();
    (directory, root)
}

/// A project whose `assignee` field is backed by a populated `people`
/// section — the fixture for resource-reference warnings on mutation.
fn setup_resource_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), RESOURCE_SCHEMA).unwrap();
    fs::write(root.join(".workdown/resources.yaml"), RESOURCE_RESOURCES).unwrap();
    (directory, root)
}

// ── Happy path ───────────────────────────────────────────────────

#[test]
fn replace_choice_value_writes_file_and_returns_previous() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\n---\nbody text\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "status",
        SetOperation::Replace(serde_yaml::Value::String("in_progress".to_owned())),
    )
    .unwrap();

    assert_eq!(outcome.previous_value.unwrap().as_str().unwrap(), "open");
    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "in_progress");
    assert!(!outcome.mutation_caused_warning);

    let file = read_item(&root, "task-1");
    assert!(file.contains("status: in_progress"));
    assert!(!file.contains("status: open"));
}

#[test]
fn replace_preserves_body_byte_for_byte() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    let body = "Line one of the body.\n\n## Heading\n\nMore body.\n";
    write_item(
        &root,
        "task-1",
        &format!("---\ntitle: Task 1\nstatus: open\n---\n{body}"),
    );

    run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "status",
        SetOperation::Replace(serde_yaml::Value::String("done".to_owned())),
    )
    .unwrap();

    let file = read_item(&root, "task-1");
    let body_offset = file.find("---\n").unwrap();
    let after_first = body_offset + 4;
    let closing = file[after_first..].find("---\n").unwrap();
    let body_in_file = &file[after_first + closing + 4..];
    assert_eq!(body_in_file, body);
}

#[test]
fn set_on_crlf_file_does_not_duplicate_closing_delimiter() {
    // Regression: on a CRLF file the body offset used to land before the
    // closing `---`, so the leftover delimiter bytes rode along in the
    // body and a second `---` was emitted after the rewritten frontmatter.
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\r\ntitle: Task 1\r\nstatus: open\r\n---\r\nSome Content\r\n",
    );

    run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "status",
        SetOperation::Replace(serde_yaml::Value::String("done".to_owned())),
    )
    .unwrap();

    let file = read_item(&root, "task-1");
    // Exactly two `---` delimiters (open + close), not three.
    assert_eq!(
        file.matches("---").count(),
        2,
        "expected one opening and one closing delimiter, got: {file:?}"
    );
    // The body after the closing delimiter is exactly the original body,
    // with no leftover delimiter bytes riding along in front of it.
    let after_opening = file.find("---\n").unwrap() + 4;
    let closing = file[after_opening..].find("---\n").unwrap();
    let body_in_file = &file[after_opening + closing + 4..];
    assert_eq!(body_in_file, "Some Content\r\n");
}

#[test]
fn previous_value_is_none_when_field_was_absent() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "priority",
        SetOperation::Replace(serde_yaml::Value::String("high".to_owned())),
    )
    .unwrap();

    assert!(outcome.previous_value.is_none());
    let file = read_item(&root, "task-1");
    assert!(file.contains("priority: high"));
}

// ── Save-with-warning on coercion failure ────────────────────────

#[test]
fn invalid_choice_value_saves_with_warning_and_flags_mutation_caused() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "status",
        SetOperation::Replace(serde_yaml::Value::String("urgent".to_owned())),
    )
    .unwrap();

    assert!(outcome.mutation_caused_warning);
    assert!(!outcome.warnings.is_empty());

    // File was written despite the invalid value.
    let file = read_item(&root, "task-1");
    assert!(file.contains("status: urgent"));
}

// ── List replacement ─────────────────────────────────────────────

#[test]
fn list_field_replace_writes_sequence() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let value = serde_yaml::Value::Sequence(vec![
        serde_yaml::Value::String("auth".to_owned()),
        serde_yaml::Value::String("backend".to_owned()),
    ]);

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "tags",
        SetOperation::Replace(value),
    )
    .unwrap();

    assert!(!outcome.mutation_caused_warning);
    let file = read_item(&root, "task-1");
    assert!(file.contains("tags:"));
    assert!(file.contains("auth"));
    assert!(file.contains("backend"));
}

// ── Hard errors ──────────────────────────────────────────────────

#[test]
fn unknown_item_errors_without_writing() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("does-not-exist".to_owned()),
        "status",
        SetOperation::Replace(serde_yaml::Value::String("done".to_owned())),
    );

    assert!(matches!(result, Err(SetError::UnknownItem { .. })));
}

#[test]
fn unknown_field_errors_without_writing() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "nonexistent",
        SetOperation::Replace(serde_yaml::Value::String("x".to_owned())),
    );

    assert!(matches!(result, Err(SetError::UnknownField { .. })));

    // File untouched.
    let file = read_item(&root, "task-1");
    assert!(!file.contains("nonexistent"));
}

#[test]
fn setting_id_returns_error_with_rename_hint() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "id",
        SetOperation::Replace(serde_yaml::Value::String("new-id".to_owned())),
    );

    let error = result.unwrap_err();
    assert!(matches!(error, SetError::IdNotMutable));
    assert!(error.to_string().contains("workdown rename"));
}

// ── Explicit id in frontmatter is preserved ──────────────────────

#[test]
fn explicit_id_in_frontmatter_is_preserved_after_set() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    // Filename and frontmatter id differ — id was user-set.
    write_item(
        &root,
        "filename-slug",
        "---\nid: custom-id\ntitle: Task\nstatus: open\n---\n",
    );

    run_set(
        &config,
        &root,
        &WorkItemId::from("custom-id".to_owned()),
        "status",
        SetOperation::Replace(serde_yaml::Value::String("done".to_owned())),
    )
    .unwrap();

    let file = read_item(&root, "filename-slug");
    assert!(file.contains("id: custom-id"));
    assert!(file.contains("status: done"));
}

// ── Diff-based mutation_caused_warning (covers a previous gap) ───

#[test]
fn set_with_broken_link_flags_mutation_caused_warning() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "parent",
        SetOperation::Replace(serde_yaml::Value::String("does-not-exist".to_owned())),
    )
    .unwrap();

    // Broken link is a *new* diagnostic introduced by this mutation
    // (the parent field passes coerce — the BrokenLink finding is
    // emitted by Store::load on reload). The diff catches it.
    assert!(outcome.mutation_caused_warning);
    let has_broken_link = outcome.warnings.iter().any(|d| match &d.body {
        DiagnosticBody::Item(item) => matches!(
            &item.kind,
            ItemDiagnosticKind::BrokenLink { field, .. } if field == "parent"
        ),
        _ => false,
    });
    assert!(has_broken_link);
}

#[test]
fn set_with_unknown_resource_value_warns_at_write_time() {
    let (_directory, root) = setup_resource_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\nstatus: open\n---\n");

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "assignee",
        SetOperation::Replace(serde_yaml::Value::String("carol".to_owned())),
    )
    .unwrap();

    // The whole reason this check sits in `Store::load` rather than
    // in a project-level pass: `set` builds its own store, so a
    // project-level check would let this write land in silence.
    assert!(outcome.mutation_caused_warning);
    let unknown_reference = outcome
        .warnings
        .iter()
        .find(|diagnostic| match &diagnostic.body {
            DiagnosticBody::Item(item) => matches!(
                &item.kind,
                ItemDiagnosticKind::UnknownResourceRef { field, value, .. }
                    if field == "assignee" && value == "carol"
            ),
            _ => false,
        })
        .expect("an unknown assignee must warn");
    assert_eq!(unknown_reference.severity, Severity::Warning);

    // Save-with-warning: the value is on disk regardless.
    assert!(read_item(&root, "task-1").contains("assignee: carol"));
}

#[test]
fn set_with_known_resource_value_is_silent() {
    let (_directory, root) = setup_resource_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\nstatus: open\n---\n");

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "assignee",
        SetOperation::Replace(serde_yaml::Value::String("alice".to_owned())),
    )
    .unwrap();

    assert!(!outcome.mutation_caused_warning);
    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
}

// ── Unset ────────────────────────────────────────────────────────

#[test]
fn unset_removes_field_and_writes_file() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\npriority: high\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "priority",
        SetOperation::Unset,
    )
    .unwrap();

    assert_eq!(outcome.previous_value.unwrap().as_str().unwrap(), "high");
    assert!(outcome.new_value.is_none());
    assert!(!outcome.mutation_caused_warning);

    let file = read_item(&root, "task-1");
    assert!(!file.contains("priority:"));
    assert!(file.contains("status: open"));
}

#[test]
fn unset_absent_field_is_noop_and_exits_zero() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    let original = "---\ntitle: Task 1\nstatus: open\n---\nbody\n";
    write_item(&root, "task-1", original);

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "priority",
        SetOperation::Unset,
    )
    .unwrap();

    assert!(outcome.previous_value.is_none());
    assert!(outcome.new_value.is_none());
    assert!(!outcome.mutation_caused_warning);

    // File untouched byte-for-byte.
    let file = read_item(&root, "task-1");
    assert_eq!(file, original);
}

#[test]
fn unset_required_field_saves_with_missing_required_warning_and_flags_mutation_caused() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "status",
        SetOperation::Unset,
    )
    .unwrap();

    assert!(outcome.mutation_caused_warning);

    // File written despite the required violation.
    let file = read_item(&root, "task-1");
    assert!(!file.contains("status:"));

    let has_missing = outcome.warnings.iter().any(|d| match &d.body {
        DiagnosticBody::Item(item) => matches!(
            &item.kind,
            ItemDiagnosticKind::MissingRequired { field } if field == "status"
        ),
        _ => false,
    });
    assert!(has_missing);
}

#[test]
fn unset_id_returns_idnotmutable_with_reworded_message() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "id",
        SetOperation::Unset,
    );

    let error = result.unwrap_err();
    assert!(matches!(error, SetError::IdNotMutable));
    let message = error.to_string();
    assert!(message.contains("modify"));
    assert!(message.contains("workdown rename"));
}

#[test]
fn unset_unknown_field_errors_without_writing() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    let original = "---\ntitle: Task 1\nstatus: open\n---\nbody\n";
    write_item(&root, "task-1", original);

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "nonexistent",
        SetOperation::Unset,
    );

    assert!(matches!(result, Err(SetError::UnknownField { .. })));
    let file = read_item(&root, "task-1");
    assert_eq!(file, original);
}

#[test]
fn unset_unknown_item_errors_without_writing() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("does-not-exist".to_owned()),
        "priority",
        SetOperation::Unset,
    );

    assert!(matches!(result, Err(SetError::UnknownItem { .. })));
}

#[test]
fn unset_preserves_body_byte_for_byte() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    let body = "Line one of the body.\n\n## Heading\n\nMore body.\n";
    write_item(
        &root,
        "task-1",
        &format!("---\ntitle: Task 1\nstatus: open\npriority: high\n---\n{body}"),
    );

    run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "priority",
        SetOperation::Unset,
    )
    .unwrap();

    let file = read_item(&root, "task-1");
    let body_offset = file.find("---\n").unwrap();
    let after_first = body_offset + 4;
    let closing = file[after_first..].find("---\n").unwrap();
    let body_in_file = &file[after_first + closing + 4..];
    assert_eq!(body_in_file, body);
}

#[test]
fn unset_explicit_id_in_frontmatter_is_preserved() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "filename-slug",
        "---\nid: custom-id\ntitle: Task\nstatus: open\npriority: high\n---\n",
    );

    run_set(
        &config,
        &root,
        &WorkItemId::from("custom-id".to_owned()),
        "priority",
        SetOperation::Unset,
    )
    .unwrap();

    let file = read_item(&root, "filename-slug");
    assert!(file.contains("id: custom-id"));
    assert!(!file.contains("priority:"));
}

#[test]
fn unset_does_not_flag_mutation_caused_warning_for_unrelated_existing_warnings() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    // Pre-existing item with an UnknownField warning — should be
    // visible in the post-write output but must not flip
    // mutation_caused_warning on an unrelated unset.
    write_item(
        &root,
        "noisy",
        "---\ntitle: Noisy\nstatus: open\nextra_unknown: foo\n---\n",
    );
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\npriority: high\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "priority",
        SetOperation::Unset,
    )
    .unwrap();

    assert!(!outcome.mutation_caused_warning);
    // Pre-existing warning still surfaces (milestone "always show all").
    assert!(!outcome.warnings.is_empty());
}

// ── Aggregate field interaction ──────────────────────────────────

#[test]
fn unset_aggregate_field_with_error_on_missing_surfaces_warning() {
    let (_directory, root) = setup_aggregate_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\neffort: 5\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "effort",
        SetOperation::Unset,
    )
    .unwrap();

    assert!(outcome.mutation_caused_warning);

    let file = read_item(&root, "task-1");
    assert!(!file.contains("effort:"));

    // The rollup pass on reload surfaces AggregateMissingValue for
    // the now-empty aggregate field with error_on_missing.
    let has_missing = outcome.warnings.iter().any(|d| match &d.body {
        DiagnosticBody::Item(item) => matches!(
            &item.kind,
            ItemDiagnosticKind::AggregateMissingValue { field } if field == "effort"
        ),
        _ => false,
    });
    assert!(has_missing);
}

// ── Collection modes (from operations/set/collection.rs) ─────────────

// ── Collection modes: append ─────────────────────────────────────

#[test]
fn append_to_list_appends_in_order() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\ntags: [auth]\n---\n",
    );

    let appended = vec![serde_yaml::Value::String("backend".to_owned())];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "tags",
        SetOperation::Collection(CollectionMode::Append(appended)),
    )
    .unwrap();

    let new_sequence = outcome.new_value.unwrap();
    let elements = new_sequence.as_sequence().unwrap();
    assert_eq!(elements.len(), 2);
    assert_eq!(elements[0].as_str().unwrap(), "auth");
    assert_eq!(elements[1].as_str().unwrap(), "backend");
    assert!(outcome.info_messages.is_empty());
    assert!(!outcome.mutation_caused_warning);
}

#[test]
fn append_to_absent_field_creates_sequence() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let appended = vec![serde_yaml::Value::String("qa".to_owned())];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "tags",
        SetOperation::Collection(CollectionMode::Append(appended)),
    )
    .unwrap();

    assert!(outcome.previous_value.is_none());
    let elements = outcome.new_value.unwrap();
    let sequence = elements.as_sequence().unwrap();
    assert_eq!(sequence.len(), 1);
    assert_eq!(sequence[0].as_str().unwrap(), "qa");
}

#[test]
fn append_duplicate_writes_and_emits_info() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\ntags: [auth, qa]\n---\n",
    );

    let appended = vec![serde_yaml::Value::String("qa".to_owned())];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "tags",
        SetOperation::Collection(CollectionMode::Append(appended)),
    )
    .unwrap();

    let elements = outcome.new_value.unwrap();
    let sequence = elements.as_sequence().unwrap();
    assert_eq!(sequence.len(), 3);
    assert_eq!(sequence[2].as_str().unwrap(), "qa");
    assert_eq!(outcome.info_messages.len(), 1);
    assert!(outcome.info_messages[0].contains("'qa'"));
    assert!(outcome.info_messages[0].contains("already present"));
    // Duplicate append is intentional and never flips exit code.
    assert!(!outcome.mutation_caused_warning);
}

#[test]
fn append_multi_value_in_order() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\ntags: [auth]\n---\n",
    );

    let appended = vec![
        serde_yaml::Value::String("backend".to_owned()),
        serde_yaml::Value::String("qa".to_owned()),
    ];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "tags",
        SetOperation::Collection(CollectionMode::Append(appended)),
    )
    .unwrap();

    let elements = outcome.new_value.unwrap();
    let sequence = elements.as_sequence().unwrap();
    let names: Vec<&str> = sequence.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(names, vec!["auth", "backend", "qa"]);
}

#[test]
fn append_on_links_field_works() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");
    write_item(&root, "alice", "---\ntitle: Alice\nstatus: open\n---\n");

    let appended = vec![serde_yaml::Value::String("alice".to_owned())];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "assignees",
        SetOperation::Collection(CollectionMode::Append(appended)),
    )
    .unwrap();

    let elements = outcome.new_value.unwrap();
    let sequence = elements.as_sequence().unwrap();
    assert_eq!(sequence.len(), 1);
    assert_eq!(sequence[0].as_str().unwrap(), "alice");
}

#[test]
fn append_on_multichoice_field_works() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let appended = vec![serde_yaml::Value::String("bug".to_owned())];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "labels",
        SetOperation::Collection(CollectionMode::Append(appended)),
    )
    .unwrap();

    let elements = outcome.new_value.unwrap();
    let sequence = elements.as_sequence().unwrap();
    assert_eq!(sequence.len(), 1);
    assert_eq!(sequence[0].as_str().unwrap(), "bug");
    assert!(!outcome.mutation_caused_warning);
}

// ── Collection modes: remove ─────────────────────────────────────

#[test]
fn remove_value_removes_all_occurrences() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\ntags: [auth, backend, auth]\n---\n",
    );

    let to_remove = vec![serde_yaml::Value::String("auth".to_owned())];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "tags",
        SetOperation::Collection(CollectionMode::Remove(to_remove)),
    )
    .unwrap();

    let elements = outcome.new_value.unwrap();
    let sequence = elements.as_sequence().unwrap();
    assert_eq!(sequence.len(), 1);
    assert_eq!(sequence[0].as_str().unwrap(), "backend");
    assert!(outcome.info_messages.is_empty());
}

#[test]
fn remove_absent_value_emits_info() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\ntags: [auth]\n---\n",
    );

    let to_remove = vec![serde_yaml::Value::String("qa".to_owned())];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "tags",
        SetOperation::Collection(CollectionMode::Remove(to_remove)),
    )
    .unwrap();

    // Sequence unchanged → write skipped, file byte-identical.
    let file_after = read_item(&root, "task-1");
    assert!(file_after.contains("tags:"));
    assert!(file_after.contains("auth"));
    assert!(!file_after.contains("qa"));

    assert_eq!(outcome.info_messages.len(), 1);
    assert!(outcome.info_messages[0].contains("'qa'"));
    assert!(outcome.info_messages[0].contains("not present"));
    assert!(!outcome.mutation_caused_warning);
}

#[test]
fn remove_from_absent_field_is_noop_with_info() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    let original = "---\ntitle: Task 1\nstatus: open\n---\nbody\n";
    write_item(&root, "task-1", original);

    let to_remove = vec![serde_yaml::Value::String("qa".to_owned())];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "tags",
        SetOperation::Collection(CollectionMode::Remove(to_remove)),
    )
    .unwrap();

    // File untouched byte-for-byte; field stays absent.
    assert_eq!(read_item(&root, "task-1"), original);
    assert!(outcome.previous_value.is_none());
    assert_eq!(outcome.info_messages.len(), 1);
}

#[test]
fn remove_multi_value_with_some_absent() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\ntags: [auth, backend]\n---\n",
    );

    let to_remove = vec![
        serde_yaml::Value::String("auth".to_owned()),
        serde_yaml::Value::String("qa".to_owned()),
    ];
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "tags",
        SetOperation::Collection(CollectionMode::Remove(to_remove)),
    )
    .unwrap();

    let elements = outcome.new_value.unwrap();
    let sequence = elements.as_sequence().unwrap();
    assert_eq!(sequence.len(), 1);
    assert_eq!(sequence[0].as_str().unwrap(), "backend");
    assert_eq!(outcome.info_messages.len(), 1);
    assert!(outcome.info_messages[0].contains("'qa'"));
}

// ── Collection modes: mode-type validity ─────────────────────────

#[test]
fn append_on_choice_field_returns_mode_not_valid_error() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let appended = vec![serde_yaml::Value::String("done".to_owned())];
    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "status",
        SetOperation::Collection(CollectionMode::Append(appended)),
    );

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        SetError::ModeNotValidForFieldType { mode, ref field, .. }
            if mode == "append" && field == "status"
    ));
    let message = error.to_string();
    assert!(message.contains("--append"));
    assert!(message.contains("'status'"));
    assert!(message.contains("choice"));
}

#[test]
fn remove_on_integer_field_returns_mode_not_valid_error() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\npoints: 3\n---\n",
    );

    let to_remove = vec![serde_yaml::Value::String("3".to_owned())];
    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "points",
        SetOperation::Collection(CollectionMode::Remove(to_remove)),
    );

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        SetError::ModeNotValidForFieldType { mode, ref field, .. }
            if mode == "remove" && field == "points"
    ));
}

#[test]
fn append_on_link_singular_field_returns_mode_not_valid_error() {
    // `parent: link` is single-valued — collection modes must reject.
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let appended = vec![serde_yaml::Value::String("other-task".to_owned())];
    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "parent",
        SetOperation::Collection(CollectionMode::Append(appended)),
    );

    assert!(matches!(
        result,
        Err(SetError::ModeNotValidForFieldType { .. })
    ));
}

// ── Numeric delta (from operations/set/numeric.rs) ───────────────────

// ── Delta: numeric ───────────────────────────────────────────────

#[test]
fn delta_on_integer_adds_value() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\npoints: 5\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "points",
        SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
    )
    .unwrap();

    assert_eq!(outcome.previous_value.unwrap().as_i64().unwrap(), 5);
    assert_eq!(outcome.new_value.unwrap().as_i64().unwrap(), 8);
    let file = read_item(&root, "task-1");
    assert!(file.contains("points: 8"));
}

#[test]
fn delta_on_integer_with_negative_subtracts() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\npoints: 5\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "points",
        SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(-3_i64))),
    )
    .unwrap();

    assert_eq!(outcome.new_value.unwrap().as_i64().unwrap(), 2);
}

#[test]
fn delta_on_float_adds_value() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\nvelocity: 2.5\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "velocity",
        SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(1.5_f64))),
    )
    .unwrap();

    assert!((outcome.new_value.unwrap().as_f64().unwrap() - 4.0).abs() < 1e-9);
}

#[test]
fn delta_on_absent_numeric_field_returns_requires_existing() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "points",
        SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
    );

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        SetError::MutationRequiresExistingValue { mode, ref field }
            if mode == "delta" && field == "points"
    ));
}

#[test]
fn delta_on_numeric_field_written_with_no_value_returns_requires_existing() {
    // A null is absent, not malformed — there is no value to be
    // invalid. Same rule as duration, different consequence: a count
    // still asks for an initial value.
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\npoints:\n---\n",
    );

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "points",
        SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
    );

    assert!(matches!(
        result.unwrap_err(),
        SetError::MutationRequiresExistingValue { mode, ref field }
            if mode == "delta" && field == "points"
    ));
}

#[test]
fn delta_on_malformed_numeric_returns_malformed_error() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\npoints: high\n---\n",
    );

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "points",
        SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
    );

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        SetError::MutationCurrentValueMalformed { mode, ref field, .. }
            if mode == "delta" && field == "points"
    ));
}

#[test]
fn numeric_delta_on_choice_field_returns_mode_not_valid() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "status",
        SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(1))),
    );

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        SetError::ModeNotValidForFieldType { mode, ref field, .. }
            if mode == "delta" && field == "status"
    ));
}

// ── Duration and date delta (from operations/set/temporal.rs) ────────

// ── Delta: duration ──────────────────────────────────────────────

#[test]
fn delta_on_duration_adds_seconds() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\nestimate: 2d\n---\n",
    );

    // +1d = 86_400 seconds
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "estimate",
        SetOperation::Duration(DurationMode::Delta(86_400)),
    )
    .unwrap();

    let new_string = outcome.new_value.unwrap();
    assert_eq!(new_string.as_str().unwrap(), "3d");
}

#[test]
fn delta_on_duration_with_negative_subtracts() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\nestimate: 1w\n---\n",
    );

    // -3d = -259_200 seconds. 1w - 3d = 4d.
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "estimate",
        SetOperation::Duration(DurationMode::Delta(-259_200)),
    )
    .unwrap();

    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "4d");
}

// ── Delta: duration on an absent field ───────────────────────────

/// Run `--delta` on `estimate` against a one-item project whose
/// frontmatter is `content`, and return the outcome.
fn delta_estimate(content: &str, delta_seconds: i64) -> SetOutcome {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", content);

    run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "estimate",
        SetOperation::Duration(DurationMode::Delta(delta_seconds)),
    )
    .unwrap()
}

#[test]
fn delta_on_absent_duration_field_creates_it_from_zero() {
    // +30min = 1_800 seconds
    let outcome = delta_estimate("---\ntitle: Task 1\nstatus: open\n---\n", 1_800);

    assert!(outcome.previous_value.is_none());
    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
}

#[test]
fn delta_on_duration_field_written_with_no_value_creates_it_from_zero() {
    // `estimate:` parses as YAML null — nothing to start from, and
    // nothing a delta could destroy either.
    let outcome = delta_estimate("---\ntitle: Task 1\nstatus: open\nestimate:\n---\n", 1_800);

    assert!(
        outcome.previous_value.is_none(),
        "a null on disk reports as absent, not as `null` — the file never said `null`"
    );
    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
}

#[test]
fn delta_on_duration_field_holding_an_empty_string_creates_it_from_zero() {
    let outcome = delta_estimate(
        "---\ntitle: Task 1\nstatus: open\nestimate: ''\n---\n",
        1_800,
    );

    assert!(outcome.previous_value.is_none());
    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
}

#[test]
fn negative_delta_on_absent_duration_field_writes_a_negative_duration() {
    // Zero minus thirty minutes is minus thirty minutes. A project
    // that considers that nonsense sets `min: 0` on the field.
    let outcome = delta_estimate("---\ntitle: Task 1\nstatus: open\n---\n", -1_800);

    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "-30min");
}

#[test]
fn zero_delta_on_absent_duration_field_creates_it_at_zero() {
    // A delta always writes. Deciding a short session records nothing
    // belongs to whatever measured it, before it asks for a delta.
    let outcome = delta_estimate("---\ntitle: Task 1\nstatus: open\n---\n", 0);

    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "0s");
}

#[test]
fn delta_on_malformed_duration_still_returns_malformed_error() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\nestimate: two weeks\n---\n",
    );

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "estimate",
        SetOperation::Duration(DurationMode::Delta(1_800)),
    );

    // Replacing a typo with a measured number would destroy the
    // evidence that something was wrong.
    assert!(matches!(
        result.unwrap_err(),
        SetError::MutationCurrentValueMalformed { mode, ref field, .. }
            if mode == "delta" && field == "estimate"
    ));
    assert!(read_item(&root, "task-1").contains("estimate: two weeks"));
}

// ── Delta: duration on a derived field ───────────────────────────
//
// A schema default has no test of its own: a `duration` field cannot
// declare a default at all, so there is nothing stamped in for a
// delta to pick up.

#[test]
fn delta_on_a_rolled_up_duration_starts_from_zero_and_warns() {
    // Starting from the roll-up would freeze it into the file, where
    // it would go stale the next time a child changed.
    let (_directory, root) = setup_derived_duration_project();
    let config = load_test_config(&root);
    write_item(&root, "epic-1", "---\ntitle: Epic 1\nstatus: open\n---\n");
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\nparent: epic-1\nrolled_up_effort: 2h\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("epic-1".to_owned()),
        "rolled_up_effort",
        SetOperation::Duration(DurationMode::Delta(1_800)),
    )
    .unwrap();

    assert!(outcome.previous_value.is_none());
    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
    // Nothing happens quietly: the hand-written value now competes
    // with the roll-up from `task-1`.
    assert!(
        outcome.mutation_caused_warning,
        "a manual value competing with a roll-up must surface, warnings: {:?}",
        outcome.warnings
    );
}

#[test]
fn delta_on_a_computed_duration_starts_from_zero() {
    // `computed_effort` is `base_effort * 2` — 2h here, and still not
    // the delta's starting point.
    let (_directory, root) = setup_derived_duration_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\nbase_effort: 1h\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "computed_effort",
        SetOperation::Duration(DurationMode::Delta(1_800)),
    )
    .unwrap();

    assert!(outcome.previous_value.is_none());
    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
}

#[test]
fn delta_on_a_pulled_duration_starts_from_zero() {
    // `pulled_effort` sums `base_effort` over `depends_on` — 3h here.
    let (_directory, root) = setup_derived_duration_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\nbase_effort: 3h\n---\n",
    );
    write_item(
        &root,
        "task-2",
        "---\ntitle: Task 2\nstatus: open\ndepends_on: [task-1]\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-2".to_owned()),
        "pulled_effort",
        SetOperation::Duration(DurationMode::Delta(1_800)),
    )
    .unwrap();

    assert!(outcome.previous_value.is_none());
    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "30min");
}

// ── Delta: date ──────────────────────────────────────────────────

#[test]
fn delta_on_date_adds_duration() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\ndue_date: '2026-05-14'\n---\n",
    );

    // +1w
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "due_date",
        SetOperation::Date(DateMode::Delta(604_800)),
    )
    .unwrap();

    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "2026-05-21");
}

#[test]
fn delta_on_date_with_negative_subtracts_duration() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\ndue_date: '2026-05-14'\n---\n",
    );

    // -3d
    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "due_date",
        SetOperation::Date(DateMode::Delta(-259_200)),
    )
    .unwrap();

    assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "2026-05-11");
}

#[test]
fn delta_on_absent_date_field_returns_requires_existing() {
    // A date has no zero to count from. The asymmetry with duration
    // is deliberate and written down under `--delta` in the help.
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "due_date",
        SetOperation::Date(DateMode::Delta(86_400)),
    );

    assert!(matches!(
        result,
        Err(SetError::MutationRequiresExistingValue { .. })
    ));
}

#[test]
fn delta_on_date_field_written_with_no_value_returns_requires_existing() {
    // A null is absent, not malformed — there is no value to be
    // invalid. Same rule as duration, different consequence.
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\ndue_date:\n---\n",
    );

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "due_date",
        SetOperation::Date(DateMode::Delta(86_400)),
    );

    assert!(matches!(
        result.unwrap_err(),
        SetError::MutationRequiresExistingValue { mode, ref field }
            if mode == "delta" && field == "due_date"
    ));
}

#[test]
fn date_delta_on_integer_field_returns_mode_not_valid() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\npoints: 3\n---\n",
    );

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "points",
        SetOperation::Date(DateMode::Delta(86_400)),
    );

    assert!(matches!(
        result,
        Err(SetError::ModeNotValidForFieldType { .. })
    ));
}

// ── Boolean toggle (from operations/set/boolean.rs) ──────────────────

// ── Toggle: boolean ──────────────────────────────────────────────

#[test]
fn toggle_flips_boolean_from_false_to_true() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\narchived: false\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "archived",
        SetOperation::Boolean(BooleanMode::Toggle),
    )
    .unwrap();

    assert!(!outcome.previous_value.unwrap().as_bool().unwrap());
    assert!(outcome.new_value.unwrap().as_bool().unwrap());
    let file = read_item(&root, "task-1");
    assert!(file.contains("archived: true"));
}

#[test]
fn toggle_flips_boolean_from_true_to_false() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\narchived: true\n---\n",
    );

    let outcome = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "archived",
        SetOperation::Boolean(BooleanMode::Toggle),
    )
    .unwrap();

    assert!(!outcome.new_value.unwrap().as_bool().unwrap());
}

#[test]
fn toggle_on_absent_field_returns_requires_existing() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "archived",
        SetOperation::Boolean(BooleanMode::Toggle),
    );

    assert!(matches!(
        result,
        Err(SetError::MutationRequiresExistingValue { mode, ref field })
            if mode == "toggle" && field == "archived"
    ));
}

#[test]
fn toggle_on_field_written_with_no_value_returns_requires_existing() {
    // A null is absent, not malformed — there is no value to be
    // invalid, and no "flip nothing" either.
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\narchived:\n---\n",
    );

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "archived",
        SetOperation::Boolean(BooleanMode::Toggle),
    );

    assert!(matches!(
        result.unwrap_err(),
        SetError::MutationRequiresExistingValue { mode, ref field }
            if mode == "toggle" && field == "archived"
    ));
}

#[test]
fn toggle_on_non_boolean_field_returns_mode_not_valid() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "status",
        SetOperation::Boolean(BooleanMode::Toggle),
    );

    let error = result.unwrap_err();
    assert!(matches!(
        error,
        SetError::ModeNotValidForFieldType { mode, ref field, .. }
            if mode == "toggle" && field == "status"
    ));
}

#[test]
fn toggle_on_malformed_boolean_returns_malformed_error() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    // `archived: yes` — YAML parses this as a string, not a bool.
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task 1\nstatus: open\narchived: yes\n---\n",
    );

    let result = run_set(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "archived",
        SetOperation::Boolean(BooleanMode::Toggle),
    );

    assert!(matches!(
        result,
        Err(SetError::MutationCurrentValueMalformed { mode, ref field, .. })
            if mode == "toggle" && field == "archived"
    ));
}
