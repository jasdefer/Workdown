//! `workdown add` — create a new work item file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::generators::{resolve_default, resolve_template_tokens};
use crate::model::config::Config;
use crate::model::diagnostic::{Diagnostic, DiagnosticKind};
use crate::model::schema::{Schema, Severity};
use crate::model::template::TemplateError;
use crate::model::WorkItemId;
use crate::operations::templates::load_template_by_name;
use crate::parser;
use crate::parser::schema::SchemaLoadError;

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
    SchemaLoad(#[from] SchemaLoadError),

    #[error("failed to load work items: {0}")]
    StoreLoad(#[from] std::io::Error),

    #[error("provide --id or --title to name the new work item")]
    MissingFilenameSource,

    #[error("cannot create a valid filename from title '{title}': {reason}")]
    InvalidSlug { title: String, reason: String },

    #[error("'{id}' is not a valid id: must be lowercase alphanumeric with hyphens, starting with a letter or digit")]
    InvalidId { id: String },

    #[error("work item '{id}' already exists at {path}")]
    AlreadyExists { id: String, path: PathBuf },

    #[error("validation failed for new work item")]
    ValidationFailed { diagnostics: Vec<Diagnostic> },

    #[error("failed to write '{path}': {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error(transparent)]
    Template(#[from] TemplateError),
}

// ── Public API ───────────────────────────────────────────────────────

/// Create a new work item file.
///
/// `field_values` is the user-supplied field map (parsed from CLI flags
/// or constructed directly by tests). Schema defaults fill in any fields
/// the user did not set. Validation runs via the shared coercion path.
///
/// When `template` is `Some(name)`, the named template is loaded from
/// `config.paths.templates`, its frontmatter and body seed the new item,
/// and `field_values` override on a per-field basis. Precedence: CLI >
/// template > schema defaults.
pub fn run_add(
    config: &Config,
    project_root: &Path,
    field_values: HashMap<String, serde_yaml::Value>,
    template: Option<&str>,
) -> Result<AddOutcome, AddError> {
    let schema_path = project_root.join(&config.schema);
    let items_path = project_root.join(&config.paths.work_items);

    tracing::debug!(schema = %schema_path.display(), "loading schema");
    let schema = parser::schema::load_schema(&schema_path)?;

    tracing::debug!(items = %items_path.display(), "loading work items");
    let store = crate::store::Store::load(&items_path, &schema)?;

    // Load template if requested; start the merged map from template
    // frontmatter, then overlay CLI values (shallow replace).
    let (mut frontmatter, body) = if let Some(name) = template {
        let templates_dir = project_root.join(&config.paths.templates);
        let template = load_template_by_name(&templates_dir, name)?;
        let mut merged = template.frontmatter;
        for (field_name, value) in field_values.iter() {
            merged.insert(field_name.clone(), value.clone());
        }
        (merged, template.body)
    } else {
        (field_values.clone(), String::new())
    };

    // First pass: resolve slug-independent tokens ($today, $uuid,
    // $max_plus_one). This makes `id: $uuid` in a template produce a
    // concrete id before slug derivation. $filename / $filename_pretty
    // are skipped here — they need the slug.
    resolve_template_tokens(&mut frontmatter, None, &store);

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

    // Second pass: resolve slug-dependent tokens ($filename,
    // $filename_pretty) now that the slug is known.
    resolve_template_tokens(&mut frontmatter, Some(&slug), &store);

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
        body: body.clone(),
        source_path: file_path.clone(),
    };

    // Coerce fields — block on errors *before* writing the file.
    let (_, coercion_diagnostics) = crate::store::coerce_fields(&raw_work_item, &schema);
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

    // Write the file. Body (template or empty) follows the closing delimiter.
    let file_content = format!("---\n{yaml_content}---\n{body}");
    std::fs::write(&file_path, &file_content).map_err(|source| AddError::WriteFile {
        path: file_path.clone(),
        source,
    })?;

    // Reload the store from disk: the new file is now part of the items
    // directory, so a fresh `Store::load` resolves aggregates and reverse
    // links correctly. Avoids in-memory `insert` which can't recompute
    // aggregates without per-field provenance.
    let reloaded = crate::store::Store::load(&items_path, &schema)?;

    // Surface diagnostics produced by the reload that concern this new
    // item (broken links, chain conflicts, missing requireds, etc.) plus
    // any rule violations against the post-write store.
    let mut item_warnings: Vec<Diagnostic> = reloaded
        .diagnostics()
        .iter()
        .filter(|diagnostic| is_diagnostic_for_item(diagnostic, &work_item_id))
        .cloned()
        .collect();

    let rule_diagnostics = crate::rules::evaluate(&reloaded, &schema);
    item_warnings.extend(
        rule_diagnostics
            .into_iter()
            .filter(|diagnostic| is_diagnostic_for_item(diagnostic, &work_item_id)),
    );

    Ok(AddOutcome {
        path: file_path,
        warnings: item_warnings,
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
        let title = title_value.as_str().ok_or_else(|| AddError::InvalidSlug {
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

    // Strip leading and trailing hyphens. Leading digits are preserved —
    // `is_valid_id` now accepts digit-first ids.
    let trimmed = collapsed.trim_start_matches('-').trim_end_matches('-');

    if trimmed.is_empty() || !parser::is_valid_id(trimmed) {
        return Err(AddError::InvalidSlug {
            title: title.to_owned(),
            reason: "title must contain at least one alphanumeric character".to_owned(),
        });
    }

    Ok(trimmed.to_owned())
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
            mapping.insert(serde_yaml::Value::String(field_name.clone()), value.clone());
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
        }
        | DiagnosticKind::AggregateChainConflict {
            item_id: diagnostic_item_id,
            ..
        } => diagnostic_item_id == item_id,
        DiagnosticKind::AggregateMissingValue { leaf_id, .. } => leaf_id == item_id,
        DiagnosticKind::DuplicateId { id, .. } => id == item_id,
        // File errors, count violations, and view-level diagnostics don't
        // attach to an individual item.
        DiagnosticKind::FileError { .. }
        | DiagnosticKind::Cycle { .. }
        | DiagnosticKind::CountViolation { .. }
        | DiagnosticKind::ViewDuplicateId { .. }
        | DiagnosticKind::ViewMissingSlot { .. }
        | DiagnosticKind::ViewUnknownField { .. }
        | DiagnosticKind::ViewFieldTypeMismatch { .. }
        | DiagnosticKind::ViewWhereParseError { .. }
        | DiagnosticKind::ViewBucketWithoutDateAxis { .. }
        | DiagnosticKind::ViewCountAggregateWithValue { .. }
        | DiagnosticKind::ViewAggregateTypeMismatch { .. }
        | DiagnosticKind::ViewGroupByCyclic { .. }
        | DiagnosticKind::ViewGroupByInverseNotAllowed { .. }
        | DiagnosticKind::ViewGanttEndOrDurationRequired { .. }
        | DiagnosticKind::ViewGanttEndAndDurationConflict { .. }
        | DiagnosticKind::ViewGanttAfterRequiresDuration { .. }
        | DiagnosticKind::ViewGanttAfterWithEndConflict { .. }
        | DiagnosticKind::ViewGanttAfterCyclic { .. }
        | DiagnosticKind::ViewGanttAfterInverseNotAllowed { .. }
        | DiagnosticKind::ViewGanttRootLinkCyclic { .. }
        | DiagnosticKind::ViewGanttRootLinkInverseNotAllowed { .. }
        | DiagnosticKind::ViewGanttDepthLinkCyclic { .. }
        | DiagnosticKind::ViewGanttDepthLinkInverseNotAllowed { .. }
        | DiagnosticKind::ViewMetricRowUnknownField { .. }
        | DiagnosticKind::ViewMetricRowAggregateTypeMismatch { .. }
        | DiagnosticKind::ViewMetricRowCountWithValue { .. }
        | DiagnosticKind::ViewMetricRowWhereParseError { .. } => false,
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
    fn slugify_preserves_leading_digits() {
        assert_eq!(slugify("123 Task").unwrap(), "123-task");
    }

    #[test]
    fn slugify_only_special_characters_fails() {
        assert!(slugify("###!!!").is_err());
    }

    #[test]
    fn slugify_only_digits_succeeds() {
        assert_eq!(slugify("12345").unwrap(), "12345");
    }

    #[test]
    fn slugify_preserves_internal_digits() {
        assert_eq!(slugify("Task 42 Done").unwrap(), "task-42-done");
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
}
