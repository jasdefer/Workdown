---
id: view-data-intermediate
type: issue
status: to_do
title: Design ViewData and extractors
parent: renderers
---

Define the shared intermediate representation renderers consume, plus the extractor functions that build it from items + a view config.

## Proposed shape

```rust
pub enum ViewData {
    Board(BoardView),
    Tree(TreeView),
    Graph(GraphView),
}

pub struct BoardView {
    pub field: String,
    pub columns: Vec<BoardColumn>,
}
pub struct BoardColumn { pub value: String, pub cards: Vec<CardSummary> }

pub struct TreeView {
    pub field: String,                    // the link field, e.g. "parent"
    pub roots: Vec<TreeNode>,
}
pub struct TreeNode { pub item: CardSummary, pub children: Vec<TreeNode> }

pub struct GraphView {
    pub field: String,                    // the links field, e.g. "depends_on"
    pub nodes: Vec<CardSummary>,
    pub edges: Vec<(String, String)>,     // (from_id, to_id)
}

pub struct CardSummary {
    pub id: String,
    pub title: String,
    pub fields: BTreeMap<String, String>, // extra fields chosen for display
}
```

## Scope

- Data structs (above, refined during implementation)
- Extractors: `fn extract_board(items, view_cfg) -> BoardView`, `extract_tree`, `extract_graph`
- Unit tests with small fixtures

## Out of scope

- Rendering — each format is its own issue
- What fields show on a card beyond id/title — defer unless a default doesn't emerge naturally
