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
//! having to enumerate them. Only events that record a write count,
//! never an access: Linux reports every `open()` as an event, and the
//! server reads the very files it watches on each request — counting
//! reads would make each page load ping the next (see
//! `records_change`).
//!
//! A `config.yaml` change pings like any other, but the server holds the
//! config it read at startup — see ADR-013 for that asymmetry, and for
//! why the timer and the git status get channels of their own rather
//! than riding this one. The git channel's watcher also lives here —
//! see [`start_git_watch`].

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use notify_debouncer_full::notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{
    new_debouncer, DebounceEventResult, DebouncedEvent, Debouncer, RecommendedCache,
};
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
///
/// Creates the work-items directory if it is missing — see
/// `ensure_items_directory` below for why. This is the one place the
/// server writes to the project without being asked to.
pub fn start(
    config: &Config,
    project_root: &Path,
    events: broadcast::Sender<()>,
) -> Result<WatchGuard> {
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        // A watch error means we may have *missed* events; the safe
        // response is identical to a real change — tell clients to refetch.
        let should_notify = match result {
            Ok(batch) => changed_paths(&batch).any(is_watched_file),
            Err(_errors) => true,
        };
        if should_notify {
            // `Err` here only means no browser is currently connected.
            let _ = events.send(());
        }
    })
    .context("initialising filesystem watcher")?;

    ensure_items_directory(config, project_root);

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

/// Start watching the repository's git directory, so repository
/// movement made *outside* the server — a commit in a terminal, a
/// fetch, a branch switch — pings `git_events` and the pill refreshes
/// without waiting for the user to touch anything. Started only when
/// the project opted into the git controls; returns the same kind of
/// guard as [`start`].
///
/// What counts as movement is an allowlist, mirroring the file
/// watcher's: the head pointer files (`HEAD`, `ORIG_HEAD`,
/// `FETCH_HEAD`) and anything under `logs/`, which git appends to on
/// every ref update. Everything else in `.git` is deliberately ignored
/// — above all the index, which the status endpoint's own `git status`
/// may refresh: reacting to it would make every refresh trigger the
/// next one. For the same reason only writes count, not accesses:
/// every git command opens `HEAD` to read it, so an access event there
/// would turn each status recount into the next `git` event.
pub fn start_git_watch(
    git_directory: &Path,
    git_events: broadcast::Sender<()>,
) -> Result<WatchGuard> {
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        let should_notify = match result {
            Ok(batch) => changed_paths(&batch).any(is_repository_movement),
            Err(_errors) => true,
        };
        if should_notify {
            let _ = git_events.send(());
        }
    })
    .context("initialising git watcher")?;

    // The head pointer files live directly in the git directory; the
    // ref logs live under `logs/`. The logs directory may not exist in
    // a repository that has never moved a ref (fresh `git init`) — the
    // non-recursive watch on the git directory itself still catches the
    // first `HEAD` change after which a restart picks up the rest, so a
    // missing `logs/` is skipped rather than an error.
    debouncer
        .watch(git_directory, RecursiveMode::NonRecursive)
        .with_context(|| format!("watching {}", git_directory.display()))?;
    let logs_directory = git_directory.join("logs");
    if logs_directory.exists() {
        debouncer
            .watch(logs_directory.as_path(), RecursiveMode::Recursive)
            .with_context(|| format!("watching {}", logs_directory.display()))?;
    }

    Ok(debouncer)
}

/// The paths of every event in a batch that records a write, in batch
/// order. Both watchers judge their allowlists over these.
fn changed_paths(batch: &[DebouncedEvent]) -> impl Iterator<Item = &Path> {
    batch
        .iter()
        .filter(|event| records_change(&event.kind))
        .flat_map(|event| event.paths.iter().map(PathBuf::as_path))
}

/// True for an event kind that can only come from a write — a create,
/// a modify (data, metadata, rename) or a remove — and for the
/// backend's unclassified kinds, which are kept out of caution. False
/// for every access: Linux inotify reports each `open()` and
/// `close()` of a watched file, and a read changes nothing. A write
/// never arrives as an access alone (the data write is its own event),
/// so nothing is lost by dropping them.
fn records_change(kind: &EventKind) -> bool {
    !matches!(kind, EventKind::Access(_))
}

/// True when a path inside the git directory records the repository
/// moving: one of the head pointer files, or a ref log under `logs/`.
fn is_repository_movement(path: &Path) -> bool {
    let file_name = path.file_name().and_then(|name| name.to_str());
    if matches!(file_name, Some("HEAD" | "ORIG_HEAD" | "FETCH_HEAD")) {
        return true;
    }
    path.components()
        .any(|component| component.as_os_str() == "logs")
}

/// Create the work-items directory if it is missing.
///
/// A missing one is a valid empty project — git keeps no empty
/// directories — but a watch can only be placed on a path that exists,
/// and item mutations are announced to browsers by this watcher alone
/// (nothing else pings the channel). Without the directory, the first
/// item added through the UI would land unwatched and no browser would
/// ever see it appear.
///
/// A failure here is not fatal: the watch is skipped, live updates stay
/// off, and the next project load reports the real problem — so it warns
/// rather than aborting server boot.
fn ensure_items_directory(config: &Config, project_root: &Path) {
    let items_directory = project_root.join(&config.paths.work_items);
    if let Err(error) = std::fs::create_dir_all(&items_directory) {
        tracing::warn!(
            error = %error,
            path = %items_directory.display(),
            "could not create the work-items directory; live updates may stay off",
        );
    }
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
    fn repository_movement_is_head_pointers_and_ref_logs() {
        assert!(is_repository_movement(Path::new(".git/HEAD")));
        assert!(is_repository_movement(Path::new(".git/ORIG_HEAD")));
        assert!(is_repository_movement(Path::new(".git/FETCH_HEAD")));
        assert!(is_repository_movement(Path::new(".git/logs/HEAD")));
        assert!(is_repository_movement(Path::new(
            ".git/logs/refs/remotes/origin/main"
        )));
        // The index is touched by `git status` itself — reacting to it
        // would loop refresh → status → index write → refresh.
        assert!(!is_repository_movement(Path::new(".git/index")));
        assert!(!is_repository_movement(Path::new(".git/index.lock")));
        assert!(!is_repository_movement(Path::new(".git/config")));
    }

    #[test]
    fn only_writes_record_a_change() {
        use notify_debouncer_full::notify::event::{
            AccessKind, AccessMode, CreateKind, DataChange, ModifyKind, RemoveKind, RenameMode,
        };
        // Reads: the server opening the files it watches, git opening
        // HEAD on every command.
        assert!(!records_change(&EventKind::Access(AccessKind::Open(
            AccessMode::Any
        ))));
        assert!(!records_change(&EventKind::Access(AccessKind::Read)));
        assert!(!records_change(&EventKind::Access(AccessKind::Close(
            AccessMode::Write
        ))));
        // Writes, in every form an editor or git produces them.
        assert!(records_change(&EventKind::Create(CreateKind::File)));
        assert!(records_change(&EventKind::Modify(ModifyKind::Data(
            DataChange::Any
        ))));
        assert!(records_change(&EventKind::Modify(ModifyKind::Name(
            RenameMode::Both
        ))));
        assert!(records_change(&EventKind::Remove(RemoveKind::File)));
        // Unclassified kinds are kept, out of caution.
        assert!(records_change(&EventKind::Any));
        assert!(records_change(&EventKind::Other));
    }

    #[test]
    fn start_creates_missing_items_directory() {
        let state = crate::state::AppState::test_stub();
        let root = tempfile::tempdir().unwrap();
        // Only `.workdown` exists — the items directory is missing, the
        // normal state of a fresh clone with no items.
        std::fs::create_dir_all(root.path().join(".workdown")).unwrap();

        let (events, _) = broadcast::channel(1);
        let _guard = start(&state.config, root.path(), events).unwrap();

        assert!(root.path().join("workdown-items").is_dir());
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
