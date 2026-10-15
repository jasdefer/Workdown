//! Contract tests for the schema-write endpoints — `POST
//! /api/schema/fields`, `PUT`/`DELETE /api/schema/fields/{name}`, `PUT
//! /api/schema/field-order`, `POST /api/schema/field-preview` — and `GET
//! /api/schema/fields/{name}/usage`.
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
async fn post_field_with_a_pull_recipe_writes_the_block_and_the_preview_judges_one() {
    let (directory, state) = temp_project();
    let integer = |recipe: Value| {
        json!({
            "field_type": "integer",
            "required": false,
            "shape": { "kind": "numeric", "min": null, "max": null },
            "recipe": recipe
        })
    };

    let response = send(
        state.clone(),
        "POST",
        "/api/schema/fields",
        Some(json!({ "name": "points", "definition": integer(Value::Null) })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let pull = json!({
        "kind": "pull",
        "over": "depends_on",
        "field": "points",
        "function": "sum",
        "error_on_missing": false
    });
    let response = send(
        state.clone(),
        "POST",
        "/api/schema/fields",
        Some(json!({ "name": "blocked_points", "definition": integer(pull) })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let schema = load_schema(&directory.path().join(".workdown/schema.yaml")).unwrap();
    let written = schema.fields["blocked_points"].pull.as_ref().unwrap();
    assert_eq!(
        (written.over.as_str(), written.field.as_str()),
        ("depends_on", "points")
    );

    // A pull the parser takes but the load disables is the preview's
    // problem, not a parse error.
    let unreducible =
        json!({ "kind": "pull", "over": "parent", "field": "status", "function": "count" });
    let response = send(
        state,
        "POST",
        "/api/schema/field-preview",
        Some(json!({ "name": "blocked_points", "replacing": true, "definition": integer(unreducible) })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert!(envelope["data"]["problem"]
        .as_str()
        .unwrap()
        .contains("no function can reduce"));
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

// ── POST /api/schema/field-preview ───────────────────────────────────

#[tokio::test]
async fn post_field_preview_returns_200_with_the_yaml_and_the_verdict_without_writing() {
    let (directory, state) = temp_project();
    let before = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    let rollup = |over: &str| {
        json!({
            "field_type": "integer",
            "required": false,
            "description": null,
            "default": null,
            "resource": null,
            "shape": { "kind": "numeric", "min": null, "max": null },
            "aggregate": { "function": "sum", "error_on_missing": false, "over": over }
        })
    };

    let response = send(
        state.clone(),
        "POST",
        "/api/schema/field-preview",
        Some(json!({ "name": "points", "replacing": false, "definition": rollup("parent") })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert!(envelope["data"]["yaml"]
        .as_str()
        .unwrap()
        .starts_with("type: integer\n"));
    assert_eq!(envelope["data"]["problem"], Value::Null);

    let response = send(
        state,
        "POST",
        "/api/schema/field-preview",
        Some(json!({ "name": "points", "definition": rollup("depends_on") })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert!(envelope["data"]["problem"]
        .as_str()
        .unwrap()
        .contains("must be 'link'"));

    let after = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert_eq!(after, before);
}

// ── /api/schema/rules ────────────────────────────────────────────────

/// `match: {status: in_progress}`, `require: {<reference>: required}`,
/// as the rows the editor sends.
fn title_rule(name: &str, require_reference: &str) -> Value {
    json!({
        "name": name,
        "description": null,
        "severity": "error",
        "shape": {
            "match_rows": [
                { "reference": "status", "quantifier": null,
                  "test": { "kind": "equals", "value": "in_progress" } }
            ],
            "require_rows": [
                { "reference": require_reference, "test": { "kind": "required" } }
            ],
            "count": null
        }
    })
}

#[tokio::test]
async fn post_rule_writes_and_returns_201() {
    let (directory, state) = temp_project();

    let response = send(
        state,
        "POST",
        "/api/schema/rules",
        Some(title_rule("in-progress-needs-title", "title")),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["rule_name"], "in-progress-needs-title");
    assert_eq!(envelope["data"]["field_name"], Value::Null);
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);
    assert!(envelope["diagnostics"].is_array());
    let written = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert!(
        written.contains("name: in-progress-needs-title"),
        "{written}"
    );
}

#[tokio::test]
async fn post_existing_rule_returns_409() {
    let (_directory, state) = temp_project();
    let rule = title_rule("in-progress-needs-title", "title");
    let first = send(
        state.clone(),
        "POST",
        "/api/schema/rules",
        Some(rule.clone()),
    )
    .await;
    assert_eq!(first.status(), StatusCode::CREATED);

    let response = send(state, "POST", "/api/schema/rules", Some(rule)).await;

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let envelope = body_json(response).await;
    assert!(envelope["error"]
        .as_str()
        .unwrap()
        .contains("already exists"));
}

#[tokio::test]
async fn post_rule_naming_no_field_returns_422() {
    let (_directory, state) = temp_project();

    let response = send(
        state,
        "POST",
        "/api/schema/rules",
        Some(title_rule("needs-nothing", "no_such_field")),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let envelope = body_json(response).await;
    assert!(envelope["error"]
        .as_str()
        .unwrap()
        .contains("no_such_field"));
}

#[tokio::test]
async fn put_rule_replaces_and_returns_200() {
    let (directory, state) = temp_project();
    let created = send(
        state.clone(),
        "POST",
        "/api/schema/rules",
        Some(title_rule("in-progress-needs-title", "title")),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    let response = send(
        state,
        "PUT",
        "/api/schema/rules/in-progress-needs-title",
        Some(title_rule("in-progress-needs-parent", "parent")),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["rule_name"], "in-progress-needs-parent");
    let written = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert!(
        written.contains("name: in-progress-needs-parent"),
        "{written}"
    );
    assert!(!written.contains("in-progress-needs-title"), "{written}");
}

#[tokio::test]
async fn put_unknown_rule_returns_404() {
    let (_directory, state) = temp_project();

    let response = send(
        state,
        "PUT",
        "/api/schema/rules/no-such-rule",
        Some(title_rule("no-such-rule", "title")),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_rule_returns_200() {
    let (directory, state) = temp_project();
    let created = send(
        state.clone(),
        "POST",
        "/api/schema/rules",
        Some(title_rule("in-progress-needs-title", "title")),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);

    let response = send(
        state,
        "DELETE",
        "/api/schema/rules/in-progress-needs-title",
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["rule_name"], "in-progress-needs-title");
    let written = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert!(!written.contains("in-progress-needs-title"), "{written}");
}

#[tokio::test]
async fn post_rule_preview_returns_200_with_the_yaml_and_the_verdict() {
    let (directory, state) = temp_project();
    let before = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();

    let response = send(
        state.clone(),
        "POST",
        "/api/schema/rule-preview",
        Some(json!({
            "replacing": null,
            "definition": title_rule("in-progress-needs-title", "title")
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert!(envelope["data"]["yaml"]
        .as_str()
        .unwrap()
        .starts_with("name: in-progress-needs-title\n"));
    assert_eq!(envelope["data"]["problem"], Value::Null);

    let response = send(
        state,
        "POST",
        "/api/schema/rule-preview",
        Some(json!({ "definition": title_rule("needs-nothing", "no_such_field") })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert!(envelope["data"]["problem"]
        .as_str()
        .unwrap()
        .contains("no_such_field"));

    let after = fs::read_to_string(directory.path().join(".workdown/schema.yaml")).unwrap();
    assert_eq!(after, before);
}
