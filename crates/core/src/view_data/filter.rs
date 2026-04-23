//! Filter wiring: produces the filtered item set for a view.
//!
//! Reuses the query engine so every view sees the same filter semantics
//! as `workdown query --where`. Items come back sorted by id ascending
//! for CI-diff-clean output; per-variant extractors re-sort as needed
//! (e.g. board columns by schema order).

use crate::model::schema::Schema;
use crate::model::views::View;
use crate::model::WorkItem;
use crate::query::engine::filter_and_sort;
use crate::query::parse::parse_where;
use crate::query::types::{Predicate, QueryRequest};
use crate::store::Store;

/// Return items matching the view's `where` clauses, sorted by id ascending.
///
/// Panics if a `where` clause fails to parse — `views_check` must have
/// validated the view before it reaches this layer.
pub fn filtered_items<'store>(
    view: &View,
    store: &'store Store,
    schema: &Schema,
) -> Vec<&'store WorkItem> {
    let request = QueryRequest {
        predicate: build_predicate(&view.where_clauses),
        sort: vec![],
        fields: vec![],
    };
    let (_columns, mut items) = filter_and_sort(&request, store, schema)
        .expect("views_check validates where clauses before extraction");
    items.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
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
