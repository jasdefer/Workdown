//! Server-side state — what every handler needs to find the workdown
//! project on disk, plus the live-update channel.
//!
//! Per the cold-load decision in ADR-013, the server
//! never caches the loaded project. Each request goes through
//! `core::load_project()` against `project_root` and `config`. The
//! state is therefore just the two pieces needed to re-load: where the
//! project lives, and the already-parsed `Config` (which the CLI
//! reads at startup for port resolution anyway, so we avoid re-reading
//! `config.yaml` per request).
//!
//! On top of that it carries the live-update broadcast channel — the
//! "announcement board" the file watcher publishes to and each open SSE
//! connection subscribes to (see `crate::watcher` and `crate::api::events`).
//!
//! Clone is cheap (`PathBuf` is one allocation, `Config` is small, a
//! broadcast `Sender` is a handful of refcounted pointers), and axum's
//! `State` extractor clones per handler, so we derive it rather than
//! wrapping in `Arc`.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::broadcast;

use workdown_core::model::config::Config;
use workdown_core::project::{load_project, Project};

use crate::envelope::ApiResponse;
use crate::timer::TimerService;

/// Capacity of the live-update broadcast channel. Pings are contentless
/// and the client coalesces anyway (any ping → one refetch of the
/// current page), so a small buffer is ample; a slow consumer that
/// overflows it receives a `Lagged`, which the SSE handler treats as
/// just another "changed".
const EVENT_CHANNEL_CAPACITY: usize = 16;

#[derive(Clone)]
pub struct AppState {
    pub project_root: PathBuf,
    pub config: Config,
    /// Where `config.yaml` was read from (the CLI's `--config`). Passed
    /// to `core::load_project` per request so config-scope diagnostics
    /// can point at the file; relative to `project_root` or absolute.
    pub config_path: PathBuf,
    /// The live-update "announcement board": the file watcher publishes a
    /// unit value here on every debounced change, and each open SSE
    /// connection subscribes a receiver. `Sender` stays usable with zero
    /// receivers (no browser connected), so `send` failing is not an error.
    pub events: broadcast::Sender<()>,
    /// The timer's own announcement board, same mechanics as `events`
    /// but delivered as a *named* SSE event so tabs refetch only the
    /// timer state — deliberately not the generic file-change ping,
    /// which would refetch the timer on every file save and reload the
    /// whole page on every timer action. A separate channel (rather
    /// than a kind enum on one channel) keeps a lagged receiver
    /// unambiguous: overflow on this channel can only mean missed
    /// timer pings.
    pub timer_events: broadcast::Sender<()>,
    /// Pinned evaluation date from `serve --as-of`, forwarded to every
    /// per-request `load_project` call. `None` (the default) means each
    /// request evaluates `$today` at its own current local date, so a
    /// long-running unpinned server stays current across midnight.
    pub evaluation_date_override: Option<chrono::NaiveDate>,
    /// The one effort timer this process owns (see [`crate::timer`]).
    /// The single exception to "no `Arc` here": unlike everything else
    /// in this struct the timer is genuinely shared mutable state — every
    /// cloned handler must see the *same* lock, not a copy of it.
    pub timer: Arc<TimerService>,
}

/// Cold-load the project this request is about, mapping a load failure
/// to the envelope's rejected tier.
///
/// Every project-reading handler opens with exactly this, so it is
/// written once. The error side is a ready-made [`ApiResponse`], which
/// is why the return type is generic in `T`: the caller's own payload
/// type flows through, and the handler's opening line is
///
/// ```ignore
/// let project = match load_state_project(&state) {
///     Ok(project) => project,
///     Err(response) => return response,
/// };
/// ```
pub fn load_state_project<T: serde::Serialize>(
    state: &AppState,
) -> Result<Project, ApiResponse<T>> {
    load_project(
        &state.config,
        &state.project_root,
        &state.config_path,
        state.evaluation_date_override,
    )
    .map_err(|error| ApiResponse::rejected(vec![error.to_diagnostic()]))
}

impl AppState {
    /// Build state with a fresh live-update channel. The watcher is wired
    /// separately, against the same channel, by [`crate::watcher::start`].
    pub fn new(
        project_root: PathBuf,
        config: Config,
        config_path: PathBuf,
        evaluation_date_override: Option<chrono::NaiveDate>,
    ) -> Self {
        let (events, _initial_receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let (timer_events, _initial_receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            project_root,
            config,
            config_path,
            events,
            timer_events,
            evaluation_date_override,
            timer: Arc::new(TimerService::system()),
        }
    }
}

#[cfg(test)]
impl AppState {
    /// Minimal state for tests that exercise routing/handlers without a
    /// real project on disk. The paths don't have to resolve.
    pub(crate) fn test_stub() -> Self {
        use workdown_core::model::config::{Config, Paths, ProjectMeta, ViewDefaults};
        use workdown_core::model::views::DisplayConfig;

        let config = Config {
            project: ProjectMeta {
                name: "test".into(),
                description: String::new(),
            },
            paths: Paths {
                work_items: PathBuf::from("workdown-items"),
                templates: PathBuf::from(".workdown/templates"),
                resources: PathBuf::from(".workdown/resources.yaml"),
                views: PathBuf::from(".workdown/views.yaml"),
            },
            schema: PathBuf::from(".workdown/schema.yaml"),
            defaults: ViewDefaults {
                board_field: "status".into(),
                tree_field: "parent".into(),
                graph_field: "depends_on".into(),
                effort_field: None,
                display: DisplayConfig::default(),
            },
            working_days: None,
            serve: None,
        };
        Self::new(
            PathBuf::from("/tmp/workdown-test-stub"),
            config,
            PathBuf::from(".workdown/config.yaml"),
            None,
        )
    }
}
