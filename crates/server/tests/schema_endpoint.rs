//! Integration tests for `GET /api/schema`.
//!
//! Drives the router with `tower::ServiceExt::oneshot` against the
//! checked-in fixture project under `tests/fixtures/project/`, pinning
//! the editing-vocabulary contract the UI editors and create form rely
//! on: field order, per-type metadata, and the item id index.

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
async fn get_schema_returns_fields_in_declaration_order() {
    let app = router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/schema")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let fields = envelope["data"]["fields"]
        .as_array()
        .expect("fields is array");

    // Schema-declaration order is preserved (the IndexMap), matching the
    // order the board uses for columns and the form uses for inputs.
    let names: Vec<&str> = fields
        .iter()
        .map(|field| field["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "id",
            "title",
            "status",
            "parent",
            "effort",
            "start_date",
            "deadline",
            "team",
        ]
    );

    // Envelope always carries diagnostics, even when empty.
    assert!(envelope["diagnostics"].is_array());
}

#[tokio::test]
async fn get_schema_carries_per_type_editor_metadata() {
    let app = router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/schema")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let envelope = body_json(response).await;
    let fields = envelope["data"]["fields"]
        .as_array()
        .expect("fields is array");

    let field = |name: &str| {
        fields
            .iter()
            .find(|field| field["name"] == name)
            .unwrap_or_else(|| panic!("field {name} present"))
            .clone()
    };

    // choice → type + the allowed values list that drives the select.
    let status = field("status");
    assert_eq!(status["field_type"], "choice");
    assert_eq!(status["required"], true);
    let status_values: Vec<&str> = status["values"]
        .as_array()
        .expect("choice carries values")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(status_values, vec!["open", "in_progress", "done"]);

    // link → type only; the editor picks targets from the item index.
    assert_eq!(field("parent")["field_type"], "link");

    // integer → type; min/max absent in the fixture (null, not omitted).
    let effort = field("effort");
    assert_eq!(effort["field_type"], "integer");
    assert!(effort["min"].is_null());
    assert!(effort["max"].is_null());

    // Non-choice fields carry no values list.
    assert!(field("parent")["values"].is_null());
}

#[tokio::test]
async fn get_schema_lists_item_ids_sorted() {
    let app = router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/schema")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let envelope = body_json(response).await;

    // The item id index that populates link/links pickers, sorted for a
    // stable order the UI can render directly.
    let items: Vec<&str> = envelope["data"]["items"]
        .as_array()
        .expect("items is array")
        .iter()
        .map(|item| item.as_str().unwrap())
        .collect();
    assert_eq!(items, vec!["task-a", "task-b", "task-c"]);
}
