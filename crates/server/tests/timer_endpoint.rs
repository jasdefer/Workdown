//! Integration tests for the timer endpoints (`GET /api/timer`,
//! `POST /api/timer/start`, `POST /api/timer/stop`,
//! `POST /api/timer/break/end`).
//!
//! Stop mutates files, so each test runs against a throwaway project in
//! a `TempDir`. Time is driven by a `ManualClock` handle injected into
//! the state's `TimerService`, so no test ever waits. Drives the router
//! with `tower::ServiceExt::oneshot`, pinning the refusal taxonomy
//! (start conflict, stop with nothing running or during a break, break
//! end without a break), the needs-confirmation round trip and how far
//! a confirmation carries, the write behaviour on stop, and the
//! pomodoro loop through work, break and out again.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::TimeZone;
use serde_json::{json, Value};
use tempfile::TempDir;
use tower::ServiceExt;

use workdown_core::parser::config::parse_config;
use workdown_server::timer::{Clock, ManualClock, TimerService};
use workdown_server::{router, AppState};

const CONFIG_BASE: &str = "\
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

/// The effort field aggregates over `parent`, so start on an item with
/// children takes the confirmation round trip while leaves start plainly.
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
    allow_cycles: false
    inverse: children
  effort:
    type: duration
    required: false
    aggregate:
      function: sum
      over: parent
";

fn config_with_effort_field(field: &str) -> String {
    format!("{CONFIG_BASE}  effort_field: {field}\n")
}

/// Throwaway project: `epic` with child `task-a` (no effort value), the
/// standalone leaf `task-b` carrying `effort: 2h`, and a second
/// confirmation-qualifying pair `epic-b`/`task-c` for the tests on how
/// far a confirmation carries. The returned `TempDir` must be held for
/// the test's lifetime; the `ManualClock` handle drives every elapsed
/// second.
fn temp_project(config: &str) -> (TempDir, AppState, Arc<ManualClock>) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), config).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();
    fs::write(
        root.join("workdown-items/epic.md"),
        "---\ntitle: Epic\nstatus: open\n---\n",
    )
    .unwrap();
    fs::write(
        root.join("workdown-items/task-a.md"),
        "---\ntitle: Task A\nstatus: open\nparent: epic\n---\n",
    )
    .unwrap();
    fs::write(
        root.join("workdown-items/task-b.md"),
        "---\ntitle: Task B\nstatus: open\neffort: 2h\n---\n",
    )
    .unwrap();
    fs::write(
        root.join("workdown-items/epic-b.md"),
        "---\ntitle: Epic B\nstatus: open\n---\n",
    )
    .unwrap();
    fs::write(
        root.join("workdown-items/task-c.md"),
        "---\ntitle: Task C\nstatus: open\nparent: epic-b\n---\n",
    )
    .unwrap();

    let clock = Arc::new(ManualClock::starting_at(
        chrono::Utc.with_ymd_and_hms(2026, 8, 21, 9, 0, 0).unwrap(),
    ));
    let parsed = parse_config(config).expect("parse config");
    let mut state = AppState::new(
        root,
        parsed,
        std::path::PathBuf::from(".workdown/config.yaml"),
        None,
    );
    state.timer = Arc::new(TimerService::new(Arc::clone(&clock) as Arc<dyn Clock>));
    (directory, state, clock)
}

fn read_item(root: &Path, id: &str) -> String {
    fs::read_to_string(root.join(format!("workdown-items/{id}.md"))).unwrap()
}

async fn get(state: AppState, uri: &str) -> axum::http::Response<Body> {
    let request = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    router(state).oneshot(request).await.unwrap()
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

async fn body_json(response: axum::http::Response<Body>) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collect body");
    serde_json::from_slice(&bytes).expect("body parses as JSON")
}

async fn start(state: AppState, body: Value) -> axum::http::Response<Body> {
    post(state, "/api/timer/start", body).await
}

async fn stop(state: AppState) -> axum::http::Response<Body> {
    post(state, "/api/timer/stop", json!({})).await
}

// ── GET /api/timer: the effort-field states ─────────────────────────

#[tokio::test]
async fn get_reports_ready_field_and_no_running_timer() {
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("effort"));

    let response = get(state, "/api/timer").await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["effort_field"]["state"], "ready");
    assert_eq!(envelope["data"]["effort_field"]["field"], "effort");
    assert_eq!(envelope["data"]["phase"]["phase"], "idle");
    // The sticky mode is present even when idle — the split button's
    // default, stopwatch until a pomodoro session has ever started.
    assert_eq!(envelope["data"]["last_mode"], "stopwatch");
}

#[tokio::test]
async fn get_reports_unconfigured_when_key_is_unset() {
    let (_dir, state, _clock) = temp_project(CONFIG_BASE);

    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["effort_field"]["state"], "unconfigured");
}

#[tokio::test]
async fn get_reports_invalid_for_a_non_duration_field() {
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("status"));

    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["effort_field"]["state"], "invalid");
    assert_eq!(envelope["data"]["effort_field"]["field"], "status");
    assert!(envelope["data"]["effort_field"]["problem"]
        .as_str()
        .unwrap()
        .contains("duration"));
}

// ── POST /api/timer/start ────────────────────────────────────────────

#[tokio::test]
async fn start_on_a_leaf_starts_and_get_reports_it_running() {
    let (_dir, state, clock) = temp_project(&config_with_effort_field("effort"));

    let response = start(
        state.clone(),
        json!({ "item": "task-a", "mode": "stopwatch" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["outcome"], "started");
    assert_eq!(envelope["data"]["timer"]["phase"]["phase"], "work");
    assert_eq!(envelope["data"]["timer"]["phase"]["item_id"], "task-a");
    assert_eq!(envelope["data"]["timer"]["phase"]["mode"], "stopwatch");
    // The stopwatch counts toward nothing — no countdown target.
    assert_eq!(
        envelope["data"]["timer"]["phase"]["phase_length_seconds"],
        Value::Null
    );
    // No effort value on the item yet — the projected write starts at zero.
    assert_eq!(
        envelope["data"]["timer"]["phase"]["effort_before_seconds"],
        Value::Null
    );

    clock.advance(chrono::Duration::seconds(95));
    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["item_id"], "task-a");
    assert_eq!(envelope["data"]["phase"]["elapsed_seconds"], 95);
}

#[tokio::test]
async fn start_reports_the_existing_effort_value_as_the_write_basis() {
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("effort"));

    let envelope =
        body_json(start(state, json!({ "item": "task-b", "mode": "stopwatch" })).await).await;
    assert_eq!(
        envelope["data"]["timer"]["phase"]["effort_before_seconds"],
        7200
    );
}

#[tokio::test]
async fn the_projected_write_basis_ignores_a_rolled_up_value() {
    let (dir, state, _clock) = temp_project(&config_with_effort_field("effort"));
    // Give the child a value so `epic` carries a rolled-up `1h`. The
    // roll-up is derived, so the projection must not start from it —
    // the stop's delta reads the frontmatter and starts from zero.
    fs::write(
        dir.path().join("workdown-items/task-a.md"),
        "---\ntitle: Task A\nstatus: open\nparent: epic\neffort: 1h\n---\n",
    )
    .unwrap();

    let envelope = body_json(
        start(
            state,
            json!({ "item": "epic", "mode": "stopwatch", "confirmed": true }),
        )
        .await,
    )
    .await;
    assert_eq!(envelope["data"]["outcome"], "started");
    assert_eq!(
        envelope["data"]["timer"]["phase"]["effort_before_seconds"],
        Value::Null
    );
}

#[tokio::test]
async fn start_while_a_timer_runs_is_a_conflict() {
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("effort"));
    start(
        state.clone(),
        json!({ "item": "task-a", "mode": "stopwatch" }),
    )
    .await;

    let response = start(state, json!({ "item": "task-b", "mode": "stopwatch" })).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let envelope = body_json(response).await;
    assert!(envelope["error"].as_str().unwrap().contains("task-a"));
}

#[tokio::test]
async fn start_while_a_timer_runs_refuses_before_asking_for_confirmation() {
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("effort"));
    start(
        state.clone(),
        json!({ "item": "task-a", "mode": "stopwatch" }),
    )
    .await;

    // `epic` qualifies for the roll-up confirmation, but with a timer
    // already running the answer must be the conflict — not a dialog
    // whose confirmed retry could only meet this same refusal.
    let response = start(state, json!({ "item": "epic", "mode": "stopwatch" })).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn start_on_an_unknown_item_is_not_found() {
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("effort"));

    let response = start(state, json!({ "item": "task-z", "mode": "stopwatch" })).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn start_with_no_effort_field_is_refused() {
    let (_dir, state, _clock) = temp_project(CONFIG_BASE);

    let response = start(state, json!({ "item": "task-a", "mode": "stopwatch" })).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn start_without_a_mode_is_rejected() {
    // The mode is required on every start request — no default.
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("effort"));

    let response = start(state.clone(), json!({ "item": "task-a" })).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    // Nothing started.
    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["phase"], "idle");
}

#[tokio::test]
async fn pomodoro_start_reports_the_mode_and_the_countdown_target() {
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("effort"));

    let envelope =
        body_json(start(state, json!({ "item": "task-a", "mode": "pomodoro" })).await).await;
    assert_eq!(envelope["data"]["outcome"], "started");
    assert_eq!(envelope["data"]["timer"]["phase"]["phase"], "work");
    assert_eq!(envelope["data"]["timer"]["phase"]["mode"], "pomodoro");
    assert_eq!(
        envelope["data"]["timer"]["phase"]["phase_length_seconds"],
        25 * 60
    );
    // The start set the sticky mode.
    assert_eq!(envelope["data"]["timer"]["last_mode"], "pomodoro");
}

#[tokio::test]
async fn start_on_an_item_with_children_takes_the_confirmation_round_trip() {
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("effort"));

    // First leg: refused as a typed outcome, nothing started.
    let envelope = body_json(
        start(
            state.clone(),
            json!({ "item": "epic", "mode": "stopwatch" }),
        )
        .await,
    )
    .await;
    assert_eq!(envelope["data"]["outcome"], "needs_confirmation");
    let envelope = body_json(get(state.clone(), "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["phase"], "idle");

    // Second leg: the same request, confirmed.
    let envelope = body_json(
        start(
            state,
            json!({ "item": "epic", "mode": "stopwatch", "confirmed": true }),
        )
        .await,
    )
    .await;
    assert_eq!(envelope["data"]["outcome"], "started");
    assert_eq!(envelope["data"]["timer"]["phase"]["item_id"], "epic");
}

// ── POST /api/timer/stop ─────────────────────────────────────────────

#[tokio::test]
async fn stop_writes_the_rounded_delta_and_reports_before_and_after() {
    let (dir, state, clock) = temp_project(&config_with_effort_field("effort"));
    start(
        state.clone(),
        json!({ "item": "task-b", "mode": "stopwatch" }),
    )
    .await;

    // 40min 20s of measured time rounds down to 40min.
    clock.advance(chrono::Duration::seconds(40 * 60 + 20));
    let response = stop(state.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["item_id"], "task-b");
    assert_eq!(envelope["data"]["field"], "effort");
    assert_eq!(envelope["data"]["elapsed_seconds"], 2420);
    assert_eq!(envelope["data"]["write"]["added_seconds"], 2400);
    assert_eq!(envelope["data"]["write"]["previous_value"], "2h");
    assert_eq!(envelope["data"]["write"]["previous_seconds"], 7200);
    assert_eq!(envelope["data"]["write"]["new_seconds"], 9600);

    let file = read_item(dir.path(), "task-b");
    assert!(file.contains("effort: 2h 40min"), "{file}");

    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["phase"], "idle");
}

#[tokio::test]
async fn stop_on_an_absent_effort_field_starts_from_zero() {
    let (dir, state, clock) = temp_project(&config_with_effort_field("effort"));
    start(
        state.clone(),
        json!({ "item": "task-a", "mode": "stopwatch" }),
    )
    .await;

    clock.advance(chrono::Duration::seconds(120));
    let envelope = body_json(stop(state).await).await;
    assert_eq!(envelope["data"]["write"]["previous_value"], Value::Null);
    assert_eq!(envelope["data"]["write"]["previous_seconds"], Value::Null);
    assert_eq!(envelope["data"]["write"]["new_seconds"], 120);

    let file = read_item(dir.path(), "task-a");
    assert!(file.contains("effort: 2min"), "{file}");
}

#[tokio::test]
async fn stop_under_half_a_minute_stops_but_writes_nothing() {
    let (dir, state, clock) = temp_project(&config_with_effort_field("effort"));
    start(
        state.clone(),
        json!({ "item": "task-a", "mode": "stopwatch" }),
    )
    .await;

    clock.advance(chrono::Duration::seconds(29));
    let response = stop(state.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["elapsed_seconds"], 29);
    assert_eq!(envelope["data"]["write"], Value::Null);

    // Nothing written, and the timer is gone.
    let file = read_item(dir.path(), "task-a");
    assert!(!file.contains("effort:"), "{file}");
    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["phase"], "idle");
}

#[tokio::test]
async fn stop_with_no_timer_is_a_conflict() {
    let (_dir, state, _clock) = temp_project(&config_with_effort_field("effort"));

    let response = stop(state).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn stop_on_a_deleted_item_keeps_the_timer_running() {
    let (dir, state, clock) = temp_project(&config_with_effort_field("effort"));
    start(
        state.clone(),
        json!({ "item": "task-a", "mode": "stopwatch" }),
    )
    .await;
    clock.advance(chrono::Duration::seconds(60));

    fs::remove_file(dir.path().join("workdown-items/task-a.md")).unwrap();
    let response = stop(state.clone()).await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Measured time was not discarded — the timer still runs and counts.
    clock.advance(chrono::Duration::seconds(60));
    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["item_id"], "task-a");
    assert_eq!(envelope["data"]["phase"]["elapsed_seconds"], 120);
}

// ── The pomodoro loop: break, break end, the confirmation carry ──────

/// Start a pomodoro interval on `item`, run it for 26 minutes and stop
/// it — leaving the server in a break following `item`.
async fn run_into_a_break(state: &AppState, clock: &ManualClock, item: &str) {
    let response = start(
        state.clone(),
        json!({ "item": item, "mode": "pomodoro", "confirmed": true }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    clock.advance(chrono::Duration::seconds(26 * 60));
    let response = stop(state.clone()).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn pomodoro_stop_writes_and_begins_the_break() {
    let (dir, state, clock) = temp_project(&config_with_effort_field("effort"));
    start(
        state.clone(),
        json!({ "item": "task-b", "mode": "pomodoro" }),
    )
    .await;

    // 32 minutes — the countdown paced the work, the measurement is
    // what lands: on top of task-b's existing 2h.
    clock.advance(chrono::Duration::seconds(32 * 60));
    let envelope = body_json(stop(state.clone()).await).await;
    assert_eq!(envelope["data"]["write"]["added_seconds"], 32 * 60);
    assert_eq!(envelope["data"]["write"]["new_seconds"], 7200 + 32 * 60);
    let file = read_item(dir.path(), "task-b");
    assert!(file.contains("effort: 2h 32m"), "{file}");

    // The write landed as the break began.
    clock.advance(chrono::Duration::seconds(60));
    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["phase"], "break");
    assert_eq!(envelope["data"]["phase"]["followed_item"], "task-b");
    assert_eq!(envelope["data"]["phase"]["elapsed_seconds"], 60);
    assert_eq!(envelope["data"]["phase"]["phase_length_seconds"], 5 * 60);
    assert_eq!(envelope["data"]["last_mode"], "pomodoro");
}

#[tokio::test]
async fn pomodoro_stop_under_half_a_minute_goes_idle_without_a_break() {
    let (dir, state, clock) = temp_project(&config_with_effort_field("effort"));
    start(
        state.clone(),
        json!({ "item": "task-a", "mode": "pomodoro" }),
    )
    .await;

    clock.advance(chrono::Duration::seconds(29));
    let envelope = body_json(stop(state.clone()).await).await;
    assert_eq!(envelope["data"]["write"], Value::Null);

    // Nothing written, no break — a stop that did nothing leaves
    // nothing behind.
    let file = read_item(dir.path(), "task-a");
    assert!(!file.contains("effort:"), "{file}");
    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["phase"], "idle");
}

#[tokio::test]
async fn stop_during_a_break_is_a_conflict() {
    let (_dir, state, clock) = temp_project(&config_with_effort_field("effort"));
    run_into_a_break(&state, &clock, "task-a").await;

    let response = stop(state.clone()).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    // The break survived the refused stop.
    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["phase"], "break");
}

#[tokio::test]
async fn break_end_returns_the_idle_state_and_is_refused_without_a_break() {
    let (_dir, state, clock) = temp_project(&config_with_effort_field("effort"));

    // Idle: nothing to end.
    let response = post(state.clone(), "/api/timer/break/end", json!({})).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);

    // A work interval's exit is stop, not break end.
    start(
        state.clone(),
        json!({ "item": "task-a", "mode": "stopwatch" }),
    )
    .await;
    let response = post(state.clone(), "/api/timer/break/end", json!({})).await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    stop(state.clone()).await;

    run_into_a_break(&state, &clock, "task-a").await;
    let response = post(state.clone(), "/api/timer/break/end", json!({})).await;
    assert_eq!(response.status(), StatusCode::OK);
    let envelope = body_json(response).await;
    assert_eq!(envelope["data"]["phase"]["phase"], "idle");
    // Ending a break does not touch the sticky mode.
    assert_eq!(envelope["data"]["last_mode"], "pomodoro");
}

#[tokio::test]
async fn start_during_a_break_ends_it_and_begins_the_interval() {
    let (_dir, state, clock) = temp_project(&config_with_effort_field("effort"));
    run_into_a_break(&state, &clock, "task-b").await;

    // Any item works — the followed one is a default, not a rule; and
    // the mode is free too.
    let envelope = body_json(
        start(
            state.clone(),
            json!({ "item": "task-a", "mode": "stopwatch" }),
        )
        .await,
    )
    .await;
    assert_eq!(envelope["data"]["outcome"], "started");
    assert_eq!(envelope["data"]["timer"]["phase"]["phase"], "work");
    assert_eq!(envelope["data"]["timer"]["phase"]["item_id"], "task-a");
    assert_eq!(envelope["data"]["timer"]["last_mode"], "stopwatch");
}

#[tokio::test]
async fn the_confirmation_carries_only_to_the_item_the_break_followed() {
    let (_dir, state, clock) = temp_project(&config_with_effort_field("effort"));

    // A break following the qualifying `epic`: starting `epic` again
    // needs no second confirmation — the loop was confirmed when its
    // first interval was.
    run_into_a_break(&state, &clock, "epic").await;
    let envelope =
        body_json(start(state.clone(), json!({ "item": "epic", "mode": "pomodoro" })).await).await;
    assert_eq!(envelope["data"]["outcome"], "started");

    // Back into a break, then switch to the other qualifying item:
    // that one takes the normal round trip.
    clock.advance(chrono::Duration::seconds(26 * 60));
    stop(state.clone()).await;
    let envelope = body_json(
        start(
            state.clone(),
            json!({ "item": "epic-b", "mode": "pomodoro" }),
        )
        .await,
    )
    .await;
    assert_eq!(envelope["data"]["outcome"], "needs_confirmation");
    // The refused-as-outcome start left the break untouched.
    let envelope = body_json(get(state, "/api/timer").await).await;
    assert_eq!(envelope["data"]["phase"]["phase"], "break");
    assert_eq!(envelope["data"]["phase"]["followed_item"], "epic");
}

// ── The timer-named live-update ping ─────────────────────────────────

#[tokio::test]
async fn start_and_stop_ping_the_timer_stream_and_refusals_stay_silent() {
    let (_dir, state, clock) = temp_project(&config_with_effort_field("effort"));
    let mut timer_pings = state.timer_events.subscribe();

    start(
        state.clone(),
        json!({ "item": "task-a", "mode": "stopwatch" }),
    )
    .await;
    assert!(timer_pings.try_recv().is_ok(), "start pings other tabs");

    // A refused start changed nothing, so nothing is announced.
    let response = start(
        state.clone(),
        json!({ "item": "task-b", "mode": "stopwatch" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(timer_pings.try_recv().is_err());

    // A no-write stop still removed the timer — that is a change.
    clock.advance(chrono::Duration::seconds(10));
    stop(state.clone()).await;
    assert!(timer_pings.try_recv().is_ok(), "stop pings other tabs");

    // A refused stop (nothing running) stays silent.
    stop(state.clone()).await;
    assert!(timer_pings.try_recv().is_err());

    // The break transitions ping too: into it (a stop) and out of it.
    run_into_a_break(&state, &clock, "task-a").await;
    assert!(timer_pings.try_recv().is_ok(), "start pings");
    assert!(timer_pings.try_recv().is_ok(), "stop-into-break pings");
    post(state.clone(), "/api/timer/break/end", json!({})).await;
    assert!(timer_pings.try_recv().is_ok(), "break end pings");

    // A refused break end (nothing to end) stays silent.
    post(state, "/api/timer/break/end", json!({})).await;
    assert!(timer_pings.try_recv().is_err());
}
