//! Integration tests for `workdown add`.

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

// ── Happy path ──────────────────────────────────────────────────────

#[test]
fn add_creates_work_item_file() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(&config, &root, "My First Task", &[]).unwrap();

    assert!(outcome.path.exists());
    assert_eq!(
        outcome.path,
        root.join("workdown-items/my-first-task.md")
    );

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

    let outcome = run_add(&config, &root, "Test Defaults", &[]).unwrap();
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

    let outcome = run_add(&config, &root, "Fix Bug #123", &[]).unwrap();
    assert_eq!(
        outcome.path.file_name().unwrap().to_str().unwrap(),
        "fix-bug-123.md"
    );
}

#[test]
fn add_slugifies_spaces_and_symbols() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(&config, &root, "Hello, World!", &[]).unwrap();
    assert_eq!(
        outcome.path.file_name().unwrap().to_str().unwrap(),
        "hello-world.md"
    );
}

// ── --set overrides ─────────────────────────────────────────────────

#[test]
fn add_with_set_overrides() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let set_flags = vec![
        "type=bug".to_owned(),
        "priority=high".to_owned(),
    ];
    let outcome = run_add(&config, &root, "Login Bug", &set_flags).unwrap();
    let content = fs::read_to_string(&outcome.path).unwrap();

    assert!(content.contains("type: bug"));
    assert!(content.contains("priority: high"));
}

#[test]
fn add_set_title_overrides_positional() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let set_flags = vec!["title=Overridden Title".to_owned()];
    let outcome = run_add(&config, &root, "Original Title", &set_flags).unwrap();
    let content = fs::read_to_string(&outcome.path).unwrap();

    assert!(content.contains("title: Overridden Title"));
}

// ── --set id overrides slug ─────────────────────────────────────────

#[test]
fn add_set_id_uses_custom_filename() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let set_flags = vec!["id=custom-id".to_owned()];
    let outcome = run_add(&config, &root, "Some Title", &set_flags).unwrap();

    assert_eq!(
        outcome.path.file_name().unwrap().to_str().unwrap(),
        "custom-id.md"
    );
}

// ── Duplicate detection ─────────────────────────────────────────────

#[test]
fn add_refuses_duplicate_filename() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    run_add(&config, &root, "Unique Task", &[]).unwrap();
    let result = run_add(&config, &root, "Unique Task", &[]);

    assert!(matches!(result, Err(AddError::AlreadyExists { .. })));
}

// ── Validation blocks creation ──────────────────────────────────────

#[test]
fn add_blocks_on_invalid_choice_value() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let set_flags = vec!["status=nonexistent".to_owned()];
    let result = run_add(&config, &root, "Bad Status", &set_flags);

    assert!(matches!(result, Err(AddError::ValidationFailed { .. })));

    // File should NOT have been created.
    assert!(!root.join("workdown-items/bad-status.md").exists());
}

// ── Schema field ordering ───────────────────────────────────────────

#[test]
fn add_frontmatter_follows_schema_order() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let set_flags = vec!["priority=high".to_owned()];
    let outcome = run_add(&config, &root, "Ordered Fields", &set_flags).unwrap();
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
    let set_flags = vec!["type=bug".to_owned()];
    let outcome = run_add(&config, &root, "Missing Priority Bug", &set_flags).unwrap();

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

    let set_flags = vec!["status=in_progress".to_owned()];
    let outcome = run_add(&config, &root, "No Assignee Task", &set_flags).unwrap();

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

    let set_flags = vec![
        "type=bug".to_owned(),
        "priority=high".to_owned(),
    ];
    let outcome = run_add(&config, &root, "Good Bug", &set_flags).unwrap();

    assert!(outcome.path.exists());
    assert!(
        outcome.warnings.is_empty(),
        "expected no warnings, got: {:?}",
        outcome.warnings
    );
}
