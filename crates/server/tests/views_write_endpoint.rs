//! Integration tests for `POST /api/views` and `PATCH /api/views/:id`.
//!
//! These mutate `views.yaml`, so each test runs against a throwaway
//! project built in a `TempDir` — never the committed read-only fixture
//! under `tests/fixtures/project/`. Drives the router with
//! `tower::ServiceExt::oneshot`, pinning the status-code taxonomy and the
//! save-with-warning behaviour the view-authoring UI relies on.

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
  status:
    type: choice
    values: [open, in_progress, done]
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
";

/// Build a throwaway project with no `views.yaml` yet. The returned
/// `TempDir` must outlive the test — dropping it deletes the project.
fn temp_project() -> (TempDir, AppState) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();

    let config = parse_config(CONFIG).expect("parse config");
    let state = AppState::new(root, config);
    (directory, state)
}

fn write_views(root: &Path, content: &str) {
    fs::write(root.join(".workdown/views.yaml"), content).unwrap();
}

fn read_views(root: &Path) -> String {
    fs::read_to_string(root.join(".workdown/views.yaml")).unwrap()
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

async fn patch(state: AppState, uri: &str, body: Value) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("PATCH")
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

// ── Create (POST /api/views) ─────────────────────────────────────────

#[tokio::test]
async fn create_view_writes_file_and_returns_201() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();

    let response = post(
        state,
        "/api/views",
        json!({ "definition": { "id": "status-board", "type": "board", "field": "status" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["view_id"], "status-board");
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);
    assert!(envelope.get("error").is_none());

    let file = read_views(&root);
    assert!(file.contains("id: status-board"));
    assert!(file.contains("type: board"));
}

#[tokio::test]
async fn create_view_with_existing_id_returns_409() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    let original = "views:\n  - id: dup\n    type: board\n    field: status\n";
    write_views(&root, original);

    let response = post(
        state,
        "/api/views",
        json!({ "definition": { "id": "dup", "type": "board", "field": "status" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
    assert_eq!(read_views(&root), original, "file must be untouched");
}

#[tokio::test]
async fn create_view_missing_required_slot_returns_422() {
    let (_directory, state) = temp_project();

    // A board with no `field` can't be constructed — hard 422, nothing written.
    let response = post(
        state,
        "/api/views",
        json!({ "definition": { "id": "b", "type": "board" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
}

#[tokio::test]
async fn create_view_with_bad_field_reference_saves_with_warning() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();

    // `field: nope` parses but fails cross-file validation — save-with-warning:
    // 201 with the problem surfaced in diagnostics.
    let response = post(
        state,
        "/api/views",
        json!({ "definition": { "id": "b", "type": "board", "field": "nope" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["mutation_caused_warning"], true);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    assert!(read_views(&root).contains("id: b"));
}

// ── Filter change (PATCH /api/views/:id) ─────────────────────────────

#[tokio::test]
async fn patch_filter_updates_where_and_returns_200() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, "views:\n  - id: board\n    type: board\n    field: status\n");

    let response = patch(
        state,
        "/api/views/board",
        json!({ "where_clauses": ["status=open", "title~fix"] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["view_id"], "board");
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);

    let file = read_views(&root);
    assert!(file.contains("status=open"));
    assert!(file.contains("title~fix"));
}

#[tokio::test]
async fn patch_filter_unknown_view_returns_404_with_error() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    let original = "views:\n  - id: board\n    type: board\n    field: status\n";
    write_views(&root, original);

    let response = patch(
        state,
        "/api/views/no-such-view",
        json!({ "where_clauses": ["status=open"] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
    assert_eq!(read_views(&root), original, "file must be untouched");
}

#[tokio::test]
async fn patch_filter_with_unknown_field_saves_with_warning() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, "views:\n  - id: board\n    type: board\n    field: status\n");

    // References a field absent from the schema: parses, fails cross-file
    // validation — save-with-warning, written and surfaced.
    let response = patch(
        state,
        "/api/views/board",
        json!({ "where_clauses": ["nonexistent=x"] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["mutation_caused_warning"], true);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    assert!(read_views(&root).contains("nonexistent=x"));
}
