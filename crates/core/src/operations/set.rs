//! `workdown set` — replace a single field on an existing work item.
//!
//! Foundation for every frontmatter mutation. `unset`, `move`, and the
//! type-aware modes (`--append`, `--remove`, `--delta`) reuse this code
//! path; the public API is shaped so they add `SetOperation` variants
//! rather than parallel functions.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::model::config::Config;
use crate::model::diagnostic::Diagnostic;
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
/// (CLI or future server) builds a `Vec<Value>` from comma-separated
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
    /// `None` means the field was absent.
    pub previous_value: Option<serde_yaml::Value>,
    /// The value written, if any. `None` for future `Unset`.
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
struct MutationContext {
    schema: Schema,
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
) -> Result<MutationContext, SetError> {
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
    let store = crate::store::Store::load(&items_path, &schema)?;

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

    Ok(MutationContext {
        schema,
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
/// modes (`Append`, `Remove`, `Delta`, `Toggle`) added in later steps
/// add an arm here, pinning each mode to the family it applies to.
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

/// Reject mutations whose semantics need an existing, parseable current
/// value (`--delta`, `--toggle` once added) when the field is absent or
/// the on-disk value can't be interpreted.
///
/// Runs after the frontmatter has been read so it can inspect the
/// current value. Hard error — the file is not written.
fn check_operation_preconditions(
    operation: &SetOperation,
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match operation {
        SetOperation::Replace(_) | SetOperation::Unset | SetOperation::Collection(_) => Ok(()),
        SetOperation::Numeric(NumericMode::Delta(_)) => {
            require_existing_numeric(frontmatter, field)
        }
        SetOperation::Duration(DurationMode::Delta(_)) => {
            require_existing_duration(frontmatter, field)
        }
        SetOperation::Date(DateMode::Delta(_)) => require_existing_date(frontmatter, field),
        SetOperation::Boolean(BooleanMode::Toggle) => require_existing_boolean(frontmatter, field),
    }
}

fn require_existing_boolean(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match frontmatter.get(field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "toggle",
            field: field.to_owned(),
        }),
        Some(serde_yaml::Value::Bool(_)) => Ok(()),
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "toggle",
            field: field.to_owned(),
            expected: "boolean",
        }),
    }
}

fn require_existing_numeric(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match frontmatter.get(field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "delta",
            field: field.to_owned(),
        }),
        Some(value) if value.as_i64().is_some() || value.as_f64().is_some() => Ok(()),
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "number",
        }),
    }
}

fn require_existing_duration(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match frontmatter.get(field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "delta",
            field: field.to_owned(),
        }),
        Some(serde_yaml::Value::String(string))
            if crate::model::duration::parse_duration(string).is_ok() =>
        {
            Ok(())
        }
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "duration string (e.g. '1w 2d', '-3h')",
        }),
    }
}

fn require_existing_date(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match frontmatter.get(field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "delta",
            field: field.to_owned(),
        }),
        Some(serde_yaml::Value::String(string))
            if chrono::NaiveDate::parse_from_str(string, "%Y-%m-%d").is_ok() =>
        {
            Ok(())
        }
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "date (YYYY-MM-DD)",
        }),
    }
}

// ── Phase 2: compute ────────────────────────────────────────────────

/// Post-mutation frontmatter and what to report back about the change.
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
    context: &MutationContext,
    field: &str,
    operation: SetOperation,
) -> ComputedMutation {
    let previous_value = context.frontmatter.get(field).cloned();
    let mut new_frontmatter = context.frontmatter.clone();

    match operation {
        SetOperation::Replace(new_value) => {
            new_frontmatter.insert(field.to_owned(), new_value.clone());
            ComputedMutation {
                new_frontmatter,
                previous_value,
                new_value: Some(new_value),
                write_needed: true,
                info_messages: Vec::new(),
            }
        }
        SetOperation::Unset => {
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
        SetOperation::Collection(CollectionMode::Append(values)) => {
            let current_sequence = current_value_as_sequence(previous_value.as_ref());
            let (new_sequence, info_messages) = append_to_sequence(current_sequence, values, field);
            let new_value = serde_yaml::Value::Sequence(new_sequence);
            new_frontmatter.insert(field.to_owned(), new_value.clone());
            ComputedMutation {
                new_frontmatter,
                previous_value,
                new_value: Some(new_value),
                // Append always writes — duplicate-append is intentional
                // (decision 4 in cli-set-modes), surfaced via info_messages.
                write_needed: true,
                info_messages,
            }
        }
        SetOperation::Collection(CollectionMode::Remove(values)) => {
            let current_sequence = current_value_as_sequence(previous_value.as_ref());
            let (new_sequence, info_messages) =
                remove_from_sequence(current_sequence.clone(), values, field);
            // Skip the write when the sequence is unchanged — covers
            // both "remove from absent field" and "remove value that
            // wasn't there". Keeps the file byte-identical when nothing
            // happened on disk.
            let write_needed = current_sequence != new_sequence;
            let new_value = if write_needed {
                let value = serde_yaml::Value::Sequence(new_sequence);
                new_frontmatter.insert(field.to_owned(), value.clone());
                Some(value)
            } else {
                previous_value.clone()
            };
            ComputedMutation {
                new_frontmatter,
                previous_value,
                new_value,
                write_needed,
                info_messages,
            }
        }
        SetOperation::Numeric(NumericMode::Delta(delta)) => {
            // Preconditions guarantee `previous_value` is `Some(Number)`.
            let new_value = apply_numeric_delta(
                previous_value
                    .as_ref()
                    .expect("precondition ensures existing numeric value"),
                &delta,
            );
            new_frontmatter.insert(field.to_owned(), new_value.clone());
            ComputedMutation {
                new_frontmatter,
                previous_value,
                new_value: Some(new_value),
                write_needed: true,
                info_messages: Vec::new(),
            }
        }
        SetOperation::Duration(DurationMode::Delta(delta_seconds)) => {
            let current_string = previous_value
                .as_ref()
                .and_then(|value| value.as_str())
                .expect("precondition ensures existing duration string");
            let current_seconds = crate::model::duration::parse_duration(current_string)
                .expect("precondition ensures parseable duration");
            let new_seconds = current_seconds.saturating_add(delta_seconds);
            let new_string = crate::model::duration::format_duration_seconds(new_seconds);
            let new_value = serde_yaml::Value::String(new_string);
            new_frontmatter.insert(field.to_owned(), new_value.clone());
            ComputedMutation {
                new_frontmatter,
                previous_value,
                new_value: Some(new_value),
                write_needed: true,
                info_messages: Vec::new(),
            }
        }
        SetOperation::Date(DateMode::Delta(delta_seconds)) => {
            let current_string = previous_value
                .as_ref()
                .and_then(|value| value.as_str())
                .expect("precondition ensures existing date string");
            let current_date = chrono::NaiveDate::parse_from_str(current_string, "%Y-%m-%d")
                .expect("precondition ensures parseable date");
            let new_date = current_date
                .checked_add_signed(chrono::Duration::seconds(delta_seconds))
                .expect("date arithmetic must fit chrono's NaiveDate range");
            let new_value = serde_yaml::Value::String(new_date.format("%Y-%m-%d").to_string());
            new_frontmatter.insert(field.to_owned(), new_value.clone());
            ComputedMutation {
                new_frontmatter,
                previous_value,
                new_value: Some(new_value),
                write_needed: true,
                info_messages: Vec::new(),
            }
        }
        SetOperation::Boolean(BooleanMode::Toggle) => {
            let current = previous_value
                .as_ref()
                .and_then(|value| value.as_bool())
                .expect("precondition ensures existing boolean value");
            let new_value = serde_yaml::Value::Bool(!current);
            new_frontmatter.insert(field.to_owned(), new_value.clone());
            ComputedMutation {
                new_frontmatter,
                previous_value,
                new_value: Some(new_value),
                write_needed: true,
                info_messages: Vec::new(),
            }
        }
    }
}

/// Add a signed delta to a numeric field's current value, preserving
/// the field's int/float typing.
///
/// Float arithmetic kicks in only when either operand is itself a
/// float; pure-integer adds stay as i64 so the on-disk YAML reads as
/// `points: 8` and not `points: 8.0`.
fn apply_numeric_delta(
    current: &serde_yaml::Value,
    delta: &serde_yaml::Number,
) -> serde_yaml::Value {
    let current_number = match current {
        serde_yaml::Value::Number(number) => number,
        _ => unreachable!("preconditions ensure numeric current value"),
    };

    let use_float = delta.is_f64() || current_number.is_f64();
    if use_float {
        let a = current_number
            .as_f64()
            .expect("number coerces to f64 unless infinitely large");
        let b = delta.as_f64().expect("delta coerces to f64");
        serde_yaml::Value::Number(serde_yaml::Number::from(a + b))
    } else {
        let a = current_number.as_i64().expect("integer number stays i64");
        let b = delta.as_i64().expect("integer delta stays i64");
        serde_yaml::Value::Number(serde_yaml::Number::from(a.saturating_add(b)))
    }
}

/// Normalize a possibly-absent, possibly-scalar field value into a
/// `Vec<Value>` ready for collection-mode arithmetic.
///
/// Scalar promotion handles a hand-edited file where a `list`/`links`/
/// `multichoice` field accidentally holds a single scalar — we treat it
/// as a one-element sequence so the operation still produces a clean
/// sequence on disk. The coerce pass on reload will reconcile.
fn current_value_as_sequence(previous_value: Option<&serde_yaml::Value>) -> Vec<serde_yaml::Value> {
    match previous_value {
        Some(serde_yaml::Value::Sequence(sequence)) => sequence.clone(),
        Some(value) => vec![value.clone()],
        None => Vec::new(),
    }
}

/// Append each value to the end of `current`, flagging duplicates via
/// an info message but appending them anyway (decision 4 in
/// cli-set-modes — honors the literal request, lets the user notice).
fn append_to_sequence(
    mut current: Vec<serde_yaml::Value>,
    values_to_append: Vec<serde_yaml::Value>,
    field: &str,
) -> (Vec<serde_yaml::Value>, Vec<String>) {
    let mut info_messages = Vec::new();
    for value in values_to_append {
        if current.contains(&value) {
            info_messages.push(format!(
                "value {} was already present in '{}'",
                format_value_for_info(&value),
                field
            ));
        }
        current.push(value);
    }
    (current, info_messages)
}

/// Remove every occurrence of each value from `current`. Values that
/// weren't there emit an info message (decision 3 in cli-set-modes:
/// "remove all" semantics across `list`/`links`/`multichoice`).
fn remove_from_sequence(
    mut current: Vec<serde_yaml::Value>,
    values_to_remove: Vec<serde_yaml::Value>,
    field: &str,
) -> (Vec<serde_yaml::Value>, Vec<String>) {
    let mut info_messages = Vec::new();
    for value in values_to_remove {
        let before_length = current.len();
        current.retain(|element| element != &value);
        if current.len() == before_length {
            info_messages.push(format!(
                "value {} was not present in '{}'",
                format_value_for_info(&value),
                field
            ));
        }
    }
    (current, info_messages)
}

/// Compact rendering of a value for inclusion in an info message.
/// Strings are quoted; other scalars are stringified plainly; complex
/// shapes (rare in collection elements) fall back to single-line YAML.
fn format_value_for_info(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(string) => format!("'{string}'"),
        serde_yaml::Value::Bool(boolean) => boolean.to_string(),
        serde_yaml::Value::Number(number) => number.to_string(),
        serde_yaml::Value::Null => "(null)".to_owned(),
        _ => serde_yaml::to_string(value)
            .unwrap_or_default()
            .trim()
            .to_owned(),
    }
}

// ── Phase 3: finalize ───────────────────────────────────────────────

fn finalize_mutation(
    context: MutationContext,
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
    let reloaded = crate::store::Store::load(&context.items_path, &context.schema)?;
    let mut post_diagnostics: Vec<Diagnostic> = reloaded.diagnostics().to_vec();
    post_diagnostics.extend(crate::rules::evaluate(&reloaded, &context.schema));

    let mutation_caused_warning =
        post_diagnostics_introduced_by_mutation(&context.pre_diagnostics, &post_diagnostics);

    Ok(SetOutcome {
        path: context.file_path,
        previous_value: computed.previous_value,
        new_value: computed.new_value,
        warnings: post_diagnostics,
        info_messages: computed.info_messages,
        mutation_caused_warning,
    })
}

/// `true` iff any diagnostic exists in `post` that wasn't already in `pre`.
///
/// Identity is by stable JSON serialization — every `Diagnostic` field
/// is `Serialize`, and re-serializing the same data produces the same
/// string. Cheap because `pre` is hashed once.
fn post_diagnostics_introduced_by_mutation(pre: &[Diagnostic], post: &[Diagnostic]) -> bool {
    let pre_keys: HashSet<String> = pre.iter().filter_map(diagnostic_key).collect();
    post.iter().any(|d| {
        diagnostic_key(d)
            .map(|k| !pre_keys.contains(&k))
            .unwrap_or(true)
    })
}

fn diagnostic_key(d: &Diagnostic) -> Option<String> {
    serde_json::to_string(d).ok()
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
  assignees:
    type: links
    required: false
    allow_cycles: false
  labels:
    type: multichoice
    values: [bug, feature, chore]
    required: false
  velocity:
    type: float
    required: false
  estimate:
    type: duration
    required: false
  due_date:
    type: date
    required: false
  archived:
    type: boolean
    required: false
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

    // ── Diff-based mutation_caused_warning (covers a previous gap) ───

    #[test]
    fn set_with_broken_link_flags_mutation_caused_warning() {
        use crate::model::diagnostic::{DiagnosticBody, ItemDiagnosticKind};

        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "parent",
            SetOperation::Replace(serde_yaml::Value::String("does-not-exist".to_owned())),
        )
        .unwrap();

        // Broken link is a *new* diagnostic introduced by this mutation
        // (the parent field passes coerce — the BrokenLink finding is
        // emitted by Store::load on reload). The diff catches it.
        assert!(outcome.mutation_caused_warning);
        let has_broken_link = outcome.warnings.iter().any(|d| match &d.body {
            DiagnosticBody::Item(item) => matches!(
                &item.kind,
                ItemDiagnosticKind::BrokenLink { field, .. } if field == "parent"
            ),
            _ => false,
        });
        assert!(has_broken_link);
    }

    // ── Unset ────────────────────────────────────────────────────────

    #[test]
    fn unset_removes_field_and_writes_file() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npriority: high\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "priority",
            SetOperation::Unset,
        )
        .unwrap();

        assert_eq!(outcome.previous_value.unwrap().as_str().unwrap(), "high");
        assert!(outcome.new_value.is_none());
        assert!(!outcome.mutation_caused_warning);

        let file = read_item(&root, "task-1");
        assert!(!file.contains("priority:"));
        assert!(file.contains("status: open"));
    }

    #[test]
    fn unset_absent_field_is_noop_and_exits_zero() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        let original = "---\ntitle: Task 1\nstatus: open\n---\nbody\n";
        write_item(&root, "task-1", original);

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "priority",
            SetOperation::Unset,
        )
        .unwrap();

        assert!(outcome.previous_value.is_none());
        assert!(outcome.new_value.is_none());
        assert!(!outcome.mutation_caused_warning);

        // File untouched byte-for-byte.
        let file = read_item(&root, "task-1");
        assert_eq!(file, original);
    }

    #[test]
    fn unset_required_field_saves_with_missing_required_warning_and_flags_mutation_caused() {
        use crate::model::diagnostic::{DiagnosticBody, ItemDiagnosticKind};

        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Unset,
        )
        .unwrap();

        assert!(outcome.mutation_caused_warning);

        // File written despite the required violation.
        let file = read_item(&root, "task-1");
        assert!(!file.contains("status:"));

        let has_missing = outcome.warnings.iter().any(|d| match &d.body {
            DiagnosticBody::Item(item) => matches!(
                &item.kind,
                ItemDiagnosticKind::MissingRequired { field } if field == "status"
            ),
            _ => false,
        });
        assert!(has_missing);
    }

    #[test]
    fn unset_id_returns_idnotmutable_with_reworded_message() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "id",
            SetOperation::Unset,
        );

        let error = result.unwrap_err();
        assert!(matches!(error, SetError::IdNotMutable));
        let message = error.to_string();
        assert!(message.contains("modify"));
        assert!(message.contains("workdown rename"));
    }

    #[test]
    fn unset_unknown_field_errors_without_writing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        let original = "---\ntitle: Task 1\nstatus: open\n---\nbody\n";
        write_item(&root, "task-1", original);

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "nonexistent",
            SetOperation::Unset,
        );

        assert!(matches!(result, Err(SetError::UnknownField { .. })));
        let file = read_item(&root, "task-1");
        assert_eq!(file, original);
    }

    #[test]
    fn unset_unknown_item_errors_without_writing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("does-not-exist".to_owned()),
            "priority",
            SetOperation::Unset,
        );

        assert!(matches!(result, Err(SetError::UnknownItem { .. })));
    }

    #[test]
    fn unset_preserves_body_byte_for_byte() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        let body = "Line one of the body.\n\n## Heading\n\nMore body.\n";
        write_item(
            &root,
            "task-1",
            &format!("---\ntitle: Task 1\nstatus: open\npriority: high\n---\n{body}"),
        );

        run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "priority",
            SetOperation::Unset,
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
    fn unset_explicit_id_in_frontmatter_is_preserved() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "filename-slug",
            "---\nid: custom-id\ntitle: Task\nstatus: open\npriority: high\n---\n",
        );

        run_set(
            &config,
            &root,
            &WorkItemId::from("custom-id".to_owned()),
            "priority",
            SetOperation::Unset,
        )
        .unwrap();

        let file = read_item(&root, "filename-slug");
        assert!(file.contains("id: custom-id"));
        assert!(!file.contains("priority:"));
    }

    #[test]
    fn unset_does_not_flag_mutation_caused_warning_for_unrelated_existing_warnings() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);

        // Pre-existing item with an UnknownField warning — should be
        // visible in the post-write output but must not flip
        // mutation_caused_warning on an unrelated unset.
        write_item(
            &root,
            "noisy",
            "---\ntitle: Noisy\nstatus: open\nextra_unknown: foo\n---\n",
        );
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npriority: high\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "priority",
            SetOperation::Unset,
        )
        .unwrap();

        assert!(!outcome.mutation_caused_warning);
        // Pre-existing warning still surfaces (milestone "always show all").
        assert!(!outcome.warnings.is_empty());
    }

    // ── Aggregate field interaction ──────────────────────────────────

    const AGGREGATE_SCHEMA: &str = "\
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
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
  effort:
    type: integer
    required: false
    aggregate:
      function: sum
      error_on_missing: true
";

    fn setup_aggregate_project() -> (TempDir, PathBuf) {
        let directory = TempDir::new().unwrap();
        let root = directory.path().to_path_buf();
        fs::create_dir_all(root.join(".workdown/templates")).unwrap();
        fs::create_dir_all(root.join("workdown-items")).unwrap();
        fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
        fs::write(root.join(".workdown/schema.yaml"), AGGREGATE_SCHEMA).unwrap();
        (directory, root)
    }

    // ── Collection modes: append ─────────────────────────────────────

    #[test]
    fn append_to_list_appends_in_order() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth]\n---\n",
        );

        let appended = vec![serde_yaml::Value::String("backend".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let new_sequence = outcome.new_value.unwrap();
        let elements = new_sequence.as_sequence().unwrap();
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].as_str().unwrap(), "auth");
        assert_eq!(elements[1].as_str().unwrap(), "backend");
        assert!(outcome.info_messages.is_empty());
        assert!(!outcome.mutation_caused_warning);
    }

    #[test]
    fn append_to_absent_field_creates_sequence() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("qa".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        assert!(outcome.previous_value.is_none());
        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "qa");
    }

    #[test]
    fn append_duplicate_writes_and_emits_info() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth, qa]\n---\n",
        );

        let appended = vec![serde_yaml::Value::String("qa".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 3);
        assert_eq!(sequence[2].as_str().unwrap(), "qa");
        assert_eq!(outcome.info_messages.len(), 1);
        assert!(outcome.info_messages[0].contains("'qa'"));
        assert!(outcome.info_messages[0].contains("already present"));
        // Duplicate append is intentional and never flips exit code.
        assert!(!outcome.mutation_caused_warning);
    }

    #[test]
    fn append_multi_value_in_order() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth]\n---\n",
        );

        let appended = vec![
            serde_yaml::Value::String("backend".to_owned()),
            serde_yaml::Value::String("qa".to_owned()),
        ];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        let names: Vec<&str> = sequence.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, vec!["auth", "backend", "qa"]);
    }

    #[test]
    fn append_on_links_field_works() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");
        write_item(&root, "alice", "---\ntitle: Alice\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("alice".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "assignees",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "alice");
    }

    #[test]
    fn append_on_multichoice_field_works() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("bug".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "labels",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "bug");
        assert!(!outcome.mutation_caused_warning);
    }

    // ── Collection modes: remove ─────────────────────────────────────

    #[test]
    fn remove_value_removes_all_occurrences() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth, backend, auth]\n---\n",
        );

        let to_remove = vec![serde_yaml::Value::String("auth".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "backend");
        assert!(outcome.info_messages.is_empty());
    }

    #[test]
    fn remove_absent_value_emits_info() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth]\n---\n",
        );

        let to_remove = vec![serde_yaml::Value::String("qa".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        )
        .unwrap();

        // Sequence unchanged → write skipped, file byte-identical.
        let file_after = read_item(&root, "task-1");
        assert!(file_after.contains("tags:"));
        assert!(file_after.contains("auth"));
        assert!(!file_after.contains("qa"));

        assert_eq!(outcome.info_messages.len(), 1);
        assert!(outcome.info_messages[0].contains("'qa'"));
        assert!(outcome.info_messages[0].contains("not present"));
        assert!(!outcome.mutation_caused_warning);
    }

    #[test]
    fn remove_from_absent_field_is_noop_with_info() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        let original = "---\ntitle: Task 1\nstatus: open\n---\nbody\n";
        write_item(&root, "task-1", original);

        let to_remove = vec![serde_yaml::Value::String("qa".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        )
        .unwrap();

        // File untouched byte-for-byte; field stays absent.
        assert_eq!(read_item(&root, "task-1"), original);
        assert!(outcome.previous_value.is_none());
        assert_eq!(outcome.info_messages.len(), 1);
    }

    #[test]
    fn remove_multi_value_with_some_absent() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth, backend]\n---\n",
        );

        let to_remove = vec![
            serde_yaml::Value::String("auth".to_owned()),
            serde_yaml::Value::String("qa".to_owned()),
        ];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "backend");
        assert_eq!(outcome.info_messages.len(), 1);
        assert!(outcome.info_messages[0].contains("'qa'"));
    }

    // ── Collection modes: mode-type validity ─────────────────────────

    #[test]
    fn append_on_choice_field_returns_mode_not_valid_error() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("done".to_owned())];
        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Collection(CollectionMode::Append(appended)),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::ModeNotValidForFieldType { ref mode, ref field, .. }
                if *mode == "append" && field == "status"
        ));
        let message = error.to_string();
        assert!(message.contains("--append"));
        assert!(message.contains("'status'"));
        assert!(message.contains("choice"));
    }

    #[test]
    fn remove_on_integer_field_returns_mode_not_valid_error() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: 3\n---\n",
        );

        let to_remove = vec![serde_yaml::Value::String("3".to_owned())];
        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::ModeNotValidForFieldType { ref mode, ref field, .. }
                if *mode == "remove" && field == "points"
        ));
    }

    #[test]
    fn append_on_link_singular_field_returns_mode_not_valid_error() {
        // `parent: link` is single-valued — collection modes must reject.
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("other-task".to_owned())];
        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "parent",
            SetOperation::Collection(CollectionMode::Append(appended)),
        );

        assert!(matches!(
            result,
            Err(SetError::ModeNotValidForFieldType { .. })
        ));
    }

    // ── Delta: numeric ───────────────────────────────────────────────

    #[test]
    fn delta_on_integer_adds_value() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: 5\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
        )
        .unwrap();

        assert_eq!(outcome.previous_value.unwrap().as_i64().unwrap(), 5);
        assert_eq!(outcome.new_value.unwrap().as_i64().unwrap(), 8);
        let file = read_item(&root, "task-1");
        assert!(file.contains("points: 8"));
    }

    #[test]
    fn delta_on_integer_with_negative_subtracts() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: 5\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(-3_i64))),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_i64().unwrap(), 2);
    }

    #[test]
    fn delta_on_float_adds_value() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nvelocity: 2.5\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "velocity",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(1.5_f64))),
        )
        .unwrap();

        assert!((outcome.new_value.unwrap().as_f64().unwrap() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn delta_on_absent_numeric_field_returns_requires_existing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::MutationRequiresExistingValue { ref mode, ref field }
                if *mode == "delta" && field == "points"
        ));
    }

    #[test]
    fn delta_on_malformed_numeric_returns_malformed_error() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: high\n---\n",
        );

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::MutationCurrentValueMalformed { ref mode, ref field, .. }
                if *mode == "delta" && field == "points"
        ));
    }

    // ── Delta: duration ──────────────────────────────────────────────

    #[test]
    fn delta_on_duration_adds_seconds() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nestimate: 2d\n---\n",
        );

        // +1d = 86_400 seconds
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(86_400)),
        )
        .unwrap();

        let new_string = outcome.new_value.unwrap();
        assert_eq!(new_string.as_str().unwrap(), "3d");
    }

    #[test]
    fn delta_on_duration_with_negative_subtracts() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nestimate: 1w\n---\n",
        );

        // -3d = -259_200 seconds. 1w - 3d = 4d.
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(-259_200)),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "4d");
    }

    #[test]
    fn delta_on_absent_duration_field_returns_requires_existing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(3600)),
        );

        assert!(matches!(
            result,
            Err(SetError::MutationRequiresExistingValue { .. })
        ));
    }

    // ── Delta: date ──────────────────────────────────────────────────

    #[test]
    fn delta_on_date_adds_duration() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ndue_date: '2026-05-14'\n---\n",
        );

        // +1w
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(604_800)),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "2026-05-21");
    }

    #[test]
    fn delta_on_date_with_negative_subtracts_duration() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ndue_date: '2026-05-14'\n---\n",
        );

        // -3d
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(-259_200)),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "2026-05-11");
    }

    #[test]
    fn delta_on_absent_date_field_returns_requires_existing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(86_400)),
        );

        assert!(matches!(
            result,
            Err(SetError::MutationRequiresExistingValue { .. })
        ));
    }

    // ── Delta: mode-type validity ────────────────────────────────────

    #[test]
    fn numeric_delta_on_choice_field_returns_mode_not_valid() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(1))),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::ModeNotValidForFieldType { ref mode, ref field, .. }
                if *mode == "delta" && field == "status"
        ));
    }

    #[test]
    fn date_delta_on_integer_field_returns_mode_not_valid() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: 3\n---\n",
        );

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Date(DateMode::Delta(86_400)),
        );

        assert!(matches!(
            result,
            Err(SetError::ModeNotValidForFieldType { .. })
        ));
    }

    // ── Toggle: boolean ──────────────────────────────────────────────

    #[test]
    fn toggle_flips_boolean_from_false_to_true() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\narchived: false\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "archived",
            SetOperation::Boolean(BooleanMode::Toggle),
        )
        .unwrap();

        assert_eq!(outcome.previous_value.unwrap().as_bool().unwrap(), false);
        assert_eq!(outcome.new_value.unwrap().as_bool().unwrap(), true);
        let file = read_item(&root, "task-1");
        assert!(file.contains("archived: true"));
    }

    #[test]
    fn toggle_flips_boolean_from_true_to_false() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\narchived: true\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "archived",
            SetOperation::Boolean(BooleanMode::Toggle),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_bool().unwrap(), false);
    }

    #[test]
    fn toggle_on_absent_field_returns_requires_existing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "archived",
            SetOperation::Boolean(BooleanMode::Toggle),
        );

        assert!(matches!(
            result,
            Err(SetError::MutationRequiresExistingValue { ref mode, ref field })
                if *mode == "toggle" && field == "archived"
        ));
    }

    #[test]
    fn toggle_on_non_boolean_field_returns_mode_not_valid() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Boolean(BooleanMode::Toggle),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::ModeNotValidForFieldType { ref mode, ref field, .. }
                if *mode == "toggle" && field == "status"
        ));
    }

    #[test]
    fn toggle_on_malformed_boolean_returns_malformed_error() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        // `archived: yes` — YAML parses this as a string, not a bool.
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\narchived: yes\n---\n",
        );

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "archived",
            SetOperation::Boolean(BooleanMode::Toggle),
        );

        assert!(matches!(
            result,
            Err(SetError::MutationCurrentValueMalformed { ref mode, ref field, .. })
                if *mode == "toggle" && field == "archived"
        ));
    }

    // ── Aggregate field interaction ──────────────────────────────────

    #[test]
    fn unset_aggregate_field_with_error_on_missing_surfaces_warning() {
        use crate::model::diagnostic::{DiagnosticBody, ItemDiagnosticKind};

        let (_directory, root) = setup_aggregate_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\neffort: 5\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "effort",
            SetOperation::Unset,
        )
        .unwrap();

        assert!(outcome.mutation_caused_warning);

        let file = read_item(&root, "task-1");
        assert!(!file.contains("effort:"));

        // The rollup pass on reload surfaces AggregateMissingValue for
        // the now-empty aggregate field with error_on_missing.
        let has_missing = outcome.warnings.iter().any(|d| match &d.body {
            DiagnosticBody::Item(item) => matches!(
                &item.kind,
                ItemDiagnosticKind::AggregateMissingValue { field } if field == "effort"
            ),
            _ => false,
        });
        assert!(has_missing);
    }
}
