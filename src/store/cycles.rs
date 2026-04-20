//! Cycle detection for link fields.
//!
//! Uses DFS with white/gray/black coloring to find cycles in directed graphs
//! formed by `Link` and `Links` fields where `allow_cycles` is `false`.

use std::collections::HashMap;

use crate::model::diagnostic::{Diagnostic, DiagnosticKind};
use crate::model::schema::{FieldTypeConfig, Schema, Severity};
use crate::model::{FieldValue, WorkItemId};

use super::Store;

// ── Public entry point ──────────────────────────────────────────────

/// Detect cycles in all link fields where `allow_cycles` is `false`.
///
/// Returns one [`Diagnostic`] per unique cycle found, with the chain
/// canonicalized to start at the lexicographically smallest ID.
pub(crate) fn detect_cycles(store: &Store, schema: &Schema) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (field_name, field_def) in &schema.fields {
        let allow_cycles = match &field_def.type_config {
            FieldTypeConfig::Link { allow_cycles, .. }
            | FieldTypeConfig::Links { allow_cycles, .. } => allow_cycles,
            _ => continue,
        };
        if *allow_cycles != Some(false) {
            continue;
        }
        detect_cycles_in_field(store, field_name, &mut diagnostics);
    }

    diagnostics
}

// ── Per-field DFS ───────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Color {
    White,
    Gray,
    Black,
}

fn detect_cycles_in_field(store: &Store, field_name: &str, diagnostics: &mut Vec<Diagnostic>) {
    let items = store.items_map();

    let mut color: HashMap<&str, Color> =
        items.keys().map(|id| (id.as_str(), Color::White)).collect();

    // Sort for deterministic traversal order.
    let mut ids: Vec<&str> = color.keys().copied().collect();
    ids.sort();

    let mut path: Vec<WorkItemId> = Vec::new();

    for id in ids {
        if color[id] == Color::White {
            dfs(store, field_name, id, &mut color, &mut path, diagnostics);
        }
    }
}

fn dfs<'store>(
    store: &'store Store,
    field_name: &str,
    node: &'store str,
    color: &mut HashMap<&'store str, Color>,
    path: &mut Vec<WorkItemId>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    color.insert(node, Color::Gray);
    path.push(WorkItemId::from(node.to_owned()));

    for target in targets(store, node, field_name) {
        // Skip broken links (already reported by Store::load).
        if store.get(target).is_none() {
            continue;
        }

        match color.get(target) {
            Some(Color::White) => {
                dfs(store, field_name, target, color, path, diagnostics);
            }
            Some(Color::Gray) => {
                let start = path.iter().position(|id| id == target).unwrap();
                let chain = canonicalize_cycle(&path[start..]);
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    kind: DiagnosticKind::Cycle {
                        field: field_name.to_owned(),
                        chain,
                    },
                });
            }
            Some(Color::Black) | None => {}
        }
    }

    color.insert(node, Color::Black);
    path.pop();
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Get the target IDs for a node's field value.
fn targets<'a>(store: &'a Store, node_id: &str, field_name: &str) -> Vec<&'a str> {
    store
        .get(node_id)
        .and_then(|item| item.fields.get(field_name))
        .map(|field_value| match field_value {
            FieldValue::Link(t) => vec![t.as_str()],
            FieldValue::Links(ts) => ts.iter().map(|id| id.as_str()).collect(),
            _ => vec![],
        })
        .unwrap_or_default()
}

/// Canonicalize a cycle so it starts at the lexicographically smallest ID.
///
/// Input: `["b", "c", "a"]` (the cycle body from the DFS path).
/// Output: `["a", "b", "c", "a"]` (rotated + closed).
fn canonicalize_cycle(cycle_body: &[WorkItemId]) -> Vec<WorkItemId> {
    let min_pos = cycle_body
        .iter()
        .enumerate()
        .min_by_key(|(_, id)| id.as_str())
        .map(|(i, _)| i)
        .unwrap_or(0);

    let len = cycle_body.len();
    let mut result = Vec::with_capacity(len + 1);
    for i in 0..len {
        result.push(cycle_body[(min_pos + i) % len].clone());
    }
    result.push(result[0].clone());
    result
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use indexmap::IndexMap;
    use std::fs;
    use std::path::PathBuf;

    // ── Test helpers (mirrored from store/mod.rs tests) ─────────────

    fn setup_items_dir(items: Vec<(&str, &str)>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let items_path = dir.path().to_path_buf();
        for (filename, content) in items {
            fs::write(items_path.join(filename), content).expect("failed to write test file");
        }
        (dir, items_path)
    }

    fn link_field(allow_cycles: bool) -> FieldDefinition {
        FieldDefinition::new(FieldTypeConfig::Link {
            allow_cycles: Some(allow_cycles),
            inverse: None,
        })
    }

    fn links_field(allow_cycles: bool) -> FieldDefinition {
        FieldDefinition::new(FieldTypeConfig::Links {
            allow_cycles: Some(allow_cycles),
            inverse: None,
        })
    }

    fn schema_with(fields: Vec<(&str, FieldDefinition)>) -> Schema {
        let mut map = IndexMap::new();
        // Always include title + status so items parse cleanly.
        map.insert(
            "title".to_owned(),
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        );
        let mut status = FieldDefinition::new(FieldTypeConfig::Choice {
            values: vec!["open".into(), "done".into()],
        });
        status.required = true;
        map.insert("status".to_owned(), status);
        for (name, def) in fields {
            map.insert(name.to_owned(), def);
        }
        let inverse_table = Schema::build_inverse_table(&map);
        Schema {
            fields: map,
            rules: vec![],
            inverse_table,
        }
    }

    /// Convert a list of string slices into a `Vec<WorkItemId>` for test assertions.
    fn ids(strs: &[&str]) -> Vec<WorkItemId> {
        strs.iter()
            .map(|s| WorkItemId::from(s.to_string()))
            .collect()
    }

    // ── Tests ───────────────────────────────────────────────────────

    #[test]
    fn no_link_fields_returns_empty() {
        let schema = schema_with(vec![]);
        let (_dir, path) = setup_items_dir(vec![("a.md", "---\nstatus: open\n---\n")]);
        let store = Store::load(&path, &schema).unwrap();
        assert!(detect_cycles(&store, &schema).is_empty());
    }

    #[test]
    fn acyclic_chain_returns_empty() {
        let schema = schema_with(vec![("parent", link_field(false))]);
        let (_dir, path) = setup_items_dir(vec![
            ("a.md", "---\nstatus: open\n---\n"),
            ("b.md", "---\nstatus: open\nparent: a\n---\n"),
            ("c.md", "---\nstatus: open\nparent: b\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        assert!(detect_cycles(&store, &schema).is_empty());
    }

    #[test]
    fn two_node_cycle() {
        let schema = schema_with(vec![("parent", link_field(false))]);
        let (_dir, path) = setup_items_dir(vec![
            ("a.md", "---\nstatus: open\nparent: b\n---\n"),
            ("b.md", "---\nstatus: open\nparent: a\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = detect_cycles(&store, &schema);

        assert_eq!(diagnostics.len(), 1);
        match &diagnostics[0].kind {
            DiagnosticKind::Cycle { field, chain } => {
                assert_eq!(field, "parent");
                assert_eq!(*chain, ids(&["a", "b", "a"]));
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn self_loop() {
        let schema = schema_with(vec![("parent", link_field(false))]);
        let (_dir, path) = setup_items_dir(vec![("a.md", "---\nstatus: open\nparent: a\n---\n")]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = detect_cycles(&store, &schema);

        assert_eq!(diagnostics.len(), 1);
        match &diagnostics[0].kind {
            DiagnosticKind::Cycle { field, chain } => {
                assert_eq!(field, "parent");
                assert_eq!(*chain, ids(&["a", "a"]));
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn three_node_cycle_canonicalized() {
        let schema = schema_with(vec![("parent", link_field(false))]);
        let (_dir, path) = setup_items_dir(vec![
            ("c.md", "---\nstatus: open\nparent: b\n---\n"),
            ("b.md", "---\nstatus: open\nparent: a\n---\n"),
            ("a.md", "---\nstatus: open\nparent: c\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = detect_cycles(&store, &schema);

        assert_eq!(diagnostics.len(), 1);
        match &diagnostics[0].kind {
            DiagnosticKind::Cycle { chain, .. } => {
                assert_eq!(*chain, ids(&["a", "c", "b", "a"]));
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn links_field_cycle() {
        let schema = schema_with(vec![("depends_on", links_field(false))]);
        let (_dir, path) = setup_items_dir(vec![
            ("a.md", "---\nstatus: open\ndepends_on: [b]\n---\n"),
            ("b.md", "---\nstatus: open\ndepends_on: [c]\n---\n"),
            ("c.md", "---\nstatus: open\ndepends_on: [a]\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = detect_cycles(&store, &schema);

        assert_eq!(diagnostics.len(), 1);
        match &diagnostics[0].kind {
            DiagnosticKind::Cycle { field, chain } => {
                assert_eq!(field, "depends_on");
                assert_eq!(*chain, ids(&["a", "b", "c", "a"]));
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn two_independent_cycles() {
        let schema = schema_with(vec![("parent", link_field(false))]);
        let (_dir, path) = setup_items_dir(vec![
            ("a.md", "---\nstatus: open\nparent: b\n---\n"),
            ("b.md", "---\nstatus: open\nparent: a\n---\n"),
            ("c.md", "---\nstatus: open\nparent: d\n---\n"),
            ("d.md", "---\nstatus: open\nparent: c\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = detect_cycles(&store, &schema);

        assert_eq!(diagnostics.len(), 2);
        let mut chains: Vec<Vec<WorkItemId>> = diagnostics
            .iter()
            .map(|diagnostic| match &diagnostic.kind {
                DiagnosticKind::Cycle { chain, .. } => chain.clone(),
                other => panic!("expected Cycle, got {other:?}"),
            })
            .collect();
        chains.sort_by(|a, b| a[0].as_str().cmp(b[0].as_str()));
        assert_eq!(chains[0], ids(&["a", "b", "a"]));
        assert_eq!(chains[1], ids(&["c", "d", "c"]));
    }

    #[test]
    fn broken_link_stops_traversal() {
        let schema = schema_with(vec![("parent", link_field(false))]);
        let (_dir, path) = setup_items_dir(vec![
            ("a.md", "---\nstatus: open\nparent: b\n---\n"),
            ("b.md", "---\nstatus: open\nparent: nonexistent\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let cycle_diags: Vec<_> = detect_cycles(&store, &schema)
            .into_iter()
            .filter(|diagnostic| matches!(diagnostic.kind, DiagnosticKind::Cycle { .. }))
            .collect();
        assert!(cycle_diags.is_empty());
    }

    #[test]
    fn allow_cycles_true_skipped() {
        let schema = schema_with(vec![("related_to", links_field(true))]);
        let (_dir, path) = setup_items_dir(vec![
            ("a.md", "---\nstatus: open\nrelated_to: [b]\n---\n"),
            ("b.md", "---\nstatus: open\nrelated_to: [a]\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        assert!(detect_cycles(&store, &schema).is_empty());
    }

    #[test]
    fn allow_cycles_none_skipped() {
        let schema = schema_with(vec![(
            "custom_link",
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: None,
                inverse: None,
            }),
        )]);
        let (_dir, path) = setup_items_dir(vec![
            ("a.md", "---\nstatus: open\ncustom_link: b\n---\n"),
            ("b.md", "---\nstatus: open\ncustom_link: a\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        assert!(detect_cycles(&store, &schema).is_empty());
    }

    #[test]
    fn empty_store() {
        let schema = schema_with(vec![("parent", link_field(false))]);
        let (_dir, path) = setup_items_dir(vec![]);
        let store = Store::load(&path, &schema).unwrap();
        assert!(detect_cycles(&store, &schema).is_empty());
    }

    #[test]
    fn cycle_in_one_field_not_another() {
        let schema = schema_with(vec![
            ("parent", link_field(false)),
            ("depends_on", links_field(false)),
        ]);
        let (_dir, path) = setup_items_dir(vec![
            (
                "a.md",
                "---\nstatus: open\nparent: b\ndepends_on: [b]\n---\n",
            ),
            ("b.md", "---\nstatus: open\nparent: a\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = detect_cycles(&store, &schema);

        assert_eq!(diagnostics.len(), 1);
        match &diagnostics[0].kind {
            DiagnosticKind::Cycle { field, .. } => assert_eq!(field, "parent"),
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn node_with_cyclic_and_acyclic_edges() {
        let schema = schema_with(vec![("depends_on", links_field(false))]);
        let (_dir, path) = setup_items_dir(vec![
            ("a.md", "---\nstatus: open\ndepends_on: [b, c]\n---\n"),
            ("b.md", "---\nstatus: open\ndepends_on: [a]\n---\n"),
            ("c.md", "---\nstatus: open\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = detect_cycles(&store, &schema);

        assert_eq!(diagnostics.len(), 1);
        match &diagnostics[0].kind {
            DiagnosticKind::Cycle { field, chain } => {
                assert_eq!(field, "depends_on");
                assert_eq!(*chain, ids(&["a", "b", "a"]));
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn all_cycle_diagnostics_are_errors() {
        let schema = schema_with(vec![("parent", link_field(false))]);
        let (_dir, path) = setup_items_dir(vec![
            ("a.md", "---\nstatus: open\nparent: b\n---\n"),
            ("b.md", "---\nstatus: open\nparent: a\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = detect_cycles(&store, &schema);

        for diagnostic in &diagnostics {
            assert_eq!(diagnostic.severity, Severity::Error);
        }
    }
}
