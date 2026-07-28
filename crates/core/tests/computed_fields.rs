//! Integration tests for computed fields through the full project
//! loader: constants defined in `resources.yaml` reach the store's
//! derive pass, and a check-failed compute config surfaces as exactly
//! one schema diagnostic with no per-item noise.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use workdown_core::model::diagnostic::DiagnosticBody;
use workdown_core::model::FieldValue;
use workdown_core::parser::config::load_config;
use workdown_core::project::{load_project, Project};

// ── Test fixtures ───────────────────────────────────────────────────

const CONFIG_YAML: &str = "\
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

/// The fields the config's defaults reference, shared by every schema
/// fixture below.
const COMMON_FIELDS: &str = "\
fields:
  status:
    type: choice
    values: [open, done]
  parent:
    type: link
    allow_cycles: false
  depends_on:
    type: links
    allow_cycles: true
";

fn setup_project(
    schema_yaml: &str,
    resources_yaml: &str,
    items: &[(&str, &str)],
) -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();

    fs::create_dir_all(root.join(".workdown")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();

    fs::write(root.join(".workdown/config.yaml"), CONFIG_YAML).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), schema_yaml).unwrap();
    fs::write(root.join(".workdown/resources.yaml"), resources_yaml).unwrap();

    for (file_name, content) in items {
        fs::write(root.join("workdown-items").join(file_name), content).unwrap();
    }

    (directory, root)
}

fn load(root: &Path) -> Project {
    let config = load_config(&root.join(".workdown/config.yaml")).unwrap();
    load_project(&config, root, Path::new(".workdown/config.yaml")).unwrap()
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn constants_reach_computed_fields_through_the_project_loader() {
    let schema_yaml = format!(
        "{COMMON_FIELDS}  effort:
    type: duration
  cost:
    type: float
    compute: effort / $constants.work_hours_per_day * $constants.daily_rate
"
    );
    let resources_yaml = "\
constants:
  daily_rate:
    type: float
    value: 800
  work_hours_per_day:
    type: duration
    value: \"8h\"
";
    let (_directory, root) = setup_project(
        &schema_yaml,
        resources_yaml,
        &[("task.md", "---\neffort: 16h\n---\nBody.\n")],
    );

    let project = load(&root);

    assert!(
        project.diagnostics.is_empty(),
        "got: {:?}",
        project.diagnostics
    );
    // 16h of effort at 8h per day and a daily rate of 800 costs 1600.
    let task = project.store.get("task").expect("task must load");
    assert_eq!(task.fields.get("cost"), Some(&FieldValue::Float(1600.0)));
}

#[test]
fn check_failed_compute_is_one_schema_diagnostic_without_item_noise() {
    // The typo'd reference must surface once, against schema.yaml —
    // not per item, despite `error_on_missing: true`.
    let schema_yaml = format!(
        "{COMMON_FIELDS}  start_date:
    type: date
  end_date:
    type: date
    compute:
      expression: strat_date + duration
      error_on_missing: true
"
    );
    let (_directory, root) = setup_project(
        &schema_yaml,
        "",
        &[
            ("task-a.md", "---\nstart_date: 2026-01-05\n---\n"),
            ("task-b.md", "---\nstart_date: 2026-01-19\n---\n"),
        ],
    );

    let project = load(&root);

    assert_eq!(
        project.diagnostics.len(),
        1,
        "got: {:?}",
        project.diagnostics
    );
    assert!(
        matches!(project.diagnostics[0].body, DiagnosticBody::Config(_)),
        "got: {:?}",
        project.diagnostics[0]
    );
    for id in ["task-a", "task-b"] {
        let item = project.store.get(id).expect("item must load");
        assert_eq!(item.fields.get("end_date"), None);
    }
}
