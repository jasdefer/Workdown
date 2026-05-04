//! Shared fixture builders for view_data extractor unit tests.

#![cfg(test)]

use std::collections::HashMap;
use std::path::PathBuf;

use indexmap::IndexMap;

use crate::model::schema::{FieldDefinition, FieldTypeConfig, Schema};
use crate::model::{FieldValue, WorkItem, WorkItemId};
use crate::store::Store;

pub(super) fn make_item(id: &str, fields: Vec<(&str, FieldValue)>, body: &str) -> WorkItem {
    let mut map = HashMap::new();
    for (name, value) in fields {
        map.insert(name.to_owned(), value);
    }
    WorkItem {
        id: WorkItemId::from(id.to_owned()),
        fields: map,
        body: body.to_owned(),
        source_path: PathBuf::from(format!("{id}.md")),
    }
}

pub(super) fn make_schema(fields: Vec<(&str, FieldTypeConfig)>) -> Schema {
    let mut map = IndexMap::new();
    for (name, config) in fields {
        map.insert(name.to_owned(), FieldDefinition::new(config));
    }
    let inverse_table = Schema::build_inverse_table(&map);
    Schema {
        fields: map,
        rules: vec![],
        inverse_table,
    }
}

pub(super) fn make_store(schema: &Schema, items: Vec<WorkItem>) -> Store {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let mut store = Store::load(temp_dir.path(), schema).expect("empty store loads");
    for item in items {
        store.insert(item);
    }
    store
}

/// Write Markdown files to a tempdir and return a Store with accurate
/// reverse links. Required for tests that rely on
/// [`Store::referring_items`] — [`make_store`] uses `insert()` which
/// skips reverse-link computation.
///
/// The returned [`tempfile::TempDir`] must be kept alive for the duration
/// of the test; dropping it deletes the underlying files.
pub(super) fn make_store_with_files(
    schema: &Schema,
    files: Vec<(&str, &str)>,
) -> (tempfile::TempDir, Store) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    for (name, content) in files {
        std::fs::write(temp_dir.path().join(name), content).expect("write test file");
    }
    let store = Store::load(temp_dir.path(), schema).expect("store load");
    (temp_dir, store)
}
