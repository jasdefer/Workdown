//! Contract tests for the schema-write endpoints — `POST
//! /api/schema/fields`, `PUT`/`DELETE /api/schema/fields/{name}`, `PUT
//! /api/schema/field-order` — and `GET /api/schema/fields/{name}/usage`.
//!
//! What the operations do to `schema.yaml` is core's to prove
//! (`tests/schema_write.rs` there); this file pins the HTTP contract:
//! one success per endpoint and each status code once. These mutate the
//! schema, so every test runs against a throwaway project in a
//! `TempDir` — never the committed read-only fixture.

use std::fs;
use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

use workdown_core::parser::config::parse_config;
use workdown_core::parser::schema::load_schema;
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
  depends_on:
    type: links
    required: false
    allow_cycles: false
";

fn temp_project() -> (TempDir, AppState) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();

    let config = parse_config(CONFIG).expect("parse config");
    let state = AppState::new(
        root,
        config,
        std::path::PathBuf::from(".workdown/config.yaml"),
        None,
    );
    (directory, state)
}

fn write_item(root: &Path, id: &str, content: &str) {
    fs::write(root.join(format!("workdown-items/{id}.md")), content).unwrap();
}

async fn send(
    state: AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> axum::http::Response<Body> {
    let builder = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap())),
        None => builder.body(Body::empty()),
    }
    .unwrap();
    router(state).oneshot(request).await.unwrap()
}

async fn body_json(response: axum::http::Response<Body>) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body");
    serde_json::from_slice(&bytes).expect("body parses as JSON")
}

/// A plain date field — the smallest definition the write accepts.
fn date_field() -> Value {
    json!({
        "field_type": "date",
        "required": false,
        "description": "When it is due",
        "default": null,
        "resource": null,
        "shape": { "kind": "scalar" }
    })
}

// ── POST /api/schema/fields ──────────────────────────────────────────

#[tokio::test]
async fn post_field_writes_and_returns_201() {
    let (directory, state) = temp_project();

    let response = send(
        state,
        "POST",
        "/api/schema/fields",
        Some(json!({ "name": "due", "definition": date_field() })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["field_name"], "due");
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);
    assert!(envelope["diagnostics"].is_array());
    assert!(envelope.get("error").is_none());
    let written = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert!(written.contains("due:"), "{written}");
}

#[tokio::test]
async fn post_existing_field_returns_409() {
    let (_directory, state) = temp_project();

    let response = send(
        state,
        "POST",
        "/api/schema/fields",
        Some(json!({ "name": "status", "definition": date_field() })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let envelope = body_json(response).await;
    assert!(envelope["data"].is_null());
    assert!(envelope["error"].as_str().unwrap().contains("status"));
}

#[tokio::test]
async fn post_unloadable_field_returns_422_and_writes_nothing() {
    let (directory, state) = temp_project();

    // A choice with no values parses as YAML but fails schema validation.
    let response = send(
        state,
        "POST",
        "/api/schema/fields",
        Some(json!({
            "name": "mood",
            "definition": {
                "field_type": "choice",
                "required": false,
                "shape": { "kind": "values", "values": [] }
            }
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"]
        .as_str()
        .unwrap()
        .contains("'values' must not be empty"));
    let on_disk = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert_eq!(on_disk, SCHEMA);
}

// ── PUT /api/schema/fields/{name} ────────────────────────────────────

#[tokio::test]
async fn put_field_replaces_the_definition_and_returns_200() {
    let (directory, state) = temp_project();

    let response = send(
        state,
        "PUT",
        "/api/schema/fields/title",
        Some(json!({
            "field_type": "string",
            "required": true,
            "description": "The heading",
            "shape": { "kind": "text", "pattern": null }
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["field_name"], "title");
    let written = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert!(written.contains("The heading"), "{written}");
}

#[tokio::test]
async fn put_unknown_field_returns_404_with_error() {
    let (_directory, state) = temp_project();

    let response = send(state, "PUT", "/api/schema/fields/nope", Some(date_field())).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let envelope = body_json(response).await;
    assert!(envelope["error"].as_str().unwrap().contains("nope"));
}

// ── DELETE /api/schema/fields/{name} ─────────────────────────────────

#[tokio::test]
async fn delete_field_saves_with_warning_and_returns_200() {
    let (directory, state) = temp_project();
    write_item(directory.path(), "a", "---\ntitle: Has a title\n---\n");

    let response = send(state, "DELETE", "/api/schema/fields/title", None).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["field_name"], "title");
    // The item keeps its `title:` key and now warns about it — save with
    // warning, flagged as caused by this write.
    assert_eq!(envelope["data"]["mutation_caused_warning"], true);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    let written = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert!(!written.contains("title:"), "{written}");
}

#[tokio::test]
async fn delete_field_with_drop_values_names_the_rewritten_items_and_returns_200() {
    let (directory, state) = temp_project();
    write_item(directory.path(), "a", "---\ntitle: Has a title\n---\n");
    write_item(directory.path(), "b", "---\nstatus: open\n---\n");

    let response = send(
        state,
        "DELETE",
        "/api/schema/fields/title?drop_values=true",
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["field_name"], "title");
    assert_eq!(envelope["data"]["rewritten_items"], json!(["a"]));
    // The value went with the field, so nothing is left to warn about.
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);
    assert!(envelope["diagnostics"].as_array().unwrap().is_empty());
}

// ── PUT /api/schema/field-order ──────────────────────────────────────

#[tokio::test]
async fn put_field_order_rewrites_the_order_and_returns_200() {
    let (directory, state) = temp_project();

    let response = send(
        state,
        "PUT",
        "/api/schema/field-order",
        Some(json!({ "order": ["status", "title", "depends_on", "parent"] })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert!(envelope["data"]["field_name"].is_null());
    let reloaded = load_schema(&directory.path().join(".workdown/schema.yaml")).unwrap();
    let names: Vec<&str> = reloaded.fields.keys().map(String::as_str).collect();
    assert_eq!(names, ["status", "title", "depends_on", "parent"]);
}

// ── GET /api/schema/fields/{name}/usage ──────────────────────────────

#[tokio::test]
async fn get_usage_returns_200_with_the_introduced_diagnostics() {
    let (directory, state) = temp_project();
    write_item(directory.path(), "a", "---\ntitle: Has a title\n---\n");

    let response = send(state, "GET", "/api/schema/fields/title/usage", None).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert!(envelope["data"]["parse_error"].is_null());
    let introduced = envelope["data"]["introduced"].as_array().unwrap();
    assert_eq!(introduced.len(), 1, "{introduced:?}");
    assert_eq!(introduced[0]["scope"], "item");
    assert_eq!(introduced[0]["item_id"], "a");
    // Nothing was written.
    let on_disk = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert_eq!(on_disk, SCHEMA);
}

#[tokio::test]
async fn get_usage_of_unknown_field_returns_404() {
    let (_directory, state) = temp_project();

    let response = send(state, "GET", "/api/schema/fields/nope/usage", None).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
