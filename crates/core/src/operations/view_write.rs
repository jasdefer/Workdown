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
use crate::model::views::Views;
use crate::operations::frontmatter_io::write_file_atomically;
use crate::parser;
use crate::parser::schema::SchemaLoadError;
use crate::parser::views::{serialize_views, view_from_value};
use crate::query::clause::{clauses_to_strings, Clause};
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
    let inputs = load_check_inputs(config, project_root)?;
    let path = views_path(config, project_root);
    let mut views = load_current_views(&path)?;

    let pre_diagnostics = check(&views, &inputs, &path);

    let new_view =
        view_from_value(definition).map_err(|error| ViewWriteError::InvalidDefinition {
            detail: error.to_string(),
        })?;

    if views.views.iter().any(|view| view.id == new_view.id) {
        return Err(ViewWriteError::DuplicateId { id: new_view.id });
    }

    let view_id = new_view.id.clone();
    views.views.push(new_view);

    finalize(views, &path, &inputs, pre_diagnostics, view_id)
}

/// Create a view from a human *name* plus a flat definition (kind + slots +
/// optional `where`, with **no** `id`). The name is slugged to the view's
/// id using the shared [`crate::slug`] rule — the same one work-item ids
/// use — then persisted through [`add_view`]. Any `id` in the definition is
/// overwritten by the slug (the name is authoritative). A name with no
/// alphanumeric characters is rejected.
pub fn create_view(
    config: &Config,
    project_root: &Path,
    name: &str,
    definition: serde_yaml::Value,
    filter: &[Clause],
) -> Result<ViewWriteOutcome, ViewWriteError> {
    let id = crate::slug::slugify(name).map_err(|error| ViewWriteError::InvalidName {
        name: error.input,
        reason: error.reason,
    })?;
    let definition = prepare_definition(definition, &id, filter)?;
    add_view(config, project_root, definition)
}

/// Inject the slugged `id` and, when non-empty, the serialized `where`
/// clauses into a definition mapping. The filter arrives structured and is
/// serialized here (via [`clauses_to_strings`]) so the clause grammar stays
/// in `core`, not the UI.
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
    Ok(serde_yaml::Value::Mapping(mapping))
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
    let inputs = load_check_inputs(config, project_root)?;
    let path = views_path(config, project_root);
    let mut views = load_current_views(&path)?;

    let pre_diagnostics = check(&views, &inputs, &path);

    let view = views
        .views
        .iter_mut()
        .find(|view| view.id == view_id)
        .ok_or_else(|| ViewWriteError::ViewNotFound {
            id: view_id.to_owned(),
        })?;
    view.where_clauses = clauses_to_strings(clauses)?;

    finalize(views, &path, &inputs, pre_diagnostics, view_id.to_owned())
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
    let inputs = load_check_inputs(config, project_root)?;
    let path = views_path(config, project_root);
    let mut views = load_current_views(&path)?;

    let pre_diagnostics = check(&views, &inputs, &path);

    let position = views
        .views
        .iter()
        .position(|view| view.id == view_id)
        .ok_or_else(|| ViewWriteError::ViewNotFound {
            id: view_id.to_owned(),
        })?;

    let target_id = match new_name {
        None => view_id.to_owned(),
        Some(name) => crate::slug::slugify(name).map_err(|error| ViewWriteError::InvalidName {
            name: error.input,
            reason: error.reason,
        })?,
    };
    if target_id != view_id && views.views.iter().any(|view| view.id == target_id) {
        return Err(ViewWriteError::DuplicateId { id: target_id });
    }

    let definition = prepare_definition(definition, &target_id, filter)?;
    let new_view =
        view_from_value(definition).map_err(|error| ViewWriteError::InvalidDefinition {
            detail: error.to_string(),
        })?;

    let renamed_from = (target_id != view_id).then(|| view_id.to_owned());
    views.views[position] = new_view;

    let output_dir = views.output_dir.clone();
    let mut outcome = finalize(views, &path, &inputs, pre_diagnostics, target_id)?;
    if let Some(old_id) = renamed_from {
        remove_rendered_file(
            project_root,
            &output_dir,
            &old_id,
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
    let inputs = load_check_inputs(config, project_root)?;
    let path = views_path(config, project_root);
    let mut views = load_current_views(&path)?;

    let pre_diagnostics = check(&views, &inputs, &path);

    let position = views
        .views
        .iter()
        .position(|view| view.id == view_id)
        .ok_or_else(|| ViewWriteError::ViewNotFound {
            id: view_id.to_owned(),
        })?;
    views.views.remove(position);

    let output_dir = views.output_dir.clone();
    let mut outcome = finalize(views, &path, &inputs, pre_diagnostics, view_id.to_owned())?;
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
    let rendered_path = project_root.join(output_dir).join(format!("{view_id}.md"));
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
    views: Views,
    path: &Path,
    inputs: &CheckInputs,
    pre_diagnostics: Vec<Diagnostic>,
    view_id: String,
) -> Result<ViewWriteOutcome, ViewWriteError> {
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
    let warnings = check(&reparsed, inputs, path);

    write_file_atomically(path, &candidate).map_err(|source| ViewWriteError::WriteFile {
        path: path.to_path_buf(),
        source,
    })?;

    let mutation_caused_warning =
        crate::operations::diagnostics::introduced_by_mutation(&pre_diagnostics, &warnings);

    Ok(ViewWriteOutcome {
        path: path.to_path_buf(),
        view_id,
        warnings,
        mutation_caused_warning,
        info_messages: Vec::new(),
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    use crate::model::schema::Severity;
    use crate::parser::config::load_config;
    use crate::parser::views::load_views;
    use crate::query::clause::Condition;
    use crate::query::types::Operator;

    /// A raw passthrough clause — used where a test only cares that the
    /// clause string lands in the file, not how it was built.
    fn raw(clause: &str) -> Clause {
        Clause::Raw {
            raw: clause.to_owned(),
        }
    }

    fn condition(field: &str, operator: Operator, value: Option<&str>) -> Clause {
        Clause::Comparison(Condition {
            field: field.to_owned(),
            operator,
            value: value.map(str::to_owned),
            values: Vec::new(),
        })
    }

    fn membership(field: &str, operator: Operator, values: &[&str]) -> Clause {
        Clause::Comparison(Condition {
            field: field.to_owned(),
            operator,
            value: None,
            values: values.iter().map(|value| (*value).to_owned()).collect(),
        })
    }

    const CONFIG: &str = "\
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

    const SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
  status:
    type: choice
    values: [open, in_progress, done]
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
";

    fn setup() -> (TempDir, PathBuf, Config) {
        let directory = TempDir::new().unwrap();
        let root = directory.path().to_path_buf();
        fs::create_dir_all(root.join(".workdown")).unwrap();
        // A view write reads the work items to check filter operands
        // against the ids they may name, so the fixture scaffolds the
        // directory `workdown init` would have created. Items are added
        // per-test by `write_item` where a clause needs one.
        fs::create_dir_all(root.join("workdown-items")).unwrap();
        fs::write(root.join(".workdown/config.yaml"), CONFIG).unwrap();
        fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();
        let config = load_config(&root.join(".workdown/config.yaml")).unwrap();
        (directory, root, config)
    }

    fn write_item(root: &Path, id: &str) {
        fs::write(root.join(format!("workdown-items/{id}.md")), "---\n---\n").unwrap();
    }

    fn write_views(root: &Path, content: &str) {
        fs::write(root.join(".workdown/views.yaml"), content).unwrap();
    }

    fn read_views(root: &Path) -> String {
        fs::read_to_string(root.join(".workdown/views.yaml")).unwrap()
    }

    fn board(id: &str) -> serde_yaml::Value {
        serde_yaml::from_str(&format!("id: {id}\ntype: board\nfield: status\n")).unwrap()
    }

    // ── add_view ─────────────────────────────────────────────────────

    #[test]
    fn add_view_creates_file_when_absent() {
        let (_dir, root, config) = setup();
        assert!(!root.join(".workdown/views.yaml").exists());

        let outcome = add_view(&config, &root, board("status-board")).unwrap();

        assert_eq!(outcome.view_id, "status-board");
        assert!(!outcome.mutation_caused_warning);
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views.len(), 1);
        assert_eq!(reloaded.views[0].id, "status-board");
    }

    #[test]
    fn add_view_appends_to_existing() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: first\n    type: board\n    field: status\n",
        );

        add_view(&config, &root, board("second")).unwrap();

        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        let ids: Vec<&str> = reloaded.views.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, vec!["first", "second"]);
    }

    #[test]
    fn add_view_duplicate_id_errors_without_writing() {
        let (_dir, root, config) = setup();
        let original = "views:\n  - id: dup\n    type: board\n    field: status\n";
        write_views(&root, original);

        let error = add_view(&config, &root, board("dup")).unwrap_err();

        assert!(matches!(error, ViewWriteError::DuplicateId { id } if id == "dup"));
        assert_eq!(read_views(&root), original, "file must be untouched");
    }

    #[test]
    fn add_view_missing_required_slot_errors_without_writing() {
        let (_dir, root, config) = setup();
        let definition: serde_yaml::Value = serde_yaml::from_str("id: b\ntype: board\n").unwrap();

        let error = add_view(&config, &root, definition).unwrap_err();

        assert!(matches!(error, ViewWriteError::InvalidDefinition { .. }));
        assert!(!root.join(".workdown/views.yaml").exists());
    }

    #[test]
    fn add_view_unknown_slot_errors_without_writing() {
        let (_dir, root, config) = setup();
        let definition: serde_yaml::Value =
            serde_yaml::from_str("id: b\ntype: board\nfield: status\nbogus: x\n").unwrap();

        let error = add_view(&config, &root, definition).unwrap_err();

        assert!(matches!(error, ViewWriteError::InvalidDefinition { .. }));
        assert!(!root.join(".workdown/views.yaml").exists());
    }

    #[test]
    fn add_view_with_bad_field_reference_writes_with_warning() {
        let (_dir, root, config) = setup();
        // `field: nope` parses fine but fails cross-file validation —
        // save-with-warning: the view is written, the problem is surfaced.
        let definition: serde_yaml::Value =
            serde_yaml::from_str("id: b\ntype: board\nfield: nope\n").unwrap();

        let outcome = add_view(&config, &root, definition).unwrap();

        assert!(outcome.mutation_caused_warning);
        assert!(!outcome.warnings.is_empty());
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views[0].id, "b");
    }

    #[test]
    fn add_view_over_invalid_existing_file_errors() {
        let (_dir, root, config) = setup();
        write_views(&root, "views:\n  - id: x\n    type: not_a_real_kind\n");

        let error = add_view(&config, &root, board("new")).unwrap_err();

        assert!(matches!(error, ViewWriteError::ExistingInvalid { .. }));
    }

    // ── create_view (name → slug) ────────────────────────────────────

    #[test]
    fn create_view_slugs_name_to_id() {
        let (_dir, root, config) = setup();
        let definition: serde_yaml::Value =
            serde_yaml::from_str("type: board\nfield: status\n").unwrap();

        let outcome = create_view(&config, &root, "My Status Board", definition, &[]).unwrap();

        assert_eq!(outcome.view_id, "my-status-board");
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views[0].id, "my-status-board");
    }

    #[test]
    fn create_view_injects_the_filter_clauses() {
        let (_dir, root, config) = setup();
        let definition: serde_yaml::Value =
            serde_yaml::from_str("type: board\nfield: status\n").unwrap();

        create_view(
            &config,
            &root,
            "Open Board",
            definition,
            &[raw("status=open")],
        )
        .unwrap();

        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views[0].where_clauses, vec!["status=open"]);
    }

    #[test]
    fn create_view_overwrites_supplied_id_with_the_slug() {
        let (_dir, root, config) = setup();
        // A stray `id` in the definition is ignored — the name is authoritative.
        let definition: serde_yaml::Value =
            serde_yaml::from_str("id: ignored\ntype: board\nfield: status\n").unwrap();

        let outcome = create_view(&config, &root, "Real Name", definition, &[]).unwrap();

        assert_eq!(outcome.view_id, "real-name");
    }

    #[test]
    fn create_view_blank_name_errors_without_writing() {
        let (_dir, root, config) = setup();
        let definition: serde_yaml::Value =
            serde_yaml::from_str("type: board\nfield: status\n").unwrap();

        let error = create_view(&config, &root, "   ", definition, &[]).unwrap_err();

        assert!(matches!(error, ViewWriteError::InvalidName { .. }));
        assert!(!root.join(".workdown/views.yaml").exists());
    }

    // ── set_view_filter ──────────────────────────────────────────────

    #[test]
    fn set_view_filter_updates_where() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: board\n    type: board\n    field: status\n",
        );

        // Structured conditions, to exercise the serializer end to end.
        let outcome = set_view_filter(
            &config,
            &root,
            "board",
            &[
                condition("status", Operator::Equal, Some("open")),
                condition("title", Operator::Contains, Some("fix")),
            ],
        )
        .unwrap();

        assert_eq!(outcome.view_id, "board");
        assert!(!outcome.mutation_caused_warning);
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(
            reloaded.views[0].where_clauses,
            vec!["status=open", "title~fix"]
        );
    }

    /// A membership condition reaches `views.yaml` as `in` / `not in`, with the
    /// comma-join happening in the serializer and nowhere else.
    #[test]
    fn set_view_filter_writes_membership_clauses() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: board\n    type: board\n    field: status\n",
        );

        let outcome = set_view_filter(
            &config,
            &root,
            "board",
            &[
                membership("status", Operator::In, &["open", "in_progress"]),
                membership("status", Operator::NotIn, &["done"]),
            ],
        )
        .unwrap();

        assert!(!outcome.mutation_caused_warning);
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(
            reloaded.views[0].where_clauses,
            vec!["status in open,in_progress", "status not in done"]
        );
    }

    /// An operand that doesn't match its operator's arity fails the write
    /// outright — the guided builder cannot produce one, so it is a malformed
    /// request rather than a filter to save with a warning.
    #[test]
    fn set_view_filter_rejects_operand_arity_mismatch_without_writing() {
        let (_dir, root, config) = setup();
        let source = "views:\n  - id: board\n    type: board\n    field: status\n";
        write_views(&root, source);

        let error = set_view_filter(
            &config,
            &root,
            "board",
            &[condition("status", Operator::In, Some("open"))],
        )
        .unwrap_err();

        assert!(matches!(error, ViewWriteError::InvalidCondition(_)));
        assert_eq!(
            std::fs::read_to_string(root.join(".workdown/views.yaml")).unwrap(),
            source
        );
    }

    #[test]
    fn set_view_filter_replaces_previous_where() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=done\"\n",
        );

        set_view_filter(&config, &root, "board", &[raw("status=open")]).unwrap();

        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views[0].where_clauses, vec!["status=open"]);
    }

    #[test]
    fn set_view_filter_empty_clears_where() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=done\"\n",
        );

        set_view_filter(&config, &root, "board", &[]).unwrap();

        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert!(reloaded.views[0].where_clauses.is_empty());
        assert!(
            !read_views(&root).contains("where:"),
            "empty where should not be emitted"
        );
    }

    #[test]
    fn set_view_filter_unknown_view_errors_without_writing() {
        let (_dir, root, config) = setup();
        let original = "views:\n  - id: board\n    type: board\n    field: status\n";
        write_views(&root, original);

        let error = set_view_filter(&config, &root, "nope", &[raw("status=open")]).unwrap_err();

        assert!(matches!(error, ViewWriteError::ViewNotFound { id } if id == "nope"));
        assert_eq!(read_views(&root), original, "file must be untouched");
    }

    #[test]
    fn set_view_filter_with_unknown_field_writes_with_warning() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: board\n    type: board\n    field: status\n",
        );

        // References a field not in the schema: parses, but fails cross-file
        // validation. Save-with-warning — written and surfaced.
        let outcome = set_view_filter(&config, &root, "board", &[raw("nonexistent=x")]).unwrap();

        assert!(outcome.mutation_caused_warning);
        assert!(!outcome.warnings.is_empty());
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views[0].where_clauses, vec!["nonexistent=x"]);
    }

    /// The write path checks operands, not just field names: the value
    /// is written and the problem comes back as a warning, so a filter
    /// that can never match is caught as it is authored rather than at
    /// the next `validate`.
    #[test]
    fn set_view_filter_with_unknown_value_writes_with_warning() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: board\n    type: board\n    field: status\n",
        );

        let outcome = set_view_filter(&config, &root, "board", &[raw("status=nonsense")]).unwrap();

        assert!(outcome.mutation_caused_warning);
        assert_eq!(outcome.warnings.len(), 1, "{:?}", outcome.warnings);
        assert_eq!(outcome.warnings[0].severity, Severity::Warning);
        assert!(
            outcome.warnings[0].message.contains("nonsense"),
            "{}",
            outcome.warnings[0].message
        );
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views[0].where_clauses, vec!["status=nonsense"]);
    }

    /// Item ids are the option set that only the store can supply, which
    /// is why this path loads it. The same clause is clean or not
    /// depending on whether the item exists.
    #[test]
    fn set_view_filter_checks_item_ids_against_the_store() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: board\n    type: board\n    field: status\n",
        );

        let outcome = set_view_filter(&config, &root, "board", &[raw("parent=epic-1")]).unwrap();
        assert!(outcome.mutation_caused_warning, "no such item yet");

        write_item(&root, "epic-1");
        let outcome = set_view_filter(&config, &root, "board", &[raw("parent=epic-1")]).unwrap();
        assert!(!outcome.mutation_caused_warning);
        assert!(outcome.warnings.is_empty(), "{:?}", outcome.warnings);
    }

    // ── update_view ──────────────────────────────────────────────────

    /// Two views, so replacement can be checked to stay in place.
    const TWO_VIEWS: &str = "\
views:
  - id: first
    type: board
    field: status
  - id: second
    type: tree
    field: parent
";

    fn definition(yaml: &str) -> serde_yaml::Value {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn update_view_replaces_definition_in_place() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        // Switch `first` from a board to a tree — a full kind change.
        let outcome = update_view(
            &config,
            &root,
            "first",
            None,
            definition("type: tree\nfield: parent\n"),
            &[],
        )
        .unwrap();

        assert_eq!(outcome.view_id, "first");
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        let ids: Vec<&str> = reloaded.views.iter().map(|view| view.id.as_str()).collect();
        assert_eq!(ids, vec!["first", "second"], "position must be preserved");
        assert!(matches!(
            &reloaded.views[0].kind,
            crate::model::views::ViewKind::Tree { field } if field == "parent"
        ));
    }

    #[test]
    fn update_view_replaces_the_filter() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=done\"\n",
        );

        update_view(
            &config,
            &root,
            "board",
            None,
            definition("type: board\nfield: status\n"),
            &[condition("status", Operator::Equal, Some("open"))],
        )
        .unwrap();

        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views[0].where_clauses, vec!["status=open"]);
    }

    #[test]
    fn update_view_empty_filter_clears_where() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: board\n    type: board\n    field: status\n    where:\n      - \"status=done\"\n",
        );

        update_view(
            &config,
            &root,
            "board",
            None,
            definition("type: board\nfield: status\n"),
            &[],
        )
        .unwrap();

        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert!(reloaded.views[0].where_clauses.is_empty());
    }

    #[test]
    fn update_view_unknown_id_errors_without_writing() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        let error = update_view(
            &config,
            &root,
            "nope",
            None,
            definition("type: board\nfield: status\n"),
            &[],
        )
        .unwrap_err();

        assert!(matches!(error, ViewWriteError::ViewNotFound { id } if id == "nope"));
        assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
    }

    #[test]
    fn update_view_invalid_definition_errors_without_writing() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        // A board without its required `field` slot cannot be constructed.
        let error = update_view(
            &config,
            &root,
            "first",
            None,
            definition("type: board\n"),
            &[],
        )
        .unwrap_err();

        assert!(matches!(error, ViewWriteError::InvalidDefinition { .. }));
        assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
    }

    #[test]
    fn update_view_with_bad_field_reference_writes_with_warning() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        // `field: nope` loads but fails cross-file validation —
        // save-with-warning, same as create.
        let outcome = update_view(
            &config,
            &root,
            "first",
            None,
            definition("type: board\nfield: nope\n"),
            &[],
        )
        .unwrap();

        assert!(outcome.mutation_caused_warning);
        assert!(!outcome.warnings.is_empty());
    }

    #[test]
    fn update_view_with_new_name_renames_the_id() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        let outcome = update_view(
            &config,
            &root,
            "first",
            Some("Sprint Board"),
            definition("type: board\nfield: status\n"),
            &[],
        )
        .unwrap();

        assert_eq!(outcome.view_id, "sprint-board");
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        let ids: Vec<&str> = reloaded.views.iter().map(|view| view.id.as_str()).collect();
        assert_eq!(ids, vec!["sprint-board", "second"]);
    }

    #[test]
    fn update_view_rename_removes_the_old_rendered_file() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);
        fs::create_dir_all(root.join("views")).unwrap();
        fs::write(root.join("views/first.md"), "# rendered\n").unwrap();

        let outcome = update_view(
            &config,
            &root,
            "first",
            Some("Sprint Board"),
            definition("type: board\nfield: status\n"),
            &[],
        )
        .unwrap();

        assert!(
            !root.join("views/first.md").exists(),
            "the old id's rendered file must be removed"
        );
        assert_eq!(
            outcome.info_messages.len(),
            1,
            "{:?}",
            outcome.info_messages
        );
        assert!(outcome.info_messages[0].contains("first.md"));
    }

    #[test]
    fn update_view_rename_to_same_slug_is_not_a_rename() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);
        fs::create_dir_all(root.join("views")).unwrap();
        fs::write(root.join("views/first.md"), "# rendered\n").unwrap();

        // "First" slugs back to "first" — not a rename, nothing removed.
        let outcome = update_view(
            &config,
            &root,
            "first",
            Some("First"),
            definition("type: board\nfield: status\n"),
            &[],
        )
        .unwrap();

        assert_eq!(outcome.view_id, "first");
        assert!(root.join("views/first.md").exists());
        assert!(outcome.info_messages.is_empty());
    }

    #[test]
    fn update_view_rename_to_existing_id_errors_without_writing() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        let error = update_view(
            &config,
            &root,
            "first",
            Some("Second"),
            definition("type: board\nfield: status\n"),
            &[],
        )
        .unwrap_err();

        assert!(matches!(error, ViewWriteError::DuplicateId { id } if id == "second"));
        assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
    }

    #[test]
    fn update_view_blank_name_errors_without_writing() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        let error = update_view(
            &config,
            &root,
            "first",
            Some("   "),
            definition("type: board\nfield: status\n"),
            &[],
        )
        .unwrap_err();

        assert!(matches!(error, ViewWriteError::InvalidName { .. }));
        assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
    }

    // ── delete_view ──────────────────────────────────────────────────

    #[test]
    fn delete_view_removes_the_entry_and_keeps_the_rest() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        let outcome = delete_view(&config, &root, "first").unwrap();

        assert_eq!(outcome.view_id, "first");
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        let ids: Vec<&str> = reloaded.views.iter().map(|view| view.id.as_str()).collect();
        assert_eq!(ids, vec!["second"]);
    }

    #[test]
    fn delete_view_last_entry_leaves_a_loadable_empty_file() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: only\n    type: board\n    field: status\n",
        );

        delete_view(&config, &root, "only").unwrap();

        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert!(reloaded.views.is_empty());
    }

    #[test]
    fn delete_view_unknown_id_errors_without_writing() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        let error = delete_view(&config, &root, "nope").unwrap_err();

        assert!(matches!(error, ViewWriteError::ViewNotFound { id } if id == "nope"));
        assert_eq!(read_views(&root), TWO_VIEWS, "file must be untouched");
    }

    #[test]
    fn delete_view_removes_the_rendered_file() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);
        fs::create_dir_all(root.join("views")).unwrap();
        fs::write(root.join("views/first.md"), "# rendered\n").unwrap();

        let outcome = delete_view(&config, &root, "first").unwrap();

        assert!(!root.join("views/first.md").exists());
        assert_eq!(
            outcome.info_messages.len(),
            1,
            "{:?}",
            outcome.info_messages
        );
        assert!(outcome.info_messages[0].contains("removed"));
    }

    #[test]
    fn delete_view_without_rendered_file_is_silent() {
        let (_dir, root, config) = setup();
        write_views(&root, TWO_VIEWS);

        let outcome = delete_view(&config, &root, "first").unwrap();

        assert!(
            outcome.info_messages.is_empty(),
            "{:?}",
            outcome.info_messages
        );
    }

    /// The removal honours a non-default `directory:` — the file lives
    /// wherever `workdown render` would have written it.
    #[test]
    fn delete_view_removes_the_rendered_file_from_a_custom_directory() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "directory: rendered/views\nviews:\n  - id: only\n    type: board\n    field: status\n",
        );
        fs::create_dir_all(root.join("rendered/views")).unwrap();
        fs::write(root.join("rendered/views/only.md"), "# rendered\n").unwrap();

        delete_view(&config, &root, "only").unwrap();

        assert!(!root.join("rendered/views/only.md").exists());
    }

    #[test]
    fn set_view_filter_preserves_other_views() {
        let (_dir, root, config) = setup();
        write_views(
            &root,
            "views:\n  - id: a\n    type: board\n    field: status\n  - id: b\n    type: tree\n    field: parent\n",
        );

        set_view_filter(&config, &root, "a", &[raw("status=open")]).unwrap();

        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views.len(), 2);
        assert_eq!(reloaded.views[1].id, "b");
        assert!(matches!(
            &reloaded.views[1].kind,
            crate::model::views::ViewKind::Tree { field, .. } if field == "parent"
        ));
    }
}
