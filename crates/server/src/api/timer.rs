//! `/api/timer` — the effort timer's four endpoints: get state, start,
//! stop, and break end.
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
//!
//! Pomodoro's break rides the same contracts: a pomodoro stop whose
//! write landed begins the break, stop during a break is `409` (a
//! break has nothing to write), and `POST /api/timer/break/end` is the
//! state-only exit — refused whenever no break runs. Start during a
//! break is one transition, and while a break runs, start on the item
//! it followed skips the roll-up confirmation: the loop was confirmed
//! when its first interval was.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};

use workdown_core::model::WorkItemId;
use workdown_core::operations::set::{run_set, DurationMode, SetOperation};
use workdown_core::project::{load_project, Project};
use workdown_core::timer_data::{
    hand_written_duration_seconds, rounded_write_seconds, EffortFieldState, StartTimer, TimerMode,
    TimerPhase, TimerStartOutcome, TimerState, TimerStopResult, TimerWrite, POMODORO_BREAK_SECONDS,
    POMODORO_WORK_SECONDS,
};

use super::items::set_error_status;
use crate::envelope::ApiResponse;
use crate::state::AppState;
use crate::timer::{BreakEndError, PhaseSnapshot, StopError, WorkSnapshot};

/// Router for the timer endpoints under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/timer", get(get_timer))
        .route("/timer/start", post(start_timer))
        .route("/timer/stop", post(stop_timer))
        .route("/timer/break/end", post(end_break))
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

    // Refuse a start while a work interval runs *before* the roll-up
    // confirmation below: a stale tab must hear the conflict, not be
    // walked through a confirmation dialog whose confirmed retry could
    // only meet this same refusal. Advisory only — the check under the
    // lock in `TimerService::start` stays the authority for a start
    // racing past this one.
    let phase_before_start = state.timer.snapshot().phase;
    if let PhaseSnapshot::Work(running) = &phase_before_start {
        return ApiResponse::failed(
            StatusCode::CONFLICT,
            format!("a timer is already running on '{}'", running.item_id),
        );
    }

    // The roll-up confirmation is decided here — the browser cannot see
    // an item's children. An item qualifies when the effort field
    // aggregates and the item has at least one child over the
    // aggregate's link field; children *with values* would make the
    // dialog appear and vanish as children gain their first value.
    // While a break runs, the item it followed is confirmed already —
    // the confirmation of the loop's first interval carries through;
    // any other item takes the normal round trip.
    let confirmed = request.confirmed
        || matches!(
            &phase_before_start,
            PhaseSnapshot::Break(running_break)
                if running_break.followed_item.as_str() == request.item
        );
    if !confirmed && rollup_confirmation_needed(&project, &field, &request.item) {
        return ApiResponse::ok(TimerStartOutcome::NeedsConfirmation);
    }

    match state
        .timer
        .start(WorkItemId::from(request.item), request.mode)
    {
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
        Err(StopError::BreakRunning) => ApiResponse::failed(
            StatusCode::CONFLICT,
            "a break is running and records nothing — end it or start the next interval".to_owned(),
        ),
        Err(StopError::Write(error)) => {
            ApiResponse::failed(set_error_status(&error), error.to_string())
        }
    }
}

/// End a running break: back to idle, nothing written — so no result
/// beyond the new timer state, and nothing for a toast to say. Refused
/// whenever no break runs; a work interval's exit is stop.
async fn end_break(State(state): State<AppState>) -> ApiResponse<TimerState> {
    let project = match load_state_project(&state) {
        Ok(project) => project,
        Err(response) => return response,
    };
    match state.timer.end_break() {
        Ok(()) => {
            let _ = state.timer_events.send(());
            ApiResponse::ok(timer_state(&state, &project))
        }
        Err(BreakEndError::NotRunning) => {
            ApiResponse::failed(StatusCode::CONFLICT, "no break is running".to_owned())
        }
        Err(BreakEndError::WorkRunning) => ApiResponse::failed(
            StatusCode::CONFLICT,
            "a work interval is running — stop it instead of ending a break".to_owned(),
        ),
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
    let snapshot = state.timer.snapshot();
    let phase = match snapshot.phase {
        PhaseSnapshot::Idle => TimerPhase::Idle,
        PhaseSnapshot::Work(work) => work_phase(work, &effort_field, project),
        PhaseSnapshot::Break(running_break) => TimerPhase::Break {
            followed_item: running_break.followed_item,
            started_at_ms: running_break.started_at.timestamp_millis(),
            elapsed_seconds: running_break.elapsed_seconds,
            phase_length_seconds: POMODORO_BREAK_SECONDS,
        },
    };
    TimerState {
        effort_field,
        phase,
        last_mode: snapshot.last_mode,
    }
}

fn work_phase(
    snapshot: WorkSnapshot,
    effort_field: &EffortFieldState,
    project: &Project,
) -> TimerPhase {
    // The projected write's basis is the item's *hand-written* value,
    // read from the file's own frontmatter — deliberately not from the
    // loaded store, whose derive pass writes rolled-up and computed
    // values into item fields where they are indistinguishable from
    // hand-written ones. The delta a stop performs starts from zero on
    // those, and the projection must agree with it. An item deleted
    // mid-session reads as absent; stopping on it fails cleanly and
    // keeps the timer running.
    let effort_before_seconds = match effort_field {
        EffortFieldState::Ready { field } => project
            .store
            .get(snapshot.item_id.as_str())
            .and_then(|item| hand_written_duration_seconds(&item.source_path, field)),
        _ => None,
    };
    TimerPhase::Work {
        item_id: snapshot.item_id,
        started_at_ms: snapshot.started_at.timestamp_millis(),
        elapsed_seconds: snapshot.elapsed_seconds,
        effort_before_seconds,
        mode: snapshot.mode,
        phase_length_seconds: match snapshot.mode {
            TimerMode::Pomodoro => Some(POMODORO_WORK_SECONDS),
            TimerMode::Stopwatch => None,
        },
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
    let over = aggregate.over.as_str();
    !project.store.referring_items(item_id, over).is_empty()
}
