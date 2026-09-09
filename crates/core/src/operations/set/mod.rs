//! `workdown set` — replace a single field on an existing work item.
//!
//! Foundation for every frontmatter mutation. `unset`, `move`, and the
//! type-aware modes (`--append`, `--remove`, `--delta`, `--toggle`)
//! reuse this code path; the public API is shaped so they add
//! `SetOperation` variants rather than parallel functions.
//!
//! Dispatch lives here; per-family compute logic lives in the
//! `collection`, `numeric`, `temporal`, and `boolean` submodules.
//! `Replace` and `Unset` are mode-agnostic and stay inline below.

mod boolean;
mod collection;
mod numeric;
mod temporal;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::config::Config;
use crate::model::diagnostic::Diagnostic;
use crate::model::resources::Resources;
use crate::model::schema::{FieldDefinition, FieldType, Schema};
use crate::model::WorkItemId;
use crate::operations::frontmatter_io::{build_frontmatter_yaml, write_file_atomically};
use crate::parser;
use crate::parser::schema::SchemaLoadError;

// ── Public types ─────────────────────────────────────────────────────

/// Per-field mutation. `run_set` dispatches on this variant in its
/// compute phase.
///
/// `Replace` and `Unset` are uniform across every field type — the
/// value is whatever the caller built, and the post-write reload's
/// coerce pass surfaces any type mismatch as a warning (save-with-warning
/// per ADR-001).
///
/// Type-aware modes live in per-family sub-enums so invalid mode/type
/// combinations are partially unrepresentable: there is no
/// `CollectionMode::Delta`, so `Append`/`Remove`/`Delta`/`Toggle` cannot
/// be mixed across families. The outer variant tells `check_mode_valid`
/// which family the caller intends; the field's schema type is checked
/// against it at the boundary.
#[derive(Debug, Clone)]
pub enum SetOperation {
    /// Replace the field's value (or set it if absent).
    Replace(serde_yaml::Value),
    /// Remove the field from frontmatter entirely.
    Unset,
    /// Type-aware modes for `list`, `links`, and `multichoice` fields.
    Collection(CollectionMode),
    /// Type-aware modes for `integer` / `float` fields.
    Numeric(NumericMode),
    /// Type-aware modes for `duration` fields.
    Duration(DurationMode),
    /// Type-aware modes for `date` fields.
    Date(DateMode),
    /// Type-aware modes for `boolean` fields.
    Boolean(BooleanMode),
}

/// Mutations available on collection-shaped fields (`list`, `links`,
/// `multichoice`). Both modes accept one or more values; the caller
/// (the CLI or the server) builds a `Vec<Value>` from comma-separated
/// input or a JSON array.
#[derive(Debug, Clone)]
pub enum CollectionMode {
    /// Append values to the end of the current sequence. Duplicates are
    /// allowed and emit an info message — `list` is a true sequence and
    /// honoring the literal request beats silent idempotency.
    Append(Vec<serde_yaml::Value>),
    /// Remove every occurrence of each value from the current sequence.
    /// Values that weren't present emit an info message.
    Remove(Vec<serde_yaml::Value>),
}

/// Mutations available on `integer` and `float` fields.
#[derive(Debug, Clone)]
pub enum NumericMode {
    /// Add a signed number to the current value. The caller picks the
    /// `Number` shape (`i64` for `integer` fields, `f64` for `float`)
    /// — `compute_mutation` preserves the field's typing.
    Delta(serde_yaml::Number),
}

/// Mutations available on `duration` fields. The delta is canonical
/// signed seconds (use [`crate::model::duration::parse_duration`] from
/// the user's input).
#[derive(Debug, Clone)]
pub enum DurationMode {
    Delta(i64),
}

/// Mutations available on `date` fields. The delta is a signed duration
/// in seconds, applied as a `chrono::Duration` to the current
/// `NaiveDate`. Sub-day units truncate the same way `chrono`'s date
/// arithmetic does.
#[derive(Debug, Clone)]
pub enum DateMode {
    Delta(i64),
}

/// Mutations available on `boolean` fields.
#[derive(Debug, Clone)]
pub enum BooleanMode {
    /// Flip the current value. Requires an existing `Value::Bool`; an
    /// absent or non-boolean current value is a hard error.
    Toggle,
}

/// The outcome of a successful `workdown set`.
#[derive(Debug)]
pub struct SetOutcome {
    /// Path to the file that was written.
    pub path: PathBuf,
    /// The value that was in frontmatter before the write, if any.
    /// `None` means the field was absent. The value-dependent modes
    /// (`--delta`, `--toggle`) count a field written with no value as
    /// absent too, and report `None` for it rather than a null the file
    /// never really said.
    pub previous_value: Option<serde_yaml::Value>,
    /// The value written, if any. `None` for an `Unset` operation.
    pub new_value: Option<serde_yaml::Value>,
    /// All non-blocking diagnostics from the post-write store reload
    /// plus rule evaluation. Includes any coercion warning produced by
    /// this mutation as well as unrelated pre-existing warnings.
    pub warnings: Vec<Diagnostic>,
    /// Operation-level informational messages (e.g. "value 'qa' was
    /// already present in 'tags'" on a duplicate append). These describe
    /// what the operation *did* rather than a problem with the resulting
    /// file state, so they do not affect the exit code.
    pub info_messages: Vec<String>,
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

    #[error("cannot modify 'id' — use `workdown rename` to change an item's id")]
    IdNotMutable,

    #[error("cannot --{mode} on field '{field}' (type: {field_type})")]
    ModeNotValidForFieldType {
        mode: &'static str,
        field: String,
        field_type: FieldType,
    },

    #[error("cannot --{mode} on absent field '{field}' — set an initial value first")]
    MutationRequiresExistingValue { mode: &'static str, field: String },

    #[error("cannot --{mode} on field '{field}': current value is not a valid {expected}")]
    MutationCurrentValueMalformed {
        mode: &'static str,
        field: String,
        expected: &'static str,
    },

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

/// Apply a single field mutation to a work item.
///
/// Three phases:
///
/// 1. **Pre-flight** — schema/store load, validate id/field/`id`-key,
///    read the target file, capture pre-mutation diagnostics for the
///    diff. Hard errors here never touch disk.
/// 2. **Compute** — build the new frontmatter map from the requested
///    [`SetOperation`]. Decides whether a write is actually needed
///    (no-op unsets skip it).
/// 3. **Finalize** — atomic write (if needed), reload, diff diagnostics.
///    Any diagnostic present after the mutation but not before flips
///    `mutation_caused_warning`. Per ADR-001's save-with-warning
///    convention, every reload diagnostic is surfaced; the diff is what
///    drives exit code, not severity or scope.
pub fn run_set(
    config: &Config,
    project_root: &Path,
    id: &WorkItemId,
    field: &str,
    operation: SetOperation,
) -> Result<SetOutcome, SetError> {
    let context = preflight(config, project_root, id, field, &operation)?;
    let computed = compute_mutation(&context, field, operation);
    finalize_mutation(context, computed)
}

// ── Phase 1: pre-flight ─────────────────────────────────────────────

/// Loaded inputs and pre-mutation state, shared between compute and finalize.
struct SetContext {
    schema: Schema,
    /// Resources feed `$constants.<name>` in compute expressions —
    /// loaded once so the pre- and post-write snapshots derive the same
    /// values.
    resources: Resources,
    items_path: PathBuf,
    file_path: PathBuf,
    frontmatter: HashMap<String, serde_yaml::Value>,
    body: String,
    user_set_id: bool,
    /// `Store::load` + `rules::evaluate` snapshot taken *before* the write.
    /// Diffed against the post-write snapshot to drive
    /// `mutation_caused_warning`.
    pre_diagnostics: Vec<Diagnostic>,
}

fn preflight(
    config: &Config,
    project_root: &Path,
    id: &WorkItemId,
    field: &str,
    operation: &SetOperation,
) -> Result<SetContext, SetError> {
    if field == "id" {
        return Err(SetError::IdNotMutable);
    }

    let schema_path = project_root.join(&config.schema);
    let schema = parser::schema::load_schema(&schema_path)?;

    let field_definition = schema
        .fields
        .get(field)
        .ok_or_else(|| SetError::UnknownField {
            field: field.to_owned(),
        })?;

    check_mode_valid(operation, field_definition, field)?;

    let items_path = project_root.join(&config.paths.work_items);
    // A missing or malformed resources.yaml degrades to empty resources
    // here — `workdown validate` owns reporting it.
    let (resources, _) =
        crate::resources_check::load_and_check(&project_root.join(&config.paths.resources));
    let store = crate::store::Store::load_with_resources(&items_path, &schema, &resources)?;

    let work_item = store
        .get(id.as_str())
        .ok_or_else(|| SetError::UnknownItem { id: id.to_string() })?;
    let file_path = work_item.source_path.clone();

    // Snapshot pre-write diagnostics for the post-write diff.
    let mut pre_diagnostics: Vec<Diagnostic> = store.diagnostics().to_vec();
    pre_diagnostics.extend(crate::rules::evaluate(&store, &schema));

    // Read the file fresh and split frontmatter ourselves so we can see
    // whether `id` was present in the on-disk frontmatter (the parser's
    // `parse_work_item` strips it before handing the map back).
    let file_content =
        std::fs::read_to_string(&file_path).map_err(|source| SetError::ReadTarget {
            path: file_path.clone(),
            source,
        })?;
    let (frontmatter, body) =
        parser::split_frontmatter(&file_content, &file_path).map_err(|source| {
            SetError::ParseTarget {
                path: file_path.clone(),
                source,
            }
        })?;
    let user_set_id = frontmatter.contains_key("id");

    // Preconditions that need access to the current value live here:
    // `--delta` and `--toggle` need an existing, parseable value, which
    // we can only check after the frontmatter is read.
    check_operation_preconditions(operation, &frontmatter, field)?;

    Ok(SetContext {
        schema,
        resources,
        items_path,
        file_path,
        frontmatter,
        body,
        user_set_id,
        pre_diagnostics,
    })
}

/// Reject mode/field-type combinations that aren't supported.
///
/// `Replace` and `Unset` are valid for every field type. Type-aware
/// modes (`Append`, `Remove`, `Delta`, `Toggle`) pin each mode to the
/// family it applies to.
///
/// The match on `operation` is intentionally exhaustive: when a new
/// `SetOperation` variant is added, this function fails to compile
/// until the new variant's validity is decided.
fn check_mode_valid(
    operation: &SetOperation,
    field_definition: &FieldDefinition,
    field: &str,
) -> Result<(), SetError> {
    use crate::model::schema::FieldTypeConfig;

    let mode_label = operation_mode_label(operation);
    let valid = match operation {
        SetOperation::Replace(_) | SetOperation::Unset => true,
        SetOperation::Collection(_) => matches!(
            &field_definition.type_config,
            FieldTypeConfig::List
                | FieldTypeConfig::Links { .. }
                | FieldTypeConfig::Multichoice { .. }
        ),
        SetOperation::Numeric(_) => matches!(
            &field_definition.type_config,
            FieldTypeConfig::Integer { .. } | FieldTypeConfig::Float { .. }
        ),
        SetOperation::Duration(_) => {
            matches!(
                &field_definition.type_config,
                FieldTypeConfig::Duration { .. }
            )
        }
        SetOperation::Date(_) => matches!(&field_definition.type_config, FieldTypeConfig::Date),
        SetOperation::Boolean(_) => {
            matches!(&field_definition.type_config, FieldTypeConfig::Boolean)
        }
    };

    if valid {
        Ok(())
    } else {
        Err(SetError::ModeNotValidForFieldType {
            mode: mode_label,
            field: field.to_owned(),
            field_type: field_definition.field_type(),
        })
    }
}

/// Short human-readable label for the mode an operation represents.
/// Used in user-facing error messages (`cannot --delta on …`).
fn operation_mode_label(operation: &SetOperation) -> &'static str {
    match operation {
        SetOperation::Replace(_) => "replace",
        SetOperation::Unset => "unset",
        SetOperation::Collection(CollectionMode::Append(_)) => "append",
        SetOperation::Collection(CollectionMode::Remove(_)) => "remove",
        SetOperation::Numeric(NumericMode::Delta(_))
        | SetOperation::Duration(DurationMode::Delta(_))
        | SetOperation::Date(DateMode::Delta(_)) => "delta",
        SetOperation::Boolean(BooleanMode::Toggle) => "toggle",
    }
}

/// The field's current value as the value-dependent modes see it.
///
/// Three spellings mean the same thing — "nothing to start from": the
/// key is missing, the key is written with no value (`effort:` parses as
/// YAML null), and the key holds an empty string. None of them carries a
/// value that arithmetic could destroy, so one rule decides what
/// "absent" means and each family decides what to do about it: a
/// `duration` starts its delta at zero, the strict families ask for an
/// initial value.
///
/// Deliberately not applied to `Replace`, `Unset` or the collection
/// modes. Those operate on the key itself, where `effort:` and a missing
/// `effort` really are different — one has to be removed from the file,
/// the other is already gone.
pub(crate) fn current_value<'a>(
    frontmatter: &'a HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Option<&'a serde_yaml::Value> {
    match frontmatter.get(field) {
        None | Some(serde_yaml::Value::Null) => None,
        Some(serde_yaml::Value::String(string)) if string.trim().is_empty() => None,
        Some(value) => Some(value),
    }
}

/// Reject mutations whose semantics need a current value the mode can
/// interpret (`--delta`, `--toggle`) when the on-disk value can't be
/// interpreted, or — for every family but `duration` — when there is no
/// value at all.
///
/// Runs after the frontmatter has been read so it can inspect the
/// current value. Hard error — the file is not written. Each family
/// module owns its own check; this dispatcher just routes by variant.
fn check_operation_preconditions(
    operation: &SetOperation,
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match operation {
        SetOperation::Replace(_) | SetOperation::Unset | SetOperation::Collection(_) => Ok(()),
        SetOperation::Numeric(NumericMode::Delta(_)) => {
            numeric::require_existing(frontmatter, field)
        }
        SetOperation::Duration(DurationMode::Delta(_)) => {
            temporal::require_absent_or_valid_duration(frontmatter, field)
        }
        SetOperation::Date(DateMode::Delta(_)) => {
            temporal::require_existing_date(frontmatter, field)
        }
        SetOperation::Boolean(BooleanMode::Toggle) => boolean::require_existing(frontmatter, field),
    }
}

// ── Phase 2: compute ────────────────────────────────────────────────

/// Post-mutation frontmatter and what to report back about the change.
///
/// Constructed by `compute_mutation` (for `Replace`/`Unset`) or by each
/// family module's `compute_*` function. Private to the `set` module
/// tree — descendant modules access the fields directly.
struct ComputedMutation {
    new_frontmatter: HashMap<String, serde_yaml::Value>,
    previous_value: Option<serde_yaml::Value>,
    new_value: Option<serde_yaml::Value>,
    /// `false` when the operation is a no-op on disk (e.g. unsetting an
    /// absent field). Finalize skips the write but still reloads so
    /// unrelated diagnostics surface.
    write_needed: bool,
    /// Operation-level info messages (e.g. duplicate-append, remove-of-absent).
    /// Surfaced to the user but do not affect the exit code.
    info_messages: Vec<String>,
}

fn compute_mutation(
    context: &SetContext,
    field: &str,
    operation: SetOperation,
) -> ComputedMutation {
    let previous_value = context.frontmatter.get(field).cloned();
    let new_frontmatter = context.frontmatter.clone();

    // The value-dependent modes see the current value through the same
    // rule their preconditions used, so what they report as the previous
    // value matches what they computed from.
    let previous_interpreted_value = current_value(&context.frontmatter, field).cloned();

    match operation {
        SetOperation::Replace(new_value) => {
            compute_replace(new_frontmatter, field, previous_value, new_value)
        }
        SetOperation::Unset => compute_unset(new_frontmatter, field, previous_value),
        SetOperation::Collection(mode) => {
            collection::compute(new_frontmatter, field, mode, previous_value)
        }
        SetOperation::Numeric(NumericMode::Delta(delta)) => {
            numeric::compute_delta(new_frontmatter, field, delta, previous_interpreted_value)
        }
        SetOperation::Duration(DurationMode::Delta(seconds)) => temporal::compute_duration_delta(
            new_frontmatter,
            field,
            seconds,
            previous_interpreted_value,
        ),
        SetOperation::Date(DateMode::Delta(seconds)) => temporal::compute_date_delta(
            new_frontmatter,
            field,
            seconds,
            previous_interpreted_value,
        ),
        SetOperation::Boolean(BooleanMode::Toggle) => {
            boolean::compute_toggle(new_frontmatter, field, previous_interpreted_value)
        }
    }
}

fn compute_replace(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    previous_value: Option<serde_yaml::Value>,
    new_value: serde_yaml::Value,
) -> ComputedMutation {
    new_frontmatter.insert(field.to_owned(), new_value.clone());
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: Some(new_value),
        write_needed: true,
        info_messages: Vec::new(),
    }
}

fn compute_unset(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    // Idempotent: unset on an absent field leaves the file
    // byte-identical. Typo'd field names are already caught by
    // the `UnknownField` check in pre-flight, so silent success
    // here doesn't hide bad input.
    let write_needed = previous_value.is_some();
    if write_needed {
        new_frontmatter.remove(field);
    }
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: None,
        write_needed,
        info_messages: Vec::new(),
    }
}

// ── Phase 3: finalize ───────────────────────────────────────────────

fn finalize_mutation(
    context: SetContext,
    computed: ComputedMutation,
) -> Result<SetOutcome, SetError> {
    if computed.write_needed {
        let yaml_content = build_frontmatter_yaml(
            &computed.new_frontmatter,
            &context.schema,
            context.user_set_id,
        );
        let new_file_content = format!("---\n{yaml_content}---\n{}", context.body);

        write_file_atomically(&context.file_path, &new_file_content).map_err(|source| {
            SetError::WriteFile {
                path: context.file_path.clone(),
                source,
            }
        })?;
    }

    // Reload and surface every diagnostic. The pre/post diff is what
    // drives `mutation_caused_warning` — pre-existing problems elsewhere
    // in the project remain visible (per the milestone's "always show
    // all" convention) but don't fail this mutation.
    let reloaded = crate::store::Store::load_with_resources(
        &context.items_path,
        &context.schema,
        &context.resources,
    )?;
    let mut post_diagnostics: Vec<Diagnostic> = reloaded.diagnostics().to_vec();
    post_diagnostics.extend(crate::rules::evaluate(&reloaded, &context.schema));

    let mutation_caused_warning = crate::operations::diagnostics::introduced_by_mutation(
        &context.pre_diagnostics,
        &post_diagnostics,
    );

    Ok(SetOutcome {
        path: context.file_path,
        previous_value: computed.previous_value,
        new_value: computed.new_value,
        warnings: post_diagnostics,
        info_messages: computed.info_messages,
        mutation_caused_warning,
    })
}
