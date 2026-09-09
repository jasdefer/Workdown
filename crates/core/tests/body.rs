//! Integration tests for `workdown body`.
//!
//! Each test builds a throwaway project, replaces one item's body through
//! the public operation, and asserts on the outcome and the file on disk.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use workdown_core::model::config::Config;
use workdown_core::model::WorkItemId;
use workdown_core::operations::body::{run_body_replace, BodyError};
use workdown_core::parser::config::load_config;

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
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
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

fn setup_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), TEST_SCHEMA).unwrap();
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

// ── Happy path ───────────────────────────────────────────────────

#[test]
fn replaces_body_text() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
    );

    let outcome = run_body_replace(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "New body content.".to_owned(),
    )
    .unwrap();

    assert_eq!(outcome.previous_body, "Old body.\n");
    assert_eq!(outcome.new_body, "New body content.\n");
    assert_eq!(
        read_item(&root, "task-1"),
        "---\ntitle: Task one\nstatus: open\n---\nNew body content.\n"
    );
}

#[test]
fn empty_body_clears_body() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
    );

    let outcome = run_body_replace(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        String::new(),
    )
    .unwrap();

    assert_eq!(outcome.new_body, "");
    // File ends right after the closing `---\n` — no extra trailing newline.
    assert_eq!(
        read_item(&root, "task-1"),
        "---\ntitle: Task one\nstatus: open\n---\n"
    );
}

#[test]
fn trims_multiple_trailing_newlines_to_one() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
    );

    let outcome = run_body_replace(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "hello\n\n\n".to_owned(),
    )
    .unwrap();

    assert_eq!(outcome.new_body, "hello\n");
}

#[test]
fn adds_trailing_newline_when_missing() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
    );

    let outcome = run_body_replace(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "hello".to_owned(),
    )
    .unwrap();

    assert_eq!(outcome.new_body, "hello\n");
}

#[test]
fn strips_crlf_trailing_whitespace() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
    );

    let outcome = run_body_replace(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "hello\r\n\r\n".to_owned(),
    )
    .unwrap();

    assert_eq!(outcome.new_body, "hello\n");
}

#[test]
fn whitespace_only_body_clears_body() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
    );

    let outcome = run_body_replace(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "\n\n\r\n".to_owned(),
    )
    .unwrap();

    assert_eq!(outcome.new_body, "");
    assert_eq!(
        read_item(&root, "task-1"),
        "---\ntitle: Task one\nstatus: open\n---\n"
    );
}

#[test]
fn preserves_multiline_body() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: Task one\nstatus: open\n---\n");

    let new_body = "Line one.\n\nLine three after a blank line.\n\n## Heading\n\nMore text.";

    let outcome = run_body_replace(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        new_body.to_owned(),
    )
    .unwrap();

    assert_eq!(
        outcome.new_body,
        "Line one.\n\nLine three after a blank line.\n\n## Heading\n\nMore text.\n"
    );
    let on_disk = read_item(&root, "task-1");
    assert!(on_disk.contains("\n\nLine three after a blank line.\n\n"));
}

// ── Frontmatter preservation ─────────────────────────────────────

#[test]
fn frontmatter_bytes_are_preserved_verbatim() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    // Hand-edited frontmatter with quirky-but-valid formatting:
    // out-of-schema-order fields, an inline comment, extra blank
    // lines inside the frontmatter, and an explicit `id:` field.
    let original = "---\nstatus: open\nid: task-1\ntitle: Task one  # important\n\n\n---\nBody.\n";
    write_item(&root, "task-1", original);

    run_body_replace(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "Replaced.".to_owned(),
    )
    .unwrap();

    let after = read_item(&root, "task-1");
    // The bytes from start through the closing `---\n` must be
    // byte-identical to the original — only the body changes.
    let original_through_close =
        "---\nstatus: open\nid: task-1\ntitle: Task one  # important\n\n\n---\n";
    assert!(
        after.starts_with(original_through_close),
        "frontmatter bytes diverged.\nexpected prefix: {original_through_close:?}\nafter:           {after:?}"
    );
    assert_eq!(after, format!("{original_through_close}Replaced.\n"));
}

// ── Errors ───────────────────────────────────────────────────────

#[test]
fn unknown_id_errors_and_writes_nothing() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
    );
    let before = read_item(&root, "task-1");

    let result = run_body_replace(
        &config,
        &root,
        &WorkItemId::from("does-not-exist".to_owned()),
        "Whatever.".to_owned(),
    );

    assert!(matches!(result, Err(BodyError::UnknownItem { .. })));
    // The existing item is untouched.
    assert_eq!(read_item(&root, "task-1"), before);
}

// ── Diagnostics ──────────────────────────────────────────────────

#[test]
fn surfaces_preexisting_warnings_from_other_items() {
    let (_directory, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
    );
    // Second item with a broken `parent` link — a pre-existing warning
    // that has nothing to do with our mutation.
    write_item(
        &root,
        "task-2",
        "---\ntitle: Task two\nstatus: open\nparent: ghost-item\n---\n",
    );

    let outcome = run_body_replace(
        &config,
        &root,
        &WorkItemId::from("task-1".to_owned()),
        "Replaced.".to_owned(),
    )
    .unwrap();

    assert!(
        !outcome.warnings.is_empty(),
        "expected the pre-existing broken-link warning on task-2 to surface"
    );
}
