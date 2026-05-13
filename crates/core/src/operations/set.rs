//! `workdown set` — replace a single field on an existing work item.
//!
//! Foundation for every frontmatter mutation. `unset`, `move`, and the
//! type-aware modes (`--append`, `--remove`, `--delta`) reuse this code
//! path; the public API is shaped so they add `SetOperation` variants
//! rather than parallel functions.

use std::path::{Path, PathBuf};

use crate::model::config::Config;
use crate::model::diagnostic::{Diagnostic, ItemDiagnosticKind};
use crate::model::schema::Severity;
use crate::model::WorkItemId;
use crate::operations::frontmatter_io::{build_frontmatter_yaml, write_file_atomically};
use crate::parser;
use crate::parser::schema::SchemaLoadError;

// ── Public types ─────────────────────────────────────────────────────

/// Which mutation to apply to the field.
///
/// Only `Replace` is implemented for now. The type-aware modes
/// (`Append`, `Remove`, `Delta`) and `Unset` land as additional variants
/// in their own issues without changing `run_set`'s signature.
#[derive(Debug, Clone)]
pub enum SetOperation {
    Replace(serde_yaml::Value),
}

/// The outcome of a successful `workdown set`.
#[derive(Debug)]
pub struct SetOutcome {
    /// Path to the file that was written.
    pub path: PathBuf,
    /// The value that was in frontmatter before the write, if any.
    /// `None` means the field was absent.
    pub previous_value: Option<serde_yaml::Value>,
    /// The value written, if any. `None` for future `Unset`.
    pub new_value: Option<serde_yaml::Value>,
    /// All non-blocking diagnostics from the post-write store reload
    /// plus rule evaluation. Includes any coercion warning produced by
    /// this mutation as well as unrelated pre-existing warnings.
    pub warnings: Vec<Diagnostic>,
    /// `true` if the value supplied by this mutation failed coercion
    /// against the field's schema definition. Used by the CLI to set
    /// the exit code — independent from pre-existing warnings on other
    /// items.
    pub mutation_caused_warning: bool,
}

/// Errors returned by [`run_set`].
///
/// Errors here are hard-fails: the file is not written. Soft problems
/// (schema violations on the new value) flow through `SetOutcome.warnings`
/// and `mutation_caused_warning` instead — the file still gets written.
#[derive(Debug, thiserror::Error)]
pub enum SetError {
    #[error("failed to load schema: {0}")]
    SchemaLoad(#[from] SchemaLoadError),

    #[error("failed to load work items: {0}")]
    StoreLoad(#[from] std::io::Error),

    #[error("unknown work item '{id}'")]
    UnknownItem { id: String },

    #[error("unknown field '{field}' (not defined in schema)")]
    UnknownField { field: String },

    #[error("cannot set 'id' — use `workdown rename` to change an item's id")]
    IdNotMutable,

    #[error("failed to read '{path}': {source}")]
    ReadTarget {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse '{path}': {source}")]
    ParseTarget {
        path: PathBuf,
        source: parser::ParseError,
    },

    #[error("failed to write '{path}': {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
}

// ── Public API ───────────────────────────────────────────────────────

/// Replace a single field on a work item.
///
/// Pre-flight checks (unknown id, unknown field, `id` rejection) are
/// hard errors with no disk write. Coercion failures on the new value
/// are soft warnings — the file is written anyway per ADR-001's
/// save-with-warning convention, with `mutation_caused_warning = true`.
pub fn run_set(
    config: &Config,
    project_root: &Path,
    id: &WorkItemId,
    field: &str,
    operation: SetOperation,
) -> Result<SetOutcome, SetError> {
    if field == "id" {
        return Err(SetError::IdNotMutable);
    }

    let schema_path = project_root.join(&config.schema);
    let schema = parser::schema::load_schema(&schema_path)?;

    let field_def = schema
        .fields
        .get(field)
        .ok_or_else(|| SetError::UnknownField {
            field: field.to_owned(),
        })?;

    let items_path = project_root.join(&config.paths.work_items);
    let store = crate::store::Store::load(&items_path, &schema)?;

    let work_item = store
        .get(id.as_str())
        .ok_or_else(|| SetError::UnknownItem { id: id.to_string() })?;
    let file_path = work_item.source_path.clone();

    // Read the file fresh and split frontmatter ourselves so we can see
    // whether `id` was present in the on-disk frontmatter (the parser's
    // `parse_work_item` strips it before handing the map back).
    let file_content =
        std::fs::read_to_string(&file_path).map_err(|source| SetError::ReadTarget {
            path: file_path.clone(),
            source,
        })?;
    let (mut frontmatter, body) =
        parser::split_frontmatter(&file_content, &file_path).map_err(|source| {
            SetError::ParseTarget {
                path: file_path.clone(),
                source,
            }
        })?;

    let user_set_id = frontmatter.contains_key("id");
    let previous_value = frontmatter.get(field).cloned();

    let SetOperation::Replace(new_value) = operation;
    frontmatter.insert(field.to_owned(), new_value.clone());

    // Coerce the new value to surface any schema mismatch as a warning.
    // We still write the file — hand-editing the same bad value would
    // produce the same outcome, so the CLI shouldn't be stricter.
    let mut warnings: Vec<Diagnostic> = Vec::new();
    let mut mutation_caused_warning = false;
    if let Err(detail) = crate::store::coerce_value(&new_value, field_def) {
        warnings.push(Diagnostic::item(
            Severity::Warning,
            file_path.clone(),
            id.clone(),
            ItemDiagnosticKind::InvalidFieldValue {
                field: field.to_owned(),
                detail,
            },
        ));
        mutation_caused_warning = true;
    }

    let yaml_content = build_frontmatter_yaml(&frontmatter, &schema, user_set_id);
    let new_file_content = format!("---\n{yaml_content}---\n{body}");

    write_file_atomically(&file_path, &new_file_content).map_err(|source| SetError::WriteFile {
        path: file_path.clone(),
        source,
    })?;

    // Reload and surface every diagnostic, not just ones tagged to this
    // item — chain conflicts and cross-item warnings need to be visible
    // at the moment the user touches that area.
    let reloaded = crate::store::Store::load(&items_path, &schema)?;
    warnings.extend(reloaded.diagnostics().iter().cloned());
    warnings.extend(crate::rules::evaluate(&reloaded, &schema));

    Ok(SetOutcome {
        path: file_path,
        previous_value,
        new_value: Some(new_value),
        warnings,
        mutation_caused_warning,
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    use tempfile::TempDir;

    use crate::parser::config::load_config;

    const TEST_SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
    default: $filename_pretty
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
    default: open
  priority:
    type: choice
    values: [low, medium, high]
    required: false
  points:
    type: integer
    required: false
  tags:
    type: list
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
";

    const TEST_CONFIG: &str = "\
project:
  name: Test Project
  description: ''
paths:
  work_items: workdown-items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
";

    fn setup_project() -> (TempDir, PathBuf) {
        let directory = TempDir::new().unwrap();
        let root = directory.path().to_path_buf();
        fs::create_dir_all(root.join(".workdown/templates")).unwrap();
        fs::create_dir_all(root.join("workdown-items")).unwrap();
        fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
        fs::write(root.join(".workdown/schema.yaml"), TEST_SCHEMA).unwrap();
        (directory, root)
    }

    fn load_test_config(root: &Path) -> Config {
        load_config(&root.join(".workdown/config.yaml")).unwrap()
    }

    fn write_item(root: &Path, id: &str, content: &str) {
        fs::write(root.join(format!("workdown-items/{id}.md")), content).unwrap();
    }

    fn read_item(root: &Path, id: &str) -> String {
        fs::read_to_string(root.join(format!("workdown-items/{id}.md"))).unwrap()
    }

    // ── Happy path ───────────────────────────────────────────────────

    #[test]
    fn replace_choice_value_writes_file_and_returns_previous() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\n---\nbody text\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Replace(serde_yaml::Value::String("in_progress".to_owned())),
        )
        .unwrap();

        assert_eq!(outcome.previous_value.unwrap().as_str().unwrap(), "open");
        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "in_progress");
        assert!(!outcome.mutation_caused_warning);

        let file = read_item(&root, "task-1");
        assert!(file.contains("status: in_progress"));
        assert!(!file.contains("status: open"));
    }

    #[test]
    fn replace_preserves_body_byte_for_byte() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        let body = "Line one of the body.\n\n## Heading\n\nMore body.\n";
        write_item(
            &root,
            "task-1",
            &format!("---\ntitle: Task 1\nstatus: open\n---\n{body}"),
        );

        run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Replace(serde_yaml::Value::String("done".to_owned())),
        )
        .unwrap();

        let file = read_item(&root, "task-1");
        let body_offset = file.find("---\n").unwrap();
        let after_first = body_offset + 4;
        let closing = file[after_first..].find("---\n").unwrap();
        let body_in_file = &file[after_first + closing + 4..];
        assert_eq!(body_in_file, body);
    }

    #[test]
    fn previous_value_is_none_when_field_was_absent() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "priority",
            SetOperation::Replace(serde_yaml::Value::String("high".to_owned())),
        )
        .unwrap();

        assert!(outcome.previous_value.is_none());
        let file = read_item(&root, "task-1");
        assert!(file.contains("priority: high"));
    }

    // ── Save-with-warning on coercion failure ────────────────────────

    #[test]
    fn invalid_choice_value_saves_with_warning_and_flags_mutation_caused() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Replace(serde_yaml::Value::String("urgent".to_owned())),
        )
        .unwrap();

        assert!(outcome.mutation_caused_warning);
        assert!(!outcome.warnings.is_empty());

        // File was written despite the invalid value.
        let file = read_item(&root, "task-1");
        assert!(file.contains("status: urgent"));
    }

    // ── List replacement ─────────────────────────────────────────────

    #[test]
    fn list_field_replace_writes_sequence() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let value = serde_yaml::Value::Sequence(vec![
            serde_yaml::Value::String("auth".to_owned()),
            serde_yaml::Value::String("backend".to_owned()),
        ]);

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Replace(value),
        )
        .unwrap();

        assert!(!outcome.mutation_caused_warning);
        let file = read_item(&root, "task-1");
        assert!(file.contains("tags:"));
        assert!(file.contains("auth"));
        assert!(file.contains("backend"));
    }

    // ── Hard errors ──────────────────────────────────────────────────

    #[test]
    fn unknown_item_errors_without_writing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("does-not-exist".to_owned()),
            "status",
            SetOperation::Replace(serde_yaml::Value::String("done".to_owned())),
        );

        assert!(matches!(result, Err(SetError::UnknownItem { .. })));
    }

    #[test]
    fn unknown_field_errors_without_writing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "nonexistent",
            SetOperation::Replace(serde_yaml::Value::String("x".to_owned())),
        );

        assert!(matches!(result, Err(SetError::UnknownField { .. })));

        // File untouched.
        let file = read_item(&root, "task-1");
        assert!(!file.contains("nonexistent"));
    }

    #[test]
    fn setting_id_returns_error_with_rename_hint() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "id",
            SetOperation::Replace(serde_yaml::Value::String("new-id".to_owned())),
        );

        let error = result.unwrap_err();
        assert!(matches!(error, SetError::IdNotMutable));
        assert!(error.to_string().contains("workdown rename"));
    }

    // ── Explicit id in frontmatter is preserved ──────────────────────

    #[test]
    fn explicit_id_in_frontmatter_is_preserved_after_set() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        // Filename and frontmatter id differ — id was user-set.
        write_item(
            &root,
            "filename-slug",
            "---\nid: custom-id\ntitle: Task\nstatus: open\n---\n",
        );

        run_set(
            &config,
            &root,
            &WorkItemId::from("custom-id".to_owned()),
            "status",
            SetOperation::Replace(serde_yaml::Value::String("done".to_owned())),
        )
        .unwrap();

        let file = read_item(&root, "filename-slug");
        assert!(file.contains("id: custom-id"));
        assert!(file.contains("status: done"));
    }
}
