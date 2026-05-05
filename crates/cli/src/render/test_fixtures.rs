//! Test-only fixtures shared across renderer test modules.
//!
//! Renderers all build the same minimal `Card` and `MissingValue`
//! `UnplacedCard` shapes in their tests; centralizing those keeps each
//! renderer's tests focused on its own data variants. Renderer-specific
//! shapes (gantt bars, heatmap cells, workload buckets, alternate
//! `UnplacedReason` variants tied to renderer-specific field literals)
//! stay local to each test module.

#![cfg(test)]

use workdown_core::model::WorkItemId;
use workdown_core::view_data::{Card, UnplacedCard, UnplacedReason};

/// Build a minimal `Card` with no fields and an empty body.
pub fn card(id: &str, title: Option<&str>) -> Card {
    Card {
        id: WorkItemId::from(id.to_owned()),
        title: title.map(str::to_owned),
        fields: vec![],
        body: String::new(),
    }
}

/// Build an `UnplacedCard` with `UnplacedReason::MissingValue`. The
/// shared "this card filter-matched but lacks the value field" shape
/// every chart-style renderer surfaces.
pub fn unplaced_missing(id: &str, title: Option<&str>, field: &str) -> UnplacedCard {
    UnplacedCard {
        card: card(id, title),
        reason: UnplacedReason::MissingValue {
            field: field.to_owned(),
        },
    }
}
