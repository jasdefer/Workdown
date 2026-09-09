//! The same-origin layer over `/api`: every mutating route refuses a
//! foreign `Origin`, reads and non-browser clients pass. Exercised on
//! the item and timer surfaces, which had no guard before the layer —
//! the git surface has its own tests.

use std::fs;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::json;
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
  graph_field: parent
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
";

fn temp_project() -> (TempDir, AppState) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();
    fs::write(
        root.join("workdown-items/task-1.md"),
        "---\ntitle: Task 1\nstatus: open\n---\n",
    )
    .unwrap();
    let config = parse_config(CONFIG).unwrap();
    let state = AppState::new(root, config, ".workdown/config.yaml".into(), None);
    (directory, state)
}

async fn send(
    state: AppState,
    method: &str,
    uri: &str,
    origin: Option<&str>,
    body: Option<serde_json::Value>,
) -> StatusCode {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }
    let request = match body {
        Some(body) => builder
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    router(state).oneshot(request).await.unwrap().status()
}

#[tokio::test]
async fn foreign_origin_cannot_mutate_an_item() {
    let (_directory, state) = temp_project();
    let mutation = json!({ "op": "replace", "value": "done" });

    let status = send(
        state.clone(),
        "POST",
        "/api/items/task-1/fields/status",
        Some("https://evil.example"),
        Some(mutation.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // The page's own origin, and a client with no origin, both pass.
    let status = send(
        state.clone(),
        "POST",
        "/api/items/task-1/fields/status",
        Some("http://localhost:3141"),
        Some(mutation.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let status = send(
        state,
        "POST",
        "/api/items/task-1/fields/status",
        None,
        Some(json!({ "op": "replace", "value": "open" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn reads_pass_whatever_the_origin() {
    // The browser already withholds a cross-origin read's response from
    // the foreign page; refusing it here would gain nothing and would
    // break nothing either — the layer simply does not look at reads.
    let (_directory, state) = temp_project();
    let status = send(
        state.clone(),
        "GET",
        "/api/project",
        Some("https://evil.example"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let status = send(
        state,
        "GET",
        "/api/items/task-1",
        Some("https://evil.example"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
