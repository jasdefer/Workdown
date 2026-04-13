//! `workdown init` — scaffold a new project in the current directory.

use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG: &str = include_str!("../../defaults/config.yaml");
const DEFAULT_SCHEMA: &str = include_str!("../../defaults/schema.yaml");
const DEFAULT_RESOURCES: &str = include_str!("../../defaults/resources.yaml");

const WORKDOWN_DIR: &str = ".workdown";
const ITEMS_DIR: &str = "workdown-items";

// ── Public types ─────────────────────────────────────────────────────

/// The outcome of running `workdown init`.
#[derive(Debug, PartialEq, Eq)]
pub enum InitOutcome {
    /// Project was created successfully.
    Created,
    /// `.workdown/` already existed; nothing was modified.
    AlreadyExists,
}

/// An error from the init command.
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("failed to create directory '{}': {source}", path.display())]
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write '{}': {source}", path.display())]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
}

// ── Public API ───────────────────────────────────────────────────────

/// Initialize a new workdown project in `root`.
///
/// Creates `.workdown/` with config, schema, and resources files,
/// plus an empty `workdown-items/` directory and `.workdown/templates/`.
///
/// If `.workdown/` already exists, returns `Ok(InitOutcome::AlreadyExists)`
/// without touching any files.
pub fn run_init(root: &Path, project_name: Option<&str>) -> Result<InitOutcome, InitError> {
    let workdown_dir = root.join(WORKDOWN_DIR);

    // Idempotent: skip if already initialized.
    if workdown_dir.exists() {
        return Ok(InitOutcome::AlreadyExists);
    }

    // Resolve project name: argument > directory name > fallback.
    let name = match project_name {
        Some(n) => n.to_owned(),
        None => root
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_owned())
            .unwrap_or_else(|| "My Project".to_owned()),
    };

    // Prepare config with the project name substituted.
    let config_content = render_config(DEFAULT_CONFIG, &name);

    // Create directories.
    create_dir(&workdown_dir.join("templates"))?;
    create_dir(&root.join(ITEMS_DIR))?;

    // Write default files.
    write_file(&workdown_dir.join("config.yaml"), &config_content)?;
    write_file(&workdown_dir.join("schema.yaml"), DEFAULT_SCHEMA)?;
    write_file(&workdown_dir.join("resources.yaml"), DEFAULT_RESOURCES)?;

    Ok(InitOutcome::Created)
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Replace the project name placeholder in the default config template.
fn render_config(template: &str, project_name: &str) -> String {
    let safe_name = yaml_safe_name(project_name);
    template.replacen("name: My Project", &format!("name: {safe_name}"), 1)
}

/// Wrap a name in double quotes if it contains YAML-special characters.
fn yaml_safe_name(name: &str) -> String {
    let needs_quoting = name.contains(':')
        || name.contains('#')
        || name.contains('{')
        || name.contains('}')
        || name.contains('[')
        || name.contains(']')
        || name.contains('\'')
        || name.contains('"')
        || name.starts_with(' ')
        || name.ends_with(' ');

    if needs_quoting {
        format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        name.to_owned()
    }
}

fn create_dir(path: &Path) -> Result<(), InitError> {
    fs::create_dir_all(path).map_err(|source| InitError::CreateDir {
        path: path.to_path_buf(),
        source,
    })
}

fn write_file(path: &Path, content: &str) -> Result<(), InitError> {
    fs::write(path, content).map_err(|source| InitError::WriteFile {
        path: path.to_path_buf(),
        source,
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_config_replaces_project_name() {
        let result = render_config(DEFAULT_CONFIG, "My Cool App");
        assert!(result.contains("name: My Cool App"));
        assert!(!result.contains("name: My Project"));
    }

    #[test]
    fn render_config_preserves_comments() {
        let result = render_config(DEFAULT_CONFIG, "Test");
        assert!(result.contains("# Default Workdown Configuration"));
    }

    #[test]
    fn yaml_safe_name_plain() {
        assert_eq!(yaml_safe_name("Foo Bar"), "Foo Bar");
    }

    #[test]
    fn yaml_safe_name_with_colon() {
        assert_eq!(yaml_safe_name("Foo: Bar"), "\"Foo: Bar\"");
    }

    #[test]
    fn yaml_safe_name_with_hash() {
        assert_eq!(yaml_safe_name("My #1 Project"), "\"My #1 Project\"");
    }

    #[test]
    fn yaml_safe_name_with_quotes() {
        assert_eq!(yaml_safe_name("He said \"hi\""), "\"He said \\\"hi\\\"\"");
    }

    #[test]
    fn yaml_safe_name_with_leading_space() {
        assert_eq!(yaml_safe_name(" spaced"), "\" spaced\"");
    }
}
