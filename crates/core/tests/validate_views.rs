//! Integration tests for views.yaml validation in `operations::validate::validate`.
//!
//! These tests focus on the **wiring**: that `validate` loads `views.yaml`,
//! runs `views_check`, routes parse errors through the diagnostic pipeline,
//! and silently skips when the file is absent. Content-level correctness of
//! individual checks is covered by unit tests in `crates/core/src/views_check.rs`.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use workdown_core::model::config::Config;
use workdown_core::model::diagnostic::{ConfigDiagnosticKind, Diagnostic, DiagnosticBody};
use workdown_core::operations::validate::validate;
use workdown_core::parser::config::load_config;

// ── Fixture helper ──────────────────────────────────────────────────────

/// Stage a minimal project in a tempdir: config.yaml, schema.yaml, an
/// optional views.yaml, and work items. Returns the handles needed to
/// call `validate(&config, &project_root)`.
fn setup_project(
    schema_yaml: &str,
    views_yaml: Option<&str>,
    items: &[(&str, &str)],
) -> (TempDir, Config, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();

    fs::create_dir_all(root.join(".workdown")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();

    let config_yaml = "\
project:
  name: Test
  description: \"\"
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
    fs::write(root.join(".workdown/config.yaml"), config_yaml).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), schema_yaml).unwrap();
    if let Some(yaml) = views_yaml {
        fs::write(root.join(".workdown/views.yaml"), yaml).unwrap();
    }
    for (name, content) in items {
        fs::write(root.join("workdown-items").join(name), content).unwrap();
    }

    let config = load_config(&root.join(".workdown/config.yaml")).unwrap();
    (tmp, config, root)
}

/// True iff the diagnostic is config-scoped (every view-level diagnostic is).
fn is_view_diagnostic(diagnostic: &Diagnostic) -> bool {
    matches!(diagnostic.body, DiagnosticBody::Config(_))
}

/// Shared schema: `status` (choice) so a valid board view can reference it.
fn schema_with_status() -> &'static str {
    "\
fields:
  status:
    type: choice
    values: [open, done]
"
}

fn run_validate(project: &(TempDir, Config, PathBuf)) -> Vec<Diagnostic> {
    let (_tmp, config, root) = project;
    validate(config, root.as_path()).unwrap().diagnostics
}

fn view_diagnostics(diagnostics: &[Diagnostic]) -> Vec<&Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| is_view_diagnostic(d))
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn views_yaml_present_and_valid_produces_no_view_diagnostics() {
    let views = "\
views:
  - id: status-board
    type: board
    field: status
";
    let project = setup_project(
        schema_with_status(),
        Some(views),
        &[("a.md", "---\nstatus: open\n---\n")],
    );

    let diagnostics = run_validate(&project);
    let view_level = view_diagnostics(&diagnostics);
    assert!(
        view_level.is_empty(),
        "expected no view diagnostics, got: {view_level:?}"
    );
}

#[test]
fn views_yaml_with_cross_file_error_surfaces_diagnostic() {
    // `nonexistent` isn't in the schema — evaluate() should emit ViewUnknownField.
    let views = "\
views:
  - id: bad-board
    type: board
    field: nonexistent
";
    let project = setup_project(
        schema_with_status(),
        Some(views),
        &[("a.md", "---\nstatus: open\n---\n")],
    );

    let diagnostics = run_validate(&project);
    let view_level = view_diagnostics(&diagnostics);
    assert_eq!(view_level.len(), 1, "got: {view_level:?}");
    assert!(matches!(
        &view_level[0].body,
        DiagnosticBody::Config(c)
            if matches!(
                &c.kind,
                ConfigDiagnosticKind::ViewUnknownField { view_id, field_name, .. }
                if view_id == "bad-board" && field_name == "nonexistent"
            )
    ));
}

#[test]
fn views_yaml_with_parse_error_surfaces_diagnostic() {
    // `type: board` without `field:` — parse_views() returns MissingSlot, which
    // parse_errors_to_diagnostics converts to ViewMissingSlot.
    let views = "\
views:
  - id: incomplete
    type: board
";
    let project = setup_project(
        schema_with_status(),
        Some(views),
        &[("a.md", "---\nstatus: open\n---\n")],
    );

    let diagnostics = run_validate(&project);
    let view_level = view_diagnostics(&diagnostics);
    assert_eq!(view_level.len(), 1, "got: {view_level:?}");
    assert!(matches!(
        &view_level[0].body,
        DiagnosticBody::Config(c)
            if matches!(
                &c.kind,
                ConfigDiagnosticKind::ViewMissingSlot { view_id, slot, .. }
                if view_id == "incomplete" && *slot == "field"
            )
    ));
}

#[test]
fn missing_views_yaml_is_silently_skipped() {
    let project = setup_project(
        schema_with_status(),
        None, // no views.yaml written
        &[("a.md", "---\nstatus: open\n---\n")],
    );

    // Sanity: file really isn't there.
    assert!(!Path::new(&project.2.join(".workdown/views.yaml")).exists());

    let diagnostics = run_validate(&project);
    let view_level = view_diagnostics(&diagnostics);
    assert!(
        view_level.is_empty(),
        "expected silent skip, got: {view_level:?}"
    );
}
