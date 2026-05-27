//! Integration tests for `/api/views` and `/api/views/:id`.
//!
//! Drives the router with `tower::ServiceExt::oneshot` against a
//! checked-in fixture project under `tests/fixtures/project/`. No real
//! server, no browser — just contract pinning for the envelope shape,
//! status codes, and payload structure.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use workdown_core::parser::config::parse_config;
use workdown_server::{router, AppState};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("project")
}

fn fixture_state() -> AppState {
    let project_root = fixture_root();
    let config_yaml = std::fs::read_to_string(project_root.join(".workdown/config.yaml"))
        .expect("read fixture config.yaml");
    let config = parse_config(&config_yaml).expect("parse fixture config.yaml");
    AppState {
        project_root,
        config,
    }
}

async fn body_json(response: axum::http::Response<Body>) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body");
    serde_json::from_slice(&bytes).expect("body parses as JSON")
}

#[tokio::test]
async fn list_views_returns_summary_array() {
    let app = router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/views")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let views = envelope["data"].as_array().expect("data is array");
    assert_eq!(views.len(), 3);
    assert_eq!(views[0]["id"], "status-board");
    assert_eq!(views[0]["kind"], "board");
    assert_eq!(views[1]["id"], "hierarchy");
    assert_eq!(views[1]["kind"], "tree");
    assert_eq!(views[2]["id"], "items-table");
    assert_eq!(views[2]["kind"], "table");

    // Envelope always carries diagnostics, even when empty.
    assert!(envelope["diagnostics"].is_array());
}

#[tokio::test]
async fn get_table_view_returns_table_data_with_resolved_link_titles() {
    let app = router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/views/items-table")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let data = &envelope["data"];
    assert_eq!(data["type"], "table");

    let columns = data["columns"].as_array().expect("columns is array");
    let column_names: Vec<&str> = columns
        .iter()
        .map(|column| column["name"].as_str().unwrap())
        .collect();
    assert_eq!(column_names, vec!["id", "title", "status", "parent"]);

    // Each column carries its schema-derived field type. The virtual
    // `id` column is treated as a String — it has no schema definition.
    let column_types: Vec<&str> = columns
        .iter()
        .map(|column| column["field_type"].as_str().unwrap())
        .collect();
    assert_eq!(column_types, vec!["string", "string", "choice", "link"]);

    // task-b's parent points at task-a, which exists — so the items
    // sidecar resolves task-a's title via the view's `title:` slot.
    let items = data["items"].as_object().expect("items is object");
    assert_eq!(items["task-a"]["title"], "Wire OAuth provider");
}

#[tokio::test]
async fn get_board_view_returns_board_data() {
    let app = router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/views/status-board")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let data = &envelope["data"];
    assert_eq!(data["type"], "board");
    assert_eq!(data["field"], "status");

    // Schema declares three status values plus a trailing synthetic
    // column for items with no value. Items in the fixture cover all
    // three configured values.
    let columns = data["columns"].as_array().expect("columns is array");
    assert_eq!(columns.len(), 4);
    let column_values: Vec<&str> = columns
        .iter()
        .map(|column| column["value"].as_str().unwrap_or("(synthetic)"))
        .collect();
    assert_eq!(
        column_values,
        vec!["open", "in_progress", "done", "(synthetic)"]
    );
}

#[tokio::test]
async fn unknown_view_id_returns_404_with_empty_body() {
    let app = router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/views/no-such-view")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body");
    // 404 returns no body at all per the API decisions — the UI
    // builds any "did you mean…" affordance from the views list it
    // loaded for navigation.
    assert!(bytes.is_empty(), "404 body should be empty, got {bytes:?}");
}
