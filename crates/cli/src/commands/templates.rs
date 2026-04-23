//! `workdown templates` — CLI output for template listing and display.

use std::fs;
use std::path::Path;

use crate::cli::QueryFormat;
use workdown_core::model::config::Config;
use workdown_core::model::template::TemplateError;
use workdown_core::operations::templates::{list_template_names, load_template_by_name};

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
                let path = templates_dir
                    .join(format!("{name}.md"))
                    .display()
                    .to_string();
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
