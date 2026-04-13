//! Field reference resolution: plain fields and dot-notation traversal.
//!
//! Resolves field references like `"status"` (plain) or `"parent.status"`
//! (dot-notation forward) or `"children.type"` (dot-notation inverse)
//! against a work item and the store.

use crate::model::schema::FieldType;
use crate::model::{FieldValue, WorkItem};

use super::EvalContext;

// ── Resolved values ─────────────────────────────────────────────────

/// The result of resolving a field reference.
#[derive(Debug)]
pub(crate) enum ResolvedValues<'a> {
    /// A single value from the current item or a single related item.
    Single(Option<&'a FieldValue>),
    /// Multiple values from a one-to-many traversal (links field or inverse).
    Many(Vec<Option<&'a FieldValue>>),
}

// ── Field reference resolution ──────────────────────────────────────

/// Resolve a field reference against an item and the store.
///
/// - Plain reference (`"status"`): returns `Single(item.fields.get("status"))`.
/// - Dot-notation forward via link (`"parent.status"`): follows the link,
///   reads the field on the target item. Returns `Single`.
/// - Dot-notation forward via links (`"depends_on.status"`): follows each
///   target, reads the field. Returns `Many`.
/// - Dot-notation via inverse (`"children.type"`): finds items linking to
///   this one, reads the field on each. Returns `Many`.
pub(crate) fn resolve_field_ref<'a>(
    item: &'a WorkItem,
    reference: &str,
    ctx: &'a EvalContext<'a>,
) -> ResolvedValues<'a> {
    let parts: Vec<&str> = reference.split('.').collect();

    if parts.len() == 1 {
        // Plain field reference
        return ResolvedValues::Single(item.fields.get(parts[0]));
    }

    let relationship = parts[0];
    let field_name = parts[1];

    // Check if relationship is a forward link/links field
    if let Some(field_def) = ctx.schema.fields.get(relationship) {
        match field_def.field_type {
            FieldType::Link => {
                // Follow the single link
                let target_value = item
                    .fields
                    .get(relationship)
                    .and_then(|fv| match fv {
                        FieldValue::Link(target_id) => ctx.store.get(target_id),
                        _ => None,
                    })
                    .and_then(|target| target.fields.get(field_name));
                return ResolvedValues::Single(target_value);
            }
            FieldType::Links => {
                // Follow multiple links
                let values = match item.fields.get(relationship) {
                    Some(FieldValue::Links(target_ids)) => target_ids
                        .iter()
                        .filter_map(|id| ctx.store.get(id))
                        .map(|target| target.fields.get(field_name))
                        .collect(),
                    _ => vec![],
                };
                return ResolvedValues::Many(values);
            }
            _ => {
                // Not a link field — shouldn't happen (parser validates),
                // but defensively return null.
                return ResolvedValues::Single(None);
            }
        }
    }

    // Check if relationship is an inverse name
    if let Some(original_field) = ctx.inverse_table.get(relationship) {
        let related = ctx.store.referring_items(&item.id, original_field);
        let values = related
            .iter()
            .map(|rel_item| rel_item.fields.get(field_name))
            .collect();
        return ResolvedValues::Many(values);
    }

    // Unknown reference — shouldn't happen (parser validates). Defensively null.
    ResolvedValues::Single(None)
}

/// Resolve a bare relationship reference to related work items.
///
/// Used for count-based assertions like `children: { min_count: 1 }` where
/// the reference is an inverse or links field name without a dot-notation
/// field access.
pub(crate) fn resolve_related_items<'a>(
    item: &'a WorkItem,
    reference: &str,
    ctx: &'a EvalContext<'a>,
) -> Vec<&'a WorkItem> {
    // Bare reference — no dot. Check if it's a links field or inverse.
    if let Some(field_def) = ctx.schema.fields.get(reference) {
        if field_def.field_type == FieldType::Links {
            return match item.fields.get(reference) {
                Some(FieldValue::Links(ids)) => {
                    ids.iter().filter_map(|id| ctx.store.get(id)).collect()
                }
                _ => vec![],
            };
        }
    }

    // Check inverse table
    if let Some(original_field) = ctx.inverse_table.get(reference) {
        return ctx.store.referring_items(&item.id, original_field);
    }

    vec![]
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDef, FieldType, Schema};
    use crate::store::Store;
    use indexmap::IndexMap;
    use std::fs;
    use std::path::PathBuf;

    fn base_field(ft: FieldType) -> FieldDef {
        FieldDef {
            field_type: ft,
            description: None,
            required: false,
            default: None,
            values: None,
            pattern: None,
            min: None,
            max: None,
            allow_cycles: None,
            inverse: None,
            resource: None,
            aggregate: None,
        }
    }

    fn test_schema() -> Schema {
        let mut fields = IndexMap::new();
        fields.insert("title".to_owned(), base_field(FieldType::String));
        fields.insert(
            "status".to_owned(),
            FieldDef {
                required: true,
                values: Some(vec!["open".into(), "done".into()]),
                ..base_field(FieldType::Choice)
            },
        );
        fields.insert(
            "type_field".to_owned(),
            FieldDef {
                values: Some(vec!["task".into(), "epic".into()]),
                ..base_field(FieldType::Choice)
            },
        );
        fields.insert(
            "parent".to_owned(),
            FieldDef {
                allow_cycles: Some(false),
                inverse: Some("children".into()),
                ..base_field(FieldType::Link)
            },
        );
        fields.insert(
            "depends_on".to_owned(),
            FieldDef {
                allow_cycles: Some(false),
                inverse: Some("dependents".into()),
                ..base_field(FieldType::Links)
            },
        );
        Schema {
            fields,
            rules: vec![],
        }
    }

    fn setup_items(items: Vec<(&str, &str)>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        for (name, content) in items {
            fs::write(path.join(name), content).unwrap();
        }
        (dir, path)
    }

    #[test]
    fn resolve_plain_field() {
        let schema = test_schema();
        let (_dir, path) = setup_items(vec![(
            "task-a.md",
            "---\ntitle: A\nstatus: open\n---\n",
        )]);
        let store = Store::load(&path, &schema).unwrap();
        let ctx = EvalContext::new(&store, &schema);
        let item = store.get("task-a").unwrap();

        match resolve_field_ref(item, "status", &ctx) {
            ResolvedValues::Single(Some(FieldValue::Choice(v))) => assert_eq!(v, "open"),
            other => panic!("expected Single(Choice), got: {other:?}"),
        }
    }

    #[test]
    fn resolve_plain_field_absent() {
        let schema = test_schema();
        let (_dir, path) = setup_items(vec![(
            "task-a.md",
            "---\nstatus: open\n---\n",
        )]);
        let store = Store::load(&path, &schema).unwrap();
        let ctx = EvalContext::new(&store, &schema);
        let item = store.get("task-a").unwrap();

        match resolve_field_ref(item, "title", &ctx) {
            ResolvedValues::Single(None) => {}
            other => panic!("expected Single(None), got: {other:?}"),
        }
    }

    #[test]
    fn resolve_forward_link() {
        let schema = test_schema();
        let (_dir, path) = setup_items(vec![
            ("epic.md", "---\ntitle: Epic\nstatus: open\n---\n"),
            ("task-a.md", "---\ntitle: A\nstatus: done\nparent: epic\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let ctx = EvalContext::new(&store, &schema);
        let item = store.get("task-a").unwrap();

        match resolve_field_ref(item, "parent.status", &ctx) {
            ResolvedValues::Single(Some(FieldValue::Choice(v))) => assert_eq!(v, "open"),
            other => panic!("expected Single(Choice(open)), got: {other:?}"),
        }
    }

    #[test]
    fn resolve_forward_link_null() {
        let schema = test_schema();
        let (_dir, path) = setup_items(vec![(
            "task-a.md",
            "---\ntitle: A\nstatus: open\n---\n",
        )]);
        let store = Store::load(&path, &schema).unwrap();
        let ctx = EvalContext::new(&store, &schema);
        let item = store.get("task-a").unwrap();

        match resolve_field_ref(item, "parent.status", &ctx) {
            ResolvedValues::Single(None) => {}
            other => panic!("expected Single(None), got: {other:?}"),
        }
    }

    #[test]
    fn resolve_forward_links() {
        let schema = test_schema();
        let (_dir, path) = setup_items(vec![
            ("dep-a.md", "---\nstatus: open\n---\n"),
            ("dep-b.md", "---\nstatus: done\n---\n"),
            ("task.md", "---\nstatus: open\ndepends_on: [dep-a, dep-b]\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let ctx = EvalContext::new(&store, &schema);
        let item = store.get("task").unwrap();

        match resolve_field_ref(item, "depends_on.status", &ctx) {
            ResolvedValues::Many(values) => {
                assert_eq!(values.len(), 2);
                let strs: Vec<&str> = values
                    .iter()
                    .filter_map(|v| match v {
                        Some(FieldValue::Choice(s)) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect();
                assert!(strs.contains(&"open"));
                assert!(strs.contains(&"done"));
            }
            other => panic!("expected Many, got: {other:?}"),
        }
    }

    #[test]
    fn resolve_inverse() {
        let schema = test_schema();
        let (_dir, path) = setup_items(vec![
            ("epic.md", "---\nstatus: open\n---\n"),
            ("child-a.md", "---\nstatus: open\nparent: epic\n---\n"),
            ("child-b.md", "---\nstatus: done\nparent: epic\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let ctx = EvalContext::new(&store, &schema);
        let item = store.get("epic").unwrap();

        match resolve_field_ref(item, "children.status", &ctx) {
            ResolvedValues::Many(values) => {
                assert_eq!(values.len(), 2);
            }
            other => panic!("expected Many, got: {other:?}"),
        }
    }

    #[test]
    fn resolve_related_items_inverse() {
        let schema = test_schema();
        let (_dir, path) = setup_items(vec![
            ("epic.md", "---\nstatus: open\n---\n"),
            ("child-a.md", "---\nstatus: open\nparent: epic\n---\n"),
            ("child-b.md", "---\nstatus: done\nparent: epic\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let ctx = EvalContext::new(&store, &schema);
        let item = store.get("epic").unwrap();

        let related = resolve_related_items(item, "children", &ctx);
        assert_eq!(related.len(), 2);
    }

    #[test]
    fn resolve_related_items_links() {
        let schema = test_schema();
        let (_dir, path) = setup_items(vec![
            ("dep-a.md", "---\nstatus: open\n---\n"),
            ("dep-b.md", "---\nstatus: done\n---\n"),
            ("task.md", "---\nstatus: open\ndepends_on: [dep-a, dep-b]\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let ctx = EvalContext::new(&store, &schema);
        let item = store.get("task").unwrap();

        let related = resolve_related_items(item, "depends_on", &ctx);
        assert_eq!(related.len(), 2);
    }
}
