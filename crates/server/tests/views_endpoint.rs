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
    AppState::new(project_root, config)
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
async fn get_treemap_view_rolls_up_size_into_synthetic_root() {
    let app = router(fixture_state());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/views/effort-treemap")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let data = &envelope["data"];
    assert_eq!(data["type"], "treemap");
    assert_eq!(data["group_field"], "parent");
    assert_eq!(data["size_field"], "effort");

    // Synthetic root carries no card and the grand total. The fixture
    // has three items: task-c (effort 8, no parent) and task-a (effort
    // 5, but has child task-b effort 3 — its children's sum overrides
    // its own field). So the grand total is 3 + 8 = 11, and the two
    // top-level children are task-a and task-c.
    let root = &data["root"];
    assert!(root["card"].is_null());
    assert_eq!(root["size"]["type"], "number");
    assert_eq!(root["size"]["value"], 11.0);

    let top_children = root["children"].as_array().expect("children is array");
    assert_eq!(top_children.len(), 2);
    let top_ids: Vec<&str> = top_children
        .iter()
        .map(|child| child["card"]["id"].as_str().unwrap())
        .collect();
    assert!(top_ids.contains(&"task-a"));
    assert!(top_ids.contains(&"task-c"));

    // task-a is an internal node: its size mirrors task-b (3), not its
    // own effort field (5).
    let task_a = top_children
        .iter()
        .find(|child| child["card"]["id"] == "task-a")
        .expect("task-a present");
    assert_eq!(task_a["size"]["value"], 3.0);
    let task_a_children = task_a["children"].as_array().expect("task-a children");
    assert_eq!(task_a_children.len(), 1);
    assert_eq!(task_a_children[0]["card"]["id"], "task-b");
    assert_eq!(task_a_children[0]["size"]["value"], 3.0);

    // Every item has an effort, so nothing is dropped.
    assert!(data["unplaced"].as_array().unwrap().is_empty());
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

#[tokio::test]
async fn display_override_unset_roles_inherit_from_view() {
    let app = router(fixture_state());
    // Override only the title; the view's own columns must survive.
    let encoded = "%7B%22title%22%3A%22status%22%7D"; // {"title":"status"}
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
    assert_eq!(columns.len(), 4, "view's own fields role must survive");

    // task-b's parent resolves task-a via the overridden title role —
    // its status value instead of its title field.
    let items = envelope["data"]["items"].as_object().expect("items");
    assert_eq!(items["task-a"]["title"], "in_progress");
}

#[tokio::test]
async fn malformed_display_parameter_returns_422() {
    let app = router(fixture_state());
    // `bogus` is not a display role — deny_unknown_fields rejects it.
    let encoded = "%7B%22bogus%22%3A%22x%22%7D"; // {"bogus":"x"}
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/api/views/items-table?display={encoded}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
