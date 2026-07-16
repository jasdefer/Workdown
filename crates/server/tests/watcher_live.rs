//! Live filesystem-watcher test: proves a real file change produces a
//! ping on the broadcast channel via the actual `notify` inotify path.
//!
//! Runs against a tempdir on the test machine's *native* filesystem — not
//! a bind mount — because inotify does not reliably deliver events for
//! bind-mounted directories (e.g. a repo mounted from a Windows host into
//! a dev container). The waits use generous timeouts so the test stays
//! non-flaky despite the inherent timing of filesystem events.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::broadcast;

use workdown_core::model::config::{Config, Paths, ProjectMeta, ViewDefaults};
use workdown_server::watcher;

/// A config whose paths match what `start` watches: a `workdown-items`
/// directory and the `.workdown` config directory.
fn config() -> Config {
    Config {
        project: ProjectMeta {
            name: "live-test".into(),
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
            display: workdown_core::model::views::DisplayConfig::default(),
        },
        working_days: None,
        serve: None,
    }
}

#[tokio::test]
async fn writing_a_markdown_file_pings_the_channel() {
    let temp = tempfile::tempdir().expect("create tempdir");
    let root = temp.path();
    fs::create_dir_all(root.join("workdown-items")).expect("create items dir");
    fs::create_dir_all(root.join(".workdown")).expect("create config dir");

    let (sender, mut receiver) = broadcast::channel(16);
    let _guard = watcher::start(&config(), root, sender).expect("start watcher");

    // Let the watcher register its inotify watches before touching files.
    tokio::time::sleep(Duration::from_millis(300)).await;

    fs::write(
        root.join("workdown-items/login.md"),
        "---\ntitle: Login\n---\nbody\n",
    )
    .expect("write work item");

    // Debounce is 200ms; allow a generous window for the OS event +
    // debounce + delivery so the test doesn't flake on a slow machine.
    let result = tokio::time::timeout(Duration::from_secs(5), receiver.recv()).await;
    assert!(
        result.is_ok(),
        "expected a live-update ping after writing a .md file, timed out"
    );
}

#[tokio::test]
async fn writing_an_unwatched_file_does_not_ping() {
    let temp = tempfile::tempdir().expect("create tempdir");
    let root = temp.path();
    fs::create_dir_all(root.join("workdown-items")).expect("create items dir");
    fs::create_dir_all(root.join(".workdown")).expect("create config dir");

    let (sender, mut receiver) = broadcast::channel(16);
    let _guard = watcher::start(&config(), root, sender).expect("start watcher");

    tokio::time::sleep(Duration::from_millis(300)).await;

    // A vim swap file — excluded by the `.md`/`.yaml`/`.yml` allowlist.
    fs::write(root.join("workdown-items/.login.md.swp"), b"junk").expect("write swap file");

    // No ping should arrive. A short window is enough: if the allowlist
    // were broken the ping would land within one debounce cycle.
    let result = tokio::time::timeout(Duration::from_millis(800), receiver.recv()).await;
    assert!(
        result.is_err(),
        "an editor scratch file should not trigger a live-update ping"
    );
}
