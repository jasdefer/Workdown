//! Server-side state — what every handler needs to find the workdown
//! project on disk, plus the live-update channel.
//!
//! Per the cold-load decision in `first-view-end-to-end`, the server
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

use tokio::sync::broadcast;

use workdown_core::model::config::Config;

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
    /// The live-update "announcement board": the file watcher publishes a
    /// unit value here on every debounced change, and each open SSE
    /// connection subscribes a receiver. `Sender` stays usable with zero
    /// receivers (no browser connected), so `send` failing is not an error.
    pub events: broadcast::Sender<()>,
}

impl AppState {
    /// Build state with a fresh live-update channel. The watcher is wired
    /// separately, against the same channel, by [`crate::watcher::start`].
    pub fn new(project_root: PathBuf, config: Config) -> Self {
        let (events, _initial_receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Self {
            project_root,
            config,
            events,
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
                display: DisplayConfig::default(),
            },
            working_days: None,
            serve: None,
        };
        Self::new(PathBuf::from("/tmp/workdown-test-stub"), config)
    }
}
