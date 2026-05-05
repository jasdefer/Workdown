//! Work item store: scanning, indexing, querying, and relationship management.
//!
//! The [`Store`] is the central data layer. It scans a directory of work item
//! Markdown files, parses and coerces them against the project schema, checks
//! ID uniqueness, detects broken links, and pre-computes inverse relations.
//!
//! Individual item problems (bad YAML, type mismatches, missing fields) are
//! collected as [`Diagnostic`]s — the store loads as much as it can and
//! reports all findings.

mod coerce;
mod cycles;
mod rollup;

pub(crate) use coerce::coerce_fields;

use std::collections::HashMap;
use std::path::Path;

use crate::model::diagnostic::{
    Diagnostic, FileDiagnosticKind, FilesDiagnosticKind, ItemDiagnosticKind,
};
use crate::model::schema::{Schema, Severity};
use crate::model::{WorkItem, WorkItemId};
use crate::parser;
use crate::walker::targets_of;

// ── Store ────────────────────────────────────────────────────────────

/// An in-memory index of all work items in a project.
///
/// Built by [`Store::load`], which scans a directory, parses files, coerces
/// fields, checks uniqueness, and derives inverse relations. Problems are
/// collected rather than aborting — query the store even if some items failed.
pub struct Store {
    /// Work items indexed by ID.
    items: HashMap<WorkItemId, WorkItem>,
    /// Pre-computed reverse links: `field_name → target_id → [source_ids]`.
    ///
    /// For example, if item "login-task" has `parent: auth-epic`, then
    /// `reverse_links["parent"]["auth-epic"]` contains `"login-task"`.
    reverse_links: HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    /// All diagnostics collected during loading.
    diagnostics: Vec<Diagnostic>,
}

impl Store {
    /// Scan `items_dir` for `.md` files and load them into the store.
    ///
    /// Only returns `Err` if the directory itself cannot be read.
    /// Per-file and per-field problems are collected in [`Store::diagnostics`].
    pub fn load(items_dir: &Path, schema: &Schema) -> Result<Store, std::io::Error> {
        let mut diagnostics = Vec::new();

        // 1. Collect all .md file paths, sorted alphabetically for determinism.
        let mut paths = Vec::new();
        for entry in walkdir::WalkDir::new(items_dir)
            .min_depth(1)
            .max_depth(1)
            .sort_by_file_name()
        {
            let entry = entry.map_err(std::io::Error::other)?;
            let path = entry.into_path();
            if path.extension().is_some_and(|ext| ext == "md") {
                paths.push(path);
            }
        }

        // 2. Parse each file and check ID uniqueness.
        let mut items = HashMap::new();
        let mut seen_ids: HashMap<WorkItemId, std::path::PathBuf> = HashMap::new();

        for path in &paths {
            let raw = match parser::parse_work_item_file(path) {
                Ok(raw) => raw,
                Err(e) => {
                    diagnostics.push(Diagnostic::file(
                        Severity::Error,
                        path.clone(),
                        FileDiagnosticKind::ReadError {
                            detail: e.to_string(),
                        },
                    ));
                    continue;
                }
            };

            // Check for duplicate IDs.
            if let Some(first_path) = seen_ids.get(&raw.id) {
                diagnostics.push(Diagnostic::files(
                    Severity::Error,
                    vec![first_path.clone(), path.clone()],
                    FilesDiagnosticKind::DuplicateId { id: raw.id.clone() },
                ));
                continue;
            }
            seen_ids.insert(raw.id.clone(), path.clone());

            // 3. Coerce fields.
            let (fields, coercion_diagnostics) = coerce::coerce_fields(&raw, schema);
            diagnostics.extend(coercion_diagnostics);

            items.insert(
                raw.id.clone(),
                WorkItem {
                    id: raw.id,
                    fields,
                    body: raw.body,
                    source_path: raw.source_path,
                },
            );
        }

        // 4. Build reverse links and detect broken references.
        let mut reverse_links: HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>> =
            HashMap::new();

        for item in items.values() {
            for field_name in item.fields.keys() {
                for target_id in targets_of(item, field_name) {
                    if !items.contains_key(target_id.as_str()) {
                        diagnostics.push(Diagnostic::item(
                            Severity::Error,
                            item.source_path.clone(),
                            item.id.clone(),
                            ItemDiagnosticKind::BrokenLink {
                                field: field_name.clone(),
                                target_id: target_id.clone(),
                            },
                        ));
                    }

                    reverse_links
                        .entry(field_name.clone())
                        .or_default()
                        .entry(target_id.clone())
                        .or_default()
                        .push(item.id.clone());
                }
            }
        }

        // 5. Aggregate rollup: fill computed values into non-leaf items
        // and emit chain-conflict / missing-value diagnostics. Mutates
        // `items` in place so downstream consumers see manual + computed
        // values indistinguishably.
        diagnostics.extend(rollup::run(&mut items, &reverse_links, schema));

        Ok(Store {
            items,
            reverse_links,
            diagnostics,
        })
    }

    /// Insert a new work item into the store.
    ///
    /// Used by `workdown add` to enable rule evaluation on the newly created item.
    /// Does not rebuild reverse links — callers that need accurate reverse links
    /// after insertion should reload the store.
    pub fn insert(&mut self, item: WorkItem) {
        self.items.insert(item.id.clone(), item);
    }

    /// Look up a work item by its ID.
    pub fn get(&self, id: &str) -> Option<&WorkItem> {
        self.items.get(id)
    }

    /// Iterate over all successfully loaded work items.
    pub fn all_items(&self) -> impl Iterator<Item = &WorkItem> {
        self.items.values()
    }

    /// Get items that link TO the given item via `field_name`.
    ///
    /// For example, `referring_items("auth-epic", "parent")` returns all
    /// items whose `parent` field points to `auth-epic`.
    pub fn referring_items(&self, item_id: &str, field_name: &str) -> Vec<&WorkItem> {
        self.reverse_links
            .get(field_name)
            .and_then(|by_target| by_target.get(item_id))
            .map(|source_ids| {
                source_ids
                    .iter()
                    .filter_map(|id| self.items.get(id.as_str()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// All diagnostics collected during loading.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether any diagnostics were collected during loading.
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == Severity::Error)
    }

    /// Whether any diagnostics (errors or warnings) were collected.
    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    /// Number of successfully loaded items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the store contains no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Detect cycles in link fields where `allow_cycles` is `false`.
    ///
    /// Returns one diagnostic per unique cycle found. Each diagnostic
    /// identifies the field and the chain of IDs forming the cycle
    /// (last element repeats the first to close it).
    pub fn detect_cycles(&self, schema: &Schema) -> Vec<Diagnostic> {
        cycles::detect_cycles(self, schema)
    }

    /// Access the items map (crate-internal).
    pub(crate) fn items_map(&self) -> &HashMap<WorkItemId, WorkItem> {
        &self.items
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::diagnostic::DiagnosticBody;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use crate::model::FieldValue;
    use indexmap::IndexMap;
    use std::fs;
    use std::path::PathBuf;

    /// Build a minimal schema for store tests.
    fn test_schema() -> Schema {
        let mut fields = IndexMap::new();

        fields.insert(
            "title".to_owned(),
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        );

        let mut status = FieldDefinition::new(FieldTypeConfig::Choice {
            values: vec!["open".into(), "in_progress".into(), "done".into()],
        });
        status.required = true;
        fields.insert("status".to_owned(), status);

        fields.insert(
            "parent".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: Some(false),
                inverse: Some("children".into()),
            }),
        );

        fields.insert(
            "depends_on".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Links {
                allow_cycles: Some(false),
                inverse: Some("dependents".into()),
            }),
        );

        fields.insert(
            "tags".to_owned(),
            FieldDefinition::new(FieldTypeConfig::List),
        );

        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    /// Create a temp directory with work item files for testing.
    fn setup_items_dir(items: Vec<(&str, &str)>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let items_path = dir.path().to_path_buf();
        for (filename, content) in items {
            fs::write(items_path.join(filename), content).expect("failed to write test file");
        }
        (dir, items_path)
    }

    // ── Loading ──────────────────────────────────────────────────────

    #[test]
    fn load_multiple_valid_items() {
        let (_dir, path) = setup_items_dir(vec![
            (
                "task-a.md",
                "---\ntitle: Task A\nstatus: open\n---\nBody A\n",
            ),
            (
                "task-b.md",
                "---\ntitle: Task B\nstatus: done\n---\nBody B\n",
            ),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert_eq!(store.len(), 2);
        assert!(!store.has_diagnostics());

        let task_a = store.get("task-a").unwrap();
        assert_eq!(task_a.fields["title"], FieldValue::String("Task A".into()));
        assert_eq!(task_a.fields["status"], FieldValue::Choice("open".into()));

        let task_b = store.get("task-b").unwrap();
        assert_eq!(task_b.fields["status"], FieldValue::Choice("done".into()));
    }

    #[test]
    fn load_empty_directory() {
        let (_dir, path) = setup_items_dir(vec![]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert!(store.is_empty());
        assert!(!store.has_diagnostics());
    }

    #[test]
    fn load_skips_non_md_files() {
        let (_dir, path) = setup_items_dir(vec![
            ("task-a.md", "---\ntitle: Task A\nstatus: open\n---\n"),
            ("readme.txt", "This is not a work item"),
            ("notes.yaml", "key: value"),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert_eq!(store.len(), 1);
        assert!(!store.has_diagnostics());
    }

    // ── Diagnostic collection ────────────────────────────────────────

    #[test]
    fn parse_error_skips_file_collects_diagnostic() {
        let (_dir, path) = setup_items_dir(vec![
            ("good.md", "---\ntitle: Good\nstatus: open\n---\n"),
            ("bad.md", "no frontmatter here"),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert_eq!(store.len(), 1);
        assert!(store.get("good").is_some());
        assert!(store.has_diagnostics());
        assert!(store
            .diagnostics()
            .iter()
            .any(|diagnostic| matches!(&diagnostic.body, DiagnosticBody::File(_))));
    }

    #[test]
    fn missing_required_field_collected() {
        let (_dir, path) = setup_items_dir(vec![
            // status is required but missing
            ("task-a.md", "---\ntitle: Task A\n---\n"),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert_eq!(store.len(), 1); // item still loaded
        assert!(store.has_errors());
        assert!(store.diagnostics().iter().any(|diagnostic| matches!(
            &diagnostic.body,
            DiagnosticBody::Item(item)
                if matches!(&item.kind, ItemDiagnosticKind::MissingRequired { field } if field == "status")
        )));
    }

    #[test]
    fn unknown_field_warning_collected() {
        let (_dir, path) = setup_items_dir(vec![(
            "task-a.md",
            "---\ntitle: Task A\nstatus: open\nbogus: whatever\n---\n",
        )]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert_eq!(store.len(), 1);
        assert!(store.diagnostics().iter().any(|diagnostic| matches!(
            &diagnostic.body,
            DiagnosticBody::Item(item)
                if matches!(&item.kind, ItemDiagnosticKind::UnknownField { field } if field == "bogus")
        )));
    }

    // ── Duplicate IDs ────────────────────────────────────────────────

    #[test]
    fn duplicate_id_detected() {
        let (_dir, path) = setup_items_dir(vec![
            ("task-a.md", "---\ntitle: First\nstatus: open\n---\n"),
            (
                "task-b.md",
                "---\nid: task-a\ntitle: Second\nstatus: done\n---\n",
            ),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        // First alphabetically wins
        assert_eq!(store.len(), 1);
        let item = store.get("task-a").unwrap();
        assert_eq!(item.fields["title"], FieldValue::String("First".into()));

        assert!(store.diagnostics().iter().any(|diagnostic| matches!(
            &diagnostic.body,
            DiagnosticBody::Files(files)
                if matches!(&files.kind, FilesDiagnosticKind::DuplicateId { id } if id == "task-a")
        )));
    }

    // ── Broken links ─────────────────────────────────────────────────

    #[test]
    fn broken_link_detected() {
        let (_dir, path) = setup_items_dir(vec![(
            "task-a.md",
            "---\ntitle: Task A\nstatus: open\nparent: nonexistent\n---\n",
        )]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert!(store.diagnostics().iter().any(|diagnostic| matches!(
            &diagnostic.body,
            DiagnosticBody::Item(item)
                if matches!(&item.kind, ItemDiagnosticKind::BrokenLink { target_id, .. } if target_id == "nonexistent")
        )));
    }

    #[test]
    fn broken_links_field_detected() {
        let (_dir, path) = setup_items_dir(vec![(
            "task-a.md",
            "---\ntitle: Task A\nstatus: open\ndepends_on: [missing-x, missing-y]\n---\n",
        )]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        let broken: Vec<_> = store
            .diagnostics()
            .iter()
            .filter(|diagnostic| {
                matches!(
                    &diagnostic.body,
                    DiagnosticBody::Item(item)
                        if matches!(&item.kind, ItemDiagnosticKind::BrokenLink { .. })
                )
            })
            .collect();
        assert_eq!(broken.len(), 2);
    }

    #[test]
    fn valid_link_no_error() {
        let (_dir, path) = setup_items_dir(vec![
            ("epic.md", "---\ntitle: Epic\nstatus: open\n---\n"),
            (
                "task-a.md",
                "---\ntitle: Task\nstatus: open\nparent: epic\n---\n",
            ),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert!(!store.diagnostics().iter().any(|diagnostic| matches!(
            &diagnostic.body,
            DiagnosticBody::Item(item)
                if matches!(&item.kind, ItemDiagnosticKind::BrokenLink { .. })
        )));
    }

    // ── Inverse relations ────────────────────────────────────────────

    #[test]
    fn inverse_parent_to_children() {
        let (_dir, path) = setup_items_dir(vec![
            ("epic.md", "---\ntitle: Epic\nstatus: open\n---\n"),
            (
                "task-a.md",
                "---\ntitle: Task A\nstatus: open\nparent: epic\n---\n",
            ),
            (
                "task-b.md",
                "---\ntitle: Task B\nstatus: done\nparent: epic\n---\n",
            ),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        let children = store.referring_items("epic", "parent");
        assert_eq!(children.len(), 2);

        assert!(children.iter().any(|item| item.id == "task-a"));
        assert!(children.iter().any(|item| item.id == "task-b"));
    }

    #[test]
    fn inverse_links_field() {
        let (_dir, path) = setup_items_dir(vec![
            ("task-a.md", "---\ntitle: A\nstatus: open\n---\n"),
            ("task-b.md", "---\ntitle: B\nstatus: open\n---\n"),
            (
                "task-c.md",
                "---\ntitle: C\nstatus: open\ndepends_on: [task-a, task-b]\n---\n",
            ),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        let dependents_a = store.referring_items("task-a", "depends_on");
        assert_eq!(dependents_a.len(), 1);
        assert!(dependents_a[0].id == "task-c");

        let dependents_b = store.referring_items("task-b", "depends_on");
        assert_eq!(dependents_b.len(), 1);
        assert!(dependents_b[0].id == "task-c");
    }

    #[test]
    fn referring_items_empty_when_no_links() {
        let (_dir, path) =
            setup_items_dir(vec![("task-a.md", "---\ntitle: A\nstatus: open\n---\n")]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert!(store.referring_items("task-a", "parent").is_empty());
    }

    // ── Partial load (mix of valid and invalid) ──────────────────────

    #[test]
    fn partial_load_keeps_good_items() {
        let (_dir, path) = setup_items_dir(vec![
            ("good.md", "---\ntitle: Good\nstatus: open\n---\n"),
            ("bad-yaml.md", "---\n: invalid: yaml:\n---\n"),
            ("no-frontmatter.md", "just text, no delimiters"),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert_eq!(store.len(), 1);
        assert!(store.get("good").is_some());

        let parse_errors: Vec<_> = store
            .diagnostics()
            .iter()
            .filter(|diagnostic| matches!(&diagnostic.body, DiagnosticBody::File(_)))
            .collect();
        assert_eq!(parse_errors.len(), 2);
    }

    // ── Aggregate rollup integration ────────────────────────────────

    /// Schema with `parent: link` and an `effort: integer` field that
    /// aggregates as `sum` up the parent chain.
    fn schema_with_effort_sum() -> Schema {
        use crate::model::schema::{AggregateConfig, AggregateFunction};

        let mut fields = IndexMap::new();
        fields.insert(
            "title".to_owned(),
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        );
        fields.insert(
            "parent".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: Some(false),
                inverse: Some("children".into()),
            }),
        );
        let mut effort = FieldDefinition::new(FieldTypeConfig::Integer {
            min: None,
            max: None,
        });
        effort.aggregate = Some(AggregateConfig {
            function: AggregateFunction::Sum,
            error_on_missing: false,
            over: None,
        });
        fields.insert("effort".to_owned(), effort);

        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    #[test]
    fn rollup_fills_parent_with_aggregated_value_after_load() {
        let (_dir, path) = setup_items_dir(vec![
            ("epic.md", "---\ntitle: Epic\n---\n"),
            (
                "task-a.md",
                "---\ntitle: Task A\nparent: epic\neffort: 2\n---\n",
            ),
            (
                "task-b.md",
                "---\ntitle: Task B\nparent: epic\neffort: 3\n---\n",
            ),
        ]);
        let store = Store::load(&path, &schema_with_effort_sum()).unwrap();

        let epic = store.get("epic").unwrap();
        assert_eq!(epic.fields.get("effort"), Some(&FieldValue::Integer(5)));
        // Leaves keep their manual values.
        assert_eq!(
            store.get("task-a").unwrap().fields.get("effort"),
            Some(&FieldValue::Integer(2))
        );
        assert!(!store.has_diagnostics(), "{:#?}", store.diagnostics());
    }

    // ── all_items ────────────────────────────────────────────────────

    #[test]
    fn all_items_iterates_loaded() {
        let (_dir, path) = setup_items_dir(vec![
            ("task-a.md", "---\ntitle: A\nstatus: open\n---\n"),
            ("task-b.md", "---\ntitle: B\nstatus: done\n---\n"),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        let items: Vec<_> = store.all_items().collect();
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.id == "task-a"));
        assert!(items.iter().any(|item| item.id == "task-b"));
    }
}
