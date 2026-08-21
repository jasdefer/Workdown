//! Integration tests for the view-authoring write endpoints — `POST
//! /api/views`, `PATCH/PUT/DELETE /api/views/:id` — and their read-back
//! companions (`/filter`, `/definition`).
//!
//! These mutate `views.yaml`, so each test runs against a throwaway
//! project built in a `TempDir` — never the committed read-only fixture
//! under `tests/fixtures/project/`. Drives the router with
//! `tower::ServiceExt::oneshot`, pinning the status-code taxonomy and the
//! save-with-warning behaviour the view-authoring UI relies on.

use std::fs;
use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
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
  # Named by `defaults.graph_field` above: a field role pointing at a
  # field the schema does not define is a `config_check` warning of its
  # own, and these tests assert on view diagnostics.
  depends_on:
    type: links
    required: false
    allow_cycles: false
";

/// Build a throwaway project with no `views.yaml` yet. The returned
/// `TempDir` must outlive the test — dropping it deletes the project.
fn temp_project() -> (TempDir, AppState) {
    temp_project_with_config(CONFIG)
}

/// [`temp_project`] with a custom `config.yaml` — for tests exercising
/// config-sourced behavior like `defaults.display`.
fn temp_project_with_config(config_yaml: &str) -> (TempDir, AppState) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), config_yaml).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();

    let config = parse_config(config_yaml).expect("parse config");
    let state = AppState::new(
        root,
        config,
        std::path::PathBuf::from(".workdown/config.yaml"),
        None,
    );
    (directory, state)
}

fn write_views(root: &Path, content: &str) {
    fs::write(root.join(".workdown/views.yaml"), content).unwrap();
}

fn read_views(root: &Path) -> String {
    fs::read_to_string(root.join(".workdown/views.yaml")).unwrap()
}

async fn post(state: AppState, uri: &str, body: Value) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    router(state).oneshot(request).await.unwrap()
}

async fn patch(state: AppState, uri: &str, body: Value) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("PATCH")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    router(state).oneshot(request).await.unwrap()
}

async fn put(state: AppState, uri: &str, body: Value) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("PUT")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    router(state).oneshot(request).await.unwrap()
}

async fn delete(state: AppState, uri: &str) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("DELETE")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    router(state).oneshot(request).await.unwrap()
}

async fn get(state: AppState, uri: &str) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    router(state).oneshot(request).await.unwrap()
}

async fn body_json(response: axum::http::Response<Body>) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body");
    serde_json::from_slice(&bytes).expect("body parses as JSON")
}

fn write_item(root: &Path, id: &str, content: &str) {
    fs::write(root.join(format!("workdown-items/{id}.md")), content).unwrap();
}

/// Percent-encode a query-param value (used to put a JSON filter in a URL).
/// Encodes everything outside the unreserved set, so the JSON survives the
/// round trip through `axum`'s query parser.
fn encode(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Build a `?filter=` query string from a JSON clause array.
fn filter_param(clauses: Value) -> String {
    format!("?filter={}", encode(&clauses.to_string()))
}

// ── Create (POST /api/views) ─────────────────────────────────────────

#[tokio::test]
async fn create_view_writes_file_and_returns_201() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();

    let response = post(
        state,
        "/api/views",
        json!({ "name": "Status Board", "definition": { "type": "board", "field": "status" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let envelope = body_json(response).await;
    // Name slugged to the id.
    assert_eq!(envelope["data"]["view_id"], "status-board");
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);
    assert!(envelope.get("error").is_none());

    let file = read_views(&root);
    assert!(file.contains("id: status-board"));
    assert!(file.contains("type: board"));
}

#[tokio::test]
async fn create_view_with_existing_id_returns_409() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    let original = "views:\n  - id: dup\n    type: board\n    field: status\n";
    write_views(&root, original);

    // "Dup" slugs to "dup", which already exists.
    let response = post(
        state,
        "/api/views",
        json!({ "name": "Dup", "definition": { "type": "board", "field": "status" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
    assert_eq!(read_views(&root), original, "file must be untouched");
}

#[tokio::test]
async fn create_view_missing_required_slot_returns_422() {
    let (_directory, state) = temp_project();

    // A board with no `field` can't be constructed — hard 422, nothing written.
    let response = post(
        state,
        "/api/views",
        json!({ "name": "Bare", "definition": { "type": "board" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
}

#[tokio::test]
async fn create_view_blank_name_returns_422() {
    let (_directory, state) = temp_project();

    let response = post(
        state,
        "/api/views",
        json!({ "name": "  ", "definition": { "type": "board", "field": "status" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
}

#[tokio::test]
async fn create_view_with_bad_field_reference_saves_with_warning() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();

    // `field: nope` parses but fails cross-file validation — save-with-warning:
    // 201 with the problem surfaced in diagnostics.
    let response = post(
        state,
        "/api/views",
        json!({ "name": "Bad Field", "definition": { "type": "board", "field": "nope" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["mutation_caused_warning"], true);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    assert!(read_views(&root).contains("id: bad-field"));
}

// ── Filter change (PATCH /api/views/:id) ─────────────────────────────

#[tokio::test]
async fn patch_filter_updates_where_and_returns_200() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    // A guided comparison plus a raw passthrough clause.
    let response = patch(
        state,
        "/api/views/board",
        json!({ "clauses": [
            { "kind": "comparison", "field": "status", "operator": "equal", "value": "open" },
            { "kind": "raw", "raw": "title~fix" }
        ] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["view_id"], "board");
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);

    let file = read_views(&root);
    assert!(file.contains("status=open"));
    assert!(file.contains("title~fix"));
}

#[tokio::test]
async fn patch_filter_unknown_view_returns_404_with_error() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    let original = "views:\n  - id: board\n    type: board\n    field: status\n";
    write_views(&root, original);

    let response = patch(
        state,
        "/api/views/no-such-view",
        json!({ "clauses": [{ "kind": "raw", "raw": "status=open" }] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
    assert_eq!(read_views(&root), original, "file must be untouched");
}

#[tokio::test]
async fn patch_filter_with_unknown_field_saves_with_warning() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    // References a field absent from the schema: parses, fails cross-file
    // validation — save-with-warning, written and surfaced.
    let response = patch(
        state,
        "/api/views/board",
        json!({ "clauses": [
            { "kind": "comparison", "field": "nonexistent", "operator": "equal", "value": "x" }
        ] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["mutation_caused_warning"], true);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    assert!(read_views(&root).contains("nonexistent=x"));
}

// ── Preview (GET /api/views/:id?filter=) ─────────────────────────────

#[tokio::test]
async fn preview_filters_view_without_writing() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_item(&root, "task-open", "---\nstatus: open\n---\n");
    write_item(&root, "task-done", "---\nstatus: done\n---\n");
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n",
    );
    let before = read_views(&root);

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "status", "operator": "equal", "value": "done" }
        ]))
    );
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let rows = envelope["data"]["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 1, "ad-hoc filter should keep only done items");
    assert_eq!(rows[0]["id"], "task-done");

    // Preview never persists — the file is untouched.
    assert_eq!(read_views(&root), before);
}

#[tokio::test]
async fn preview_with_unknown_field_is_unrenderable() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n",
    );

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "nope", "operator": "equal", "value": "x" }
        ]))
    );
    let response = get(state, &uri).await;
    // Unrenderable (tier 2) is a 200 with no data + the diagnostic.
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert!(envelope.get("data").is_none());
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
}

/// The severity split at the endpoint: an operand that can never match
/// is a *warning*, so unlike an unknown field it must not push the view
/// into the unrenderable tier. The rows still come back, with the
/// warning riding along in `diagnostics`.
#[tokio::test]
async fn preview_with_unmatchable_value_still_renders_with_a_warning() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n",
    );
    write_item(&root, "task-open", "---\nstatus: open\n---\n");

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "status", "operator": "equal", "value": "nonsense" }
        ]))
    );
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    // Tier 3, not tier 2: data is present.
    let rows = envelope["data"]["rows"].as_array().expect("rows array");
    assert!(rows.is_empty(), "the filter genuinely matches nothing");

    let diagnostics = envelope["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0]["severity"], "warning");
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap()
            .contains("nonsense"),
        "{diagnostics:?}"
    );
}

/// Column names of a table response, for the display-role tests below.
async fn column_names(response: axum::http::Response<Body>) -> Vec<String> {
    let envelope = body_json(response).await;
    envelope["data"]["columns"]
        .as_array()
        .expect("columns array")
        .iter()
        .map(|column| column["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn display_override_with_stale_field_drops_it() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n",
    );

    // A stored override can outlive its field. The stale name must be
    // dropped at extraction (not panic, not 422 — the override as a
    // whole is well-formed); the surviving names still apply.
    let uri = format!(
        "/api/views/t?display={}",
        encode(r#"{"fields":["ghost","status"]}"#)
    );
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(column_names(response).await, vec!["status"]);
}

#[tokio::test]
async fn config_display_defaults_inherited_by_bare_view() {
    // Rung 3 of the resolution ladder, end-to-end through serve: a view
    // with no display block inherits `defaults.display` from
    // config.yaml.
    let config_yaml = format!("{CONFIG}  display:\n    fields: [status]\n");
    let (directory, state) = temp_project_with_config(&config_yaml);
    let root = directory.path().to_path_buf();
    write_views(&root, "views:\n  - id: t\n    type: table\n");

    let response = get(state, "/api/views/t").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(column_names(response).await, vec!["status"]);
}

#[tokio::test]
async fn view_display_beats_config_defaults() {
    // Rung 2 shadows rung 3: a view's own display block wins over the
    // project-wide default for the roles it sets.
    let config_yaml = format!("{CONFIG}  display:\n    fields: [status]\n");
    let (directory, state) = temp_project_with_config(&config_yaml);
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: own\n    type: table\n    display:\n      fields: [id]\n",
    );

    let response = get(state, "/api/views/own").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(column_names(response).await, vec!["id"]);
}

#[tokio::test]
async fn malformed_display_on_unrenderable_view_returns_422() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    // The view itself is broken (unknown board field) — normally tier-2
    // unrenderable. A malformed `?display=` must still be rejected with
    // 422, exactly like a malformed `?filter=`: parameter validation
    // happens before the unrenderable check.
    write_views(
        &root,
        "views:\n  - id: broken\n    type: board\n    field: nope\n",
    );

    let uri = format!("/api/views/broken?display={}", encode("{not json"));
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn preview_keeps_other_views_diagnostics() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_item(&root, "task-open", "---\nstatus: open\n---\n");
    // A second view with a broken field reference: its diagnostic must
    // survive the preview ("always show all"), pinned to `broken`, not `t`.
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n  - id: broken\n    type: board\n    field: nope\n",
    );

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "status", "operator": "equal", "value": "open" }
        ]))
    );
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert!(envelope.get("data").is_some(), "previewed view renders");
    let diagnostics = envelope["diagnostics"].as_array().unwrap();
    assert!(
        !diagnostics.is_empty(),
        "the other view's diagnostic must not vanish during preview"
    );
}

#[tokio::test]
async fn preview_replaces_stale_persisted_filter_diagnostics() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_item(&root, "task-open", "---\nstatus: open\n---\n");
    // The persisted filter is broken (unknown field) — normally tier-2
    // unrenderable. A valid draft replaces it, so the preview renders and
    // the stale diagnostic about the persisted clause is gone.
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n    where:\n      - \"nonexistent=x\"\n",
    );

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "status", "operator": "equal", "value": "open" }
        ]))
    );
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let rows = envelope["data"]["rows"].as_array().expect("rows array");
    assert_eq!(
        rows.len(),
        1,
        "draft filter applies in place of the broken one"
    );
    assert!(
        envelope["diagnostics"].as_array().unwrap().is_empty(),
        "no stale diagnostic about the replaced persisted clause"
    );
}

#[tokio::test]
async fn preview_with_malformed_filter_returns_422() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n",
    );

    let uri = format!("/api/views/t?filter={}", encode("not json"));
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
}

#[tokio::test]
async fn preview_with_arity_mismatched_condition_returns_422() {
    // `in` carries its members in `values`; a scalar `value` is a
    // malformed request the guided builder cannot produce, so it is
    // rejected outright rather than previewed or saved-with-warning.
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: t\n    type: table\n    display:\n      fields: [id, status]\n",
    );

    let uri = format!(
        "/api/views/t{}",
        filter_param(json!([
            { "kind": "comparison", "field": "status", "operator": "in", "value": "open" }
        ]))
    );
    let response = get(state, &uri).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
}

#[tokio::test]
async fn patch_filter_with_comma_member_returns_422_and_writes_nothing() {
    // A comma inside an `in` member cannot be represented in the clause
    // text (members are comma-separated, with no escaping): the
    // serializer refuses it, the endpoint maps that to a hard 422, and
    // the file stays exactly as it was.
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    let original = "views:\n  - id: board\n    type: board\n    field: status\n";
    write_views(&root, original);

    let response = patch(
        state,
        "/api/views/board",
        json!({ "clauses": [
            { "kind": "comparison", "field": "status", "operator": "in",
              "values": ["needs review, blocked", "done"] }
        ] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
    assert_eq!(read_views(&root), original, "file must be untouched");
}

// ── Update (PUT /api/views/:id) ──────────────────────────────────────

const TWO_VIEWS: &str = "\
views:
  - id: first
    type: board
    field: status
  - id: second
    type: tree
    field: parent
";

#[tokio::test]
async fn put_replaces_definition_and_returns_200() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, TWO_VIEWS);

    // A kind switch (board → tree) with a filter, name untouched.
    let response = put(
        state,
        "/api/views/first",
        json!({
            "definition": { "type": "tree", "field": "parent" },
            "filter": [
                { "kind": "comparison", "field": "status", "operator": "equal", "value": "open" }
            ]
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["view_id"], "first");
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);

    let file = read_views(&root);
    assert!(file.contains("type: tree"), "{file}");
    assert!(file.contains("status=open"), "{file}");
    let first_position = file.find("id: first").unwrap();
    let second_position = file.find("id: second").unwrap();
    assert!(
        first_position < second_position,
        "position preserved: {file}"
    );
}

#[tokio::test]
async fn put_with_name_renames_the_view() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, TWO_VIEWS);
    // A stale rendered file for the old id — must be removed by the rename.
    fs::create_dir_all(root.join("views")).unwrap();
    fs::write(root.join("views/first.md"), "# rendered\n").unwrap();

    let response = put(
        state,
        "/api/views/first",
        json!({
            "name": "Sprint Board",
            "definition": { "type": "board", "field": "status" }
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["view_id"], "sprint-board");
    let info_messages = envelope["data"]["info_messages"].as_array().unwrap();
    assert_eq!(info_messages.len(), 1, "{info_messages:?}");

    let file = read_views(&root);
    assert!(file.contains("id: sprint-board"), "{file}");
    assert!(!file.contains("id: first"), "{file}");
    assert!(!root.join("views/first.md").exists());
}

#[tokio::test]
async fn put_rename_to_existing_id_returns_409() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, TWO_VIEWS);

    let response = put(
        state,
        "/api/views/first",
        json!({
            "name": "Second",
            "definition": { "type": "board", "field": "status" }
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let envelope = body_json(response).await;
    assert!(envelope["error"].is_string());
    assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
}

#[tokio::test]
async fn put_unknown_view_returns_404() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, TWO_VIEWS);

    let response = put(
        state,
        "/api/views/no-such-view",
        json!({ "definition": { "type": "board", "field": "status" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
}

#[tokio::test]
async fn put_missing_required_slot_returns_422() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, TWO_VIEWS);

    let response = put(
        state,
        "/api/views/first",
        json!({ "definition": { "type": "board" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
}

#[tokio::test]
async fn put_with_bad_field_reference_saves_with_warning() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, TWO_VIEWS);

    let response = put(
        state,
        "/api/views/first",
        json!({ "definition": { "type": "board", "field": "nope" } }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["mutation_caused_warning"], true);
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    assert!(read_views(&root).contains("field: nope"));
}

// ── Delete (DELETE /api/views/:id) ───────────────────────────────────

#[tokio::test]
async fn delete_removes_view_and_rendered_file() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, TWO_VIEWS);
    fs::create_dir_all(root.join("views")).unwrap();
    fs::write(root.join("views/first.md"), "# rendered\n").unwrap();

    let response = delete(state, "/api/views/first").await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["view_id"], "first");
    let info_messages = envelope["data"]["info_messages"].as_array().unwrap();
    assert_eq!(info_messages.len(), 1, "{info_messages:?}");

    let file = read_views(&root);
    assert!(!file.contains("id: first"), "{file}");
    assert!(file.contains("id: second"), "{file}");
    assert!(!root.join("views/first.md").exists());
}

#[tokio::test]
async fn delete_unknown_view_returns_404() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(&root, TWO_VIEWS);

    let response = delete(state, "/api/views/no-such-view").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
}

// ── Seed (GET /api/views/:id/definition) ─────────────────────────────

#[tokio::test]
async fn get_view_definition_returns_the_put_payload_shape() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n    display:\n      title: title\n    where:\n      - \"status=open\"\n",
    );

    let response = get(state, "/api/views/board/definition").await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let definition = &envelope["data"]["definition"];
    assert_eq!(definition["type"], "board");
    assert_eq!(definition["field"], "status");
    assert_eq!(definition["display"]["title"], "title");
    // `id` and `where` travel outside the definition: the id in the path,
    // the filter as structured clauses.
    assert!(definition.get("id").is_none(), "{definition}");
    assert!(definition.get("where").is_none(), "{definition}");

    let filter = envelope["data"]["filter"].as_array().unwrap();
    assert_eq!(filter.len(), 1);
    assert_eq!(filter[0]["kind"], "comparison");
    assert_eq!(filter[0]["field"], "status");
    assert_eq!(filter[0]["operator"], "equal");
    assert_eq!(filter[0]["value"], "open");
}

#[tokio::test]
async fn get_view_definition_unknown_view_returns_404() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    let response = get(state, "/api/views/no-such-view/definition").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

/// The definition round-trips: what `/definition` returns, `PUT` accepts
/// unchanged — the contract the edit form is built on. The view is
/// created through the API first, so the byte comparison is between two
/// serializer-written files, not a hand-written one and a rewrite.
#[tokio::test]
async fn definition_round_trips_through_put() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    let response = post(
        state.clone(),
        "/api/views",
        json!({
            "name": "Board",
            "definition": { "type": "board", "field": "status" },
            "filter": [
                { "kind": "comparison", "field": "status", "operator": "equal", "value": "open" }
            ]
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let before = read_views(&root);

    let response = get(state.clone(), "/api/views/board/definition").await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;

    let response = put(
        state,
        "/api/views/board",
        json!({
            "definition": envelope["data"]["definition"],
            "filter": envelope["data"]["filter"]
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        read_views(&root),
        before,
        "an untouched round-trip is a no-op"
    );
}

/// Per-row metric filters get the same round-trip contract: they leave as
/// structured `filter` clauses, never as raw `where` strings, and PUTting
/// the seed back unchanged is a no-op.
#[tokio::test]
async fn definition_round_trips_metric_row_filters_through_put() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    let response = post(
        state.clone(),
        "/api/views",
        json!({
            "name": "Stats",
            "definition": {
                "type": "metric",
                "metrics": [
                    {
                        "label": "Open",
                        "aggregate": "count",
                        "filter": [
                            { "kind": "comparison", "field": "status", "operator": "equal", "value": "open" }
                        ]
                    },
                    { "aggregate": "count" }
                ]
            }
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let before = read_views(&root);
    assert!(before.contains("status=open"), "{before}");

    let response = get(state.clone(), "/api/views/stats/definition").await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    let rows = envelope["data"]["definition"]["metrics"]
        .as_array()
        .unwrap();
    assert_eq!(rows[0]["filter"][0]["field"], "status");
    assert_eq!(rows[0]["filter"][0]["value"], "open");
    assert!(rows[0].get("where").is_none(), "{rows:?}");
    assert_eq!(rows[1]["filter"].as_array().unwrap().len(), 0);

    let response = put(
        state,
        "/api/views/stats",
        json!({
            "definition": envelope["data"]["definition"],
            "filter": envelope["data"]["filter"]
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        read_views(&root),
        before,
        "an untouched round-trip is a no-op"
    );
}

/// Renaming a view with a long-standing warning must not report the
/// warning as introduced by the rename — the diagnostic's identity moves
/// to the new id, but nothing new went wrong.
#[tokio::test]
async fn put_rename_with_preexisting_warning_does_not_flag_mutation() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: nope\n",
    );

    let response = put(
        state,
        "/api/views/board",
        json!({
            "name": "Sprint Board",
            "definition": { "type": "board", "field": "nope" }
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["view_id"], "sprint-board");
    assert_eq!(envelope["data"]["mutation_caused_warning"], false);
    assert!(
        !envelope["diagnostics"].as_array().unwrap().is_empty(),
        "the pre-existing warning still rides in diagnostics"
    );
}

/// The seed endpoints read `views.yaml` alone: a broken schema (which
/// blocks rendering and writing) must not block reading a definition back
/// — the editor can always show what's there.
#[tokio::test]
async fn get_view_definition_works_with_a_broken_schema() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );
    fs::write(root.join(".workdown/schema.yaml"), "fields: [not a mapping").unwrap();

    let response = get(state, "/api/views/board/definition").await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["definition"]["type"], "board");
}

// ── Seed (GET /api/views/:id/filter) ─────────────────────────────────

#[tokio::test]
async fn get_view_filter_decomposes_persisted_clauses() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=open\"\n      - \"status in open,in_progress\"\n      - \"status not in done\"\n      - \"parent.status=done\"\n",
    );

    let response = get(state, "/api/views/board/filter").await;
    assert_eq!(response.status(), StatusCode::OK);

    let envelope = body_json(response).await;
    let clauses = envelope["data"].as_array().expect("clauses array");
    assert_eq!(clauses.len(), 4);
    // A single comparison decomposes to a guided condition.
    assert_eq!(clauses[0]["kind"], "comparison");
    assert_eq!(clauses[0]["field"], "status");
    assert_eq!(clauses[0]["operator"], "equal");
    assert_eq!(clauses[0]["value"], "open");
    // A membership test folds back into one condition carrying its members as
    // a list, with the scalar slot null.
    assert_eq!(clauses[1]["kind"], "comparison");
    assert_eq!(clauses[1]["field"], "status");
    assert_eq!(clauses[1]["operator"], "in");
    assert!(clauses[1]["value"].is_null());
    assert_eq!(clauses[1]["values"][0], "open");
    assert_eq!(clauses[1]["values"][1], "in_progress");
    // A one-member `not in` keeps its operator rather than collapsing to `!=`.
    assert_eq!(clauses[2]["operator"], "not_in");
    assert_eq!(clauses[2]["values"][0], "done");
    // A cross-relation reference stays raw (guided rows are local-only).
    assert_eq!(clauses[3]["kind"], "raw");
    assert_eq!(clauses[3]["raw"], "parent.status=done");
}

#[tokio::test]
async fn get_view_filter_unknown_view_returns_404() {
    let (directory, state) = temp_project();
    let root = directory.path().to_path_buf();
    write_views(
        &root,
        "views:\n  - id: board\n    type: board\n    field: status\n",
    );

    let response = get(state, "/api/views/no-such-view/filter").await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
