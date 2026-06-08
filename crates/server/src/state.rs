//! Server-side state — what every handler needs to find the workdown
//! project on disk.
//!
//! Per the cold-load decision in `first-view-end-to-end`, the server
//! never caches the loaded project. Each request goes through
//! `core::load_project()` against `project_root` and `config`. The
//! state is therefore just the two pieces needed to re-load: where the
//! project lives, and the already-parsed `Config` (which the CLI
//! reads at startup for port resolution anyway, so we avoid re-reading
//! `config.yaml` per request).
//!
//! Clone is cheap (`PathBuf` is one allocation, `Config` is small),
//! and axum's `State` extractor clones per handler, so we derive it
//! rather than wrapping in `Arc`.

use std::path::PathBuf;

use workdown_core::model::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub project_root: PathBuf,
    pub config: Config,
}
