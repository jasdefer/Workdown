//! `workdown install-hooks` — install a git pre-commit hook that keeps
//! rendered views in sync with the work items.
//!
//! Orchestration only: resolve the repository's hooks directory and the
//! project's position inside the repository via `git rev-parse`, gather
//! the watched paths from the loaded config, and hand the result to
//! `core::operations::install_hooks`. `workdown init --install-hooks`
//! delegates here after scaffolding.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use workdown_core::model::config::Config;
use workdown_core::operations::install_hooks::{
    hook_script, install_pre_commit, manual_hook_line, HookMode, HookTemplate, InstallHooksError,
    InstallOutcome,
};
use workdown_core::parser::views::{load_views, DEFAULT_OUTPUT_DIR};

use crate::cli::output;

pub fn run_install_hooks_command(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    check: bool,
) -> anyhow::Result<ExitCode> {
    let Some(git) = resolve_git_info(project_root)? else {
        output::error("not inside a git repository — hooks have nowhere to live");
        return Ok(ExitCode::FAILURE);
    };

    let template = HookTemplate {
        mode: if check {
            HookMode::Check
        } else {
            HookMode::Stage
        },
        project_prefix: git.project_prefix,
        watched_paths: watched_paths(config, config_path),
        output_dir: output_dir(config, project_root),
    };

    match install_pre_commit(&git.hooks_dir, &hook_script(&template)) {
        Ok(InstallOutcome::Installed { path }) => {
            output::success(&format!("Installed pre-commit hook at {}", path.display()));
            Ok(ExitCode::SUCCESS)
        }
        Ok(InstallOutcome::Replaced { path }) => {
            output::success(&format!(
                "Replaced the workdown pre-commit hook at {}",
                path.display()
            ));
            Ok(ExitCode::SUCCESS)
        }
        Err(error @ InstallHooksError::ForeignHook { .. }) => {
            output::error(&error.to_string());
            output::info(&format!(
                "add this to your existing hook yourself: {}",
                manual_hook_line(&template)
            ));
            Ok(ExitCode::FAILURE)
        }
        Err(error) => Err(error.into()),
    }
}

/// Where the hooks live and where the project sits inside the repo.
struct GitInfo {
    hooks_dir: PathBuf,
    /// `git rev-parse --show-prefix`: the project root relative to the
    /// repository toplevel, `None` when they coincide.
    project_prefix: Option<String>,
}

/// Ask git for the hooks path and the project's prefix. Returns
/// `Ok(None)` when the project root is not inside a git repository.
///
/// `rev-parse` prints one line per query, in argument order. The hooks
/// path honors `core.hooksPath` and gitfile worktrees, which is why
/// this shells out instead of assuming `.git/hooks`.
fn resolve_git_info(project_root: &Path) -> anyhow::Result<Option<GitInfo>> {
    let result = std::process::Command::new("git")
        .args(["rev-parse", "--show-prefix", "--git-path", "hooks"])
        .current_dir(project_root)
        .output();

    let output = match result {
        Ok(output) if output.status.success() => output,
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("git is not on PATH — cannot locate the hooks directory")
        }
        Err(e) => return Err(e.into()),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let prefix = lines.next().unwrap_or("").trim().to_owned();
    let hooks = lines.next().unwrap_or("").trim().to_owned();
    if hooks.is_empty() {
        anyhow::bail!("git rev-parse returned no hooks path");
    }

    let hooks_path = PathBuf::from(&hooks);
    let hooks_dir = if hooks_path.is_absolute() {
        hooks_path
    } else {
        project_root.join(hooks_path)
    };

    Ok(Some(GitInfo {
        hooks_dir,
        project_prefix: (!prefix.is_empty()).then_some(prefix),
    }))
}

/// The paths whose staged changes require a re-render: the work items
/// directory plus every workdown configuration file, straight from the
/// loaded config — no hardcoded names.
fn watched_paths(config: &Config, config_path: &Path) -> Vec<String> {
    let candidates = [
        &config.paths.work_items,
        &PathBuf::from(config_path),
        &config.schema,
        &config.paths.resources,
        &config.paths.views,
    ]
    .map(|path| sh_path(path));

    let mut paths: Vec<String> = Vec::new();
    for candidate in candidates {
        if !paths.contains(&candidate) {
            paths.push(candidate);
        }
    }
    paths
}

/// The render output directory from `views.yaml`, or the render
/// default when the file is absent or unparseable (the hook stays
/// installable either way — `workdown render` reports such problems
/// itself).
fn output_dir(config: &Config, project_root: &Path) -> String {
    let views_path = project_root.join(&config.paths.views);
    match load_views(&views_path) {
        Ok(views) => sh_path(&views.output_dir),
        Err(_) => DEFAULT_OUTPUT_DIR.to_owned(),
    }
}

/// Render a path for embedding in the sh hook script: forward slashes
/// regardless of platform.
fn sh_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
