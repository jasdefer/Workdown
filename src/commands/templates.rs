//! `workdown templates` — list and show work item templates.
//!
//! Also exposes [`load_template_by_name`], used by `workdown add
//! --template <name>` to resolve a template from disk.

use std::fs;
use std::path::Path;

use crate::cli::QueryFormat;
use crate::model::config::Config;
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
pub fn load_template_by_name(
    templates_dir: &Path,
    name: &str,
) -> Result<Template, TemplateError> {
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

/// Run `workdown templates list`.
pub fn run_templates_list(
    config: &Config,
    project_root: &Path,
    format: QueryFormat,
) -> anyhow::Result<()> {
    let templates_dir = project_root.join(&config.paths.templates);
    let names = list_template_names(&templates_dir);

    match format {
        QueryFormat::Table => {
            if names.is_empty() {
                crate::cli::output::info(&format!(
                    "No templates found (drop .md files in {} to add some)",
                    templates_dir.display()
                ));
            } else {
                for name in &names {
                    println!("{name}");
                }
            }
        }
        QueryFormat::Json => {
            let entries: Vec<serde_json::Value> = names
                .iter()
                .map(|name| {
                    serde_json::json!({
                        "name": name,
                        "path": templates_dir.join(format!("{name}.md")).display().to_string(),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string(&entries)?);
        }
        QueryFormat::Tsv | QueryFormat::Csv => {
            let delimiter = match format {
                QueryFormat::Tsv => b'\t',
                QueryFormat::Csv => b',',
                _ => unreachable!(),
            };
            let mut writer = csv::WriterBuilder::new()
                .delimiter(delimiter)
                .terminator(csv::Terminator::Any(b'\n'))
                .from_writer(Vec::<u8>::new());
            writer.write_record(["name", "path"])?;
            for name in &names {
                let path = templates_dir.join(format!("{name}.md")).display().to_string();
                writer.write_record([name.as_str(), path.as_str()])?;
            }
            let buffer = writer.into_inner()?;
            print!("{}", String::from_utf8(buffer)?);
        }
    }

    Ok(())
}

/// Run `workdown templates show <name>`.
///
/// Prints the raw file contents to stdout. Raw dump, not re-serialized —
/// comments and formatting are preserved as authored.
pub fn run_templates_show(
    config: &Config,
    project_root: &Path,
    name: &str,
) -> Result<(), TemplateError> {
    let templates_dir = project_root.join(&config.paths.templates);
    let template = load_template_by_name(&templates_dir, name)?;

    let content = fs::read_to_string(&template.path).map_err(|source| TemplateError::Read {
        path: template.path.clone(),
        source,
    })?;
    print!("{content}");
    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn make_dir() -> (TempDir, PathBuf) {
        let directory = TempDir::new().unwrap();
        let templates_dir = directory.path().join("templates");
        fs::create_dir_all(&templates_dir).unwrap();
        (directory, templates_dir)
    }

    #[test]
    fn list_names_alphabetical() {
        let (_directory, templates_dir) = make_dir();
        fs::write(templates_dir.join("zebra.md"), "---\ntype: a\n---\n").unwrap();
        fs::write(templates_dir.join("alpha.md"), "---\ntype: b\n---\n").unwrap();
        fs::write(templates_dir.join("middle.md"), "---\ntype: c\n---\n").unwrap();

        let names = list_template_names(&templates_dir);
        assert_eq!(names, vec!["alpha".to_owned(), "middle".to_owned(), "zebra".to_owned()]);
    }

    #[test]
    fn list_names_skips_non_md() {
        let (_directory, templates_dir) = make_dir();
        fs::write(templates_dir.join("real.md"), "---\ntype: a\n---\n").unwrap();
        fs::write(templates_dir.join("readme.txt"), "noise").unwrap();

        let names = list_template_names(&templates_dir);
        assert_eq!(names, vec!["real".to_owned()]);
    }

    #[test]
    fn list_names_missing_dir_returns_empty() {
        let directory = TempDir::new().unwrap();
        let missing = directory.path().join("nope");
        assert!(list_template_names(&missing).is_empty());
    }

    #[test]
    fn load_template_success() {
        let (_directory, templates_dir) = make_dir();
        fs::write(
            templates_dir.join("bug.md"),
            "---\ntype: bug\npriority: medium\n---\nBody here.\n",
        )
        .unwrap();

        let template = load_template_by_name(&templates_dir, "bug").unwrap();
        assert_eq!(
            template.frontmatter.get("type").unwrap(),
            &serde_yaml::Value::String("bug".into())
        );
        assert!(template.body.contains("Body here."));
    }

    #[test]
    fn load_template_not_found_lists_alphabetical() {
        let (_directory, templates_dir) = make_dir();
        fs::write(templates_dir.join("alpha.md"), "---\ntype: a\n---\n").unwrap();
        fs::write(templates_dir.join("beta.md"), "---\ntype: b\n---\n").unwrap();

        let result = load_template_by_name(&templates_dir, "missing");
        match result {
            Err(TemplateError::NotFound { name, available }) => {
                assert_eq!(name, "missing");
                assert_eq!(available, vec!["alpha".to_owned(), "beta".to_owned()]);
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn load_template_directory_missing() {
        let directory = TempDir::new().unwrap();
        let missing = directory.path().join("nope");
        let result = load_template_by_name(&missing, "anything");
        assert!(matches!(result, Err(TemplateError::DirectoryMissing { .. })));
    }

    #[test]
    fn load_template_empty_dir_not_found_with_empty_available() {
        let (_directory, templates_dir) = make_dir();
        let result = load_template_by_name(&templates_dir, "ghost");
        match result {
            Err(TemplateError::NotFound { available, .. }) => {
                assert!(available.is_empty());
            }
            other => panic!("expected NotFound, got {other:?}"),
        }
    }
}
