//! `workdown install-hooks` — write a git pre-commit hook that keeps
//! rendered views in sync with the work items.
//!
//! The hook script is generated from the project's configured paths
//! (nothing is hardcoded) and written to the repository's hooks
//! directory. Policy, per the `init-install-hooks` decisions:
//!
//! - Two modes: [`HookMode::Stage`] re-renders and stages the views
//!   into the commit being made; [`HookMode::Check`] re-renders and
//!   fails the commit when the views were stale, leaving staging to
//!   the user.
//! - Either mode exits early when the staged changes touch neither the
//!   work items nor the workdown configuration, so unrelated commits
//!   pay no render cost and pick up no `$today` date drift.
//! - A pre-commit hook we did not write stops the installation; a hook
//!   carrying [`HOOK_MARKER`] is ours and is overwritten, so reinstalls
//!   and mode switches stay idempotent.
//! - A missing `workdown` binary fails the commit loudly at hook
//!   runtime, naming both fixes.

use std::fs;
use std::path::{Path, PathBuf};

/// Marker line identifying a pre-commit hook as workdown-installed.
/// Its presence is what allows a reinstall to overwrite the file.
pub const HOOK_MARKER: &str = "workdown pre-commit hook";

/// Filename of the hook inside the hooks directory.
const HOOK_FILENAME: &str = "pre-commit";

// ── Public types ─────────────────────────────────────────────────────

/// What the installed hook does when views are stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookMode {
    /// Re-render and `git add` the output directory, so the fresh views
    /// land in the commit being made.
    Stage,
    /// Re-render, then fail the commit and let the user review and
    /// stage the changes themselves.
    Check,
}

/// Everything the hook script needs from the project, resolved by the
/// caller. All paths are relative to the project root and are embedded
/// into the script shell-quoted.
#[derive(Debug)]
pub struct HookTemplate {
    pub mode: HookMode,
    /// Path of the project root relative to the repository toplevel
    /// (`git rev-parse --show-prefix`). `None` when the project root is
    /// the toplevel. Hooks always run at the toplevel, so a non-empty
    /// prefix makes the script `cd` first.
    pub project_prefix: Option<String>,
    /// Paths whose staged changes make a re-render necessary: the work
    /// items directory plus the workdown configuration files.
    pub watched_paths: Vec<String>,
    /// The render output directory from `views.yaml`.
    pub output_dir: String,
}

/// The outcome of a successful installation.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallOutcome {
    /// No hook existed; ours was written.
    Installed { path: PathBuf },
    /// A workdown-installed hook existed and was overwritten.
    Replaced { path: PathBuf },
}

/// An error from the install-hooks operation.
#[derive(Debug, thiserror::Error)]
pub enum InstallHooksError {
    /// A pre-commit hook exists that workdown did not write. Per the
    /// item decisions the command stops; the caller tells the user what
    /// to add manually (see [`manual_hook_line`]).
    #[error(
        "a pre-commit hook already exists at '{}' and was not installed by workdown",
        path.display()
    )]
    ForeignHook { path: PathBuf },

    #[error("failed to create hooks directory '{}': {source}", path.display())]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to read existing hook '{}': {source}", path.display())]
    ReadHook {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write hook '{}': {source}", path.display())]
    WriteHook {
        path: PathBuf,
        source: std::io::Error,
    },
}

// ── Public API ───────────────────────────────────────────────────────

/// Generate the pre-commit hook script for `template`.
pub fn hook_script(template: &HookTemplate) -> String {
    let mode_line = match template.mode {
        HookMode::Stage => "Mode: stage — stale views are re-rendered and staged into this commit.",
        HookMode::Check => "Mode: check — the commit fails when the rendered views were stale.",
    };

    let mut script = String::new();
    script.push_str("#!/bin/sh\n");
    script.push_str(&format!(
        "# {HOOK_MARKER} (installed by `workdown install-hooks`).\n"
    ));
    script.push_str(&format!(
        "# Keeps the rendered views in {} in sync with the work items.\n",
        sh_quote(&template.output_dir)
    ));
    script.push_str(&format!("# {mode_line}\n\n"));

    if let Some(prefix) = &template.project_prefix {
        script.push_str("# Hooks run at the repository toplevel; the workdown project\n");
        script.push_str("# lives in a subdirectory.\n");
        script.push_str(&format!("cd {} || exit 1\n\n", sh_quote(prefix)));
    }

    script.push_str("if ! command -v workdown >/dev/null 2>&1; then\n");
    script.push_str(
        "    echo \"pre-commit: 'workdown' is not on PATH; cannot keep rendered views in sync.\" >&2\n",
    );
    script.push_str(
        "    echo \"fix: reinstall workdown, or delete the pre-commit hook it installed.\" >&2\n",
    );
    script.push_str("    exit 1\nfi\n\n");

    let watched = template
        .watched_paths
        .iter()
        .map(|path| sh_quote(path))
        .collect::<Vec<_>>()
        .join(" ");
    script.push_str("# Skip the render when the staged changes touch neither the work\n");
    script.push_str("# items nor the workdown configuration.\n");
    script.push_str(&format!(
        "if git diff --cached --quiet -- {watched}; then\n    exit 0\nfi\n\n"
    ));

    script.push_str("workdown render || exit 1\n");

    let output = sh_quote(&template.output_dir);
    match template.mode {
        HookMode::Stage => {
            script.push_str(&format!("git add -- {output}\n"));
        }
        HookMode::Check => {
            script.push_str(&format!("if ! git diff --quiet -- {output}; then\n"));
            script.push_str(&format!(
                "    echo \"pre-commit: rendered views in {} were stale; they have been re-rendered.\" >&2\n",
                template.output_dir
            ));
            script.push_str(&format!(
                "    echo \"review the changes, run 'git add {}', and commit again.\" >&2\n",
                template.output_dir
            ));
            script.push_str("    exit 1\nfi\n");
        }
    }

    script
}

/// The one-liner a user adds to an existing (foreign) pre-commit hook
/// to get the same behavior as [`HookMode::Stage`].
pub fn manual_hook_line(template: &HookTemplate) -> String {
    format!(
        "workdown render && git add -- {}",
        sh_quote(&template.output_dir)
    )
}

/// Write `script` as the pre-commit hook in `hooks_dir`.
///
/// Creates the hooks directory if needed. Refuses to overwrite a hook
/// that does not carry [`HOOK_MARKER`]. On Unix the hook is made
/// executable; git on Windows runs hooks without an executable bit.
pub fn install_pre_commit(
    hooks_dir: &Path,
    script: &str,
) -> Result<InstallOutcome, InstallHooksError> {
    fs::create_dir_all(hooks_dir).map_err(|source| InstallHooksError::CreateDir {
        path: hooks_dir.to_path_buf(),
        source,
    })?;

    let path = hooks_dir.join(HOOK_FILENAME);
    let existing = match fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => return Err(InstallHooksError::ReadHook { path, source }),
    };

    let replacing = match existing {
        // Non-UTF-8 content can't contain the marker; lossy conversion
        // keeps the check total instead of failing on exotic hooks.
        Some(bytes) if !String::from_utf8_lossy(&bytes).contains(HOOK_MARKER) => {
            return Err(InstallHooksError::ForeignHook { path });
        }
        Some(_) => true,
        None => false,
    };

    fs::write(&path, script).map_err(|source| InstallHooksError::WriteHook {
        path: path.clone(),
        source,
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let permissions = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&path, permissions).map_err(|source| InstallHooksError::WriteHook {
            path: path.clone(),
            source,
        })?;
    }

    if replacing {
        Ok(InstallOutcome::Replaced { path })
    } else {
        Ok(InstallOutcome::Installed { path })
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Quote a string for POSIX sh: wrap in single quotes, escaping any
/// embedded single quote as `'\''`.
fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn template(mode: HookMode) -> HookTemplate {
        HookTemplate {
            mode,
            project_prefix: None,
            watched_paths: vec![
                "workdown-items".to_owned(),
                ".workdown/config.yaml".to_owned(),
                ".workdown/schema.yaml".to_owned(),
            ],
            output_dir: "views".to_owned(),
        }
    }

    #[test]
    fn stage_script_renders_and_stages() {
        let script = hook_script(&template(HookMode::Stage));
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains(HOOK_MARKER));
        assert!(script.contains("workdown render || exit 1\ngit add -- 'views'"));
        assert!(!script.contains("commit again"));
    }

    #[test]
    fn check_script_fails_instead_of_staging() {
        let script = hook_script(&template(HookMode::Check));
        assert!(script.contains("if ! git diff --quiet -- 'views'; then"));
        assert!(script.contains("commit again"));
        assert!(!script.contains("git add -- 'views'\n"));
    }

    #[test]
    fn script_skips_when_watched_paths_unstaged() {
        let script = hook_script(&template(HookMode::Stage));
        assert!(script.contains(
            "if git diff --cached --quiet -- 'workdown-items' '.workdown/config.yaml' '.workdown/schema.yaml'; then"
        ));
    }

    #[test]
    fn script_fails_loudly_without_binary() {
        let script = hook_script(&template(HookMode::Stage));
        assert!(script.contains("command -v workdown"));
        assert!(script.contains("exit 1"));
        assert!(script.contains("reinstall workdown, or delete the pre-commit hook"));
    }

    #[test]
    fn subdirectory_project_gets_a_cd() {
        let mut with_prefix = template(HookMode::Stage);
        with_prefix.project_prefix = Some("tools/planning/".to_owned());
        let script = hook_script(&with_prefix);
        assert!(script.contains("cd 'tools/planning/' || exit 1"));

        let script = hook_script(&template(HookMode::Stage));
        assert!(!script.contains("cd '"));
    }

    #[test]
    fn paths_with_quotes_are_escaped() {
        let mut tricky = template(HookMode::Stage);
        tricky.output_dir = "it's here".to_owned();
        let script = hook_script(&tricky);
        assert!(script.contains("git add -- 'it'\\''s here'"));
    }

    #[test]
    fn manual_line_matches_stage_behavior() {
        assert_eq!(
            manual_hook_line(&template(HookMode::Stage)),
            "workdown render && git add -- 'views'"
        );
    }

    #[test]
    fn install_writes_fresh_hook() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = dir.path().join("hooks");
        let script = hook_script(&template(HookMode::Stage));

        let outcome = install_pre_commit(&hooks, &script).unwrap();
        let expected = hooks.join("pre-commit");
        assert_eq!(
            outcome,
            InstallOutcome::Installed {
                path: expected.clone()
            }
        );
        assert_eq!(fs::read_to_string(expected).unwrap(), script);
    }

    #[test]
    fn install_overwrites_own_hook() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = dir.path().join("hooks");
        let first = hook_script(&template(HookMode::Stage));
        install_pre_commit(&hooks, &first).unwrap();

        let second = hook_script(&template(HookMode::Check));
        let outcome = install_pre_commit(&hooks, &second).unwrap();
        assert!(matches!(outcome, InstallOutcome::Replaced { .. }));
        assert_eq!(
            fs::read_to_string(hooks.join("pre-commit")).unwrap(),
            second
        );
    }

    #[test]
    fn install_refuses_foreign_hook() {
        let dir = tempfile::tempdir().unwrap();
        let hooks = dir.path().join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        let foreign = "#!/bin/sh\nnpm run lint\n";
        fs::write(hooks.join("pre-commit"), foreign).unwrap();

        let script = hook_script(&template(HookMode::Stage));
        let error = install_pre_commit(&hooks, &script).unwrap_err();
        assert!(matches!(error, InstallHooksError::ForeignHook { .. }));
        // The foreign hook is untouched.
        assert_eq!(
            fs::read_to_string(hooks.join("pre-commit")).unwrap(),
            foreign
        );
    }

    #[cfg(unix)]
    #[test]
    fn installed_hook_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let hooks = dir.path().join("hooks");
        let script = hook_script(&template(HookMode::Stage));
        install_pre_commit(&hooks, &script).unwrap();

        let mode = fs::metadata(hooks.join("pre-commit"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
    }
}
