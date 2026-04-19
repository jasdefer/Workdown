//! Integration tests for `workdown add`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use workdown::commands::add::{run_add, AddError};
use workdown::parser::config::load_config;

const TEST_SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
    default: $filename_pretty
  type:
    type: choice
    values: [epic, task, bug]
    required: true
    default: task
  status:
    type: choice
    values: [backlog, open, in_progress, done]
    required: true
    default: backlog
  priority:
    type: choice
    values: [critical, high, medium, low]
    required: false
  assignee:
    type: string
    required: false
  created:
    type: date
    required: true
    default: $today
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
rules:
  - name: in-progress-needs-assignee
    description: Work items in progress must have an assignee
    match:
      status: in_progress
    require:
      assignee: required
  - name: bugs-need-priority
    description: Bugs must have a priority set
    match:
      type: bug
    require:
      priority: required
";

const TEST_CONFIG: &str = "\
project:
  name: Test Project
  description: ''
paths:
  work_items: workdown-items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
";

/// Set up a temp directory with config, schema, and empty items directory.
fn setup_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();

    fs::create_dir_all(root.join(".workdown")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();

    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), TEST_SCHEMA).unwrap();

    (directory, root)
}

fn load_test_config(root: &PathBuf) -> workdown::model::config::Config {
    load_config(&root.join(".workdown/config.yaml")).unwrap()
}

/// Build a field map from `(name, string_value)` pairs.
fn fields(pairs: &[(&str, &str)]) -> HashMap<String, serde_yaml::Value> {
    pairs
        .iter()
        .map(|(name, value)| {
            (
                (*name).to_owned(),
                serde_yaml::Value::String((*value).to_owned()),
            )
        })
        .collect()
}

// ── Happy path ──────────────────────────────────────────────────────

#[test]
fn add_creates_work_item_file() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(&config, &root, fields(&[("title", "My First Task")])).unwrap();

    assert!(outcome.path.exists());
    assert_eq!(outcome.path, root.join("workdown-items/my-first-task.md"));

    let content = fs::read_to_string(&outcome.path).unwrap();
    assert!(content.starts_with("---\n"));
    assert!(content.contains("title: My First Task"));
    assert!(content.contains("type: task"));
    assert!(content.contains("status: backlog"));
    assert!(content.contains("created:"));
}

#[test]
fn add_applies_default_generators() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(&config, &root, fields(&[("title", "Test Defaults")])).unwrap();
    let content = fs::read_to_string(&outcome.path).unwrap();

    // $today should produce a YYYY-MM-DD date.
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert!(
        content.contains(&format!("created: '{today}'"))
            || content.contains(&format!("created: {today}"))
            || content.contains(&format!("created: \"{today}\""))
    );
}

// ── Slugification ───────────────────────────────────────────────────

#[test]
fn add_slugifies_title_correctly() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(&config, &root, fields(&[("title", "Fix Bug #123")])).unwrap();
    assert_eq!(
        outcome.path.file_name().unwrap().to_str().unwrap(),
        "fix-bug-123.md"
    );
}

#[test]
fn add_slugifies_spaces_and_symbols() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(&config, &root, fields(&[("title", "Hello, World!")])).unwrap();
    assert_eq!(
        outcome.path.file_name().unwrap().to_str().unwrap(),
        "hello-world.md"
    );
}

// ── Field overrides ─────────────────────────────────────────────────

#[test]
fn add_with_overrides() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(
        &config,
        &root,
        fields(&[
            ("title", "Login Bug"),
            ("type", "bug"),
            ("priority", "high"),
        ]),
    )
    .unwrap();
    let content = fs::read_to_string(&outcome.path).unwrap();

    assert!(content.contains("type: bug"));
    assert!(content.contains("priority: high"));
}

// ── Explicit id drives filename ─────────────────────────────────────

#[test]
fn add_with_explicit_id_uses_custom_filename() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(
        &config,
        &root,
        fields(&[("id", "custom-id"), ("title", "Some Title")]),
    )
    .unwrap();

    assert_eq!(
        outcome.path.file_name().unwrap().to_str().unwrap(),
        "custom-id.md"
    );
}

#[test]
fn add_with_only_id_no_title() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(&config, &root, fields(&[("id", "orphan-task")])).unwrap();

    assert_eq!(
        outcome.path.file_name().unwrap().to_str().unwrap(),
        "orphan-task.md"
    );
    let content = fs::read_to_string(&outcome.path).unwrap();
    // Title defaulted to $filename_pretty.
    assert!(content.contains("title: Orphan Task"));
}

#[test]
fn add_without_id_or_title_errors() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let result = run_add(&config, &root, HashMap::new());
    assert!(matches!(result, Err(AddError::MissingFilenameSource)));
}

// ── Duplicate detection ─────────────────────────────────────────────

#[test]
fn add_refuses_duplicate_filename() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    run_add(&config, &root, fields(&[("title", "Unique Task")])).unwrap();
    let result = run_add(&config, &root, fields(&[("title", "Unique Task")]));

    assert!(matches!(result, Err(AddError::AlreadyExists { .. })));
}

// ── Validation blocks creation ──────────────────────────────────────

#[test]
fn add_blocks_on_invalid_choice_value() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let result = run_add(
        &config,
        &root,
        fields(&[("title", "Bad Status"), ("status", "nonexistent")]),
    );

    assert!(matches!(result, Err(AddError::ValidationFailed { .. })));

    // File should NOT have been created.
    assert!(!root.join("workdown-items/bad-status.md").exists());
}

// ── Schema field ordering ───────────────────────────────────────────

#[test]
fn add_frontmatter_follows_schema_order() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(
        &config,
        &root,
        fields(&[("title", "Ordered Fields"), ("priority", "high")]),
    )
    .unwrap();
    let content = fs::read_to_string(&outcome.path).unwrap();

    // Fields should appear in schema order: title, type, status, priority, created
    let title_position = content.find("title:").unwrap();
    let type_position = content.find("type:").unwrap();
    let status_position = content.find("status:").unwrap();
    let priority_position = content.find("priority:").unwrap();
    let created_position = content.find("created:").unwrap();

    assert!(title_position < type_position);
    assert!(type_position < status_position);
    assert!(status_position < priority_position);
    assert!(priority_position < created_position);
}

// ── Rules warn but don't block ──────────────────────────────────────

#[test]
fn add_returns_rule_warnings_without_blocking() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    // Create a bug without priority — violates "bugs-need-priority" rule.
    let outcome = run_add(
        &config,
        &root,
        fields(&[("title", "Missing Priority Bug"), ("type", "bug")]),
    )
    .unwrap();

    // File should still be created.
    assert!(outcome.path.exists());

    // But we should get a warning about the rule violation.
    assert!(
        !outcome.warnings.is_empty(),
        "expected rule warnings but got none"
    );
    let warning_text = format!("{:?}", outcome.warnings);
    assert!(
        warning_text.contains("bugs-need-priority"),
        "expected bugs-need-priority warning, got: {warning_text}"
    );
}

#[test]
fn add_in_progress_without_assignee_warns() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(
        &config,
        &root,
        fields(&[("title", "No Assignee Task"), ("status", "in_progress")]),
    )
    .unwrap();

    assert!(outcome.path.exists());
    assert!(
        !outcome.warnings.is_empty(),
        "expected rule warnings but got none"
    );
    let warning_text = format!("{:?}", outcome.warnings);
    assert!(
        warning_text.contains("in-progress-needs-assignee"),
        "expected in-progress-needs-assignee warning, got: {warning_text}"
    );
}

// ── No warnings when rules are satisfied ────────────────────────────

#[test]
fn add_no_warnings_when_rules_satisfied() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(
        &config,
        &root,
        fields(&[
            ("title", "Good Bug"),
            ("type", "bug"),
            ("priority", "high"),
        ]),
    )
    .unwrap();

    assert!(outcome.path.exists());
    assert!(
        outcome.warnings.is_empty(),
        "expected no warnings, got: {:?}",
        outcome.warnings
    );
}
