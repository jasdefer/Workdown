//! Persist view definitions to `views.yaml`.
//!
//! The read side of views is handled by [`crate::parser::views`] and
//! [`crate::views_check`]; this module is the write side. It supports the
//! two mutations the view-authoring UI needs: adding a new view, and
//! replacing an existing view's `where:` filter. Everything else about a
//! view (kind, slots, ordering, deletion) stays a text-editor job.
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
use crate::model::schema::Schema;
use crate::model::views::Views;
use crate::operations::frontmatter_io::write_file_atomically;
use crate::parser;
use crate::parser::schema::SchemaLoadError;
use crate::parser::views::{serialize_views, view_from_value};
use crate::query::clause::{clauses_to_strings, Clause};

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

    #[error("existing views file at '{path}' is invalid; fix it in a text editor before writing from the UI: {detail}")]
    ExistingInvalid { path: PathBuf, detail: String },

    #[error("invalid view definition: {detail}")]
    InvalidDefinition { detail: String },

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
    let schema = load_schema(config, project_root)?;
    let path = views_path(config, project_root);
    let mut views = load_current_views(&path)?;

    let pre_diagnostics = crate::views_check::evaluate(&views, &schema, &path);

    let new_view = view_from_value(definition)
        .map_err(|error| ViewWriteError::InvalidDefinition {
            detail: error.to_string(),
        })?;

    if views.views.iter().any(|view| view.id == new_view.id) {
        return Err(ViewWriteError::DuplicateId { id: new_view.id });
    }

    let view_id = new_view.id.clone();
    views.views.push(new_view);

    finalize(views, &path, &schema, pre_diagnostics, view_id)
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
    let where_clauses = clauses_to_strings(filter);
    if !where_clauses.is_empty() {
        mapping.insert(
            serde_yaml::Value::String("where".to_owned()),
            serde_yaml::Value::Sequence(
                where_clauses.into_iter().map(serde_yaml::Value::String).collect(),
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
    let schema = load_schema(config, project_root)?;
    let path = views_path(config, project_root);
    let mut views = load_current_views(&path)?;

    let pre_diagnostics = crate::views_check::evaluate(&views, &schema, &path);

    let view = views
        .views
        .iter_mut()
        .find(|view| view.id == view_id)
        .ok_or_else(|| ViewWriteError::ViewNotFound {
            id: view_id.to_owned(),
        })?;
    view.where_clauses = clauses_to_strings(clauses);

    finalize(views, &path, &schema, pre_diagnostics, view_id.to_owned())
}

// ── Internals ────────────────────────────────────────────────────────

fn views_path(config: &Config, project_root: &Path) -> PathBuf {
    project_root.join(&config.paths.views)
}

fn load_schema(config: &Config, project_root: &Path) -> Result<Schema, ViewWriteError> {
    let schema_path = project_root.join(&config.schema);
    Ok(parser::schema::load_schema(&schema_path)?)
}

/// Load the current views, or an empty set when the file does not exist
/// yet. An existing file that won't parse is a hard error: we re-serialize
/// the whole file from the model, so we can't safely preserve views we
/// can't read.
fn load_current_views(path: &Path) -> Result<Views, ViewWriteError> {
    if !path.exists() {
        // Parsing an empty list yields the default `output_dir`, so the
        // created file matches a hand-authored one with no `directory:`.
        return Ok(parser::views::parse_views("views: []\n")
            .expect("empty views list parses"));
    }
    parser::views::load_views(path).map_err(|error| ViewWriteError::ExistingInvalid {
        path: path.to_path_buf(),
        detail: error.to_string(),
    })
}

/// Serialize the mutated model, validate the candidate before touching
/// disk, write atomically, and diff diagnostics to flag whether this write
/// introduced a new problem.
fn finalize(
    views: Views,
    path: &Path,
    schema: &Schema,
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
    let warnings = crate::views_check::evaluate(&reparsed, schema, path);

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
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

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
        fs::write(root.join(".workdown/config.yaml"), CONFIG).unwrap();
        fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();
        let config = load_config(&root.join(".workdown/config.yaml")).unwrap();
        (directory, root, config)
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
        write_views(&root, "views:\n  - id: first\n    type: board\n    field: status\n");

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
        let definition: serde_yaml::Value =
            serde_yaml::from_str("id: b\ntype: board\n").unwrap();

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
        write_views(&root, "views:\n  - id: board\n    type: board\n    field: status\n");

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
        assert_eq!(reloaded.views[0].where_clauses, vec!["status=open", "title~fix"]);
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
        assert!(!read_views(&root).contains("where:"), "empty where should not be emitted");
    }

    #[test]
    fn set_view_filter_unknown_view_errors_without_writing() {
        let (_dir, root, config) = setup();
        let original = "views:\n  - id: board\n    type: board\n    field: status\n";
        write_views(&root, original);

        let error =
            set_view_filter(&config, &root, "nope", &[raw("status=open")]).unwrap_err();

        assert!(matches!(error, ViewWriteError::ViewNotFound { id } if id == "nope"));
        assert_eq!(read_views(&root), original, "file must be untouched");
    }

    #[test]
    fn set_view_filter_with_unknown_field_writes_with_warning() {
        let (_dir, root, config) = setup();
        write_views(&root, "views:\n  - id: board\n    type: board\n    field: status\n");

        // References a field not in the schema: parses, but fails cross-file
        // validation. Save-with-warning — written and surfaced.
        let outcome =
            set_view_filter(&config, &root, "board", &[raw("nonexistent=x")]).unwrap();

        assert!(outcome.mutation_caused_warning);
        assert!(!outcome.warnings.is_empty());
        let reloaded = load_views(&root.join(".workdown/views.yaml")).unwrap();
        assert_eq!(reloaded.views[0].where_clauses, vec!["nonexistent=x"]);
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
