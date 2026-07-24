//! Integration tests for `POST /api/items/:id/fields/:field`.
//!
//! These mutate files, so each test runs against a throwaway project
//! built in a `TempDir` — never the committed read-only fixture under
//! `tests/fixtures/project/`. Drives the router with
//! `tower::ServiceExt::oneshot`, pinning the status-code taxonomy and
//! the save-with-warning behaviour the UI relies on.

use std::fs;
use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

use workdown_core::parser::config::parse_config;
use workdown_server::{router, AppState};

const CONFIG: &str = "\
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

const SCHEMA: &str = "\
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
  tags:
    type: list
    required: false
";

/// Build a throwaway project with a single `task-1` item. The returned
/// `TempDir` must be held for the lifetime of the test — dropping it
/// deletes the project.
fn temp_project() -> (TempDir, AppState) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();
    fs::write(
        root.join("workdown-items/task-1.md"),
        "---\ntitle: Task 1\nstatus: open\npriority: high\n---\nbody text\n",
    )
    .unwrap();

    let config = parse_config(CONFIG).expect("parse config");
    let state = AppState::new(
        root,
        config,
        std::path::PathBuf::from(".workdown/config.yaml"),
    );
    (directory, state)
}

fn read_item(root: &Path, id: &str) -> String {
    fs::read_to_string(root.join(format!("workdown-items/{id}.md"))).unwrap()
}

async fn get(state: AppState, uri: &str) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    router(state).oneshot(request).await.unwrap()
}

async fn post(state: AppState, uri: &str, body: Value) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    router(state).oneshot(request).await.unwrap()
}

async fn body_json(response: axum::http::Response<Body>) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body");
    serde_json::from_slice(&bytes).expect("body parses as JSON")
}

#[tokio::test]
async fn replace_writes_file_and_returns_previous_and_new() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();

    let response = post(
        state,
        "/api/items/task-1/fields/status",
        json!({ "op": "replace", "value": "in_progress" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["previous_value"], "open");
    assert_eq!(envelope["data"]["new_value"], "in_progress");
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);
    assert!(envelope["diagnostics"].is_array());
    // No hard failure → no `error` field at all.
    assert!(envelope.get("error").is_none());

    // The markdown file on disk changed; no auto-commit, just the write.
    let file = read_item(&root, "task-1");
    assert!(file.contains("status: in_progress"));
    assert!(!file.contains("status: open"));
}

#[tokio::test]
async fn unset_clears_the_field() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();

    let response = post(
        state,
        "/api/items/task-1/fields/priority",
        json!({ "op": "unset" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["previous_value"], "high");
    assert_eq!(envelope["data"]["new_value"], Value::Null);

    let file = read_item(&root, "task-1");
    assert!(!file.contains("priority:"));
}

#[tokio::test]
async fn invalid_choice_value_saves_with_warning_and_flags_mutation_caused() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();

    // ADR-001 save-with-warning: a schema violation still writes the
    // file and returns 200, with the violation surfaced in diagnostics.
    let response = post(
        state,
        "/api/items/task-1/fields/status",
        json!({ "op": "replace", "value": "not_a_real_status" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["mutation_caused_warning"], true);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());

    let file = read_item(&root, "task-1");
    assert!(file.contains("status: not_a_real_status"));
}

#[tokio::test]
async fn unknown_item_returns_404_with_error() {
    let (_directory, state) = temp_project();

    let response = post(
        state,
        "/api/items/does-not-exist/fields/status",
        json!({ "op": "replace", "value": "done" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
    assert!(envelope.get("data").is_none());
}

// ── Read (GET /api/items/:id) ────────────────────────────────────────

#[tokio::test]
async fn get_item_returns_fields_in_schema_order_and_body() {
    let (_directory, state) = temp_project();

    let response = get(state, "/api/items/task-1").await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["id"], "task-1");
    assert!(envelope["data"]["body"]
        .as_str()
        .unwrap()
        .contains("body text"));

    let field_names: Vec<&str> = envelope["data"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| field["name"].as_str().unwrap())
        .collect();
    // Schema order; `id` is the identity, returned separately, not a field.
    assert_eq!(field_names, vec!["title", "status", "priority"]);
}

#[tokio::test]
async fn get_unknown_item_returns_404() {
    let (_directory, state) = temp_project();
    let response = get(state, "/api/items/does-not-exist").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ── Create (POST /api/items) ─────────────────────────────────────────

#[tokio::test]
async fn create_from_title_slugs_the_id_and_writes_a_new_file() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();

    let response = post(
        state,
        "/api/items",
        json!({ "fields": { "title": "New Task", "status": "open" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let envelope = body_json(response).await;
    // Title → slug → id, returned so the UI can navigate to the new item.
    assert_eq!(envelope["data"]["id"], "new-task");
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);

    // The new markdown file exists on disk.
    let file = read_item(&root, "new-task");
    assert!(file.contains("status: open"));
}

#[tokio::test]
async fn create_with_existing_id_returns_409_conflict() {
    let (_directory, state) = temp_project();

    // `task-1` already exists in the fixture project.
    let response = post(
        state,
        "/api/items",
        json!({ "fields": { "id": "task-1", "status": "open" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
}

#[tokio::test]
async fn create_without_id_or_title_returns_422() {
    let (_directory, state) = temp_project();

    // No naming source: `title`'s `$filename_pretty` default is
    // slug-dependent and not yet applied, so run_add can't derive a slug.
    let response = post(
        state,
        "/api/items",
        json!({ "fields": { "status": "open" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
}

#[tokio::test]
async fn op_invalid_for_field_type_returns_422_with_error() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    let before = read_item(&root, "task-1");

    // `append` is only valid for list/links/multichoice; `status` is a
    // choice → ModeNotValidForFieldType → 422, nothing written.
    let response = post(
        state,
        "/api/items/task-1/fields/status",
        json!({ "op": "append", "values": ["extra"] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());

    // File untouched on a hard failure.
    assert_eq!(read_item(&root, "task-1"), before);
}
