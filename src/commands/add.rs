//! `workdown add` — create a new work item file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::config::Config;
use crate::model::diagnostic::{Diagnostic, DiagnosticKind};
use crate::model::schema::{DefaultValue, Generator, Schema, Severity};
use crate::model::{FieldValue, WorkItem, WorkItemId};
use crate::parser;
use crate::store::Store;

// ── Public types ─────────────────────────────────────────────────────

/// The outcome of a successful `workdown add`.
pub struct AddOutcome {
    /// Path to the created file.
    pub path: PathBuf,
    /// Rule warnings (non-blocking) for the newly created item.
    pub warnings: Vec<Diagnostic>,
}

/// An error from the add command.
#[derive(Debug, thiserror::Error)]
pub enum AddError {
    #[error("failed to load schema: {0}")]
    SchemaLoad(String),

    #[error("failed to load work items: {0}")]
    StoreLoad(#[from] std::io::Error),

    #[error("provide --id or --title to name the new work item")]
    MissingFilenameSource,

    #[error("cannot create a valid filename from title '{title}': {reason}")]
    InvalidSlug { title: String, reason: String },

    #[error("'{id}' is not a valid id: must be lowercase alphanumeric with hyphens, starting with a letter")]
    InvalidId { id: String },

    #[error("work item '{id}' already exists at {path}")]
    AlreadyExists { id: String, path: PathBuf },

    #[error("validation failed for new work item")]
    ValidationFailed { diagnostics: Vec<Diagnostic> },

    #[error("failed to write '{path}': {source}")]
    WriteFile { path: PathBuf, source: std::io::Error },
}

// ── Public API ───────────────────────────────────────────────────────

/// Create a new work item file.
///
/// `field_values` is the user-supplied field map (parsed from CLI flags
/// or constructed directly by tests). Schema defaults fill in any fields
/// the user did not set. Validation runs via the shared coercion path.
pub fn run_add(
    config: &Config,
    project_root: &Path,
    field_values: HashMap<String, serde_yaml::Value>,
) -> Result<AddOutcome, AddError> {
    let schema_path = project_root.join(&config.schema);
    let items_path = project_root.join(&config.paths.work_items);

    tracing::debug!(schema = %schema_path.display(), "loading schema");
    let schema = parser::schema::load_schema(&schema_path)
        .map_err(|e| AddError::SchemaLoad(e.to_string()))?;

    tracing::debug!(items = %items_path.display(), "loading work items");
    let mut store = Store::load(&items_path, &schema)?;

    // Copy user-supplied values into the working frontmatter map.
    let mut frontmatter: HashMap<String, serde_yaml::Value> = field_values.clone();

    // Determine the slug (filename / ID) from --id or --title.
    let user_set_id = frontmatter.contains_key("id");
    let slug = derive_slug(&frontmatter)?;

    let file_path = items_path.join(format!("{slug}.md"));

    // Check for duplicates.
    if file_path.exists() {
        return Err(AddError::AlreadyExists {
            id: slug,
            path: file_path,
        });
    }
    if store.get(&slug).is_some() {
        return Err(AddError::AlreadyExists {
            id: slug.clone(),
            path: file_path,
        });
    }

    // Apply schema defaults for fields the user did not set.
    for (field_name, field_definition) in &schema.fields {
        if field_name == "id" || frontmatter.contains_key(field_name) {
            continue;
        }
        if let Some(ref default) = field_definition.default {
            let value = resolve_default(default, &slug, &store, field_name);
            frontmatter.insert(field_name.clone(), value);
        }
    }

    // Build a RawWorkItem for coercion validation.
    let work_item_id = WorkItemId::from(slug.clone());
    let raw_work_item = parser::RawWorkItem {
        id: work_item_id.clone(),
        frontmatter: frontmatter.clone(),
        body: String::new(),
        source_path: file_path.clone(),
    };

    // Coerce fields — block on errors.
    let (coerced_fields, coercion_diagnostics) =
        crate::store::coerce_fields(&raw_work_item, &schema);
    let coercion_errors: Vec<Diagnostic> = coercion_diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .cloned()
        .collect();

    if !coercion_errors.is_empty() {
        return Err(AddError::ValidationFailed {
            diagnostics: coercion_errors,
        });
    }

    // Serialize frontmatter in schema field order.
    let yaml_content = build_frontmatter_yaml(&frontmatter, &schema, user_set_id);

    // Write the file.
    let file_content = format!("---\n{yaml_content}---\n");
    std::fs::write(&file_path, &file_content).map_err(|source| AddError::WriteFile {
        path: file_path.clone(),
        source,
    })?;

    // Insert into store and run rules for post-write warnings.
    let work_item = WorkItem {
        id: work_item_id.clone(),
        fields: coerced_fields,
        body: String::new(),
        source_path: file_path.clone(),
    };
    store.insert(work_item);

    let rule_diagnostics = crate::rules::evaluate(&store, &schema);
    let warnings: Vec<Diagnostic> = rule_diagnostics
        .into_iter()
        .filter(|diagnostic| is_diagnostic_for_item(diagnostic, &work_item_id))
        .collect();

    Ok(AddOutcome {
        path: file_path,
        warnings,
    })
}

// ── Private helpers ──────────────────────────────────────────────────

/// Determine the slug (filename / id) from the user-supplied field map.
///
/// Explicit `id` wins. Otherwise, slugify `title`. Error if neither.
fn derive_slug(field_values: &HashMap<String, serde_yaml::Value>) -> Result<String, AddError> {
    if let Some(id_value) = field_values.get("id") {
        let id_string = id_value
            .as_str()
            .ok_or_else(|| AddError::InvalidId {
                id: format!("{id_value:?}"),
            })?
            .to_owned();
        if !parser::is_valid_id(&id_string) {
            return Err(AddError::InvalidId { id: id_string });
        }
        return Ok(id_string);
    }

    if let Some(title_value) = field_values.get("title") {
        let title = title_value
            .as_str()
            .ok_or_else(|| AddError::InvalidSlug {
                title: format!("{title_value:?}"),
                reason: "title must be a string".to_owned(),
            })?;
        return slugify(title);
    }

    Err(AddError::MissingFilenameSource)
}

/// Convert a title into a valid kebab-case filename slug.
///
/// Rules: lowercase, non-alphanumeric replaced with hyphens, consecutive
/// hyphens collapsed, leading non-letters stripped, trailing hyphens stripped.
fn slugify(title: &str) -> Result<String, AddError> {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();

    // Collapse consecutive hyphens.
    let mut collapsed = String::with_capacity(slug.len());
    let mut previous_was_hyphen = false;
    for character in slug.chars() {
        if character == '-' {
            if !previous_was_hyphen {
                collapsed.push('-');
            }
            previous_was_hyphen = true;
        } else {
            collapsed.push(character);
            previous_was_hyphen = false;
        }
    }

    // Strip leading non-letters (digits and hyphens).
    let trimmed = collapsed.trim_start_matches(|character: char| !character.is_ascii_lowercase());

    // Strip trailing hyphens.
    let trimmed = trimmed.trim_end_matches('-');

    if trimmed.is_empty() || !parser::is_valid_id(trimmed) {
        return Err(AddError::InvalidSlug {
            title: title.to_owned(),
            reason: "title must contain at least one letter".to_owned(),
        });
    }

    Ok(trimmed.to_owned())
}

/// Resolve a default value into a `serde_yaml::Value`.
fn resolve_default(
    default: &DefaultValue,
    slug: &str,
    store: &Store,
    field_name: &str,
) -> serde_yaml::Value {
    match default {
        DefaultValue::String(string) => serde_yaml::Value::String(string.clone()),
        DefaultValue::Integer(number) => {
            serde_yaml::Value::Number(serde_yaml::Number::from(*number))
        }
        DefaultValue::Float(number) => {
            serde_yaml::to_value(number).unwrap_or(serde_yaml::Value::Null)
        }
        DefaultValue::Bool(flag) => serde_yaml::Value::Bool(*flag),
        DefaultValue::Generator(generator) => resolve_generator(generator, slug, store, field_name),
    }
}

/// Resolve a generator token into a concrete YAML value.
fn resolve_generator(
    generator: &Generator,
    slug: &str,
    store: &Store,
    field_name: &str,
) -> serde_yaml::Value {
    match generator {
        Generator::Filename => serde_yaml::Value::String(slug.to_owned()),
        Generator::FilenamePretty => serde_yaml::Value::String(prettify_slug(slug)),
        Generator::Uuid => serde_yaml::Value::String(uuid::Uuid::new_v4().to_string()),
        Generator::Today => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            serde_yaml::Value::String(today)
        }
        Generator::MaxPlusOne => {
            let max_value = resolve_max_plus_one(store, field_name);
            serde_yaml::Value::Number(serde_yaml::Number::from(max_value))
        }
    }
}

/// Find the maximum integer value of a field across all items, then add 1.
/// Returns 1 if no items have an integer value for this field.
fn resolve_max_plus_one(store: &Store, field_name: &str) -> i64 {
    let mut max: Option<i64> = None;
    for item in store.all_items() {
        if let Some(FieldValue::Integer(value)) = item.fields.get(field_name) {
            max = Some(max.map_or(*value, |current_max: i64| current_max.max(*value)));
        }
    }
    max.unwrap_or(0) + 1
}

/// Convert a slug like `"my-cool-task"` into `"My Cool Task"`.
fn prettify_slug(slug: &str) -> String {
    slug.split('-')
        .map(|word| {
            let mut characters = word.chars();
            match characters.next() {
                None => String::new(),
                Some(first) => {
                    let mut capitalized = first.to_uppercase().to_string();
                    capitalized.extend(characters);
                    capitalized
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build YAML frontmatter string with fields in schema-defined order.
fn build_frontmatter_yaml(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    schema: &Schema,
    user_set_id: bool,
) -> String {
    let mut mapping = serde_yaml::Mapping::new();

    // Emit fields in schema order.
    for field_name in schema.fields.keys() {
        if field_name == "id" && !user_set_id {
            continue;
        }
        if let Some(value) = frontmatter.get(field_name) {
            mapping.insert(
                serde_yaml::Value::String(field_name.clone()),
                value.clone(),
            );
        }
    }

    // Emit any fields not in the schema (alphabetical for determinism).
    let mut extra_keys: Vec<&String> = frontmatter
        .keys()
        .filter(|key| !schema.fields.contains_key(key.as_str()))
        .collect();
    extra_keys.sort();
    for key in extra_keys {
        if let Some(value) = frontmatter.get(key) {
            mapping.insert(serde_yaml::Value::String(key.clone()), value.clone());
        }
    }

    serde_yaml::to_string(&mapping).unwrap_or_default()
}

/// Check whether a diagnostic refers to a specific work item.
fn is_diagnostic_for_item(diagnostic: &Diagnostic, item_id: &WorkItemId) -> bool {
    match &diagnostic.kind {
        DiagnosticKind::InvalidFieldValue {
            item_id: diagnostic_item_id,
            ..
        }
        | DiagnosticKind::MissingRequired {
            item_id: diagnostic_item_id,
            ..
        }
        | DiagnosticKind::UnknownField {
            item_id: diagnostic_item_id,
            ..
        }
        | DiagnosticKind::BrokenLink {
            item_id: diagnostic_item_id,
            ..
        }
        | DiagnosticKind::RuleViolation {
            item_id: diagnostic_item_id,
            ..
        } => diagnostic_item_id == item_id,
        DiagnosticKind::DuplicateId { id, .. } => id == item_id,
        // File errors and count violations are not item-specific in the relevant sense.
        DiagnosticKind::FileError { .. }
        | DiagnosticKind::Cycle { .. }
        | DiagnosticKind::CountViolation { .. } => false,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── slugify ──────────────────────────────────────────────────────

    #[test]
    fn slugify_simple_title() {
        assert_eq!(slugify("My Cool Task").unwrap(), "my-cool-task");
    }

    #[test]
    fn slugify_special_characters() {
        assert_eq!(slugify("Fix Bug #123!").unwrap(), "fix-bug-123");
    }

    #[test]
    fn slugify_extra_spaces_and_symbols() {
        assert_eq!(slugify("  Hello,  World!  ").unwrap(), "hello-world");
    }

    #[test]
    fn slugify_leading_digits_stripped() {
        assert_eq!(slugify("123 Task").unwrap(), "task");
    }

    #[test]
    fn slugify_only_special_characters_fails() {
        assert!(slugify("###!!!").is_err());
    }

    #[test]
    fn slugify_only_digits_fails() {
        assert!(slugify("12345").is_err());
    }

    #[test]
    fn slugify_preserves_internal_digits() {
        assert_eq!(slugify("Task 42 Done").unwrap(), "task-42-done");
    }

    // ── prettify_slug ────────────────────────────────────────────────

    #[test]
    fn prettify_simple_slug() {
        assert_eq!(prettify_slug("my-cool-task"), "My Cool Task");
    }

    #[test]
    fn prettify_single_word() {
        assert_eq!(prettify_slug("task"), "Task");
    }

    #[test]
    fn prettify_with_digits() {
        assert_eq!(prettify_slug("task-42"), "Task 42");
    }

    // ── derive_slug ──────────────────────────────────────────────────

    #[test]
    fn derive_slug_uses_explicit_id() {
        let mut field_values = HashMap::new();
        field_values.insert(
            "id".to_owned(),
            serde_yaml::Value::String("my-id".to_owned()),
        );
        field_values.insert(
            "title".to_owned(),
            serde_yaml::Value::String("Other Title".to_owned()),
        );

        assert_eq!(derive_slug(&field_values).unwrap(), "my-id");
    }

    #[test]
    fn derive_slug_falls_back_to_title() {
        let mut field_values = HashMap::new();
        field_values.insert(
            "title".to_owned(),
            serde_yaml::Value::String("My Title".to_owned()),
        );

        assert_eq!(derive_slug(&field_values).unwrap(), "my-title");
    }

    #[test]
    fn derive_slug_errors_when_neither_given() {
        let field_values = HashMap::new();
        assert!(matches!(
            derive_slug(&field_values),
            Err(AddError::MissingFilenameSource)
        ));
    }

    #[test]
    fn derive_slug_rejects_invalid_id() {
        let mut field_values = HashMap::new();
        field_values.insert(
            "id".to_owned(),
            serde_yaml::Value::String("Invalid ID!".to_owned()),
        );

        assert!(matches!(
            derive_slug(&field_values),
            Err(AddError::InvalidId { .. })
        ));
    }

    // ── resolve_max_plus_one ─────────────────────────────────────────

    #[test]
    fn max_plus_one_empty_store() {
        let schema = minimal_schema();
        let store = empty_store(&schema);
        assert_eq!(resolve_max_plus_one(&store, "order"), 1);
    }

    // ── test helpers ─────────────────────────────────────────────────

    fn minimal_schema() -> Schema {
        use crate::model::schema::{FieldDefinition, FieldTypeConfig};
        use indexmap::IndexMap;

        let mut fields = IndexMap::new();
        fields.insert(
            "title".to_owned(),
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        );
        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    fn empty_store(schema: &Schema) -> Store {
        let directory = tempfile::tempdir().unwrap();
        Store::load(directory.path(), schema).unwrap()
    }
}
