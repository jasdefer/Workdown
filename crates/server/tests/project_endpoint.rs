//! Integration tests for `GET /api/project`.
//!
//! Pins the narrow contract the web shell titles its tab from: the
//! project's name and description out of `config.yaml`, and nothing
//! about where the project's files live.

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

fn state_from_config(config_yaml: &str) -> AppState {
    let config = parse_config(config_yaml).expect("parse config.yaml");
    AppState::new(
        fixture_root(),
        config,
        PathBuf::from(".workdown/config.yaml"),
        None,
    )
}

fn fixture_state() -> AppState {
    let config_yaml = std::fs::read_to_string(fixture_root().join(".workdown/config.yaml"))
        .expect("read fixture config.yaml");
    state_from_config(&config_yaml)
}

async fn get_project(state: AppState) -> (StatusCode, Value) {
    let response = router(state)
        .oneshot(
            Request::builder()
                .uri("/api/project")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body");
    (
        status,
        serde_json::from_slice(&bytes).expect("body parses as JSON"),
    )
}

#[tokio::test]
async fn get_project_returns_name_and_description() {
    let (status, envelope) = get_project(fixture_state()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(envelope["data"]["name"], "Fixture Project");
    assert_eq!(
        envelope["data"]["description"],
        "Minimal workdown project used by server HTTP integration tests."
    );
    assert_eq!(envelope["diagnostics"], serde_json::json!([]));
}

#[tokio::test]
async fn get_project_serves_no_file_paths() {
    // The narrow shape is the contract: the browser learns who the
    // project is, never where it lives on disk.
    let (_, envelope) = get_project(fixture_state()).await;
    let data = envelope["data"].as_object().expect("data is an object");
    let mut keys: Vec<&str> = data.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["description", "name"]);
}

#[tokio::test]
async fn get_project_reports_a_blank_description_as_null() {
    // `description: ""` (what `workdown init` writes) and no key at all
    // read the same to a human; both arrive as `null` so the client has
    // one case to handle.
    let state = state_from_config(
        "project:\n  name: Unnamed Ambitions\n  description: \"   \"\n\npaths:\n  work_items: workdown-items\n  templates: .workdown/templates\n  resources: .workdown/resources.yaml\n  views: .workdown/views.yaml\n\nschema: .workdown/schema.yaml\n\ndefaults:\n  board_field: status\n  tree_field: parent\n  graph_field: depends_on\n",
    );
    let (status, envelope) = get_project(state).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(envelope["data"]["name"], "Unnamed Ambitions");
    assert!(envelope["data"]["description"].is_null());
}

#[tokio::test]
async fn get_project_answers_while_the_project_is_unloadable() {
    // Config alone backs this endpoint, so a project that can't load
    // (every other endpoint answering 422) still has a named tab. The
    // fixture root here holds no work items or schema at all.
    let state = state_from_config(
        "project:\n  name: Broken But Named\n\npaths:\n  work_items: nowhere\n  templates: nowhere\n  resources: nowhere/resources.yaml\n  views: nowhere/views.yaml\n\nschema: nowhere/schema.yaml\n\ndefaults:\n  board_field: status\n  tree_field: parent\n  graph_field: depends_on\n",
    );
    let (status, envelope) = get_project(state).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(envelope["data"]["name"], "Broken But Named");
}
