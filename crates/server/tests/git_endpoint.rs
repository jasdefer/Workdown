//! Integration tests for the git sync endpoints (`GET /api/git`,
//! `POST /api/git/pull`, `POST /api/git/push`).
//!
//! Every repo these tests touch is a throwaway: a bare "remote" in a
//! `TempDir` with one or two working clones next to it, so pull and
//! push exercise real git plumbing without any network. Drives the
//! router with `tower::ServiceExt::oneshot`, like the other endpoint
//! tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

use workdown_core::parser::config::parse_config;
use workdown_server::{router, AppState};

/// The project config, with or without the git-controls opt-in — one
/// base plus the flag lines, so the enabled/disabled pair the tests
/// compare can never drift apart.
fn project_config(git_controls: bool) -> String {
    let base = "\
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
  graph_field: parent
";
    if git_controls {
        format!("{base}serve:\n  git_controls: true\n")
    } else {
        base.to_owned()
    }
}

const SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
    default: $filename_pretty
  status:
    type: choice
    values: [open, done]
    required: true
    default: open
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
";

/// Scaffold the workdown project files into `root` (no git involved).
fn write_project_files(root: &Path, config: &str) {
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), config).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();
    fs::write(
        root.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: open\n---\n",
    )
    .unwrap();
}

fn state_for(root: PathBuf, config: &str) -> AppState {
    let parsed = parse_config(config).expect("parse config");
    AppState::new(root, parsed, PathBuf::from(".workdown/config.yaml"), None)
}

async fn get_json(state: AppState, uri: &str) -> (StatusCode, Value) {
    let app = router(state);
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

async fn post_json(state: AppState, uri: &str, origin: Option<&str>) -> (StatusCode, Value) {
    let app = router(state);
    let mut builder = Request::builder().method("POST").uri(uri);
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }
    let response = app
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn status_reports_disabled_when_flag_off() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    write_project_files(&root, &project_config(false));

    let (status, body) = get_json(state_for(root, &project_config(false)), "/api/git").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["state"], "disabled");
}

#[tokio::test]
async fn status_reports_not_a_repo_outside_git() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    write_project_files(&root, &project_config(true));

    let (status, body) = get_json(state_for(root, &project_config(true)), "/api/git").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["state"], "not_a_repo");
}

#[tokio::test]
async fn status_reports_clean_synced_repo() {
    let (_directory, work) = init_synced_repo();

    let (status, body) = get_json(state_for(work.clone(), &project_config(true)), "/api/git").await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["state"], "ready");
    assert_eq!(data["branch"], "main");
    assert_eq!(data["has_upstream"], true);
    assert_eq!(data["ahead"], 0);
    assert_eq!(data["behind"], 0);
    assert_eq!(data["dirty_items"], 0);
    assert_eq!(data["dirty_definitions"], serde_json::json!([]));
    assert_eq!(data["fetch_error"], Value::Null);
}

#[tokio::test]
async fn status_on_repo_with_no_commits_names_the_real_branch() {
    // A supported early state: `git init`, project files written,
    // nothing committed yet. The branch is unborn — plumbing that asks
    // where HEAD points fails — but the status must still name the
    // branch instead of coercing the failure into a nameless "HEAD".
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    run_git(&root, &["init", "--initial-branch=main", "."]);
    write_project_files(&root, &project_config(true));

    let (status, body) = get_json(state_for(root, &project_config(true)), "/api/git").await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["state"], "ready");
    assert_eq!(data["branch"], "main");
    assert_eq!(data["has_upstream"], false);
    assert!(data["dirty_items"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn status_with_fetch_counts_ahead_behind_and_dirty() {
    let (directory, work) = init_synced_repo();

    // A teammate's clone pushes one commit → `work` is 1 behind.
    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-b.md"),
        "---\ntitle: Item B\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "add item-b"]);
    run_git(&other, &["push"]);

    // One local commit → 1 ahead.
    fs::write(
        work.join("workdown-items/item-c.md"),
        "---\ntitle: Item C\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-m", "add item-c"]);

    // One modified tracked file + one untracked file → dirty 2.
    fs::write(
        work.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    fs::write(
        work.join("workdown-items/item-d.md"),
        "---\ntitle: Item D\nstatus: open\n---\n",
    )
    .unwrap();

    let (status, body) = get_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git?fetch=true",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["state"], "ready");
    assert_eq!(data["ahead"], 1);
    assert_eq!(data["behind"], 1, "behind requires the fetch to have run");
    assert_eq!(data["dirty_items"], 2);
    assert_eq!(data["dirty_definitions"], serde_json::json!([]));
}

#[tokio::test]
async fn status_counts_only_changes_inside_the_workdown_paths() {
    let (_directory, work) = init_synced_repo();

    // In scope: one edited item, one new item, one deleted item, the
    // schema. Out of scope: a source file and a rendered view — the
    // kind of neighbours the items have in a code repository.
    fs::write(
        work.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    fs::write(
        work.join("workdown-items/item-new.md"),
        "---\ntitle: New\nstatus: open\n---\n",
    )
    .unwrap();
    fs::write(
        work.join(".workdown/schema.yaml"),
        format!("{SCHEMA}  extra:\n    type: string\n"),
    )
    .unwrap();
    fs::create_dir_all(work.join("src")).unwrap();
    fs::write(work.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::create_dir_all(work.join("views")).unwrap();
    fs::write(work.join("views/board.md"), "# Board\n").unwrap();

    let (status, body) = get_json(state_for(work.clone(), &project_config(true)), "/api/git").await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let data = &body["data"];
    assert_eq!(
        data["dirty_items"], 2,
        "edited + new; src/ and views/ do not count"
    );
    assert_eq!(data["dirty_definitions"], serde_json::json!(["schema"]));
}

#[tokio::test]
async fn status_names_definition_roles_in_a_fixed_order() {
    let (_directory, work) = init_synced_repo();

    // Touch config, templates and views — reported by role, in the
    // pill's order, whatever order the files were changed in.
    fs::write(
        work.join(".workdown/config.yaml"),
        format!("{}# touched\n", project_config(true)),
    )
    .unwrap();
    fs::write(
        work.join(".workdown/templates/bug.md"),
        "---\ntitle: Bug\n---\n",
    )
    .unwrap();
    fs::write(work.join(".workdown/views.yaml"), "views: []\n").unwrap();

    let (_status, body) =
        get_json(state_for(work.clone(), &project_config(true)), "/api/git").await;

    assert_eq!(body["data"]["dirty_items"], 0);
    assert_eq!(
        body["data"]["dirty_definitions"],
        serde_json::json!(["views", "templates", "config"])
    );
}

/// A repository whose root is a code project, with workdown living in
/// `tracker/` beneath it. Paths in the config are relative to `tracker/`,
/// but git reports paths relative to the repository root — the scope
/// has to bridge the two. Returns the guard, the repository, the project.
fn init_repo_with_project_in_subfolder() -> (TempDir, PathBuf, PathBuf) {
    let directory = TempDir::new().unwrap();
    let repository = directory.path().join("repo");
    let project = repository.join("tracker");
    fs::create_dir_all(&project).unwrap();
    run_git(&repository, &["init", "--initial-branch=main", "."]);
    run_git(&repository, &["config", "user.name", "Test"]);
    run_git(&repository, &["config", "user.email", "test@example.com"]);
    run_git(&repository, &["config", "commit.gpgsign", "false"]);
    run_git(&repository, &["config", "core.autocrlf", "false"]);
    write_project_files(&project, &project_config(true));
    fs::write(repository.join("README.md"), "# Code\n").unwrap();
    run_git(&repository, &["add", "-A"]);
    run_git(&repository, &["commit", "-m", "initial"]);
    (directory, repository, project)
}

#[tokio::test]
async fn status_scopes_by_the_project_folder_inside_a_larger_repository() {
    let (_directory, repository, project) = init_repo_with_project_in_subfolder();

    // One item edited inside the project, one file edited at the top.
    fs::write(
        project.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    fs::write(repository.join("README.md"), "# Code, edited\n").unwrap();
    // And a look-alike directory at the top that must not be mistaken
    // for the project's items.
    fs::create_dir_all(repository.join("workdown-items")).unwrap();
    fs::write(
        repository.join("workdown-items/stray.md"),
        "---\ntitle: Stray\n---\n",
    )
    .unwrap();

    let (status, body) = get_json(
        state_for(project.clone(), &project_config(true)),
        "/api/git",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["state"], "ready");
    assert_eq!(
        body["data"]["dirty_items"], 1,
        "only tracker/workdown-items counts"
    );
    assert_eq!(body["data"]["dirty_definitions"], serde_json::json!([]));
}

async fn get_json_from_origin(state: AppState, uri: &str, origin: &str) -> (StatusCode, Value) {
    let app = router(state);
    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("origin", origin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn commit_preview_lists_in_scope_files_and_words_the_message() {
    let (_directory, work) = init_synced_repo();

    // An edited item, a new item, and a source file the button must
    // never touch — named as outside, not listed as a file.
    fs::write(
        work.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    fs::write(
        work.join("workdown-items/item-new.md"),
        "---\ntitle: Brand new\nstatus: open\n---\n",
    )
    .unwrap();
    fs::create_dir_all(work.join("src")).unwrap();
    fs::write(work.join("src/main.rs"), "fn main() {}\n").unwrap();

    let (status, body) = get_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit-preview",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let data = &body["data"];
    assert_eq!(
        data["files"],
        serde_json::json!([
            { "path": "workdown-items/item-a.md", "role": "items", "change": "modified", "label": "edited" },
            { "path": "workdown-items/item-new.md", "role": "items", "change": "added", "label": "added" },
        ])
    );
    // No title display role in this config, so items are named by their
    // prettified id; the choice value is prettified the same way.
    assert_eq!(
        data["message"],
        "Update 1 work item, 1 added\n\nItem A: Status → Done\nItem New: added"
    );
    assert_eq!(data["outside"], serde_json::json!(["src/main.rs"]));
}

#[tokio::test]
async fn commit_preview_reads_deleted_items_from_head() {
    let (_directory, work) = init_synced_repo();
    fs::remove_file(work.join("workdown-items/item-a.md")).unwrap();

    let (status, body) = get_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit-preview",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["files"][0]["change"], "deleted");
    assert_eq!(body["data"]["message"], "Delete Item A");
}

#[tokio::test]
async fn commit_preview_mixes_items_and_definition_files() {
    let (_directory, work) = init_synced_repo();
    fs::write(
        work.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    fs::write(
        work.join(".workdown/schema.yaml"),
        format!("{SCHEMA}  extra:\n    type: string\n"),
    )
    .unwrap();

    let (_status, body) = get_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit-preview",
    )
    .await;

    let data = &body["data"];
    assert_eq!(data["files"][0]["role"], "schema");
    assert_eq!(data["files"][1]["role"], "items");
    assert_eq!(
        data["message"],
        "Update 1 work item, edit .workdown/schema.yaml\n\nItem A: Status → Done\nedit .workdown/schema.yaml"
    );
}

#[tokio::test]
async fn commit_preview_on_a_clean_tree_has_nothing_to_say() {
    let (_directory, work) = init_synced_repo();

    let (status, body) = get_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit-preview",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["files"], serde_json::json!([]));
    assert_eq!(body["data"]["message"], "No changes");
    assert_eq!(body["data"]["outside"], serde_json::json!([]));
}

#[tokio::test]
async fn commit_preview_in_a_subfolder_uses_project_relative_paths() {
    let (_directory, repository, project) = init_repo_with_project_in_subfolder();
    fs::write(
        project.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    fs::write(repository.join("README.md"), "# Code, edited\n").unwrap();

    let (status, body) = get_json(
        state_for(project.clone(), &project_config(true)),
        "/api/git/commit-preview",
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let data = &body["data"];
    assert_eq!(data["files"][0]["path"], "workdown-items/item-a.md");
    assert_eq!(data["message"], "Item A: Status → Done");
    // Outside files are repository-relative: they can be anywhere.
    assert_eq!(data["outside"], serde_json::json!(["README.md"]));
}

#[tokio::test]
async fn commit_preview_refused_when_disabled_and_for_foreign_origins() {
    let (_directory, work) = init_synced_repo();

    let (status, _body) = get_json(
        state_for(work.clone(), &project_config(false)),
        "/api/git/commit-preview",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _body) = get_json_from_origin(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit-preview",
        "https://evil.example",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _body) = get_json_from_origin(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit-preview",
        "http://localhost:3141",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

async fn post_json_body(state: AppState, uri: &str, body: Value) -> (StatusCode, Value) {
    let app = router(state);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

/// Edit item-a in `work` and return the commit request the dialog would
/// send for it: the generated message and the one-file list.
async fn edit_item_a_and_preview(work: &Path) -> Value {
    fs::write(
        work.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    let (_status, preview) = get_json(
        state_for(work.to_path_buf(), &project_config(true)),
        "/api/git/commit-preview",
    )
    .await;
    let files: Vec<Value> = preview["data"]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].clone())
        .collect();
    serde_json::json!({ "message": preview["data"]["message"], "files": files })
}

fn commit_count(dir: &Path) -> usize {
    git_stdout(dir, &["rev-list", "--count", "HEAD"])
        .parse()
        .unwrap()
}

#[tokio::test]
async fn commit_commits_the_scope_and_pushes() {
    let (directory, work) = init_synced_repo();
    let request = edit_item_a_and_preview(&work).await;

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let data = &body["data"];
    assert_eq!(data["commit"].as_str().unwrap().len(), 7);
    assert_eq!(
        data["pull"]["outcome"], "skipped",
        "not behind, nothing to pull"
    );
    assert_eq!(data["push"]["outcome"], "pushed");
    assert_eq!(data["push"]["published"], false);
    assert_eq!(data["status"]["dirty_items"], 0);
    assert_eq!(data["status"]["ahead"], 0);
    // The message landed verbatim, and the remote has the commit.
    assert_eq!(
        git_stdout(&work, &["log", "-1", "--format=%s"]),
        "Item A: Status → Done"
    );
    let remote = directory.path().join("remote.git");
    assert_eq!(
        git_stdout(&remote, &["log", "-1", "--format=%s", "main"]),
        "Item A: Status → Done"
    );
}

#[tokio::test]
async fn commit_leaves_files_outside_the_scope_exactly_as_they_were() {
    let (_directory, work) = init_synced_repo();
    // An untracked source file, and one the user staged themselves.
    fs::create_dir_all(work.join("src")).unwrap();
    fs::write(work.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(work.join("src/staged.rs"), "// staged\n").unwrap();
    run_git(&work, &["add", "src/staged.rs"]);
    let request = edit_item_a_and_preview(&work).await;

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    // The commit holds the item only; the staged file is still staged,
    // the untracked one still untracked.
    assert_eq!(
        git_stdout(&work, &["show", "--name-only", "--format=", "HEAD"]),
        "workdown-items/item-a.md"
    );
    let porcelain = git_stdout(&work, &["status", "--porcelain"]);
    assert!(porcelain.contains("A  src/staged.rs"), "got: {porcelain}");
    assert!(porcelain.contains("?? src/main.rs"), "got: {porcelain}");
    // Not behind, so the pull was skipped and the push went through
    // despite the dirty files outside.
    assert_eq!(body["data"]["pull"]["outcome"], "skipped");
    assert_eq!(body["data"]["push"]["outcome"], "pushed");
}

#[tokio::test]
async fn commit_refuses_a_stale_file_list_and_commits_nothing() {
    let (_directory, work) = init_synced_repo();
    let request = edit_item_a_and_preview(&work).await;
    let before = commit_count(&work);
    // Between preview and confirm, another change lands in scope.
    fs::write(
        work.join("workdown-items/item-late.md"),
        "---\ntitle: Late\nstatus: open\n---\n",
    )
    .unwrap();

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    assert!(
        body["error"]
            .as_str()
            .unwrap()
            .contains("moved since the preview"),
        "got: {}",
        body["error"]
    );
    assert_eq!(commit_count(&work), before);
    assert_eq!(git_stdout(&work, &["diff", "--cached", "--name-only"]), "");
}

#[tokio::test]
async fn commit_pulls_when_behind_then_pushes() {
    let (directory, work) = init_synced_repo();
    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-b.md"),
        "---\ntitle: Item B\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "add item-b"]);
    run_git(&other, &["push"]);
    let request = edit_item_a_and_preview(&work).await;

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        body["data"]["pull"],
        serde_json::json!({ "outcome": "pulled", "commits": 1 })
    );
    assert_eq!(body["data"]["push"]["outcome"], "pushed");
    assert!(work.join("workdown-items/item-b.md").exists());
    assert_eq!(body["data"]["status"]["ahead"], 0);
    assert_eq!(body["data"]["status"]["behind"], 0);
}

#[tokio::test]
async fn commit_stops_at_pull_over_outside_files_when_behind() {
    let (directory, work) = init_synced_repo();
    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-b.md"),
        "---\ntitle: Item B\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "add item-b"]);
    run_git(&other, &["push"]);
    fs::create_dir_all(work.join("src")).unwrap();
    fs::write(work.join("src/main.rs"), "fn main() {}\n").unwrap();
    let request = edit_item_a_and_preview(&work).await;

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let data = &body["data"];
    assert_eq!(data["pull"]["outcome"], "stopped");
    let reason = data["pull"]["reason"].as_str().unwrap();
    assert!(reason.contains("src/main.rs"), "got: {reason}");
    assert!(reason.contains("safe and local"), "got: {reason}");
    assert_eq!(data["push"]["outcome"], "skipped");
    assert_eq!(data["status"]["ahead"], 1);
    assert_eq!(data["status"]["behind"], 1);
}

#[tokio::test]
async fn commit_with_a_conflicting_pull_keeps_the_commit_and_aborts_the_rebase() {
    let (directory, work) = init_synced_repo();
    // The teammate changes the same item's status; we change its title.
    // Adjacent lines — git sees one contested block.
    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "finish item-a"]);
    run_git(&other, &["push"]);
    fs::write(
        work.join("workdown-items/item-a.md"),
        "---\ntitle: Item A, renamed\nstatus: open\n---\n",
    )
    .unwrap();
    let (_status, preview) = get_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit-preview",
    )
    .await;
    let request = serde_json::json!({
        "message": preview["data"]["message"],
        "files": ["workdown-items/item-a.md"],
    });

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    let data = &body["data"];
    assert_eq!(data["pull"]["outcome"], "stopped");
    let reason = data["pull"]["reason"].as_str().unwrap();
    assert!(reason.contains("safe and local"), "got: {reason}");
    assert!(reason.contains("workdown-items/item-a.md"), "got: {reason}");
    assert!(reason.contains("press Push"), "got: {reason}");
    assert_eq!(data["push"]["outcome"], "skipped");
    // No rebase left behind; our commit is HEAD; the tree is clean.
    assert!(!work.join(".git/rebase-merge").exists());
    assert!(!work.join(".git/rebase-apply").exists());
    assert_eq!(
        git_stdout(&work, &["log", "-1", "--format=%s"]),
        "Item A: Title → Item A, renamed"
    );
    assert_eq!(git_stdout(&work, &["status", "--porcelain"]), "");
}

#[tokio::test]
async fn commit_publishes_an_unpublished_branch() {
    let (directory, work) = init_synced_repo();
    create_unpublished_branch(&work, "feature");
    let request = edit_item_a_and_preview(&work).await;

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["pull"]["outcome"], "skipped");
    assert_eq!(
        body["data"]["push"],
        serde_json::json!({ "outcome": "pushed", "published": true })
    );
    let remote = directory.path().join("remote.git");
    assert_eq!(
        git_stdout(&remote, &["log", "-1", "--format=%s", "feature"]),
        "Item A: Status → Done"
    );
    assert_eq!(body["data"]["status"]["has_upstream"], true);
}

#[tokio::test]
async fn commit_refused_without_an_identity() {
    let (_directory, work) = init_synced_repo();
    // An empty local value shadows any global identity the machine has.
    run_git(&work, &["config", "user.email", ""]);
    // Keep git from inventing an identity from the host name.
    run_git(&work, &["config", "user.useConfigOnly", "true"]);
    let request = edit_item_a_and_preview(&work).await;
    let before = commit_count(&work);

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    assert!(
        body["error"].as_str().unwrap().contains("user.email"),
        "got: {}",
        body["error"]
    );
    assert_eq!(commit_count(&work), before);
}

#[tokio::test]
async fn commit_refused_for_an_empty_message_or_a_clean_tree() {
    let (_directory, work) = init_synced_repo();

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        serde_json::json!({ "message": "Anything", "files": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("nothing to commit"));

    let mut request = edit_item_a_and_preview(&work).await;
    request["message"] = Value::String("   \n".to_owned());
    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    assert!(body["error"].as_str().unwrap().contains("message is empty"));
}

#[cfg(unix)]
#[tokio::test]
async fn commit_rejected_by_a_hook_is_worded_and_leaves_nothing_staged() {
    use std::os::unix::fs::PermissionsExt;
    let (_directory, work) = init_synced_repo();
    let hook = work.join(".git/hooks/pre-commit");
    fs::write(
        &hook,
        "#!/bin/sh\necho 'items must have owners' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let request = edit_item_a_and_preview(&work).await;
    let before = commit_count(&work);

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = body["error"].as_str().unwrap();
    let (sentence, details) = error
        .split_once("\n\n")
        .expect("sentence, blank line, details");
    assert!(sentence.contains("commit failed"), "got: {sentence}");
    assert!(details.contains("items must have owners"), "got: {details}");
    assert_eq!(commit_count(&work), before);
    assert_eq!(git_stdout(&work, &["diff", "--cached", "--name-only"]), "");
}

#[cfg(unix)]
#[tokio::test]
async fn commit_absorbs_what_a_pre_commit_hook_added_and_leaves_the_index_clean() {
    use std::os::unix::fs::PermissionsExt;
    let (_directory, work) = init_synced_repo();
    // The hook plays `workdown install-hooks`: re-render something
    // outside the scope and stage it into the commit being made.
    fs::create_dir_all(work.join("rendered")).unwrap();
    fs::write(work.join("rendered/board.md"), "render v1\n").unwrap();
    run_git(&work, &["add", "--all"]);
    run_git(&work, &["commit", "-qm", "rendered views"]);
    let hook = work.join(".git/hooks/pre-commit");
    fs::write(
        &hook,
        "#!/bin/sh\necho 'render v2' > rendered/board.md\ngit add -- rendered\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let request = edit_item_a_and_preview(&work).await;

    let (status, body) = post_json_body(
        state_for(work.clone(), &project_config(true)),
        "/api/git/commit",
        request,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    // The hook's file is in the commit, alongside the item…
    let committed = git_stdout(
        &work,
        &["diff-tree", "-r", "--no-commit-id", "--name-only", "HEAD"],
    );
    assert!(committed.contains("rendered/board.md"), "got: {committed}");
    assert!(
        committed.contains("workdown-items/item-a.md"),
        "got: {committed}"
    );
    assert_eq!(
        git_stdout(&work, &["show", "HEAD:rendered/board.md"]),
        "render v2"
    );
    // …and nothing is left half-staged behind: git built the commit from
    // a temporary index, and without the fix the real index would still
    // hold `render v1` as a staged change.
    assert_eq!(git_stdout(&work, &["status", "--porcelain"]), "");
}

#[tokio::test]
async fn commit_refused_when_disabled_and_for_foreign_origins() {
    let (_directory, work) = init_synced_repo();
    let request = edit_item_a_and_preview(&work).await;

    let (status, _body) = post_json_body(
        state_for(work.clone(), &project_config(false)),
        "/api/git/commit",
        request.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let app = router(state_for(work.clone(), &project_config(true)));
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/git/commit")
                .header("origin", "https://evil.example")
                .header("content-type", "application/json")
                .body(Body::from(request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn pull_refused_over_changes_outside_the_workdown_paths_names_them() {
    let (_directory, work) = init_synced_repo();

    // Nothing in scope is dirty — the pill reads clean — but the
    // repository is not, and pull never runs over uncommitted work.
    fs::create_dir_all(work.join("src")).unwrap();
    fs::write(work.join("src/main.rs"), "fn main() {}\n").unwrap();

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/pull",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = body["error"].as_str().unwrap();
    assert!(error.contains("outside the workdown paths"), "got: {error}");
    assert!(error.contains("src/main.rs"), "got: {error}");
}

#[tokio::test]
async fn status_fetch_against_unreachable_remote_degrades_to_local_answer() {
    let (directory, work) = init_synced_repo();
    let gone = directory.path().join("nonexistent.git");
    run_git(
        &work,
        &["remote", "set-url", "origin", gone.to_str().unwrap()],
    );

    let (status, body) = get_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git?fetch=true",
    )
    .await;

    // The remote being unreachable must not hide the widget: the local
    // numbers are still served, and `fetch_error` says why `behind` is
    // only as fresh as the last successful fetch.
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let data = &body["data"];
    assert_eq!(data["state"], "ready");
    assert_eq!(data["branch"], "main");
    assert!(
        data["fetch_error"].as_str().is_some_and(|e| !e.is_empty()),
        "fetch_error should carry the failure, body: {body}"
    );
}

#[tokio::test]
async fn status_fetch_rejects_foreign_browser_origins() {
    // Plain status is a harmless local read, but `fetch=true` contacts
    // the remote and can invoke a credential helper — the same-origin
    // guard covers it like the POSTs.
    let (_directory, work) = init_synced_repo();
    let app = router(state_for(work.clone(), &project_config(true)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/git?fetch=true")
                .header("origin", "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    // Without the fetch, the same foreign origin may read: responses to
    // cross-origin requests are unreadable to the page anyway, and the
    // request has no side effects.
    let app = router(state_for(work, &project_config(true)));
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/git")
                .header("origin", "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn pull_fast_forwards_new_remote_commits() {
    let (directory, work) = init_synced_repo();

    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-b.md"),
        "---\ntitle: Item B\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "add item-b"]);
    run_git(&other, &["push"]);

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/pull",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(work.join("workdown-items/item-b.md").exists());
    let data = &body["data"];
    assert_eq!(data["pulled_commits"], 1);
    assert_eq!(data["status"]["state"], "ready");
    assert_eq!(data["status"]["behind"], 0);
}

#[tokio::test]
async fn pull_when_up_to_date_reports_zero_commits() {
    let (_directory, work) = init_synced_repo();

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/pull",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["pulled_commits"], 0);
    assert_eq!(body["data"]["status"]["state"], "ready");
}

#[tokio::test]
async fn pull_counts_only_incoming_commits_not_rebased_local_ones() {
    let (directory, work) = init_synced_repo();

    // One remote commit to pull in…
    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-b.md"),
        "---\ntitle: Item B\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "remote: add item-b"]);
    run_git(&other, &["push"]);

    // …and one local commit that the pull rebases on top of it. The
    // rebase rewrites the local commit (new hash), which a naive
    // old-HEAD..HEAD count would wrongly include.
    fs::write(
        work.join("workdown-items/item-c.md"),
        "---\ntitle: Item C\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-m", "local: add item-c"]);

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/pull",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["pulled_commits"], 1);
    // The local commit survived the rebase and still waits to be pushed.
    assert_eq!(body["data"]["status"]["ahead"], 1);
}

#[tokio::test]
async fn pull_conflict_aborts_and_leaves_tree_as_it_was() {
    let (directory, work) = init_synced_repo();

    // Remote and local commit conflicting edits to the same line.
    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "remote: item-a done"]);
    run_git(&other, &["push"]);

    let local_content = "---\ntitle: Item A renamed\nstatus: open\n---\n";
    fs::write(work.join("workdown-items/item-a.md"), local_content).unwrap();
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-m", "local: rename item-a"]);

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/pull",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = body["error"].as_str().unwrap();
    assert!(error.contains("pull failed"), "unexpected error: {error}");
    // The rebase must not be left in progress …
    assert!(!work.join(".git/rebase-merge").exists());
    assert!(!work.join(".git/rebase-apply").exists());
    // … and the work tree must hold the local commit's content.
    let content = fs::read_to_string(work.join("workdown-items/item-a.md")).unwrap();
    assert_eq!(content, local_content);
}

#[tokio::test]
async fn pull_refused_while_uncommitted_changes_exist() {
    let (_directory, work) = init_synced_repo();

    // One uncommitted edit — the state pull must never touch.
    let edited_content = "---\ntitle: Item A\nstatus: in_progress\n---\n";
    fs::write(work.join("workdown-items/item-a.md"), edited_content).unwrap();

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/pull",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = body["error"].as_str().unwrap();
    assert!(
        error.contains("uncommitted"),
        "error should explain the refusal, got: {error}"
    );
    // The edit is exactly where the user left it — not stashed, not
    // rebased over, not marked up with conflict markers.
    let content = fs::read_to_string(work.join("workdown-items/item-a.md")).unwrap();
    assert_eq!(content, edited_content);
    assert_eq!(git_stdout(&work, &["stash", "list"]), "");
}

#[tokio::test]
async fn pull_refused_while_a_rebase_is_in_progress_and_leaves_it_alone() {
    let (directory, work) = init_synced_repo();

    // Manufacture a rebase stopped on a conflict, as if the user had
    // run `git pull --rebase` in a terminal and were mid-resolution.
    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-a.md"),
        "---\ntitle: Item A\nstatus: done\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "remote: item-a done"]);
    run_git(&other, &["push"]);
    fs::write(
        work.join("workdown-items/item-a.md"),
        "---\ntitle: Item A renamed\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-m", "local: rename item-a"]);
    let conflicting_pull = Command::new("git")
        .arg("-C")
        .arg(&work)
        .args(["pull", "--rebase"])
        .output()
        .expect("spawn git");
    assert!(
        !conflicting_pull.status.success(),
        "the terminal-side pull should conflict"
    );
    assert!(work.join(".git/rebase-merge").exists());

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/pull",
        None,
    )
    .await;

    // Refused — and the user's half-resolved rebase is still there,
    // not aborted out from under them.
    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = body["error"].as_str().unwrap();
    assert!(
        error.contains("rebase"),
        "error should name the rebase, got: {error}"
    );
    assert!(work.join(".git/rebase-merge").exists());
}

#[tokio::test]
async fn pull_counts_commits_already_fetched_by_an_earlier_status_call() {
    // The primary flow: opening the board fetches (so the pill can show
    // "behind"), then the user clicks Pull. The count reported must be
    // what the pull integrated — not the tracking-ref movement during
    // the pull, which after that earlier fetch would be zero.
    let (directory, work) = init_synced_repo();

    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-b.md"),
        "---\ntitle: Item B\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "add item-b"]);
    run_git(&other, &["push"]);

    // The page-load status call, remote included.
    let (status, body) = get_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git?fetch=true",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["behind"], 1);

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/pull",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["pulled_commits"], 1);
    assert_eq!(body["data"]["status"]["behind"], 0);
}

#[tokio::test]
async fn push_publishes_local_commits() {
    let (directory, work) = init_synced_repo();

    fs::write(
        work.join("workdown-items/item-b.md"),
        "---\ntitle: Item B\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-m", "add item-b"]);

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["published"], false);
    assert_eq!(body["data"]["status"]["ahead"], 0);
    let remote = directory.path().join("remote.git");
    assert_eq!(
        git_stdout(&remote, &["rev-parse", "main"]),
        git_stdout(&work, &["rev-parse", "HEAD"]),
        "the remote's main must now hold the local commit"
    );
}

/// Switch `work` to a brand-new branch with one commit on it and no
/// upstream — the state the pill shows as "not published".
fn create_unpublished_branch(work: &Path, name: &str) {
    run_git(work, &["switch", "-c", name]);
    fs::write(
        work.join("workdown-items/item-b.md"),
        "---\ntitle: Item B\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(work, &["add", "-A"]);
    run_git(work, &["commit", "-m", "add item-b on branch"]);
}

#[tokio::test]
async fn push_on_unpublished_branch_creates_it_on_the_remote_and_sets_upstream() {
    let (directory, work) = init_synced_repo();
    create_unpublished_branch(&work, "feature");

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["published"], true);
    // The fresh status already tracks the new upstream — from here on
    // the button reads "Push" and ahead/behind are real numbers.
    assert_eq!(body["data"]["status"]["has_upstream"], true);
    assert_eq!(body["data"]["status"]["ahead"], 0);
    assert_eq!(body["data"]["status"]["behind"], 0);
    let remote = directory.path().join("remote.git");
    assert_eq!(
        git_stdout(&remote, &["rev-parse", "feature"]),
        git_stdout(&work, &["rev-parse", "HEAD"]),
        "the remote must now have the branch at the local commit"
    );
    assert_eq!(
        git_stdout(&work, &["rev-parse", "--abbrev-ref", "feature@{upstream}"]),
        "origin/feature"
    );
}

#[tokio::test]
async fn publish_uses_the_only_remote_whatever_it_is_called() {
    let (directory, work) = init_synced_repo();
    run_git(&work, &["remote", "rename", "origin", "upstream"]);
    create_unpublished_branch(&work, "feature");

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        git_stdout(&work, &["rev-parse", "--abbrev-ref", "feature@{upstream}"]),
        "upstream/feature"
    );
    let remote = directory.path().join("remote.git");
    assert_eq!(
        git_stdout(&remote, &["rev-parse", "feature"]),
        git_stdout(&work, &["rev-parse", "HEAD"])
    );
}

#[tokio::test]
async fn publish_prefers_push_default_over_origin() {
    let (directory, work) = init_synced_repo();
    // A second bare remote, made the configured push default.
    let fork = directory.path().join("fork.git");
    fs::create_dir_all(&fork).unwrap();
    run_git(&fork, &["init", "--bare", "--initial-branch=main", "."]);
    run_git(&work, &["remote", "add", "fork", fork.to_str().unwrap()]);
    run_git(&work, &["config", "remote.pushDefault", "fork"]);
    create_unpublished_branch(&work, "feature");

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        git_stdout(&work, &["rev-parse", "--abbrev-ref", "feature@{upstream}"]),
        "fork/feature"
    );
    assert_eq!(
        git_stdout(&fork, &["rev-parse", "feature"]),
        git_stdout(&work, &["rev-parse", "HEAD"])
    );
    // origin never saw the branch.
    let origin = directory.path().join("remote.git");
    assert!(
        git_stdout(&origin, &["branch", "--list", "feature"]).is_empty(),
        "origin must not have received the branch"
    );
}

#[tokio::test]
async fn publish_falls_back_to_origin_among_several_remotes() {
    let (directory, work) = init_synced_repo();
    let fork = directory.path().join("fork.git");
    fs::create_dir_all(&fork).unwrap();
    run_git(&fork, &["init", "--bare", "--initial-branch=main", "."]);
    run_git(&work, &["remote", "add", "fork", fork.to_str().unwrap()]);
    create_unpublished_branch(&work, "feature");

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        git_stdout(&work, &["rev-parse", "--abbrev-ref", "feature@{upstream}"]),
        "origin/feature"
    );
}

#[tokio::test]
async fn publish_refused_when_no_remote_can_be_chosen() {
    let (directory, work) = init_synced_repo();
    // Two remotes, neither called origin, no push default: the rule has
    // no answer and the click says so instead of guessing.
    let fork = directory.path().join("fork.git");
    fs::create_dir_all(&fork).unwrap();
    run_git(&fork, &["init", "--bare", "--initial-branch=main", "."]);
    run_git(&work, &["remote", "rename", "origin", "upstream"]);
    run_git(&work, &["remote", "add", "fork", fork.to_str().unwrap()]);
    create_unpublished_branch(&work, "feature");

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = body["error"].as_str().unwrap();
    assert!(error.contains("no remote"), "unexpected error: {error}");
    assert!(
        git_stdout(&work, &["branch", "-r", "--list", "*/feature"]).is_empty(),
        "nothing must have been pushed anywhere"
    );
}

#[tokio::test]
async fn publish_refused_on_a_branch_with_no_commits() {
    let directory = TempDir::new().unwrap();
    let remote = directory.path().join("remote.git");
    let work = directory.path().join("work");
    fs::create_dir_all(&remote).unwrap();
    fs::create_dir_all(&work).unwrap();
    run_git(&remote, &["init", "--bare", "--initial-branch=main", "."]);
    run_git(&work, &["init", "--initial-branch=main", "."]);
    run_git(
        &work,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    write_project_files(&work, &project_config(true));

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = body["error"].as_str().unwrap();
    assert!(error.contains("no commits"), "unexpected error: {error}");
}

#[tokio::test]
async fn publish_refused_on_a_detached_head() {
    let (_directory, work) = init_synced_repo();
    run_git(&work, &["checkout", "--detach"]);

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = body["error"].as_str().unwrap();
    assert!(error.contains("detached"), "unexpected error: {error}");
}

#[tokio::test]
async fn push_rejected_when_remote_has_newer_commits() {
    let (directory, work) = init_synced_repo();

    let other = clone_remote(&directory, "other");
    fs::write(
        other.join("workdown-items/item-b.md"),
        "---\ntitle: Item B\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&other, &["add", "-A"]);
    run_git(&other, &["commit", "-m", "remote: add item-b"]);
    run_git(&other, &["push"]);

    fs::write(
        work.join("workdown-items/item-c.md"),
        "---\ntitle: Item C\nstatus: open\n---\n",
    )
    .unwrap();
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-m", "local: add item-c"]);

    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "body: {body}");
    let error = body["error"].as_str().unwrap();
    assert!(error.contains("push failed"), "unexpected error: {error}");
}

#[tokio::test]
async fn mutations_refused_when_git_controls_disabled() {
    let (_directory, work) = init_synced_repo();
    // The repo is real, but this server runs without the opt-in flag.
    for uri in ["/api/git/pull", "/api/git/push"] {
        let (status, body) =
            post_json(state_for(work.clone(), &project_config(false)), uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} body: {body}");
        let error = body["error"].as_str().unwrap();
        assert!(
            error.contains("git_controls"),
            "{uri}: error should point at the config key, got: {error}"
        );
    }
}

#[tokio::test]
async fn mutations_reject_foreign_browser_origins() {
    let (_directory, work) = init_synced_repo();
    for uri in ["/api/git/pull", "/api/git/push"] {
        let (status, body) = post_json(
            state_for(work.clone(), &project_config(true)),
            uri,
            Some("https://evil.example"),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri} body: {body}");
    }
    // The UI's own origin stays allowed (any localhost port: the dev
    // server proxies from 5173, serve binds wherever the port scan
    // lands).
    let (status, body) = post_json(
        state_for(work.clone(), &project_config(true)),
        "/api/git/pull",
        Some("http://127.0.0.1:3141"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let (status, body) = post_json(
        state_for(work, &project_config(true)),
        "/api/git/pull",
        Some("http://localhost:5173"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
}

fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A second working clone of the same bare remote, for playing the
/// teammate whose pushes make `work` fall behind.
fn clone_remote(directory: &TempDir, name: &str) -> PathBuf {
    let remote = directory.path().join("remote.git");
    let clone = directory.path().join(name);
    run_git(
        directory.path(),
        &["clone", remote.to_str().unwrap(), clone.to_str().unwrap()],
    );
    run_git(&clone, &["config", "user.name", "Test Other"]);
    run_git(&clone, &["config", "user.email", "other@example.com"]);
    run_git(&clone, &["config", "commit.gpgsign", "false"]);
    run_git(&clone, &["config", "core.autocrlf", "false"]);
    clone
}

fn run_git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {:?} failed:\n{}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A working clone with the project files committed and pushed to a
/// bare "remote" beside it — the steady state the widget usually sees.
/// Returns the guard `TempDir` (holding both repos) and the clone's path.
fn init_synced_repo() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let remote = directory.path().join("remote.git");
    let work = directory.path().join("work");
    fs::create_dir_all(&remote).unwrap();
    fs::create_dir_all(&work).unwrap();

    run_git(&remote, &["init", "--bare", "--initial-branch=main", "."]);
    run_git(&work, &["init", "--initial-branch=main", "."]);
    run_git(&work, &["config", "user.name", "Test"]);
    run_git(&work, &["config", "user.email", "test@example.com"]);
    run_git(&work, &["config", "commit.gpgsign", "false"]);
    // Byte-exact content assertions must not depend on the machine's
    // line-ending conversion (autocrlf defaults to true on Windows).
    run_git(&work, &["config", "core.autocrlf", "false"]);
    run_git(
        &work,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );

    write_project_files(&work, &project_config(true));
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-m", "initial project"]);
    run_git(&work, &["push", "-u", "origin", "main"]);

    (directory, work)
}
