//! Contract tests for `GET /api/schema/definition`.
//!
//! What the payload contains is core's to prove
//! (`schema_definition_data`); this file pins the HTTP contract: the
//! success status and JSON shape against the checked-in fixture, and the
//! 422 tier when the schema file itself does not load — the one failure
//! this endpoint can report, since it reads nothing else.

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

fn state_for(project_root: PathBuf) -> AppState {
    let config_yaml = std::fs::read_to_string(project_root.join(".workdown/config.yaml"))
        .expect("read config.yaml");
    let config = parse_config(&config_yaml).expect("parse config.yaml");
    AppState::new(
        project_root,
        config,
        PathBuf::from(".workdown/config.yaml"),
        None,
    )
}

async fn get_definition(state: AppState) -> axum::http::Response<Body> {
    router(state)
        .oneshot(
            Request::builder()
                .uri("/api/schema/definition")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn body_json(response: axum::http::Response<Body>) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body");
    serde_json::from_slice(&bytes).expect("body parses as JSON")
}

#[tokio::test]
async fn get_schema_definition_returns_definitions_tables_and_hash() {
    let response = get_definition(state_for(fixture_root())).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let data = &envelope["data"];

    let names: Vec<&str> = data["fields"]
        .as_array()
        .expect("fields is an array")
        .iter()
        .map(|field| field["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "id",
            "title",
            "status",
            "parent",
            "depends_on",
            "effort",
            "start_date",
            "deadline",
            "team",
        ]
    );

    // The tagged union and the tagged default reach the wire as such.
    assert_eq!(data["fields"][2]["shape"]["kind"], "values");
    assert_eq!(data["fields"][0]["default"]["kind"], "generator");
    assert_eq!(data["fields"][0]["default"]["generator"], "$filename");

    assert!(data["rules"].is_array());
    for table in [
        "properties_by_type",
        "aggregate_functions_by_type",
        "generators_by_type",
        "widening_by_type",
    ] {
        assert_eq!(
            data[table].as_array().map(Vec::len),
            Some(12),
            "{table} has one row per field type"
        );
    }
    assert_eq!(data["hash"].as_str().map(str::len), Some(64));

    // Envelope always carries diagnostics, even when empty.
    assert!(envelope["diagnostics"].is_array());
}

#[tokio::test]
async fn get_schema_definition_returns_422_when_the_schema_does_not_load() {
    // A throwaway project whose schema.yaml the parser rejects; the
    // fixture stays untouched.
    let temp = tempfile::tempdir().expect("create tempdir");
    let workdown_dir = temp.path().join(".workdown");
    std::fs::create_dir_all(&workdown_dir).unwrap();
    std::fs::copy(
        fixture_root().join(".workdown/config.yaml"),
        workdown_dir.join("config.yaml"),
    )
    .unwrap();
    std::fs::write(
        workdown_dir.join("schema.yaml"),
        "fields:\n  status:\n    type: choice\n",
    )
    .unwrap();

    let response = get_definition(state_for(temp.path().to_path_buf())).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["data"].is_null());
    let diagnostics = envelope["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert_eq!(diagnostics.len(), 1);
    assert!(
        envelope
            .to_string()
            .contains("'values' is required for type 'choice'"),
        "the load diagnostic carries the parser's detail: {envelope}"
    );
}
