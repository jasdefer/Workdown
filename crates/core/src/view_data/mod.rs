//! View data extraction.
//!
//! Reads work items + a view configuration and produces a [`ViewData`]
//! struct that both Markdown renderers and the live web server consume.
//! This is the single piece of business logic for visualization; formatters
//! and endpoints above this layer are pure presentation over the extracted
//! struct.
//!
//! The caller is responsible for running `views_check` first — field
//! references, slot/type mismatches, and `where`-clause syntax are all
//! validated there. Extraction assumes those invariants hold; violating
//! them is a programming error and panics.
//!
//! Items that pass the filter but can't be turned into the view's natural
//! mark (a gantt bar, a chart point, a heatmap cell) end up in per-variant
//! `unplaced: Vec<UnplacedCard>` lists, carrying the reason. Renderers
//! decide whether to surface them in a separate section or ignore them.

pub mod board;
pub mod common;
pub mod filter;
pub mod graph;
pub mod table;
pub mod tree;
mod traverse;

#[cfg(test)]
mod test_support;

use serde::Serialize;

use crate::model::schema::Schema;
use crate::model::views::{View, ViewKind};
use crate::store::Store;

pub use board::{BoardColumn, BoardData};
pub use common::{
    build_card, resolve_title, AggregateValue, AxisValue, Card, CardField, UnplacedCard,
    UnplacedReason,
};
pub use graph::{Edge, GraphData};
pub use table::{TableData, TableRow};
pub use tree::{TreeData, TreeNode};

/// Extracted, fully-resolved data for a single view.
///
/// Additional variants land as each extractor is implemented.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ViewData {
    Board(BoardData),
    Graph(GraphData),
    Table(TableData),
    Tree(TreeData),
}

/// Extract view data for rendering or JSON serialization.
///
/// Infallible by design — structural problems (invalid slot, bad field
/// reference, malformed `where` clause) are caught by `views_check`;
/// data-level problems (missing dates, invalid ranges, non-numeric
/// aggregate inputs) live in each variant's `unplaced` list.
pub fn extract(view: &View, store: &Store, schema: &Schema) -> ViewData {
    match &view.kind {
        ViewKind::Board { .. } => ViewData::Board(board::extract_board(view, store, schema)),
        ViewKind::Graph { .. } => ViewData::Graph(graph::extract_graph(view, store, schema)),
        ViewKind::Table { .. } => ViewData::Table(table::extract_table(view, store, schema)),
        ViewKind::Tree { .. } => ViewData::Tree(tree::extract_tree(view, store, schema)),
        other => todo!("view type {} not yet implemented", other.view_type()),
    }
}
