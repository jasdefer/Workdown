//! Filter wiring: produces the filtered, ordered item set for a view.
//!
//! Reuses the query engine so every view sees the same filter and sort
//! semantics as `workdown query --where` and `--sort`. Items come back in
//! the view's order — its `sort:` keys applied left to right, the id as
//! the final tiebreaker, or plain id order when the slot is absent — and
//! that is the one place the order is decided: per-variant extractors
//! preserve it (board cards within a column, table rows, graph nodes),
//! re-derive it for items they discover elsewhere (tree children, via
//! `ViewOrder` in the `common` module), or replace it where the kind has an
//! intrinsic order of its own (gantt bars by time, treemap nodes by size).

use crate::model::schema::Schema;
use crate::model::views::View;
use crate::model::WorkItem;
use crate::query::engine::filter_and_sort;
use crate::query::parse::{parse_sort, parse_where};
use crate::query::types::{Predicate, QueryRequest, SortSpec};
use crate::store::Store;

/// Return items matching the view's `where` clauses, in the view's order.
///
/// Panics if a `where` clause or a `sort` key fails to parse —
/// `views_check` must have validated the view before it reaches this
/// layer.
pub fn filtered_items<'store>(
    view: &View,
    store: &'store Store,
    schema: &Schema,
) -> Vec<&'store WorkItem> {
    run_filter(&view.where_clauses, &view.sort_keys, store, schema)
}

/// Like [`filtered_items`], but AND-combines additional `where` clauses
/// with the view-level ones. Used by views that filter per-row inside a
/// single view (e.g. metric, where each row narrows the view's items
/// further).
///
/// Panics if any clause fails to parse — `views_check` must validate
/// both view-level and per-row clauses before this layer runs.
pub fn filtered_items_with_extras<'store>(
    view: &View,
    extras: &[String],
    store: &'store Store,
    schema: &Schema,
) -> Vec<&'store WorkItem> {
    if extras.is_empty() {
        return filtered_items(view, store, schema);
    }
    let mut combined = view.where_clauses.clone();
    combined.extend(extras.iter().cloned());
    run_filter(&combined, &view.sort_keys, store, schema)
}

fn run_filter<'store>(
    where_clauses: &[String],
    sort_keys: &[String],
    store: &'store Store,
    schema: &Schema,
) -> Vec<&'store WorkItem> {
    let sort: Vec<SortSpec> = sort_keys
        .iter()
        .map(|key| parse_sort(key).expect("views_check validates sort keys before extraction"))
        .collect();
    let request = QueryRequest {
        predicate: build_predicate(where_clauses),
        sort,
        fields: vec![],
    };
    // The sorter breaks every remaining tie by id, so no keys at all is
    // plain id order — nothing to add here.
    let (_columns, items) = filter_and_sort(&request, store, schema)
        .expect("views_check validates where clauses before extraction");
    items
}

fn build_predicate(where_clauses: &[String]) -> Option<Predicate> {
    if where_clauses.is_empty() {
        return None;
    }
    let predicates: Vec<Predicate> = where_clauses
        .iter()
        .map(|raw| parse_where(raw).expect("views_check validates where clauses"))
        .collect();
    if predicates.len() == 1 {
        predicates.into_iter().next()
    } else {
        Some(Predicate::And(predicates))
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    use crate::model::schema::FieldTypeConfig;
    use crate::model::views::{DisplayConfig, ViewKind};
    use crate::model::FieldValue;
    use crate::view_data::test_support::{make_item, make_schema, make_store};

    fn table_view(sort: Vec<&str>) -> View {
        View {
            id: "t".into(),
            where_clauses: vec![],
            sort_keys: sort.into_iter().map(str::to_owned).collect(),
            display: DisplayConfig::default(),
            kind: ViewKind::Table,
        }
    }

    fn date(day: u32) -> FieldValue {
        FieldValue::Date(NaiveDate::from_ymd_opt(2026, 10, day).unwrap())
    }

    /// Three items whose id order (a, b, c) disagrees with their
    /// `created` order (b, c, a) and whose `points` tie in pairs, so each
    /// assertion below can tell one order from the other.
    fn store() -> (crate::model::schema::Schema, Store) {
        let schema = make_schema(vec![
            ("created", FieldTypeConfig::Date),
            (
                "points",
                FieldTypeConfig::Integer {
                    min: None,
                    max: None,
                },
            ),
        ]);
        let store = make_store(
            &schema,
            vec![
                make_item(
                    "a",
                    vec![("created", date(30)), ("points", FieldValue::Integer(1))],
                    "",
                ),
                make_item(
                    "b",
                    vec![("created", date(10)), ("points", FieldValue::Integer(2))],
                    "",
                ),
                make_item(
                    "c",
                    vec![("created", date(20)), ("points", FieldValue::Integer(2))],
                    "",
                ),
            ],
        );
        (schema, store)
    }

    fn ids(items: &[&WorkItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| item.id.as_str().to_owned())
            .collect()
    }

    #[test]
    fn absent_sort_is_id_order() {
        let (schema, store) = store();
        let items = filtered_items(&table_view(vec![]), &store, &schema);
        assert_eq!(ids(&items), ["a", "b", "c"]);
    }

    #[test]
    fn sort_key_orders_by_the_field_with_direction() {
        let (schema, store) = store();
        let ascending = filtered_items(&table_view(vec!["created"]), &store, &schema);
        assert_eq!(ids(&ascending), ["b", "c", "a"]);
        let descending = filtered_items(&table_view(vec!["created:desc"]), &store, &schema);
        assert_eq!(ids(&descending), ["a", "c", "b"]);
    }

    #[test]
    fn later_keys_break_ties_and_id_breaks_the_last() {
        let (schema, store) = store();
        let items = filtered_items(
            &table_view(vec!["points:desc", "created:desc"]),
            &store,
            &schema,
        );
        // b and c tie on points; created:desc puts c first; a trails.
        assert_eq!(ids(&items), ["c", "b", "a"]);
        let by_points_only = filtered_items(&table_view(vec!["points:desc"]), &store, &schema);
        // Same tie, no second key: the id decides.
        assert_eq!(ids(&by_points_only), ["b", "c", "a"]);
    }

    #[test]
    fn per_row_extras_keep_the_view_order() {
        let (schema, store) = store();
        let items = filtered_items_with_extras(
            &table_view(vec!["created:desc"]),
            &["points=2".to_owned()],
            &store,
            &schema,
        );
        assert_eq!(ids(&items), ["c", "b"]);
    }
}
