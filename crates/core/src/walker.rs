//! Graph-walk primitives over `Link` / `Links` fields.
//!
//! Two shapes:
//!
//! - **Upward chain walks** ([`walk_up`], [`walk_up_in`]) follow a single
//!   `Link` field from a starting item to the chain root, yielding each
//!   ancestor. Stops silently on missing field, non-`Link` value, missing
//!   target, or cycle (revisited id).
//! - **Target enumeration** ([`target_of_link`], [`targets_of`]) reads a
//!   field as a list of referenced ids, treating `Link` as a single-element
//!   list and `Links` as a multi-element list. Anything else yields empty.
//!
//! `walk_up` and `walk_up_in` differ only in the lookup source: the former
//! takes `&Store`, the latter a `&HashMap<WorkItemId, WorkItem>` for callers
//! (currently `store::rollup`) that need mutable access to the items map
//! in a later phase and so can't go through `Store`'s read-only API.

use std::collections::{HashMap, HashSet};

use crate::model::{FieldValue, WorkItem, WorkItemId};
use crate::store::Store;

// ── Upward chain walks ──────────────────────────────────────────────

/// Walk `field` upward from `start` through `store`, yielding each ancestor.
///
/// `start` is not yielded. Stops silently on missing field, non-`Link`
/// value, missing target, or cycle.
pub fn walk_up<'s>(
    start: &'s WorkItem,
    field: &'s str,
    store: &'s Store,
) -> impl Iterator<Item = &'s WorkItem> + 's {
    let mut visited: HashSet<&'s str> = HashSet::new();
    visited.insert(start.id.as_str());
    let mut current: &'s WorkItem = start;
    std::iter::from_fn(move || {
        let next_id = target_of_link(current, field)?.as_str();
        let next_item = store.get(next_id)?;
        if !visited.insert(next_item.id.as_str()) {
            return None;
        }
        current = next_item;
        Some(current)
    })
}

/// Same as [`walk_up`] but reads from a `HashMap<WorkItemId, WorkItem>`.
///
/// Used by `store::rollup`, which holds the items map directly because it
/// mutates entries in a later phase and can't go through `Store::get`.
pub fn walk_up_in<'s>(
    start: &'s WorkItem,
    field: &'s str,
    items: &'s HashMap<WorkItemId, WorkItem>,
) -> impl Iterator<Item = &'s WorkItem> + 's {
    let mut visited: HashSet<&'s str> = HashSet::new();
    visited.insert(start.id.as_str());
    let mut current: &'s WorkItem = start;
    std::iter::from_fn(move || {
        let next_id = target_of_link(current, field)?;
        let next_item = items.get(next_id)?;
        if !visited.insert(next_item.id.as_str()) {
            return None;
        }
        current = next_item;
        Some(current)
    })
}

// ── Target enumeration ─────────────────────────────────────────────

/// Read `field` on `item` as a single `Link` target.
///
/// Returns `None` if the field is absent, non-`Link`, or a `Links` (plural).
pub fn target_of_link<'a>(item: &'a WorkItem, field: &str) -> Option<&'a WorkItemId> {
    match item.fields.get(field) {
        Some(FieldValue::Link(target)) => Some(target),
        _ => None,
    }
}

/// Read `field` on `item` as a list of referenced ids.
///
/// `Link` becomes a single-element list, `Links` becomes its full list,
/// anything else (including missing) becomes empty.
pub fn targets_of<'a>(item: &'a WorkItem, field: &str) -> Vec<&'a WorkItemId> {
    match item.fields.get(field) {
        Some(FieldValue::Link(target)) => vec![target],
        Some(FieldValue::Links(targets)) => targets.iter().collect(),
        _ => Vec::new(),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn item(id: &str, fields: Vec<(&str, FieldValue)>) -> WorkItem {
        WorkItem {
            id: WorkItemId::from(id.to_owned()),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
            body: String::new(),
            source_path: PathBuf::from(format!("{id}.md")),
        }
    }

    fn link(id: &str) -> FieldValue {
        FieldValue::Link(WorkItemId::from(id.to_owned()))
    }

    fn links(ids: &[&str]) -> FieldValue {
        FieldValue::Links(
            ids.iter()
                .map(|id| WorkItemId::from((*id).to_owned()))
                .collect(),
        )
    }

    fn map(items: Vec<WorkItem>) -> HashMap<WorkItemId, WorkItem> {
        items.into_iter().map(|i| (i.id.clone(), i)).collect()
    }

    // ── target_of_link ─────────────────────────────────────────────

    #[test]
    fn target_of_link_reads_link() {
        let i = item("a", vec![("parent", link("b"))]);
        assert_eq!(target_of_link(&i, "parent").unwrap().as_str(), "b");
    }

    #[test]
    fn target_of_link_returns_none_for_missing_field() {
        let i = item("a", vec![]);
        assert!(target_of_link(&i, "parent").is_none());
    }

    #[test]
    fn target_of_link_returns_none_for_links_plural() {
        let i = item("a", vec![("deps", links(&["b", "c"]))]);
        assert!(target_of_link(&i, "deps").is_none());
    }

    // ── targets_of ─────────────────────────────────────────────────

    #[test]
    fn targets_of_reads_link_as_single_element() {
        let i = item("a", vec![("parent", link("b"))]);
        let targets = targets_of(&i, "parent");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].as_str(), "b");
    }

    #[test]
    fn targets_of_reads_links_as_full_list() {
        let i = item("a", vec![("deps", links(&["b", "c", "d"]))]);
        let targets = targets_of(&i, "deps");
        let strs: Vec<&str> = targets.iter().map(|id| id.as_str()).collect();
        assert_eq!(strs, vec!["b", "c", "d"]);
    }

    #[test]
    fn targets_of_returns_empty_for_missing_or_non_link() {
        let i = item("a", vec![("title", FieldValue::String("x".into()))]);
        assert!(targets_of(&i, "title").is_empty());
        assert!(targets_of(&i, "missing").is_empty());
    }

    // ── walk_up_in ─────────────────────────────────────────────────

    #[test]
    fn walk_up_in_yields_chain_excluding_start() {
        let items = map(vec![
            item("a", vec![("parent", link("b"))]),
            item("b", vec![("parent", link("c"))]),
            item("c", vec![]),
        ]);
        let start = items.get("a").unwrap();
        let chain: Vec<&str> = walk_up_in(start, "parent", &items)
            .map(|i| i.id.as_str())
            .collect();
        assert_eq!(chain, vec!["b", "c"]);
    }

    #[test]
    fn walk_up_in_terminates_on_missing_field() {
        let items = map(vec![item("a", vec![])]);
        let start = items.get("a").unwrap();
        assert_eq!(walk_up_in(start, "parent", &items).count(), 0);
    }

    #[test]
    fn walk_up_in_terminates_on_missing_target() {
        let items = map(vec![item("a", vec![("parent", link("ghost"))])]);
        let start = items.get("a").unwrap();
        assert_eq!(walk_up_in(start, "parent", &items).count(), 0);
    }

    #[test]
    fn walk_up_in_stops_on_self_loop() {
        let items = map(vec![item("a", vec![("parent", link("a"))])]);
        let start = items.get("a").unwrap();
        assert_eq!(walk_up_in(start, "parent", &items).count(), 0);
    }

    #[test]
    fn walk_up_in_stops_on_indirect_cycle() {
        let items = map(vec![
            item("a", vec![("parent", link("b"))]),
            item("b", vec![("parent", link("c"))]),
            item("c", vec![("parent", link("a"))]),
        ]);
        let start = items.get("a").unwrap();
        // Yields b, c, then would re-enter a — stops.
        let chain: Vec<&str> = walk_up_in(start, "parent", &items)
            .map(|i| i.id.as_str())
            .collect();
        assert_eq!(chain, vec!["b", "c"]);
    }

    #[test]
    fn walk_up_in_treats_links_plural_as_terminator() {
        let items = map(vec![
            item("a", vec![("parent", links(&["b", "c"]))]),
            item("b", vec![]),
        ]);
        let start = items.get("a").unwrap();
        // Plural Links is not a single-Link upward chain — walk yields nothing.
        assert_eq!(walk_up_in(start, "parent", &items).count(), 0);
    }
}
