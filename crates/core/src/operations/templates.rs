//! Template loading and listing — domain logic used by `workdown add` and
//! the `workdown templates` CLI commands.

use std::fs;
use std::path::Path;

use crate::model::template::{Template, TemplateError};
use crate::parser::template::parse_template_content;

// ── Public API ───────────────────────────────────────────────────────

/// Load a named template from `templates_dir`.
///
/// Errors:
/// - [`TemplateError::DirectoryMissing`] if `templates_dir` itself is absent.
/// - [`TemplateError::NotFound`] if `<name>.md` is not in the directory.
///   The variant includes the alphabetical list of available template names.
/// - [`TemplateError::Read`] / [`TemplateError::Parse`] on IO or YAML errors.
pub fn load_template_by_name(templates_dir: &Path, name: &str) -> Result<Template, TemplateError> {
    if !templates_dir.exists() {
        return Err(TemplateError::DirectoryMissing {
            path: templates_dir.to_path_buf(),
        });
    }

    let path = templates_dir.join(format!("{name}.md"));
    if !path.exists() {
        return Err(TemplateError::NotFound {
            name: name.to_owned(),
            available: list_template_names(templates_dir),
        });
    }

    let content = fs::read_to_string(&path).map_err(|source| TemplateError::Read {
        path: path.clone(),
        source,
    })?;

    let template = parse_template_content(&content, &path)?;
    Ok(template)
}

/// Return template names (without `.md`) in `templates_dir`, sorted
/// alphabetically. Returns an empty list if the directory does not exist.
pub fn list_template_names(templates_dir: &Path) -> Vec<String> {
    if !templates_dir.exists() {
        return Vec::new();
    }

    let mut names = Vec::new();
    let entries = match fs::read_dir(templates_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            names.push(stem.to_owned());
        }
    }
    names.sort();
    names
}
