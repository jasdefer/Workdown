//! End-to-end tests for `workdown_core::view_data::extract`.
//!
//! Builds a realistic multi-item project on disk, loads it through the
//! same parser / store stack as the CLI, and exercises the `extract`
//! entry point on one view per type. Unit tests cover per-variant
//! semantics in detail; this file catches wiring problems (module
//! boundaries, parser ↔ extractor contracts, lifetime or trait issues)
//! that in-module fixtures can miss.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use workdown_core::parser::schema::load_schema;
use workdown_core::parser::views::load_views;
use workdown_core::store::Store;
use workdown_core::view_data::{extract, AggregateValue, ViewData};

const SCHEMA_YAML: &str = "\
fields:
  title:
    type: string
    required: false
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
    default: open
  points:
    type: integer
    required: false
  deadline:
    type: date
    required: false
  start:
    type: date
    required: false
  end:
    type: date
    required: false
  effort:
    type: integer
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
  depends_on:
    type: links
    required: false
    allow_cycles: false
";

const VIEWS_YAML: &str = "\
views:
  - id: status-board
    type: board
    field: status
    title: title
  - id: item-table
    type: table
    columns: [id, title, status, points]
  - id: hierarchy
    type: tree
    field: parent
  - id: deps
    type: graph
    field: depends_on
  - id: timeline
    type: gantt
    start: start
    end: end
    group: status
  - id: open-count
    type: metric
    where:
      - status=open
    metrics:
      - aggregate: count
  - id: points-by-status
    type: bar_chart
    group_by: status
    value: points
    aggregate: sum
  - id: workload-view
    type: workload
    start: start
    end: end
    effort: effort
  - id: deadline-progress
    type: line_chart
    x: deadline
    y: points
  - id: status-by-assignee
    type: heatmap
    x: status
    y: status
    aggregate: count
  - id: epic-treemap
    type: treemap
    group: parent
    size: points
";

fn setup_project() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();

    fs::create_dir_all(root.join(".workdown")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA_YAML).unwrap();
    fs::write(root.join(".workdown/views.yaml"), VIEWS_YAML).unwrap();

    let items = [
        (
            "epic-auth.md",
            "---\n\
             title: Auth Epic\n\
             status: in_progress\n\
             points: 13\n\
             ---\n",
        ),
        (
            "task-login.md",
            "---\n\
             title: Implement Login\n\
             status: open\n\
             points: 5\n\
             parent: epic-auth\n\
             start: 2026-04-01\n\
             end: 2026-04-05\n\
             effort: 8\n\
             deadline: 2026-04-10\n\
             ---\n",
        ),
        (
            "task-logout.md",
            "---\n\
             title: Implement Logout\n\
             status: done\n\
             points: 3\n\
             parent: epic-auth\n\
             start: 2026-04-02\n\
             end: 2026-04-03\n\
             effort: 4\n\
             deadline: 2026-04-08\n\
             depends_on: [task-login]\n\
             ---\n",
        ),
        (
            "task-reset.md",
            "---\n\
             title: Password Reset\n\
             status: open\n\
             points: 2\n\
             parent: epic-auth\n\
             ---\n",
        ),
    ];
    for (name, content) in items {
        fs::write(root.join("workdown-items").join(name), content).unwrap();
    }

    (tmp, root)
}

#[test]
fn extract_exercises_every_variant() {
    let (_tmp, root) = setup_project();
    let schema = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    let views = load_views(&root.join(".workdown/views.yaml")).unwrap();
    let store = Store::load(&root.join("workdown-items"), &schema).unwrap();

    assert_eq!(store.len(), 4);

    for view in &views.views {
        let data = extract(view, &store, &schema);
        match (view.id.as_str(), &data) {
            ("status-board", ViewData::Board(board)) => {
                assert_eq!(board.field, "status");
                // Synthetic "no value" column is always appended.
                assert!(board.columns.last().unwrap().value.is_none());
                // Every item has a status, so the synthetic column is empty.
                assert!(board.columns.last().unwrap().cards.is_empty());
                let total: usize = board.columns.iter().map(|c| c.cards.len()).sum();
                assert_eq!(total, 4);
            }
            ("item-table", ViewData::Table(table)) => {
                assert_eq!(table.columns, vec!["id", "title", "status", "points"]);
                assert_eq!(table.rows.len(), 4);
                // id column always produces a non-None cell.
                for row in &table.rows {
                    assert!(row.cells[0].is_some());
                }
            }
            ("hierarchy", ViewData::Tree(tree)) => {
                // Only epic-auth is a root; the three tasks are children.
                assert_eq!(tree.roots.len(), 1);
                assert_eq!(tree.roots[0].card.id.as_str(), "epic-auth");
                assert_eq!(tree.roots[0].children.len(), 3);
            }
            ("deps", ViewData::Graph(graph)) => {
                assert_eq!(graph.nodes.len(), 4);
                // task-logout → task-login is the only edge.
                assert_eq!(graph.edges.len(), 1);
                assert_eq!(graph.edges[0].from.as_str(), "task-logout");
                assert_eq!(graph.edges[0].to.as_str(), "task-login");
            }
            ("timeline", ViewData::Gantt(gantt)) => {
                // Two items have start+end; the rest land in unplaced.
                assert_eq!(gantt.bars.len(), 2);
                assert_eq!(gantt.unplaced.len(), 2);
            }
            ("open-count", ViewData::Metric(metric)) => {
                // where status=open → 2 items (task-login, task-reset).
                assert_eq!(metric.rows.len(), 1);
                match metric.rows[0].value {
                    Some(AggregateValue::Number(n)) => assert_eq!(n, 2.0),
                    other => panic!("expected Number(2), got {other:?}"),
                }
            }
            ("points-by-status", ViewData::BarChart(bar)) => {
                // Sum of points per status: open = 5 + 2 = 7; in_progress = 13; done = 3.
                let open = bar.bars.iter().find(|b| b.group == "open").unwrap();
                assert!(matches!(open.value, AggregateValue::Number(n) if (n - 7.0).abs() < 1e-9));
                let in_progress = bar.bars.iter().find(|b| b.group == "in_progress").unwrap();
                assert!(
                    matches!(in_progress.value, AggregateValue::Number(n) if (n - 13.0).abs() < 1e-9)
                );
            }
            ("workload-view", ViewData::Workload(workload)) => {
                // task-login + task-logout contribute; range spans Apr 1..Apr 5.
                assert!(!workload.buckets.is_empty());
            }
            ("deadline-progress", ViewData::LineChart(line)) => {
                // task-login and task-logout have both deadline and points.
                assert_eq!(line.points.len(), 2);
            }
            ("status-by-assignee", ViewData::Heatmap(heatmap)) => {
                // 3 distinct statuses, same on both axes.
                assert_eq!(heatmap.x_labels, vec!["done", "in_progress", "open"]);
                assert!(!heatmap.cells.is_empty());
            }
            ("epic-treemap", ViewData::Treemap(treemap)) => {
                // epic-auth is the only root; it has three child items with
                // points [5, 3, 2] = 10 total.
                assert_eq!(treemap.root.children.len(), 1);
                let epic = &treemap.root.children[0];
                assert_eq!(epic.card.as_ref().unwrap().id.as_str(), "epic-auth");
                let sum: f64 = epic.children.iter().map(|child| child.size.as_f64()).sum();
                assert!((sum - 10.0).abs() < 1e-9);
            }
            (id, data) => panic!("unexpected view/variant pair: id={id}, data={data:?}"),
        }
    }
}
