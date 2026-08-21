//! `/api/timer` — the effort timer's three endpoints: get state, start,
//! stop.
//!
//! The timer itself lives in [`crate::timer`] (server memory, one per
//! process); these handlers translate between it and the wire contracts
//! in [`workdown_core::timer_data`]. Every response carries what
//! `defaults.effort_field` resolves to, in one of three states —
//! *unconfigured*, *invalid*, *ready* — the only part of `config.yaml`
//! the UI ever sees. Config is read once at server start, so the UI's
//! hint tells the user to restart after setting the key.
//!
//! Wrong moves get clean refusals: start while a timer runs is `409`
//! (the no-takeover rule, enforced server-side too), as is stop when
//! nothing runs. The roll-up confirmation is *not* an error — start on
//! a qualifying item answers `200` with the `needs_confirmation`
//! outcome, and the app sends start again with `confirmed: true`.
//!
//! Stop writes on the server, through the same `run_set` path as
//! `workdown set --delta`, under the timer's lock — taking the timer
//! and writing are one indivisible step. A failed write keeps the timer
//! running and reports the failure; a session under half a minute
//! clears the timer and writes nothing.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use workdown_core::model::field_value::FieldValue;
use workdown_core::model::WorkItemId;
use workdown_core::operations::set::{run_set, DurationMode, SetOperation};
use workdown_core::project::{load_project, Project};
use workdown_core::timer_data::{
    rounded_write_seconds, EffortFieldState, RunningTimer, StartTimer, TimerStartOutcome,
    TimerState, TimerStopResult, TimerWrite,
};

use super::items::set_error_status;
use crate::envelope::ApiResponse;
use crate::state::AppState;
use crate::timer::{StopError, TimerSnapshot};

/// Router for the timer endpoints under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/timer", get(get_timer))
        .route("/timer/start", post(start_timer))
        .route("/timer/stop", post(stop_timer))
}

async fn get_timer(State(state): State<AppState>) -> ApiResponse<TimerState> {
    let project = match load_state_project(&state) {
        Ok(project) => project,
        Err(response) => return response,
    };
    ApiResponse::ok(timer_state(&state, &project))
}

async fn start_timer(
    State(state): State<AppState>,
    Json(request): Json<StartTimer>,
) -> ApiResponse<TimerStartOutcome> {
    let project = match load_state_project(&state) {
        Ok(project) => project,
        Err(response) => return response,
    };

    let effort_field = EffortFieldState::resolve(
        state.config.defaults.effort_field.as_deref(),
        &project.schema,
    );
    let field = match &effort_field {
        EffortFieldState::Ready { field } => field.clone(),
        EffortFieldState::Unconfigured => {
            return ApiResponse::failed(
                StatusCode::UNPROCESSABLE_ENTITY,
                "no effort field is configured — set defaults.effort_field in config.yaml and restart".to_owned(),
            );
        }
        EffortFieldState::Invalid { problem, .. } => {
            return ApiResponse::failed(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("defaults.effort_field is unusable: {problem}"),
            );
        }
    };

    if project.store.get(&request.item).is_none() {
        return ApiResponse::failed(
            StatusCode::NOT_FOUND,
            format!("no work item '{}'", request.item),
        );
    }

    // The roll-up confirmation is decided here — the browser cannot see
    // an item's children. An item qualifies when the effort field
    // aggregates and the item has at least one child over the
    // aggregate's link field; children *with values* would make the
    // dialog appear and vanish as children gain their first value.
    if !request.confirmed && rollup_confirmation_needed(&project, &field, &request.item) {
        return ApiResponse::ok(TimerStartOutcome::NeedsConfirmation);
    }

    match state.timer.start(WorkItemId::from(request.item)) {
        Ok(_started) => {
            // Tell every other tab the timer changed. Zero receivers
            // (no other tab) is fine, so the send result is ignored.
            let _ = state.timer_events.send(());
            ApiResponse::ok(TimerStartOutcome::Started {
                timer: timer_state(&state, &project),
            })
        }
        Err(running) => ApiResponse::failed(
            StatusCode::CONFLICT,
            format!("a timer is already running on '{}'", running.item_id),
        ),
    }
}

async fn stop_timer(State(state): State<AppState>) -> ApiResponse<TimerStopResult> {
    // No project load needed: `run_set` validates the item and the
    // field itself, and a stop with the key unset can only be a stray
    // client — a timer can never have started without it.
    let Some(field) = state.config.defaults.effort_field.clone() else {
        return ApiResponse::failed(
            StatusCode::UNPROCESSABLE_ENTITY,
            "no effort field is configured — set defaults.effort_field in config.yaml and restart"
                .to_owned(),
        );
    };

    let stopped = state.timer.stop_with(|snapshot| {
        let added_seconds = rounded_write_seconds(snapshot.elapsed_seconds);
        if added_seconds == 0 {
            // Under half a minute: the stop still stops, but writes
            // nothing — the toast says so instead of staying silent.
            return Ok(None);
        }
        run_set(
            &state.config,
            &state.project_root,
            &snapshot.item_id,
            &field,
            SetOperation::Duration(DurationMode::Delta(added_seconds)),
        )
        .map(|outcome| Some((added_seconds, outcome)))
    });

    // Any successful stop — written or not — removed the running timer,
    // so the other tabs get the ping either way. A failed stop changed
    // nothing and stays silent. (The write itself additionally lands on
    // the generic file-change stream via the watcher, refreshing the
    // effort value everywhere it is displayed.)
    if stopped.is_ok() {
        let _ = state.timer_events.send(());
    }

    match stopped {
        Ok((snapshot, Some((added_seconds, outcome)))) => {
            let result = TimerStopResult {
                item_id: snapshot.item_id,
                field,
                elapsed_seconds: snapshot.elapsed_seconds,
                write: Some(TimerWrite::from_outcome(added_seconds, &outcome)),
            };
            ApiResponse::ok_with(result, outcome.warnings)
        }
        Ok((snapshot, None)) => ApiResponse::ok(TimerStopResult {
            item_id: snapshot.item_id,
            field,
            elapsed_seconds: snapshot.elapsed_seconds,
            write: None,
        }),
        Err(StopError::NotRunning) => {
            ApiResponse::failed(StatusCode::CONFLICT, "no timer is running".to_owned())
        }
        Err(StopError::Write(error)) => {
            ApiResponse::failed(set_error_status(&error), error.to_string())
        }
    }
}

// ── Projection helpers ─────────────────────────────────────────────────

/// Cold-load the project, mapping a load failure to the envelope's
/// rejected tier (as every project-reading handler does).
fn load_state_project<T: serde::Serialize>(state: &AppState) -> Result<Project, ApiResponse<T>> {
    load_project(
        &state.config,
        &state.project_root,
        &state.config_path,
        state.evaluation_date_override,
    )
    .map_err(|error| ApiResponse::rejected(vec![error.to_diagnostic()]))
}

/// The timer's whole client-visible state, projected from the running
/// session (if any) and the freshly loaded project.
fn timer_state(state: &AppState, project: &Project) -> TimerState {
    let effort_field = EffortFieldState::resolve(
        state.config.defaults.effort_field.as_deref(),
        &project.schema,
    );
    let running = state
        .timer
        .snapshot()
        .map(|snapshot| running_timer(snapshot, &effort_field, project));
    TimerState {
        effort_field,
        running,
    }
}

fn running_timer(
    snapshot: TimerSnapshot,
    effort_field: &EffortFieldState,
    project: &Project,
) -> RunningTimer {
    // The projected write's basis is the item's *hand-written* value —
    // the frontmatter, which is exactly what the store's coerced fields
    // hold. A derived value (rolled up, computed) never appears there,
    // so it reads as absent, matching what the delta write will do. An
    // item deleted mid-session also reads as absent; stopping on it
    // fails cleanly and keeps the timer running.
    let effort_before_seconds = match effort_field {
        EffortFieldState::Ready { field } => project
            .store
            .get(snapshot.item_id.as_str())
            .and_then(|item| item.fields.get(field))
            .and_then(|value| match value {
                FieldValue::Duration(seconds) => Some(*seconds),
                _ => None,
            }),
        _ => None,
    };
    RunningTimer {
        item_id: snapshot.item_id,
        started_at_ms: snapshot.started_at.timestamp_millis(),
        elapsed_seconds: snapshot.elapsed_seconds,
        effort_before_seconds,
    }
}

/// Whether starting on this item needs the roll-up confirmation: the
/// effort field aggregates and the item has at least one child over the
/// aggregate's link field.
fn rollup_confirmation_needed(project: &Project, effort_field: &str, item_id: &str) -> bool {
    let Some(definition) = project.schema.fields.get(effort_field) else {
        return false;
    };
    let Some(aggregate) = definition.aggregate.as_ref() else {
        return false;
    };
    let over = aggregate.over.as_deref().unwrap_or("parent");
    !project.store.referring_items(item_id, over).is_empty()
}
