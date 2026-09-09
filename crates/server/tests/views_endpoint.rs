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
    AppState::new(
        project_root,
        config,
        PathBuf::from(".workdown/config.yaml"),
        None,
    )
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
    assert_eq!(views.len(), 9);
    assert_eq!(views[0]["id"], "status-board");
    assert_eq!(views[0]["kind"], "board");
    assert_eq!(views[1]["id"], "hierarchy");
    assert_eq!(views[1]["kind"], "tree");
    assert_eq!(views[2]["id"], "items-table");
    assert_eq!(views[2]["kind"], "table");
    assert_eq!(views[3]["id"], "project-stats");
    assert_eq!(views[3]["kind"], "metric");
    assert_eq!(views[4]["id"], "items-by-status");
    assert_eq!(views[4]["kind"], "bar_chart");
    assert_eq!(views[5]["id"], "effort-over-time");
    assert_eq!(views[5]["kind"], "line_chart");
    assert_eq!(views[6]["id"], "weekly-load");
    assert_eq!(views[6]["kind"], "workload");
    assert_eq!(views[7]["id"], "load-by-status-team");
    assert_eq!(views[7]["kind"], "heatmap");
    assert_eq!(views[8]["id"], "effort-treemap");
    assert_eq!(views[8]["kind"], "treemap");

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

#[tokio::test]
async fn display_override_replaces_table_columns() {
    let app = router(fixture_state());
    // {"fields":["id","status"]} — overrides the view's configured columns.
    let encoded = "%7B%22fields%22%3A%5B%22id%22%2C%22status%22%5D%7D";
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/views/items-table?display={encoded}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let columns = envelope["data"]["columns"].as_array().expect("columns");
    let column_names: Vec<&str> = columns
        .iter()
        .map(|column| column["name"].as_str().unwrap())
        .collect();
    assert_eq!(column_names, vec!["id", "status"]);
}
