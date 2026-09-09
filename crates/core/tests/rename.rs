//! Integration tests for `workdown rename`.
//!
//! Each test builds a throwaway project, renames one item through the
//! public operation, and asserts on the outcome and the files on disk.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use workdown_core::model::config::Config;
use workdown_core::model::WorkItemId;
use workdown_core::operations::rename::{
    run_rename, RenameError, RenameOptions, RenameOutcome, TextualMatchKind,
};
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
  depends_on:
    type: links
    required: false
    allow_cycles: false
    inverse: dependents
  related_to:
    type: links
    required: false
    allow_cycles: true
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

fn write_item(root: &Path, filename_stem: &str, content: &str) {
    fs::write(
        root.join(format!("workdown-items/{filename_stem}.md")),
        content,
    )
    .unwrap();
}

fn read_item(root: &Path, filename_stem: &str) -> String {
    fs::read_to_string(root.join(format!("workdown-items/{filename_stem}.md"))).unwrap()
}

fn item_exists(root: &Path, filename_stem: &str) -> bool {
    root.join(format!("workdown-items/{filename_stem}.md"))
        .exists()
}

fn run(
    config: &Config,
    root: &Path,
    old_id: &str,
    new_id: &str,
) -> Result<RenameOutcome, RenameError> {
    run_rename(
        config,
        root,
        &WorkItemId::from(old_id.to_owned()),
        &WorkItemId::from(new_id.to_owned()),
        RenameOptions::default(),
    )
}

fn run_dry(
    config: &Config,
    root: &Path,
    old_id: &str,
    new_id: &str,
) -> Result<RenameOutcome, RenameError> {
    run_rename(
        config,
        root,
        &WorkItemId::from(old_id.to_owned()),
        &WorkItemId::from(new_id.to_owned()),
        RenameOptions { dry_run: true },
    )
}

/// True iff any line in `text` contains `id` as a standalone token —
/// reuses the same boundary logic the textual scan uses, so the
/// "no stale references" assertions in these tests speak the same
/// language as the production code they're checking.
fn has_standalone_id(text: &str, id: &str) -> bool {
    text.lines().any(|line| line_contains_id(line, id))
}

// ── Validation (no disk changes) ─────────────────────────────────

#[test]
fn same_id_errors() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");

    let result = run(&config, &root, "task-1", "task-1");
    assert!(matches!(result, Err(RenameError::SameId { .. })));
    assert!(item_exists(&root, "task-1"));
}

#[test]
fn invalid_new_id_errors() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");

    for bad in ["Foo", "-bad", "bad-", "has space", "snake_case"] {
        let result = run(&config, &root, "task-1", bad);
        assert!(
            matches!(result, Err(RenameError::InvalidNewId { .. })),
            "expected InvalidNewId for '{bad}', got {result:?}"
        );
    }
    assert!(item_exists(&root, "task-1"));
}

#[test]
fn unknown_old_id_errors() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");

    let result = run(&config, &root, "ghost", "ghost-renamed");
    assert!(matches!(result, Err(RenameError::UnknownItem { .. })));
}

#[test]
fn id_already_exists_in_store_errors() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    write_item(&root, "task-2", "---\ntitle: Two\nstatus: open\n---\n");

    let result = run(&config, &root, "task-1", "task-2");
    assert!(matches!(result, Err(RenameError::IdAlreadyExists { .. })));
    assert!(item_exists(&root, "task-1"));
    assert!(item_exists(&root, "task-2"));
}

#[test]
fn file_already_exists_on_disk_shadow_errors() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    // Unparseable shadow file at the target name — won't enter the
    // store, but it occupies the path. Must error distinctly from
    // IdAlreadyExists.
    fs::write(
        root.join("workdown-items/task-1-renamed.md"),
        "not a work item\n",
    )
    .unwrap();

    let result = run(&config, &root, "task-1", "task-1-renamed");
    assert!(
        matches!(result, Err(RenameError::FileAlreadyExists { .. })),
        "got {result:?}"
    );
    assert!(item_exists(&root, "task-1"));
}

// ── Happy path ───────────────────────────────────────────────────

#[test]
fn rewrites_parent_referrer_and_moves_file() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: One\nstatus: open\n---\noriginal body\n",
    );
    write_item(
        &root,
        "task-2",
        "---\ntitle: Two\nstatus: open\nparent: task-1\n---\nchild body\n",
    );

    let outcome = run(&config, &root, "task-1", "task-1-renamed").unwrap();

    assert!(item_exists(&root, "task-1-renamed"));
    assert!(!item_exists(&root, "task-1"));

    let referrer = read_item(&root, "task-2");
    assert!(referrer.contains("parent: task-1-renamed"));
    assert!(!has_standalone_id(&referrer, "task-1"));
    assert!(referrer.contains("child body"));

    let renamed = read_item(&root, "task-1-renamed");
    assert!(renamed.contains("original body"));

    assert_eq!(outcome.old_id.as_str(), "task-1");
    assert_eq!(outcome.new_id.as_str(), "task-1-renamed");
    assert!(!outcome.mutation_caused_warning);
    assert!(!outcome.dry_run);
    // Referrer rewrite + renamed file move.
    assert_eq!(outcome.rewritten_files.len(), 2);
}

#[test]
fn no_referrers_just_moves_file() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\ntitle: One\nstatus: open\n---\nthe body\n",
    );

    let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
    assert!(item_exists(&root, "renamed-one"));
    assert!(!item_exists(&root, "task-1"));

    // Only the renamed file appears, with no field rewrites.
    assert_eq!(outcome.rewritten_files.len(), 1);
    assert!(outcome.rewritten_files[0].field_rewrites.is_empty());
}

#[test]
fn rewrites_links_field_with_multiple_targets() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    write_item(&root, "task-x", "---\ntitle: X\nstatus: open\n---\n");
    write_item(
        &root,
        "task-2",
        "---\ntitle: Two\nstatus: open\ndepends_on: [task-1, task-x]\n---\n",
    );

    run(&config, &root, "task-1", "task-1-renamed").unwrap();

    let referrer = read_item(&root, "task-2");
    assert!(referrer.contains("task-1-renamed"));
    assert!(referrer.contains("task-x"));
    // No stray standalone "task-1" (the sibling target survives as
    // "task-x", which is a different token).
    assert!(!has_standalone_id(&referrer, "task-1"));
}

#[test]
fn rewrites_user_defined_link_field() {
    // `related_to` is a Links field outside the default parent/depends_on set.
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    write_item(
        &root,
        "task-2",
        "---\ntitle: Two\nstatus: open\nrelated_to: [task-1]\n---\n",
    );

    run(&config, &root, "task-1", "renamed-one").unwrap();
    let referrer = read_item(&root, "task-2");
    assert!(referrer.contains("renamed-one"));
    assert!(!has_standalone_id(&referrer, "task-1"));
}

#[test]
fn self_link_rewritten_inline_with_move() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    // related_to allows cycles, so self-link is legal.
    write_item(
        &root,
        "task-1",
        "---\ntitle: One\nstatus: open\nrelated_to: [task-1]\n---\nbody\n",
    );

    let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
    assert!(item_exists(&root, "renamed-one"));
    assert!(!item_exists(&root, "task-1"));

    let renamed = read_item(&root, "renamed-one");
    assert!(renamed.contains("renamed-one"));
    assert!(!has_standalone_id(&renamed, "task-1"));

    // Renamed file is in rewritten_files with its self-link rewrite.
    assert_eq!(outcome.rewritten_files.len(), 1);
    assert_eq!(outcome.rewritten_files[0].field_rewrites.len(), 1);
    assert_eq!(
        outcome.rewritten_files[0].field_rewrites[0].field,
        "related_to"
    );
}

#[test]
fn explicit_id_key_dropped_after_rename() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(
        &root,
        "task-1",
        "---\nid: task-1\ntitle: One\nstatus: open\n---\n",
    );

    run(&config, &root, "task-1", "renamed-one").unwrap();
    let renamed = read_item(&root, "renamed-one");
    assert!(
        !renamed.contains("id:"),
        "expected `id:` key to be removed, got:\n{renamed}"
    );
    assert!(renamed.contains("title: One"));
}

#[test]
fn filename_not_equal_id_renames_to_new_filename() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    // File "whatever.md" carries `id: foo` — filename diverges from id.
    fs::write(
        root.join("workdown-items/whatever.md"),
        "---\nid: foo\ntitle: F\nstatus: open\n---\nbody\n",
    )
    .unwrap();

    run(&config, &root, "foo", "bar").unwrap();

    assert!(item_exists(&root, "bar"));
    assert!(!root.join("workdown-items/whatever.md").exists());
    let renamed = read_item(&root, "bar");
    assert!(!renamed.contains("id:"));
    assert!(renamed.contains("title: F"));
    assert!(renamed.contains("body"));
}

// ── Dry run ──────────────────────────────────────────────────────

#[test]
fn dry_run_returns_plan_without_writing() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    write_item(
        &root,
        "task-2",
        "---\ntitle: Two\nstatus: open\nparent: task-1\n---\n",
    );

    let outcome = run_dry(&config, &root, "task-1", "task-1-renamed").unwrap();
    assert!(outcome.dry_run);
    assert_eq!(outcome.rewritten_files.len(), 2);

    // Disk untouched.
    assert!(item_exists(&root, "task-1"));
    assert!(!item_exists(&root, "task-1-renamed"));
    let untouched = read_item(&root, "task-2");
    assert!(untouched.contains("parent: task-1"));
    assert!(!untouched.contains("task-1-renamed"));
}

// ── Textual scan ─────────────────────────────────────────────────

#[test]
fn body_prose_match_reported_but_not_rewritten() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    write_item(
        &root,
        "note-1",
        "---\ntitle: Note\nstatus: open\n---\nsee task-1 for context\n",
    );

    let outcome = run(&config, &root, "task-1", "task-1-renamed").unwrap();

    let body_matches: Vec<_> = outcome
        .textual_matches
        .iter()
        .filter(|m| m.kind == TextualMatchKind::ItemBody)
        .collect();
    assert_eq!(body_matches.len(), 1);

    // Body unchanged.
    let note = read_item(&root, "note-1");
    assert!(note.contains("see task-1 for context"));
}

#[test]
fn body_match_respects_kebab_boundary() {
    // The body contains "task-1-renamed", which has "task-1" as a
    // prefix. The boundary check must reject this — otherwise the
    // user gets a false positive on every successor id.
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    write_item(
        &root,
        "note-1",
        "---\ntitle: Note\nstatus: open\n---\ndo NOT match task-1-renamed text\n",
    );

    let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
    let body_matches: Vec<_> = outcome
        .textual_matches
        .iter()
        .filter(|m| m.kind == TextualMatchKind::ItemBody)
        .collect();
    assert!(
        body_matches.is_empty(),
        "expected no body matches, got {body_matches:?}"
    );
}

#[test]
fn standalone_match_at_various_positions() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    write_item(
        &root,
        "note-1",
        "---\ntitle: Note\nstatus: open\n---\n\
             task-1\n\
             blah task-1\n\
             foo task-1 bar\n\
             not-task-1-no\n",
    );

    let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
    let body_matches: Vec<_> = outcome
        .textual_matches
        .iter()
        .filter(|m| m.kind == TextualMatchKind::ItemBody)
        .collect();
    // First three lines match as standalone tokens. Fourth has the
    // id surrounded by hyphens, so the boundary check rejects it.
    assert_eq!(body_matches.len(), 3);
}

#[test]
fn views_yaml_textual_match_reported() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    fs::write(
        root.join(".workdown/views.yaml"),
        "views:\n  - id: pin\n    note: task-1 is the anchor\n",
    )
    .unwrap();

    let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
    let view_matches: Vec<_> = outcome
        .textual_matches
        .iter()
        .filter(|m| m.kind == TextualMatchKind::Views)
        .collect();
    assert_eq!(view_matches.len(), 1);
}

#[test]
fn unparseable_item_scanned_whole_file() {
    let (_dir, root) = setup_project();
    let config = load_test_config(&root);
    write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
    // Missing closing delimiter — parser fails on this file.
    fs::write(
        root.join("workdown-items/broken.md"),
        "---\nparent: task-1\nbroken file no closing\n",
    )
    .unwrap();

    let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
    let unparseable: Vec<_> = outcome
        .textual_matches
        .iter()
        .filter(|m| m.kind == TextualMatchKind::UnparseableItem)
        .collect();
    assert_eq!(unparseable.len(), 1);
}

/// Test-local copy of the boundary rule `rename` uses for its textual
/// scan (`line_contains_id` in `operations/rename.rs`, whose own shapes
/// are pinned by the unit test beside it). Kept here so the "no stale
/// references" assertions above speak the same language as the
/// production scan without reaching into a private function.
fn line_contains_id(line: &str, id: &str) -> bool {
    let is_boundary = |character: Option<char>| match character {
        None => true,
        Some(character) => {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
        }
    };
    let mut search_start = 0;
    while let Some(position) = line[search_start..].find(id) {
        let match_start = search_start + position;
        let match_end = match_start + id.len();
        let preceding = line[..match_start].chars().next_back();
        let following = line[match_end..].chars().next();
        if is_boundary(preceding) && is_boundary(following) {
            return true;
        }
        search_start = match_start + id.chars().next().map_or(1, char::len_utf8);
    }
    false
}
