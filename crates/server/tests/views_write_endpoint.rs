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

async fn get(state: AppState, uri: &str) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    router(state).oneshot(request).await.unwrap()
}

async fn body_json(response: axum::http::Response<Body>) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body");
    serde_json::from_slice(&bytes).expect("body parses as JSON")
}

fn write_item(root: &Path, id: &str, content: &str) {
    fs::write(root.join(format!("workdown-items/{id}.md")), content).unwrap();
}

/// Percent-encode a query-param value (used to put a JSON filter in a URL).
/// Encodes everything outside the unreserved set, so the JSON survives the
/// round trip through `axum`'s query parser.
fn encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Build a `?filter=` query string from a JSON clause array.
fn filter_param(clauses: Value) -> String {
    format!("?filter={}", encode(&clauses.to_string()))
}

// ── Create (POST /api/views) ─────────────────────────────────────────

#[tokio::test]
async fn create_view_writes_file_and_returns_201() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();

    let response = post(
        state,
        "/api/views",
        json!({ "name": "Status Board", "definition": { "type": "board", "field": "status" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let envelope = body_json(response).await;
    // Name slugged to the id.
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

    // "Dup" slugs to "dup", which already exists.
    let response = post(
        state,
        "/api/views",
        json!({ "name": "Dup", "definition": { "type": "board", "field": "status" } }),
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
        json!({ "name": "Bare", "definition": { "type": "board" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
}

#[tokio::test]
async fn create_view_blank_name_returns_422() {
    let (_directory, state) = temp_project();

    let response = post(
        state,
        "/api/views",
        json!({ "name": "  ", "definition": { "type": "board", "field": "status" } }),
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
        json!({ "name": "Bad Field", "definition": { "type": "board", "field": "nope" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["mutation_caused_warning"], true);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    assert!(read_views(&root).contains("id: bad-field"));
}

// ── Filter change (PATCH /api/views/:id) ─────────────────────────────

#[tokio::test]
async fn patch_filter_updates_where_and_returns_200() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    // A guided comparison plus a raw passthrough clause.
    let response = patch(
        state,
        "/api/views/board",
        json!({ "clauses": [
            { "kind": "comparison", "field": "status", "operator": "equal", "value": "open" },
            { "kind": "raw", "raw": "title~fix" }
        ] }),
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
        json!({ "clauses": [{ "kind": "raw", "raw": "status=open" }] }),
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
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    // References a field absent from the schema: parses, fails cross-file
    // validation — save-with-warning, written and surfaced.
    let response = patch(
        state,
        "/api/views/board",
        json!({ "clauses": [
            { "kind": "comparison", "field": "nonexistent", "operator": "equal", "value": "x" }
        ] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["mutation_caused_warning"], true);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    assert!(read_views(&root).contains("nonexistent=x"));
}

// ── Preview (GET /api/views/:id?filter=) ─────────────────────────────

#[tokio::test]
async fn preview_filters_view_without_writing() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_item(&root, "task-open", "---\nstatus: open\n---\n");
    write_item(&root, "task-done", "---\nstatus: done\n---\n");
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n",
    );
    let before = read_views(&root);

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "status", "operator": "equal", "value": "done" }
        ]))
    );
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let rows = envelope["data"]["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 1, "ad-hoc filter should keep only done items");
    assert_eq!(rows[0]["id"], "task-done");

    // Preview never persists — the file is untouched.
    assert_eq!(read_views(&root), before);
}

#[tokio::test]
async fn preview_with_unknown_field_is_unrenderable() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n",
    );

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "nope", "operator": "equal", "value": "x" }
        ]))
    );
    let response = get(state, &uri).await;
    // Unrenderable (tier 2) is a 200 with no data + the diagnostic.
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert!(envelope.get("data").is_none());
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn preview_keeps_other_views_diagnostics() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_item(&root, "task-open", "---\nstatus: open\n---\n");
    // A second view with a broken field reference: its diagnostic must
    // survive the preview ("always show all"), pinned to `broken`, not `t`.
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n  - id: broken\n    type: board\n    field: nope\n",
    );

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "status", "operator": "equal", "value": "open" }
        ]))
    );
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert!(envelope.get("data").is_some(), "previewed view renders");
    let diagnostics = envelope["diagnostics"].as_array().unwrap();
    assert!(
        !diagnostics.is_empty(),
        "the other view's diagnostic must not vanish during preview"
    );
}

#[tokio::test]
async fn preview_replaces_stale_persisted_filter_diagnostics() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_item(&root, "task-open", "---\nstatus: open\n---\n");
    // The persisted filter is broken (unknown field) — normally tier-2
    // unrenderable. A valid draft replaces it, so the preview renders and
    // the stale diagnostic about the persisted clause is gone.
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n    where:\n      - \"nonexistent=x\"\n",
    );

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "status", "operator": "equal", "value": "open" }
        ]))
    );
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let rows = envelope["data"]["rows"].as_array().expect("rows array");
    assert_eq!(
        rows.len(),
        1,
        "draft filter applies in place of the broken one"
    );
    assert!(
        envelope["diagnostics"].as_array().unwrap().is_empty(),
        "no stale diagnostic about the replaced persisted clause"
    );
}

#[tokio::test]
async fn preview_with_malformed_filter_returns_422() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n",
    );

    let uri = format!("/api/views/t?filter={}", encode("not json"));
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
}

// ── Seed (GET /api/views/:id/filter) ─────────────────────────────────

#[tokio::test]
async fn get_view_filter_decomposes_persisted_clauses() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=open\"\n      - \"status=open,in_progress\"\n      - \"parent.status=done\"\n",
    );

    let response = get(state, "/api/views/board/filter").await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let clauses = envelope["data"].as_array().expect("clauses array");
    assert_eq!(clauses.len(), 3);
    // A single comparison decomposes to a guided condition.
    assert_eq!(clauses[0]["kind"], "comparison");
    assert_eq!(clauses[0]["field"], "status");
    assert_eq!(clauses[0]["operator"], "equal");
    assert_eq!(clauses[0]["value"], "open");
    // IN (multi-value) folds into one multi-value comparison.
    assert_eq!(clauses[1]["kind"], "comparison");
    assert_eq!(clauses[1]["field"], "status");
    assert_eq!(clauses[1]["operator"], "equal");
    assert_eq!(clauses[1]["value"], "open,in_progress");
    // A cross-relation reference stays raw (guided rows are local-only).
    assert_eq!(clauses[2]["kind"], "raw");
    assert_eq!(clauses[2]["raw"], "parent.status=done");
}

#[tokio::test]
async fn get_view_filter_unknown_view_returns_404() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    let response = get(state, "/api/views/no-such-view/filter").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
