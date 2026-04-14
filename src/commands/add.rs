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

    #[error("invalid --set flag '{raw}': expected KEY=VALUE format")]
    InvalidSetFlag { raw: String },

    #[error("cannot create a valid filename from title '{title}': {reason}")]
    InvalidSlug { title: String, reason: String },

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
/// Loads the schema and store, applies defaults and `--set` overrides,
/// validates via field coercion, writes the file, then runs rules as
/// post-write warnings.
pub fn run_add(
    config: &Config,
    project_root: &Path,
    title: &str,
    set_flags: &[String],
) -> Result<AddOutcome, AddError> {
    let schema_path = project_root.join(&config.schema);
    let items_path = project_root.join(&config.paths.work_items);

    // Load schema.
    tracing::debug!(schema = %schema_path.display(), "loading schema");
    let schema = parser::schema::load_schema(&schema_path)
        .map_err(|e| AddError::SchemaLoad(e.to_string()))?;

    // Load store (for duplicate detection, $max_plus_one, and rules).
    tracing::debug!(items = %items_path.display(), "loading work items");
    let mut store = Store::load(&items_path, &schema)?;

    // Parse --set flags.
    let set_values = parse_set_flags(set_flags)?;

    // Build the frontmatter: start with title from the CLI argument,
    // then let --set flags override (including title if explicitly set).
    let mut frontmatter: HashMap<String, serde_yaml::Value> = HashMap::new();
    frontmatter.insert(
        "title".to_owned(),
        serde_yaml::Value::String(title.to_owned()),
    );
    for (key, value) in &set_values {
        frontmatter.insert(key.clone(), value.clone());
    }

    // Determine the slug (filename / ID).
    let slug = if let Some(id_value) = set_values.get("id") {
        // User explicitly set an ID — use it as the slug.
        let id_string = id_value
            .as_str()
            .ok_or_else(|| AddError::InvalidSlug {
                title: title.to_owned(),
                reason: "--set id value must be a string".to_owned(),
            })?
            .to_owned();
        if !parser::is_valid_id(&id_string) {
            return Err(AddError::InvalidSlug {
                title: title.to_owned(),
                reason: format!(
                    "'{id_string}' is not a valid ID (must be lowercase alphanumeric with hyphens, starting with a letter)"
                ),
            });
        }
        id_string
    } else {
        // Derive from the final title value (which may have been overridden by --set title=...).
        let effective_title = frontmatter
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or(title);
        slugify(effective_title)?
    };

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

    // Apply schema defaults for fields not already set by the user.
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
    let user_set_id = set_values.contains_key("id");
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

/// Parse `--set` flags into a map of field names to YAML values.
///
/// Each flag must be in `KEY=VALUE` format. The value is parsed as YAML,
/// so `42` becomes an integer, `true` becomes a boolean, and `[a, b]`
/// becomes a sequence.
fn parse_set_flags(flags: &[String]) -> Result<HashMap<String, serde_yaml::Value>, AddError> {
    let mut values = HashMap::new();
    for flag in flags {
        let (key, raw_value) = flag
            .split_once('=')
            .ok_or_else(|| AddError::InvalidSetFlag { raw: flag.clone() })?;

        let key = key.trim().to_owned();
        if key.is_empty() {
            return Err(AddError::InvalidSetFlag { raw: flag.clone() });
        }

        let value: serde_yaml::Value = serde_yaml::from_str(raw_value).unwrap_or_else(|_| {
            // If YAML parsing fails, treat the raw value as a plain string.
            serde_yaml::Value::String(raw_value.to_owned())
        });

        values.insert(key, value);
    }
    Ok(values)
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

    // Emit any --set fields not in the schema (alphabetical for determinism).
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
        assert_eq!(
            slugify("  Hello,  World!  ").unwrap(),
            "hello-world"
        );
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

    #[test]
    fn slugify_unicode_replaced() {
        assert_eq!(slugify("Tsk with mlauts").unwrap(), "tsk-with-mlauts");
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

    // ── parse_set_flags ──────────────────────────────────────────────

    #[test]
    fn parse_set_flag_string_value() {
        let flags = vec!["priority=high".to_owned()];
        let values = parse_set_flags(&flags).unwrap();
        assert_eq!(
            values.get("priority").unwrap(),
            &serde_yaml::Value::String("high".into())
        );
    }

    #[test]
    fn parse_set_flag_integer_value() {
        let flags = vec!["count=42".to_owned()];
        let values = parse_set_flags(&flags).unwrap();
        assert!(values.get("count").unwrap().is_number());
    }

    #[test]
    fn parse_set_flag_boolean_value() {
        let flags = vec!["active=true".to_owned()];
        let values = parse_set_flags(&flags).unwrap();
        assert_eq!(
            values.get("active").unwrap(),
            &serde_yaml::Value::Bool(true)
        );
    }

    #[test]
    fn parse_set_flag_list_value() {
        let flags = vec!["tags=[auth, backend]".to_owned()];
        let values = parse_set_flags(&flags).unwrap();
        assert!(values.get("tags").unwrap().is_sequence());
    }

    #[test]
    fn parse_set_flag_value_with_equals() {
        let flags = vec!["description=a=b".to_owned()];
        let values = parse_set_flags(&flags).unwrap();
        // "a=b" doesn't parse as valid YAML scalar cleanly, but serde_yaml
        // will parse it as the string "a=b".
        let value = values.get("description").unwrap();
        assert_eq!(value.as_str().unwrap(), "a=b");
    }

    #[test]
    fn parse_set_flag_missing_equals_fails() {
        let flags = vec!["invalid".to_owned()];
        assert!(parse_set_flags(&flags).is_err());
    }

    #[test]
    fn parse_set_flag_empty_key_fails() {
        let flags = vec!["=value".to_owned()];
        assert!(parse_set_flags(&flags).is_err());
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
