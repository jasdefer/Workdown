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

/// A `config.yaml` from the given `project:` block and one of the two
/// file layouts below. Only the project block varies per test, so it is
/// the only thing a test spells out.
fn config_yaml(project_block: &str, layout: &str) -> String {
    format!("{project_block}\n{layout}\n{DEFAULTS}")
}

const DEFAULTS: &str = r#"defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
"#;

/// The fixture project's real layout — the project loads.
const FIXTURE_LAYOUT: &str = r#"paths:
  work_items: workdown-items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml

schema: .workdown/schema.yaml
"#;

/// Paths that exist nowhere under the fixture root — the project cannot
/// load, and every endpoint that needs it answers 422.
const MISSING_LAYOUT: &str = r#"paths:
  work_items: nowhere
  templates: nowhere
  resources: nowhere/resources.yaml
  views: nowhere/views.yaml

schema: nowhere/schema.yaml
"#;

async fn get(state: AppState, uri: &str) -> (StatusCode, Value) {
    let response = router(state)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
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

async fn get_project(state: AppState) -> (StatusCode, Value) {
    get(state, "/api/project").await
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
    let state = state_from_config(&config_yaml(
        "project:\n  name: Unnamed Ambitions\n  description: \"   \"\n",
        FIXTURE_LAYOUT,
    ));
    let (status, envelope) = get_project(state).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(envelope["data"]["name"], "Unnamed Ambitions");
    assert!(envelope["data"]["description"].is_null());
}

#[tokio::test]
async fn get_project_answers_while_the_project_is_unloadable() {
    // Config alone backs this endpoint, so a project that can't load
    // still has a named tab. The config points every path at a directory
    // that does not exist under the fixture root; the views endpoint
    // confirms that is enough to make the project unloadable before the
    // project endpoint is asked to answer anyway.
    let config_yaml = config_yaml("project:\n  name: Broken But Named\n", MISSING_LAYOUT);

    let (views_status, _) = get(state_from_config(&config_yaml), "/api/views").await;
    assert_eq!(views_status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, envelope) = get_project(state_from_config(&config_yaml)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(envelope["data"]["name"], "Broken But Named");
}
