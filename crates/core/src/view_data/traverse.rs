//! Shared forest walker for tree and treemap extractors.
//!
//! Produces a tree of references into the store, walking a single link
//! field upward (child→parent as declared) to identify roots and
//! downward (via reverse links) to collect children. Cycle-safe via a
//! visited set. Roots keep the order the filtered items arrived in —
//! the view's order — and children at every level, discovered through
//! the store's reverse links rather than the filtered list, are put back
//! into that same order via [`ViewOrder`].
//!
//! Callers map [`Traversal`] to a variant-specific node type (`TreeNode`,
//! `TreemapNode`) — this module stays data-structure-agnostic.

use std::collections::HashSet;

use crate::model::{FieldValue, WorkItem, WorkItemId};
use crate::store::Store;

use super::common::ViewOrder;

pub(super) struct Traversal<'store> {
    pub item: &'store WorkItem,
    pub children: Vec<Traversal<'store>>,
}

/// Walk the forest implied by `field` over the filtered item set.
///
/// An item is a root when its `field` link is absent, points at an id
/// outside the filtered set, or points at a non-existent id (the store
/// has already flagged the broken link). Children are discovered via
/// [`Store::referring_items`] and filtered to stay within the filtered set.
pub(super) fn walk_forest<'store>(
    items: &[&'store WorkItem],
    field: &str,
    store: &'store Store,
) -> Vec<Traversal<'store>> {
    let filtered_ids: HashSet<&str> = items.iter().map(|item| item.id.as_str()).collect();
    let order = ViewOrder::of(items);
    // `items` already carries the view's order, so filtering preserves it.
    let roots: Vec<&'store WorkItem> = items
        .iter()
        .copied()
        .filter(|item| is_root(item, field, &filtered_ids))
        .collect();

    let mut visited: HashSet<WorkItemId> = HashSet::new();
    roots
        .into_iter()
        .map(|item| walk_node(item, field, store, &filtered_ids, &order, &mut visited))
        .collect()
}

fn is_root(item: &WorkItem, field: &str, filtered_ids: &HashSet<&str>) -> bool {
    match item.fields.get(field) {
        Some(FieldValue::Link(target)) => !filtered_ids.contains(target.as_str()),
        _ => true,
    }
}

fn walk_node<'store>(
    item: &'store WorkItem,
    field: &str,
    store: &'store Store,
    filtered_ids: &HashSet<&str>,
    order: &ViewOrder<'_>,
    visited: &mut HashSet<WorkItemId>,
) -> Traversal<'store> {
    visited.insert(item.id.clone());
    let mut children_items: Vec<&'store WorkItem> = store
        .referring_items(item.id.as_str(), field)
        .into_iter()
        .filter(|child| filtered_ids.contains(child.id.as_str()))
        .filter(|child| !visited.contains(&child.id))
        .collect();
    order.sort_items(&mut children_items);
    let children = children_items
        .into_iter()
        .map(|child| walk_node(child, field, store, filtered_ids, order, visited))
        .collect();
    Traversal { item, children }
}
