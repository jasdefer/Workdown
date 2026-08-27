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

const CONFIG_WITHOUT_GIT_CONTROLS: &str = "\
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

const CONFIG_WITH_GIT_CONTROLS: &str = "\
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
serve:
  git_controls: true
";

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
    write_project_files(&root, CONFIG_WITHOUT_GIT_CONTROLS);

    let (status, body) = get_json(state_for(root, CONFIG_WITHOUT_GIT_CONTROLS), "/api/git").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["state"], "disabled");
}

#[tokio::test]
async fn status_reports_not_a_repo_outside_git() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    write_project_files(&root, CONFIG_WITH_GIT_CONTROLS);

    let (status, body) = get_json(state_for(root, CONFIG_WITH_GIT_CONTROLS), "/api/git").await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["state"], "not_a_repo");
}

#[tokio::test]
async fn status_reports_clean_synced_repo() {
    let (_directory, work) = init_synced_repo();

    let (status, body) = get_json(
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
        "/api/git",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["state"], "ready");
    assert_eq!(data["branch"], "main");
    assert_eq!(data["has_upstream"], true);
    assert_eq!(data["ahead"], 0);
    assert_eq!(data["behind"], 0);
    assert_eq!(data["dirty_count"], 0);
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
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
        "/api/git?fetch=true",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    let data = &body["data"];
    assert_eq!(data["state"], "ready");
    assert_eq!(data["ahead"], 1);
    assert_eq!(data["behind"], 1, "behind requires the fetch to have run");
    assert_eq!(data["dirty_count"], 2);
}

#[tokio::test]
async fn status_fetch_against_unreachable_remote_fails_cleanly() {
    let (directory, work) = init_synced_repo();
    let gone = directory.path().join("nonexistent.git");
    run_git(
        &work,
        &["remote", "set-url", "origin", gone.to_str().unwrap()],
    );

    let (status, body) = get_json(
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
        "/api/git?fetch=true",
    )
    .await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let error = body["error"].as_str().unwrap();
    assert!(error.contains("fetch failed"), "unexpected error: {error}");
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
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
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
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
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
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
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
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
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
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
        "/api/git/push",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(body["data"]["ahead"], 0);
    let remote = directory.path().join("remote.git");
    assert_eq!(
        git_stdout(&remote, &["rev-parse", "main"]),
        git_stdout(&work, &["rev-parse", "HEAD"]),
        "the remote's main must now hold the local commit"
    );
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
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
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
        let (status, body) = post_json(
            state_for(work.clone(), CONFIG_WITHOUT_GIT_CONTROLS),
            uri,
            None,
        )
        .await;
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
            state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
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
        state_for(work.clone(), CONFIG_WITH_GIT_CONTROLS),
        "/api/git/pull",
        Some("http://127.0.0.1:3141"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let (status, body) = post_json(
        state_for(work, CONFIG_WITH_GIT_CONTROLS),
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

    write_project_files(&work, CONFIG_WITH_GIT_CONTROLS);
    run_git(&work, &["add", "-A"]);
    run_git(&work, &["commit", "-m", "initial project"]);
    run_git(&work, &["push", "-u", "origin", "main"]);

    (directory, work)
}
