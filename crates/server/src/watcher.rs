//! Filesystem watcher feeding the live-update channel.
//!
//! Watches the project's work-item directory and its `.workdown` config
//! directory. A single logical save shows up at the OS level as a burst
//! of raw events (editors write-temp-then-rename, vim deletes-and-
//! recreates, `git pull` rewrites many files at once), so the raw stream
//! goes through `notify-debouncer-full`, which coalesces a burst into one
//! batch once activity settles. Each batch that touches a watched file
//! kind posts a single ping to the broadcast channel; the SSE handler
//! forwards it to every connected browser.
//!
//! Only `.md`, `.yaml`, and `.yml` files count as changes — an allowlist,
//! which by construction ignores every editor scratch file (`.swp`,
//! trailing-`~`, vim's extension-less `4913` probe, `.tmp`) without our
//! having to enumerate them.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use tokio::sync::broadcast;

use workdown_core::model::config::Config;

/// How long file activity must settle before the debouncer emits a batch.
const DEBOUNCE: Duration = Duration::from_millis(200);

/// Live guard returned by [`start`]. Dropping it stops the watcher, so
/// the caller must hold it for the server's lifetime.
pub type WatchGuard = Debouncer<RecommendedWatcher, RecommendedCache>;

/// Start watching the project's directories. Every debounced change that
/// touches a watched file kind sends a ping on `events`. Returns a guard
/// that must be kept alive — dropping it tears the watcher down.
pub fn start(
    config: &Config,
    project_root: &Path,
    events: broadcast::Sender<()>,
) -> Result<WatchGuard> {
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        // A watch error means we may have *missed* events; the safe
        // response is identical to a real change — tell clients to refetch.
        let should_notify = match result {
            Ok(batch) => batch
                .iter()
                .flat_map(|event| event.paths.iter())
                .any(|path| is_watched_file(path)),
            Err(_errors) => true,
        };
        if should_notify {
            // `Err` here only means no browser is currently connected.
            let _ = events.send(());
        }
    })
    .context("initialising filesystem watcher")?;

    for directory in watch_directories(config, project_root) {
        // A configured directory may not exist yet (e.g. no templates
        // dir). Skip the missing ones rather than failing server boot.
        if directory.exists() {
            debouncer
                .watch(directory.as_path(), RecursiveMode::Recursive)
                .with_context(|| format!("watching {}", directory.display()))?;
        }
    }

    Ok(debouncer)
}

/// True when `path` is a file kind a project change can live in:
/// `.md` (work items) or `.yaml`/`.yml` (schema, views, resources,
/// config). Case-insensitive. Everything else — editor scratch files,
/// lock files, directories — is ignored.
fn is_watched_file(path: &Path) -> bool {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) => {
            let extension = extension.to_ascii_lowercase();
            extension == "md" || extension == "yaml" || extension == "yml"
        }
        None => false,
    }
}

/// The directories to watch, derived from config paths and rooted at
/// `project_root`. Deduplicated: the schema/views/resources files
/// typically share the one `.workdown` parent, and recursively watching
/// that parent also covers the templates directory under it.
fn watch_directories(config: &Config, project_root: &Path) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = vec![project_root.join(&config.paths.work_items)];

    // The config files live under `.workdown`; watch each one's parent
    // directory. Dedup below collapses the shared parents to one entry.
    for file in [&config.schema, &config.paths.views, &config.paths.resources] {
        if let Some(parent) = file.parent() {
            candidates.push(project_root.join(parent));
        }
    }

    let mut directories: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if !directories.contains(&candidate) {
            directories.push(candidate);
        }
    }
    directories
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watches_markdown_and_yaml() {
        assert!(is_watched_file(Path::new("workdown-items/login.md")));
        assert!(is_watched_file(Path::new(".workdown/schema.yaml")));
        assert!(is_watched_file(Path::new(".workdown/views.yml")));
    }

    #[test]
    fn watches_case_insensitively() {
        assert!(is_watched_file(Path::new("ITEM.MD")));
        assert!(is_watched_file(Path::new("Schema.YAML")));
    }

    #[test]
    fn ignores_editor_scratch_and_unrelated_files() {
        assert!(!is_watched_file(Path::new(".login.md.swp"))); // vim swap
        assert!(!is_watched_file(Path::new("login.md~"))); // editor backup
        assert!(!is_watched_file(Path::new("4913"))); // vim probe, no extension
        assert!(!is_watched_file(Path::new("notes.txt")));
        assert!(!is_watched_file(Path::new("workdown-items"))); // a directory
    }

    #[test]
    fn watch_directories_dedupes_shared_workdown_parent() {
        let state = crate::state::AppState::test_stub();
        let directories = watch_directories(&state.config, &state.project_root);
        // work_items dir + the single shared `.workdown` parent of
        // schema/views/resources = two entries, not four.
        assert_eq!(directories.len(), 2);
        assert!(directories
            .iter()
            .any(|dir| dir.ends_with("workdown-items")));
        assert!(directories.iter().any(|dir| dir.ends_with(".workdown")));
    }
}
