//! Work item store: scanning, indexing, querying, and relationship management.
//!
//! The [`Store`] is the central data layer. It scans a directory of work item
//! Markdown files, parses and coerces them against the project schema, checks
//! ID uniqueness, detects broken links, and pre-computes inverse relations.
//!
//! Individual item errors (bad YAML, type mismatches, missing fields) are
//! collected — the store loads as much as it can and reports all problems.

mod coerce;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::schema::{FieldType, Schema};
use crate::model::{FieldValue, WorkItem};
use crate::parser::{self, ParseError};

// ── Errors ───────────────────────────────────────────────────────────

/// An error encountered while loading work items into the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// A work item file could not be parsed at all (fatal for that file).
    #[error("{}: {error}", path.display())]
    Parse { path: PathBuf, error: ParseError },

    /// A field value could not be coerced to the expected type.
    #[error("item '{item_id}', field '{field}': {error}")]
    Coercion {
        item_id: String,
        field: String,
        error: CoercionError,
    },

    /// A field in the frontmatter is not defined in the schema.
    #[error("item '{item_id}': unknown field '{field}'")]
    UnknownField { item_id: String, field: String },

    /// A required field is missing from the frontmatter.
    #[error("item '{item_id}': required field '{field}' is missing")]
    MissingRequired { item_id: String, field: String },

    /// Two or more files resolved to the same ID.
    #[error("duplicate ID '{id}': {}", format_paths(paths))]
    DuplicateId { id: String, paths: Vec<PathBuf> },

    /// A link/links field references an ID that doesn't exist.
    #[error("item '{source_id}', field '{field}': broken link to '{target_id}'")]
    BrokenLink {
        source_id: String,
        field: String,
        target_id: String,
    },
}

/// An error from coercing a single field value.
#[derive(Debug, thiserror::Error)]
pub enum CoercionError {
    #[error("expected {expected}, got {got}")]
    TypeMismatch { expected: FieldType, got: String },

    #[error("'{value}' is not one of the allowed values: {allowed:?}")]
    InvalidChoice { value: String, allowed: Vec<String> },

    #[error("invalid values {values:?}, allowed: {allowed:?}")]
    InvalidMultichoice {
        values: Vec<String>,
        allowed: Vec<String>,
    },

    #[error("{value} is out of range (min: {min:?}, max: {max:?})")]
    OutOfRange {
        value: f64,
        min: Option<f64>,
        max: Option<f64>,
    },

    #[error("'{value}' is not a valid date (expected YYYY-MM-DD)")]
    InvalidDate { value: String },

    #[error("'{value}' does not match pattern '{pattern}'")]
    PatternMismatch { value: String, pattern: String },

    #[error("invalid regex pattern '{pattern}': {error}")]
    InvalidPattern { pattern: String, error: String },
}

fn format_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

// ── Store ────────────────────────────────────────────────────────────

/// An in-memory index of all work items in a project.
///
/// Built by [`Store::load`], which scans a directory, parses files, coerces
/// fields, checks uniqueness, and derives inverse relations. Errors are
/// collected rather than aborting — query the store even if some items failed.
pub struct Store {
    /// Work items indexed by ID.
    items: HashMap<String, WorkItem>,
    /// Pre-computed reverse links: `field_name → target_id → [source_ids]`.
    ///
    /// For example, if item "login-task" has `parent: auth-epic`, then
    /// `reverse_links["parent"]["auth-epic"]` contains `"login-task"`.
    reverse_links: HashMap<String, HashMap<String, Vec<String>>>,
    /// All errors encountered during loading.
    errors: Vec<StoreError>,
}

impl Store {
    /// Scan `items_dir` for `.md` files and load them into the store.
    ///
    /// Only returns `Err` if the directory itself cannot be read.
    /// Per-file and per-field errors are collected in [`Store::errors`].
    pub fn load(items_dir: &Path, schema: &Schema) -> Result<Store, std::io::Error> {
        let mut errors = Vec::new();

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
        let mut seen_ids: HashMap<String, PathBuf> = HashMap::new();

        for path in &paths {
            let raw = match parser::parse_work_item_file(path) {
                Ok(raw) => raw,
                Err(e) => {
                    errors.push(StoreError::Parse {
                        path: path.clone(),
                        error: e,
                    });
                    continue;
                }
            };

            // Check for duplicate IDs.
            if let Some(first_path) = seen_ids.get(&raw.id) {
                errors.push(StoreError::DuplicateId {
                    id: raw.id.clone(),
                    paths: vec![first_path.clone(), path.clone()],
                });
                continue;
            }
            seen_ids.insert(raw.id.clone(), path.clone());

            // 3. Coerce fields.
            let (fields, coercion_errors) = coerce::coerce_fields(&raw, schema);
            errors.extend(coercion_errors);

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
        let mut reverse_links: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();

        for item in items.values() {
            for (field_name, field_value) in &item.fields {
                let targets: Vec<&str> = match field_value {
                    FieldValue::Link(target) => vec![target.as_str()],
                    FieldValue::Links(targets) => targets.iter().map(|s| s.as_str()).collect(),
                    _ => continue,
                };

                for target_id in targets {
                    if !items.contains_key(target_id) {
                        errors.push(StoreError::BrokenLink {
                            source_id: item.id.clone(),
                            field: field_name.clone(),
                            target_id: target_id.to_owned(),
                        });
                    }

                    reverse_links
                        .entry(field_name.clone())
                        .or_default()
                        .entry(target_id.to_owned())
                        .or_default()
                        .push(item.id.clone());
                }
            }
        }

        Ok(Store {
            items,
            reverse_links,
            errors,
        })
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

    /// All errors encountered during loading.
    pub fn errors(&self) -> &[StoreError] {
        &self.errors
    }

    /// Whether any errors were encountered during loading.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Number of successfully loaded items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the store contains no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::FieldDef;
    use indexmap::IndexMap;
    use std::fs;

    /// Build a minimal schema for store tests.
    fn test_schema() -> Schema {
        let mut fields = IndexMap::new();

        fields.insert(
            "title".to_owned(),
            FieldDef {
                field_type: FieldType::String,
                description: None,
                required: false,
                default: None,
                values: None,
                pattern: None,
                min: None,
                max: None,
                allow_cycles: None,
                resource: None,
                aggregate: None,
            },
        );

        fields.insert(
            "status".to_owned(),
            FieldDef {
                field_type: FieldType::Choice,
                description: None,
                required: true,
                default: None,
                values: Some(vec!["open".into(), "in_progress".into(), "done".into()]),
                pattern: None,
                min: None,
                max: None,
                allow_cycles: None,
                resource: None,
                aggregate: None,
            },
        );

        fields.insert(
            "parent".to_owned(),
            FieldDef {
                field_type: FieldType::Link,
                description: None,
                required: false,
                default: None,
                values: None,
                pattern: None,
                min: None,
                max: None,
                allow_cycles: Some(false),
                resource: None,
                aggregate: None,
            },
        );

        fields.insert(
            "depends_on".to_owned(),
            FieldDef {
                field_type: FieldType::Links,
                description: None,
                required: false,
                default: None,
                values: None,
                pattern: None,
                min: None,
                max: None,
                allow_cycles: Some(false),
                resource: None,
                aggregate: None,
            },
        );

        fields.insert(
            "tags".to_owned(),
            FieldDef {
                field_type: FieldType::List,
                description: None,
                required: false,
                default: None,
                values: None,
                pattern: None,
                min: None,
                max: None,
                allow_cycles: None,
                resource: None,
                aggregate: None,
            },
        );

        Schema {
            fields,
            rules: vec![],
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
        assert!(!store.has_errors());

        let a = store.get("task-a").unwrap();
        assert_eq!(a.fields["title"], FieldValue::String("Task A".into()));
        assert_eq!(a.fields["status"], FieldValue::Choice("open".into()));

        let b = store.get("task-b").unwrap();
        assert_eq!(b.fields["status"], FieldValue::Choice("done".into()));
    }

    #[test]
    fn load_empty_directory() {
        let (_dir, path) = setup_items_dir(vec![]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert!(store.is_empty());
        assert!(!store.has_errors());
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
        assert!(!store.has_errors());
    }

    // ── Error collection ─────────────────────────────────────────────

    #[test]
    fn parse_error_skips_file_collects_error() {
        let (_dir, path) = setup_items_dir(vec![
            ("good.md", "---\ntitle: Good\nstatus: open\n---\n"),
            ("bad.md", "no frontmatter here"),
        ]);
        let schema = test_schema();
        let store = Store::load(&path, &schema).unwrap();

        assert_eq!(store.len(), 1);
        assert!(store.get("good").is_some());
        assert!(store.has_errors());
        assert!(store
            .errors()
            .iter()
            .any(|e| matches!(e, StoreError::Parse { .. })));
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
        assert!(store.errors().iter().any(|e| matches!(
            e,
            StoreError::MissingRequired { field, .. } if field == "status"
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
        assert!(store.errors().iter().any(|e| matches!(
            e,
            StoreError::UnknownField { field, .. } if field == "bogus"
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

        assert!(store.errors().iter().any(|e| matches!(
            e,
            StoreError::DuplicateId { id, .. } if id == "task-a"
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

        assert!(store.errors().iter().any(|e| matches!(
            e,
            StoreError::BrokenLink { target_id, .. } if target_id == "nonexistent"
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
            .errors()
            .iter()
            .filter(|e| matches!(e, StoreError::BrokenLink { .. }))
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

        assert!(!store
            .errors()
            .iter()
            .any(|e| matches!(e, StoreError::BrokenLink { .. })));
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

        let child_ids: Vec<&str> = children.iter().map(|i| i.id.as_str()).collect();
        assert!(child_ids.contains(&"task-a"));
        assert!(child_ids.contains(&"task-b"));
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
        assert_eq!(dependents_a[0].id, "task-c");

        let dependents_b = store.referring_items("task-b", "depends_on");
        assert_eq!(dependents_b.len(), 1);
        assert_eq!(dependents_b[0].id, "task-c");
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
            .errors()
            .iter()
            .filter(|e| matches!(e, StoreError::Parse { .. }))
            .collect();
        assert_eq!(parse_errors.len(), 2);
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

        let ids: Vec<&str> = store.all_items().map(|i| i.id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"task-a"));
        assert!(ids.contains(&"task-b"));
    }
}
