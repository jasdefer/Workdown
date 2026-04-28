//! Integration tests for `workdown add`.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use workdown_core::operations::add::{run_add, AddError};
use workdown_core::parser::config::load_config;

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
  tags:
    type: list
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
  views: .workdown/views.yaml
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

    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();

    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), TEST_SCHEMA).unwrap();

    (directory, root)
}

/// Write a template file under `.workdown/templates/<name>.md`.
fn write_template(root: &PathBuf, name: &str, content: &str) {
    fs::write(root.join(format!(".workdown/templates/{name}.md")), content).unwrap();
}

fn load_test_config(root: &PathBuf) -> workdown_core::model::config::Config {
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

    let outcome = run_add(&config, &root, fields(&[("title", "My First Task")]), None).unwrap();

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

    let outcome = run_add(&config, &root, fields(&[("title", "Test Defaults")]), None).unwrap();
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

    let outcome = run_add(&config, &root, fields(&[("title", "Fix Bug #123")]), None).unwrap();
    assert_eq!(
        outcome.path.file_name().unwrap().to_str().unwrap(),
        "fix-bug-123.md"
    );
}

#[test]
fn add_slugifies_spaces_and_symbols() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(&config, &root, fields(&[("title", "Hello, World!")]), None).unwrap();
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
        None,
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
        None,
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

    let outcome = run_add(&config, &root, fields(&[("id", "orphan-task")]), None).unwrap();

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

    let result = run_add(&config, &root, HashMap::new(), None);
    assert!(matches!(result, Err(AddError::MissingFilenameSource)));
}

// ── Duplicate detection ─────────────────────────────────────────────

#[test]
fn add_refuses_duplicate_filename() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    run_add(&config, &root, fields(&[("title", "Unique Task")]), None).unwrap();
    let result = run_add(&config, &root, fields(&[("title", "Unique Task")]), None);

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
        None,
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
        None,
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
        None,
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
        None,
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
        fields(&[("title", "Good Bug"), ("type", "bug"), ("priority", "high")]),
        None,
    )
    .unwrap();

    assert!(outcome.path.exists());
    assert!(
        outcome.warnings.is_empty(),
        "expected no warnings, got: {:?}",
        outcome.warnings
    );
}

// ── Templates ───────────────────────────────────────────────────────

#[test]
fn add_with_template_uses_frontmatter_and_body() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    write_template(
        &root,
        "bug",
        "---\ntype: bug\npriority: medium\n---\n\n## Steps\n1. repro\n",
    );

    let outcome = run_add(
        &config,
        &root,
        fields(&[("title", "Login Crash")]),
        Some("bug"),
    )
    .unwrap();

    let content = fs::read_to_string(&outcome.path).unwrap();
    assert!(content.contains("type: bug"));
    assert!(content.contains("priority: medium"));
    assert!(content.contains("title: Login Crash"));
    assert!(content.contains("## Steps"));
    assert!(content.contains("1. repro"));
}

#[test]
fn add_cli_overrides_template_field() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    write_template(&root, "bug", "---\ntype: bug\npriority: medium\n---\n");

    let outcome = run_add(
        &config,
        &root,
        fields(&[("title", "Login Crash"), ("priority", "critical")]),
        Some("bug"),
    )
    .unwrap();

    let content = fs::read_to_string(&outcome.path).unwrap();
    assert!(content.contains("priority: critical"));
    assert!(!content.contains("priority: medium"));
}

#[test]
fn add_template_resolves_tokens() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    write_template(
        &root,
        "dated",
        "---\ncreated: $today\nassignee: $uuid\n---\n",
    );

    let outcome = run_add(
        &config,
        &root,
        fields(&[("title", "Token Test")]),
        Some("dated"),
    )
    .unwrap();

    let content = fs::read_to_string(&outcome.path).unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert!(content.contains(&today), "file should contain today's date");
    // UUIDs are 36 chars; the `$uuid` literal should not survive.
    assert!(!content.contains("$uuid"));
    assert!(!content.contains("$today"));
}

#[test]
fn add_template_token_inside_list() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    // Template sets tags: [urgent, $today]. We need a quoted YAML string
    // because `$today` is not valid YAML without quoting (it would be
    // parsed as a literal string anyway — quote to be explicit).
    write_template(&root, "tagged", "---\ntags:\n  - urgent\n  - $today\n---\n");

    let outcome = run_add(
        &config,
        &root,
        fields(&[("title", "Tag Test")]),
        Some("tagged"),
    )
    .unwrap();

    let content = fs::read_to_string(&outcome.path).unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    assert!(content.contains("urgent"));
    assert!(content.contains(&today));
    assert!(!content.contains("$today"));
}

#[test]
fn add_template_near_miss_token_stays_literal() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    // `before $today` is NOT an exact token match; must stay literal.
    write_template(&root, "literal", "---\ntitle: before $today\n---\n");

    let outcome = run_add(
        &config,
        &root,
        fields(&[("id", "keeps-literal")]),
        Some("literal"),
    )
    .unwrap();

    let content = fs::read_to_string(&outcome.path).unwrap();
    assert!(content.contains("before $today"));
}

#[test]
fn add_template_sets_id() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    write_template(&root, "uuid-id", "---\nid: $uuid\ntype: task\n---\n");

    let outcome = run_add(&config, &root, fields(&[]), Some("uuid-id")).unwrap();

    // Filename is a UUID (36 chars) + `.md`.
    let filename = outcome.path.file_name().unwrap().to_str().unwrap();
    assert_eq!(filename.len(), 36 + 3);
    assert!(filename.ends_with(".md"));
    assert!(outcome.path.exists());
}

#[test]
fn add_cli_id_overrides_template_id() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    write_template(&root, "preset", "---\nid: from-template\ntype: task\n---\n");

    let outcome = run_add(
        &config,
        &root,
        fields(&[("id", "from-cli")]),
        Some("preset"),
    )
    .unwrap();

    assert_eq!(
        outcome.path.file_name().unwrap().to_str().unwrap(),
        "from-cli.md"
    );
}

#[test]
fn add_template_missing_returns_error() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let result = run_add(
        &config,
        &root,
        fields(&[("title", "Ghost")]),
        Some("nonexistent"),
    );

    assert!(matches!(
        result,
        Err(AddError::Template(
            workdown_core::model::template::TemplateError::NotFound { .. }
        ))
    ));
}

#[test]
fn add_template_unknown_field_passes_through() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    write_template(
        &root,
        "extras",
        "---\ntype: task\ncustom_field: hello\n---\n",
    );

    let outcome = run_add(
        &config,
        &root,
        fields(&[("title", "Extras")]),
        Some("extras"),
    )
    .unwrap();

    let content = fs::read_to_string(&outcome.path).unwrap();
    assert!(content.contains("custom_field: hello"));
}

#[test]
fn add_without_template_still_has_empty_body() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);

    let outcome = run_add(&config, &root, fields(&[("title", "No Template")]), None).unwrap();

    let content = fs::read_to_string(&outcome.path).unwrap();
    // The body is empty — content ends with the closing delimiter and a newline.
    assert!(content.ends_with("---\n"));
}

// ── Aggregate rollup at add time ────────────────────────────────────

const AGGREGATE_SCHEMA: &str = "\
fields:
  title:
    type: string
    default: $filename_pretty
  parent:
    type: link
    allow_cycles: false
    inverse: children
  effort:
    type: integer
    aggregate:
      function: sum
";

/// Set up a project whose schema declares an `effort` field aggregated
/// upward via `parent`. Used to exercise add-time chain-conflict warnings.
fn setup_aggregate_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();

    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();

    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), AGGREGATE_SCHEMA).unwrap();

    (directory, root)
}

#[test]
fn add_warns_when_new_item_creates_aggregate_chain_conflict() {
    let (_directory, root) = setup_aggregate_project();
    let config = load_test_config(&root);

    // Existing parent already has an `effort` value set manually.
    fs::write(
        root.join("workdown-items/epic.md"),
        "---\ntitle: Epic\neffort: 10\n---\n",
    )
    .unwrap();

    // Adding a child with its own manual `effort` creates a chain conflict.
    let mut field_values: HashMap<String, serde_yaml::Value> = HashMap::new();
    field_values.insert(
        "title".to_owned(),
        serde_yaml::Value::String("Conflicting Child".to_owned()),
    );
    field_values.insert(
        "parent".to_owned(),
        serde_yaml::Value::String("epic".to_owned()),
    );
    field_values.insert("effort".to_owned(), serde_yaml::Value::Number(4.into()));

    let outcome = run_add(&config, &root, field_values, None).unwrap();

    assert!(outcome.path.exists());
    let warning_text = format!("{:?}", outcome.warnings);
    assert!(
        warning_text.contains("AggregateChainConflict") && warning_text.contains("epic"),
        "expected chain-conflict warning naming epic, got: {warning_text}"
    );
}

#[test]
fn add_no_aggregate_warning_when_parent_has_no_value() {
    let (_directory, root) = setup_aggregate_project();
    let config = load_test_config(&root);

    fs::write(
        root.join("workdown-items/epic.md"),
        "---\ntitle: Epic\n---\n",
    )
    .unwrap();

    let mut field_values: HashMap<String, serde_yaml::Value> = HashMap::new();
    field_values.insert(
        "title".to_owned(),
        serde_yaml::Value::String("Clean Child".to_owned()),
    );
    field_values.insert(
        "parent".to_owned(),
        serde_yaml::Value::String("epic".to_owned()),
    );
    field_values.insert("effort".to_owned(), serde_yaml::Value::Number(7.into()));

    let outcome = run_add(&config, &root, field_values, None).unwrap();

    assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
}
