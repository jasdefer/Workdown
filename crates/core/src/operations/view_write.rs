//! Persist view definitions to `views.yaml`.
//!
//! The read side of views is handled by [`crate::parser::views`] and
//! [`crate::views_check`]; this module is the write side. It supports the
//! mutations the view-authoring UI needs: adding a new view, replacing an
//! existing view's `where:` filter, replacing a view's whole definition
//! (optionally under a new id), and deleting a view. Reordering views
//! stays a text-editor job.
//!
//! Like every other mutation in the tool, the repo stays the source of
//! truth: writes update the working tree only, nothing is staged or
//! committed. The whole file is re-serialized from the model on each
//! write — see the `view-write-backend` design notes for why, and what
//! that costs (a user's comments and key ordering are not preserved).
//!
//! ## What blocks a write vs. what only warns
//!
//! A write is rejected, leaving `views.yaml` untouched, only when it would
//! make the file fail to *load* — an unparseable existing file, a view
//! definition missing a required slot or naming an unknown slot, or a
//! duplicate id. Problems that still load but fail cross-file validation
//! (a `where:` referencing an unknown field, a slot whose field is the
//! wrong type) are written and surfaced through `warnings`, exactly as a
//! hand-edited file would surface them — the save-with-warning convention
//! from ADR-001.

use std::path::{Path, PathBuf};

use crate::model::config::Config;
use crate::model::diagnostic::Diagnostic;
use crate::model::resources::Resources;
use crate::model::schema::Schema;
use crate::model::views::{rendered_view_path, View, Views};
use crate::operations::frontmatter_io::write_file_atomically;
use crate::parser;
use crate::parser::schema::SchemaLoadError;
use crate::parser::views::{serialize_views, view_from_value};
use crate::query::clause::{clauses_to_strings, decompose_clauses, Clause};
use crate::store::Store;

// ── Public types ─────────────────────────────────────────────────────

/// The outcome of a successful view write.
#[derive(Debug)]
pub struct ViewWriteOutcome {
    /// Path to the `views.yaml` that was written.
    pub path: PathBuf,
    /// Id of the view that was created or changed.
    pub view_id: String,
    /// Every cross-file diagnostic from re-checking the written file.
    /// Includes any problem this write introduced as well as pre-existing
    /// ones on other views (surfaced, per the "always show all"
    /// convention, but not blocking).
    pub warnings: Vec<Diagnostic>,
    /// `true` if this write introduced a cross-file diagnostic that wasn't
    /// present before. Drives the caller's exit code / response, distinct
    /// from pre-existing problems elsewhere in the file.
    pub mutation_caused_warning: bool,
    /// Notes about housekeeping that isn't a problem — currently the fate
    /// of a stale rendered output file after a delete or rename. Mirrors
    /// the item mutations' `info_messages` convention.
    pub info_messages: Vec<String>,
}

/// Errors returned by the view-write operations.
///
/// Every variant here is a hard fail: `views.yaml` is left untouched.
/// Soft problems (bad field references, type mismatches) ride through
/// [`ViewWriteOutcome::warnings`] instead — the file still gets written.
#[derive(Debug, thiserror::Error)]
pub enum ViewWriteError {
    #[error("failed to load schema: {0}")]
    SchemaLoad(#[from] SchemaLoadError),

    /// The work items could not be read, so a filter's operands cannot be
    /// checked against the ids they may name. A hard fail for the same
    /// reason `add` and `set` treat it as one: a mutation decided against
    /// an unknown project state is worse than a mutation refused.
    #[error("failed to load work items from '{path}': {source}")]
    ItemsLoad {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("existing views file at '{path}' is invalid; fix it in a text editor before writing from the UI: {detail}")]
    ExistingInvalid { path: PathBuf, detail: String },

    #[error("invalid view definition: {detail}")]
    InvalidDefinition { detail: String },

    /// A structured clause whose operand does not match its operator's arity.
    /// The guided builder cannot produce one — it picks the operand widget from
    /// the operator — so this is a malformed request rather than a
    /// user-authored file problem, and it fails the write instead of riding
    /// through as a warning.
    #[error("invalid filter condition: {0}")]
    InvalidCondition(#[from] crate::query::clause::ConditionError),

    #[error("invalid view name '{name}': {reason}")]
    InvalidName { name: String, reason: String },

    #[error("a view with id '{id}' already exists")]
    DuplicateId { id: String },

    #[error("no view with id '{id}'")]
    ViewNotFound { id: String },

    #[error("failed to serialize views: {0}")]
    Serialize(serde_yaml::Error),

    /// Internal invariant violation: the model we serialized did not
    /// re-parse. Indicates a serializer bug, not bad caller input. Guarded
    /// by the parser's round-trip test; never written to disk.
    #[error("internal error: produced an invalid views file ({detail}); no changes were written")]
    ProducedInvalid { detail: String },

    #[error("failed to write '{path}': {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
}

// ── Public API ───────────────────────────────────────────────────────

/// Add a new view to `views.yaml`.
///
/// `definition` is the flat view shape — `id`, `type`, optional `where`,
/// and the type-specific slots — the same layout as one entry in the
/// `views:` list. It is validated exactly as a hand-edited file would be.
/// Creates `views.yaml` if it does not exist yet.
pub fn add_view(
    config: &Config,
    project_root: &Path,
    definition: serde_yaml::Value,
) -> Result<ViewWriteOutcome, ViewWriteError> {
    let mut context = load_for_write(config, project_root)?;
    let pre_diagnostics = context.pre_check();

    let new_view = build_view(definition)?;
    if context
        .views
        .views
        .iter()
        .any(|view| view.id == new_view.id)
    {
        return Err(ViewWriteError::DuplicateId { id: new_view.id });
    }

    let view_id = new_view.id.clone();
    context.views.views.push(new_view);

    finalize(context, WarningCause::DiffAgainst(pre_diagnostics), view_id)
}

/// Create a view from a human *name* plus a flat definition (kind + slots +
/// optional `where`, with **no** `id`). The name is slugged to the view's
/// id using the shared [`crate::slug`] rule — the same one work-item ids
/// use — then persisted through [`add_view`]. Any `id` in the definition is
/// overwritten by the slug (the name is authoritative). A name with no
/// alphanumeric characters is rejected. A metric view's rows may carry
/// their per-row filter as structured `filter` clauses — serialized into
/// each row's `where:` exactly like the view-level filter.
pub fn create_view(
    config: &Config,
    project_root: &Path,
    name: &str,
    definition: serde_yaml::Value,
    filter: &[Clause],
) -> Result<ViewWriteOutcome, ViewWriteError> {
    let id = slug_view_id(name)?;
    let definition = prepare_definition(definition, &id, filter)?;
    add_view(config, project_root, definition)
}

/// Inject the slugged `id` and, when non-empty, the serialized `where`
/// clauses into a definition mapping, and fold each metric row's structured
/// `filter` back into its `where:` strings. Filters arrive structured and
/// are serialized here (via [`clauses_to_strings`]) so the clause grammar
/// stays in `core`, not the UI.
fn prepare_definition(
    definition: serde_yaml::Value,
    id: &str,
    filter: &[Clause],
) -> Result<serde_yaml::Value, ViewWriteError> {
    let serde_yaml::Value::Mapping(mut mapping) = definition else {
        return Err(ViewWriteError::InvalidDefinition {
            detail: "view definition must be a mapping".to_owned(),
        });
    };
    mapping.insert(
        serde_yaml::Value::String("id".to_owned()),
        serde_yaml::Value::String(id.to_owned()),
    );
    let where_clauses = clauses_to_strings(filter)?;
    if !where_clauses.is_empty() {
        mapping.insert(
            serde_yaml::Value::String("where".to_owned()),
            serde_yaml::Value::Sequence(
                where_clauses
                    .into_iter()
                    .map(serde_yaml::Value::String)
                    .collect(),
            ),
        );
    }
    metric_filters_to_where(&mut mapping)?;
    Ok(serde_yaml::Value::Mapping(mapping))
}

/// Fold each metric row's structured `filter` into its persisted `where:`
/// strings — the write half of the per-row treatment; the read half is
/// [`metric_where_to_filters`]. A `filter` key replaces the row's `where:`
/// entirely (an empty list clears it); a row without one passes through
/// untouched, so a raw API caller may still embed `where` strings directly,
/// mirroring the view level.
fn metric_filters_to_where(mapping: &mut serde_yaml::Mapping) -> Result<(), ViewWriteError> {
    let Some(serde_yaml::Value::Sequence(rows)) = mapping.get_mut("metrics") else {
        return Ok(());
    };
    for row in rows {
        // A non-mapping row is left for view construction to reject with
        // its own shape error.
        let serde_yaml::Value::Mapping(row) = row else {
            continue;
        };
        let Some(filter_value) = row.remove("filter") else {
            continue;
        };
        let clauses: Vec<Clause> = serde_yaml::from_value(filter_value).map_err(|error| {
            ViewWriteError::InvalidDefinition {
                detail: format!("invalid metric row filter: {error}"),
            }
        })?;
        let where_clauses = clauses_to_strings(&clauses)?;
        row.remove("where");
        if !where_clauses.is_empty() {
            row.insert(
                serde_yaml::Value::String("where".to_owned()),
                serde_yaml::Value::Sequence(
                    where_clauses
                        .into_iter()
                        .map(serde_yaml::Value::String)
                        .collect(),
                ),
            );
        }
    }
    Ok(())
}

/// Pop each metric row's persisted `where:` strings and attach them as
/// structured `filter` clauses — the read half of the per-row treatment,
/// used by the edit-form seed (`ViewDefinition`); the write half is
/// [`metric_filters_to_where`]. The `filter` key is set on every row, even
/// when empty, so the rows the form receives all have a uniform shape.
pub(crate) fn metric_where_to_filters(
    mapping: &mut serde_yaml::Mapping,
) -> Result<(), serde_yaml::Error> {
    let Some(serde_yaml::Value::Sequence(rows)) = mapping.get_mut("metrics") else {
        return Ok(());
    };
    for row in rows {
        let serde_yaml::Value::Mapping(row) = row else {
            continue;
        };
        let where_clauses: Vec<String> = match row.shift_remove("where") {
            None => Vec::new(),
            Some(value) => serde_yaml::from_value(value)?,
        };
        row.insert(
            serde_yaml::Value::String("filter".to_owned()),
            serde_yaml::to_value(decompose_clauses(&where_clauses))?,
        );
    }
    Ok(())
}

/// Replace the `where:` filter of an existing view and persist it.
///
/// `core` serializes the structured [`Clause`]s to clause strings (raw
/// clauses pass through), so the filter grammar stays owned here, not in
/// the UI. The result is stored verbatim; its meaning is whatever
/// [`crate::query::parse::parse_where`] makes of it — the same grammar the
/// rest of the tool uses. A clause that fails to parse or references an
/// unknown field is written and reported as a warning, not rejected.
pub fn set_view_filter(
    config: &Config,
    project_root: &Path,
    view_id: &str,
    clauses: &[Clause],
) -> Result<ViewWriteOutcome, ViewWriteError> {
    let mut context = load_for_write(config, project_root)?;
    let pre_diagnostics = context.pre_check();

    let position = find_view_position(&context.views, view_id)?;
    context.views.views[position].where_clauses = clauses_to_strings(clauses)?;

    finalize(
        context,
        WarningCause::DiffAgainst(pre_diagnostics),
        view_id.to_owned(),
    )
}

/// Replace an existing view's whole definition — and, when `new_name` is
/// given, its id — keeping its position in the `views:` list.
///
/// `definition` is the same flat shape [`create_view`] takes (kind + slots,
/// no `id`); the filter arrives structured and replaces the view's `where:`.
/// `new_name` is slugged with the shared rule; `None` keeps the current id,
/// so callers that let the user edit a *name* (id is lossy in that
/// direction) can make "left untouched" mean "no rename". A rename removes
/// the old id's stale rendered output file, exactly as [`delete_view`]
/// does for the whole view.
pub fn update_view(
    config: &Config,
    project_root: &Path,
    view_id: &str,
    new_name: Option<&str>,
    definition: serde_yaml::Value,
    filter: &[Clause],
) -> Result<ViewWriteOutcome, ViewWriteError> {
    let mut context = load_for_write(config, project_root)?;
    let pre_diagnostics = context.pre_check();

    let position = find_view_position(&context.views, view_id)?;

    let target_id = match new_name {
        None => view_id.to_owned(),
        Some(name) => slug_view_id(name)?,
    };
    if target_id != view_id && context.views.views.iter().any(|view| view.id == target_id) {
        return Err(ViewWriteError::DuplicateId { id: target_id });
    }

    let definition = prepare_definition(definition, &target_id, filter)?;
    let new_view = build_view(definition)?;

    // A diagnostic's identity includes the view id, so diffing across a
    // rename would count every pre-existing warning on this view as newly
    // introduced. Measure causation under the stable old id instead: check
    // a probe where the new definition sits at the same position under the
    // old id, and diff that against the pre-write diagnostics.
    let renamed = target_id != view_id;
    let warning_cause = if renamed {
        let mut probe_views = context.views.clone();
        probe_views.views[position] = View {
            id: view_id.to_owned(),
            ..new_view.clone()
        };
        let probe_diagnostics = check(&probe_views, &context.inputs, &context.path);
        WarningCause::Known(crate::operations::diagnostics::introduced_by_mutation(
            &pre_diagnostics,
            &probe_diagnostics,
        ))
    } else {
        WarningCause::DiffAgainst(pre_diagnostics)
    };

    context.views.views[position] = new_view;

    let output_dir = context.views.output_dir.clone();
    let mut outcome = finalize(context, warning_cause, target_id)?;
    if renamed {
        remove_rendered_file(
            project_root,
            &output_dir,
            view_id,
            &mut outcome.info_messages,
        );
    }
    Ok(outcome)
}

/// Remove a view from `views.yaml`, plus its stale rendered output file
/// (`<output_dir>/<id>.md`) when one exists — `workdown render` never
/// cleans up on its own, so without this the file would linger until the
/// user spots it in `git status`.
pub fn delete_view(
    config: &Config,
    project_root: &Path,
    view_id: &str,
) -> Result<ViewWriteOutcome, ViewWriteError> {
    let mut context = load_for_write(config, project_root)?;

    let position = find_view_position(&context.views, view_id)?;
    context.views.views.remove(position);

    let output_dir = context.views.output_dir.clone();
    // No pre-write check: `views_check` is a per-view pass with no
    // cross-view findings, so removing a view can only remove diagnostics —
    // a delete can never introduce one.
    let mut outcome = finalize(context, WarningCause::Known(false), view_id.to_owned())?;
    remove_rendered_file(
        project_root,
        &output_dir,
        view_id,
        &mut outcome.info_messages,
    );
    Ok(outcome)
}

// ── Internals ────────────────────────────────────────────────────────

/// Best-effort removal of a view's rendered output file after a delete or
/// rename made it stale. A missing file is silence — nothing was rendered.
/// Any other failure becomes an info message rather than an error: the
/// `views.yaml` write, the actual mutation, has already succeeded, and the
/// leftover file is visible in `git status` either way.
fn remove_rendered_file(
    project_root: &Path,
    output_dir: &Path,
    view_id: &str,
    info_messages: &mut Vec<String>,
) {
    let rendered_path = rendered_view_path(&project_root.join(output_dir), view_id);
    match std::fs::remove_file(&rendered_path) {
        Ok(()) => info_messages.push(format!(
            "removed stale rendered file '{}'",
            rendered_path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => info_messages.push(format!(
            "could not remove stale rendered file '{}': {error}",
            rendered_path.display()
        )),
    }
}

fn views_path(config: &Config, project_root: &Path) -> PathBuf {
    project_root.join(&config.paths.views)
}

/// Everything a write operation starts from, loaded once: the cross-file
/// check inputs, the `views.yaml` path, and its current content. Owns the
/// shared preamble so the operations differ only in their actual mutation.
struct WriteContext {
    inputs: CheckInputs,
    path: PathBuf,
    views: Views,
}

fn load_for_write(config: &Config, project_root: &Path) -> Result<WriteContext, ViewWriteError> {
    let inputs = load_check_inputs(config, project_root)?;
    let path = views_path(config, project_root);
    let views = load_current_views(&path)?;
    Ok(WriteContext {
        inputs,
        path,
        views,
    })
}

impl WriteContext {
    /// The diagnostics as they stand before the mutation — the baseline
    /// [`WarningCause::DiffAgainst`] compares the post-write check to.
    fn pre_check(&self) -> Vec<Diagnostic> {
        check(&self.views, &self.inputs, &self.path)
    }
}

/// How [`finalize`] decides `mutation_caused_warning`.
enum WarningCause {
    /// Diff the post-write check against these pre-write diagnostics.
    DiffAgainst(Vec<Diagnostic>),
    /// The caller already knows: a delete can only remove diagnostics, and
    /// a rename measures causation under a stable id (see [`update_view`]).
    Known(bool),
}

/// Slug a human view name into an id with the shared [`crate::slug`] rule —
/// the same one work-item ids use. Create and rename derive ids through
/// this one path so they can never slug the same name differently.
fn slug_view_id(name: &str) -> Result<String, ViewWriteError> {
    crate::slug::slugify(name).map_err(|error| ViewWriteError::InvalidName {
        name: error.input,
        reason: error.reason,
    })
}

fn find_view_position(views: &Views, view_id: &str) -> Result<usize, ViewWriteError> {
    views
        .views
        .iter()
        .position(|view| view.id == view_id)
        .ok_or_else(|| ViewWriteError::ViewNotFound {
            id: view_id.to_owned(),
        })
}

/// Construct a validated [`View`] from a prepared definition value.
fn build_view(definition: serde_yaml::Value) -> Result<View, ViewWriteError> {
    view_from_value(definition).map_err(|error| ViewWriteError::InvalidDefinition {
        detail: error.to_string(),
    })
}

/// Everything the cross-file checks need, loaded once per write.
///
/// A view write used to read only `schema.yaml`, which was enough while
/// `views_check` looked at field *names*. Checking a filter's operands
/// needs the two option sets that live outside the schema — a
/// `resource:`-backed field's entries and the work item ids — so this
/// path now loads what the read paths already load. Mirrors `add`/`set`
/// rather than calling `load_project`: rule evaluation and the derive
/// passes have no bearing on whether a `where:` clause is sound.
struct CheckInputs {
    schema: Schema,
    resources: Resources,
    store: Store,
}

fn load_check_inputs(config: &Config, project_root: &Path) -> Result<CheckInputs, ViewWriteError> {
    let schema_path = project_root.join(&config.schema);
    let schema = parser::schema::load_schema(&schema_path)?;

    // A missing or malformed resources.yaml degrades to empty resources;
    // `workdown validate` owns reporting it, as in `add`.
    let (resources, _) =
        crate::resources_check::load_and_check(&project_root.join(&config.paths.resources));

    let items_path = project_root.join(&config.paths.work_items);
    let store = Store::load_with_resources(&items_path, &schema, &resources).map_err(|source| {
        ViewWriteError::ItemsLoad {
            path: items_path,
            source,
        }
    })?;

    Ok(CheckInputs {
        schema,
        resources,
        store,
    })
}

/// Load the current views, or an empty set when the file does not exist
/// yet. An existing file that won't parse is a hard error: we re-serialize
/// the whole file from the model, so we can't safely preserve views we
/// can't read.
fn load_current_views(path: &Path) -> Result<Views, ViewWriteError> {
    if !path.exists() {
        // Parsing an empty list yields the default `output_dir`, so the
        // created file matches a hand-authored one with no `directory:`.
        return Ok(parser::views::parse_views("views: []\n").expect("empty views list parses"));
    }
    parser::views::load_views(path).map_err(|error| ViewWriteError::ExistingInvalid {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })
}

/// Serialize the mutated model, validate the candidate before touching
/// disk, write atomically, and diff diagnostics to flag whether this write
/// introduced a new problem.
/// Run the cross-file checks over a set of views with this write's inputs.
fn check(views: &Views, inputs: &CheckInputs, path: &Path) -> Vec<Diagnostic> {
    crate::views_check::evaluate(
        views,
        &inputs.schema,
        &inputs.resources,
        &inputs.store,
        path,
    )
}

fn finalize(
    context: WriteContext,
    warning_cause: WarningCause,
    view_id: String,
) -> Result<ViewWriteOutcome, ViewWriteError> {
    let WriteContext {
        inputs,
        path,
        views,
    } = context;
    let candidate = serialize_views(&views).map_err(ViewWriteError::Serialize)?;

    // Validate the candidate in memory first: a candidate that won't parse
    // would break *every* view in the file, so it must never reach disk.
    // In practice inputs were already validated, so this only fires on a
    // serializer bug — the parser's round-trip test is the real guard.
    let reparsed = parser::views::parse_views(&candidate).map_err(|error| {
        ViewWriteError::ProducedInvalid {
            detail: error.to_string(),
        }
    })?;
    let warnings = check(&reparsed, &inputs, &path);

    write_file_atomically(&path, &candidate).map_err(|source| ViewWriteError::WriteFile {
        path: path.clone(),
        source,
    })?;

    let mutation_caused_warning = match warning_cause {
        WarningCause::DiffAgainst(pre_diagnostics) => {
            crate::operations::diagnostics::introduced_by_mutation(&pre_diagnostics, &warnings)
        }
        WarningCause::Known(flag) => flag,
    };

    Ok(ViewWriteOutcome {
        path,
        view_id,
        warnings,
        mutation_caused_warning,
        info_messages: Vec::new(),
    })
}
