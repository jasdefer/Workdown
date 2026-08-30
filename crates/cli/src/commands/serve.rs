//! `workdown serve` — boot the local web UI.

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use workdown_core::model::config::Config;
use workdown_server::AppState;

use crate::cli::output;

const DEFAULT_PORT: u16 = 3141;
const SCAN_FALLBACK_COUNT: u16 = 10;

/// Run the serve command.
///
/// Sets up a tokio runtime inline (rather than `#[tokio::main]`) so the
/// other CLI subcommands stay synchronous and pay no async-runtime cost.
pub fn run_serve_command(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    port: Option<u16>,
    open: bool,
    as_of: Option<chrono::NaiveDate>,
) -> Result<ExitCode> {
    let port_resolution = resolve_port(config, port);
    if let Some(pinned_date) = as_of {
        // Loud on purpose: a forgotten pin means trusting a stale board.
        output::info(&format!(
            "evaluating $today as {} for the lifetime of this server (pinned via --as-of)",
            pinned_date.format("%Y-%m-%d"),
        ));
    }
    let state = AppState::new(
        project_root.to_path_buf(),
        config.clone(),
        config_path.to_path_buf(),
        as_of,
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")?;
    runtime.block_on(async move { run_serve(port_resolution, state, open).await })
}

async fn run_serve(resolution: PortResolution, state: AppState, open: bool) -> Result<ExitCode> {
    let listener = match bind_with_scan(&resolution).await {
        Ok(listener) => listener,
        Err(err) => {
            tracing::error!("{err:#}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let bound = listener
        .local_addr()
        .context("reading bound socket address")?;
    let url = format!("http://localhost:{}", bound.port());

    if bound.port() != resolution.start_port {
        tracing::warn!(
            requested = resolution.start_port,
            actual = bound.port(),
            "default port busy; using nearby free port",
        );
    }

    tracing::info!(
        url = %url,
        port = bound.port(),
        pid = std::process::id(),
        "workdown serve ready",
    );
    output::info(&format!(
        "workdown serving at {url}  (pid {})",
        std::process::id()
    ));

    if open {
        if let Err(err) = open::that_detached(&url) {
            tracing::warn!(
                error = %err,
                url = %url,
                "could not launch browser; open the URL above manually",
            );
        }
    }

    // Start the filesystem watcher so files changed by an editor, the
    // CLI, or `git pull` push live updates to connected browsers. The
    // returned guard must outlive the serve loop — dropping it stops
    // watching — so it's bound here and held until `serve` returns.
    let _watch_guard = match workdown_server::watcher::start(
        &state.config,
        &state.project_root,
        state.events.clone(),
    ) {
        Ok(guard) => Some(guard),
        Err(error) => {
            // A watcher failure shouldn't sink the server: the UI
            // still works, it just won't auto-refresh. Warn and serve.
            tracing::warn!(
                error = %format!("{error:#}"),
                "live updates disabled: could not start file watcher",
            );
            None
        }
    };

    // With the git controls opted in, also watch the repository's git
    // directory: a commit or fetch made in a terminal moves nothing the
    // file watcher sees, and this is what keeps the pill truthful
    // without it. Same guard-holding rule, same failure posture — a
    // repo that can't be watched degrades to no live git updates.
    let _git_watch_guard = if git_controls_enabled(&state) {
        start_git_watch(&state).await
    } else {
        None
    };

    let router = workdown_server::router(state);
    workdown_server::serve(listener, router).await?;
    Ok(ExitCode::SUCCESS)
}

fn git_controls_enabled(state: &AppState) -> bool {
    state
        .config
        .serve
        .as_ref()
        .is_some_and(|serve| serve.git_controls)
}

/// Resolve the git directory and start the watcher on it. `None` (with
/// a warning where it matters) on any failure: the controls still work,
/// they just won't refresh on terminal-side git activity.
async fn start_git_watch(state: &AppState) -> Option<workdown_server::watcher::WatchGuard> {
    let git_directory = match workdown_server::git::git_directory(&state.project_root).await {
        Ok(Some(directory)) => directory,
        // Not a repository: the status endpoint already answers
        // `not_a_repo` and the UI hides the controls — nothing to watch.
        Ok(None) => return None,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "git live updates disabled: could not locate the git directory",
            );
            return None;
        }
    };
    match workdown_server::watcher::start_git_watch(&git_directory, state.git_events.clone()) {
        Ok(guard) => Some(guard),
        Err(error) => {
            tracing::warn!(
                error = %format!("{error:#}"),
                "git live updates disabled: could not start the git watcher",
            );
            None
        }
    }
}

// ── Port resolution ───────────────────────────────────────────────────

struct PortResolution {
    start_port: u16,
    /// Whether the user pinned the port via `--port` — in that case we
    /// never scan, we hard-fail on conflict.
    explicit: bool,
}

fn resolve_port(config: &Config, flag: Option<u16>) -> PortResolution {
    if let Some(port) = flag {
        return PortResolution {
            start_port: port,
            explicit: true,
        };
    }
    let port = config
        .serve
        .as_ref()
        .and_then(|s| s.port)
        .unwrap_or(DEFAULT_PORT);
    PortResolution {
        start_port: port,
        explicit: false,
    }
}

async fn bind_with_scan(resolution: &PortResolution) -> Result<tokio::net::TcpListener> {
    if resolution.explicit {
        return workdown_server::bind(resolution.start_port)
            .await
            .with_context(|| {
                format!(
                    "binding port {} (explicitly requested)",
                    resolution.start_port
                )
            });
    }

    let mut last_error = None;
    for offset in 0..SCAN_FALLBACK_COUNT {
        let candidate = match resolution.start_port.checked_add(offset) {
            Some(port) => port,
            None => break,
        };
        match workdown_server::bind(candidate).await {
            Ok(listener) => return Ok(listener),
            Err(err) => last_error = Some((candidate, err)),
        }
    }

    let end = resolution
        .start_port
        .saturating_add(SCAN_FALLBACK_COUNT.saturating_sub(1));
    Err(match last_error {
        Some((_, err)) => anyhow::anyhow!(
            "no free port in {}..={} (last error: {err}); pass `--port <N>` to choose one",
            resolution.start_port,
            end,
        ),
        None => anyhow::anyhow!(
            "could not scan any port starting at {}; pass `--port <N>`",
            resolution.start_port,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use workdown_core::model::config::ServeConfig;

    fn config_with(serve: Option<ServeConfig>) -> Config {
        // Minimal config — only `serve` matters for resolve_port.
        serde_yaml::from_str::<Config>(&format!(
            r#"
project:
  name: Test
paths:
  work_items: items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
{}
"#,
            match serve {
                Some(s) if s.port.is_some() => format!("serve:\n  port: {}\n", s.port.unwrap()),
                Some(_) => "serve: {}\n".to_string(),
                None => String::new(),
            },
        ))
        .unwrap()
    }

    #[test]
    fn flag_wins_over_config() {
        let config = config_with(Some(ServeConfig {
            port: Some(7000),
            ..ServeConfig::default()
        }));
        let resolution = resolve_port(&config, Some(8080));
        assert_eq!(resolution.start_port, 8080);
        assert!(resolution.explicit);
    }

    #[test]
    fn config_used_when_no_flag() {
        let config = config_with(Some(ServeConfig {
            port: Some(7000),
            ..ServeConfig::default()
        }));
        let resolution = resolve_port(&config, None);
        assert_eq!(resolution.start_port, 7000);
        assert!(!resolution.explicit);
    }

    #[test]
    fn default_used_when_neither_flag_nor_config() {
        let config = config_with(None);
        let resolution = resolve_port(&config, None);
        assert_eq!(resolution.start_port, DEFAULT_PORT);
        assert!(!resolution.explicit);
    }

    #[test]
    fn empty_serve_section_uses_default() {
        let config = config_with(Some(ServeConfig {
            port: None,
            ..ServeConfig::default()
        }));
        let resolution = resolve_port(&config, None);
        assert_eq!(resolution.start_port, DEFAULT_PORT);
        assert!(!resolution.explicit);
    }
}
