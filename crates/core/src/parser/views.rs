//! Views loader: parse `views.yaml`, validate, and produce [`Views`].
//!
//! The public API is [`parse_views`] (from a string) and [`load_views`]
//! (from disk). Semantic validation here covers:
//!
//! - Every required slot for the view type is present
//! - `id` values are unique across the file
//!
//! Cross-file checks (referenced fields exist in `schema.yaml`, field types
//! compatible with the view type) live in `workdown validate` and are the
//! subject of the `views-yaml-validation` issue.

use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;

use crate::model::views::{Aggregate, Bucket, View, ViewKind, ViewType, Views};

// ── Public API ────────────────────────────────────────────────────────

/// Parse a views file from a YAML string.
///
/// Performs serde deserialization followed by semantic validation.
/// Returns all validation errors at once (does not stop at the first).
pub fn parse_views(yaml: &str) -> Result<Views, ViewsLoadError> {
    let raw: RawViewsFile = serde_yaml::from_str(yaml).map_err(ViewsLoadError::InvalidYaml)?;

    let mut errors = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut views = Vec::with_capacity(raw.views.len());

    for raw_view in raw.views {
        if !seen_ids.insert(raw_view.id.clone()) {
            errors.push(ViewsValidationError::DuplicateId {
                id: raw_view.id.clone(),
            });
            continue;
        }

        match convert_view(raw_view) {
            Ok(view) => views.push(view),
            Err(e) => errors.push(e),
        }
    }

    if !errors.is_empty() {
        return Err(ViewsLoadError::Validation(errors));
    }

    Ok(Views { views })
}

/// Load a views file from disk.
pub fn load_views(path: &Path) -> Result<Views, ViewsLoadError> {
    let content = std::fs::read_to_string(path).map_err(ViewsLoadError::ReadFailed)?;
    parse_views(&content)
}

// ── Errors ────────────────────────────────────────────────────────────

/// Errors from loading or validating a views file.
#[derive(Debug, thiserror::Error)]
pub enum ViewsLoadError {
    #[error("failed to read views file: {0}")]
    ReadFailed(std::io::Error),

    #[error("invalid YAML in views: {0}")]
    InvalidYaml(serde_yaml::Error),

    #[error("views validation failed:\n{}", format_errors(.0))]
    Validation(Vec<ViewsValidationError>),
}

/// A single semantic validation error.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ViewsValidationError {
    #[error("view '{id}' is declared more than once")]
    DuplicateId { id: String },

    #[error("view '{id}' (type {view_type}): missing required slot '{slot}'")]
    MissingSlot {
        id: String,
        view_type: ViewType,
        slot: &'static str,
    },
}

fn format_errors(errors: &[ViewsValidationError]) -> String {
    errors
        .iter()
        .map(|e| format!("  - {e}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ── Raw deserialization target ────────────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawViewsFile {
    #[serde(default)]
    views: Vec<RawView>,
}

/// Flat deserialization struct that mirrors the YAML layout. All
/// type-specific slots are optional here; [`convert_view`] enforces the
/// per-type required-slot rules. Unknown slots are rejected by serde.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawView {
    id: String,

    #[serde(rename = "type")]
    view_type: ViewType,

    #[serde(default, rename = "where")]
    where_clauses: Vec<String>,

    // Single-field views (board / tree / graph)
    #[serde(default)]
    field: Option<String>,

    // Table
    #[serde(default)]
    columns: Option<Vec<String>>,

    // Gantt / Workload
    #[serde(default)]
    start: Option<String>,
    #[serde(default)]
    end: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    group: Option<String>,

    // Bar chart / Metric / Heatmap
    #[serde(default)]
    group_by: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    aggregate: Option<Aggregate>,
    #[serde(default)]
    label: Option<String>,

    // Line chart / Heatmap
    #[serde(default)]
    x: Option<String>,
    #[serde(default)]
    y: Option<String>,

    // Treemap
    #[serde(default)]
    size: Option<String>,

    // Heatmap
    #[serde(default)]
    bucket: Option<Bucket>,
}

// ── Conversion: raw → validated ───────────────────────────────────────

fn convert_view(raw: RawView) -> Result<View, ViewsValidationError> {
    let id = raw.id.clone();
    let view_type = raw.view_type;

    let kind = match view_type {
        ViewType::Board => ViewKind::Board {
            field: require(raw.field, &id, view_type, "field")?,
        },
        ViewType::Tree => ViewKind::Tree {
            field: require(raw.field, &id, view_type, "field")?,
        },
        ViewType::Graph => ViewKind::Graph {
            field: require(raw.field, &id, view_type, "field")?,
        },
        ViewType::Table => ViewKind::Table {
            columns: require(raw.columns, &id, view_type, "columns")?,
        },
        ViewType::Gantt => ViewKind::Gantt {
            start: require(raw.start, &id, view_type, "start")?,
            end: require(raw.end, &id, view_type, "end")?,
            group: raw.group,
        },
        ViewType::BarChart => ViewKind::BarChart {
            group_by: require(raw.group_by, &id, view_type, "group_by")?,
            aggregate: require(raw.aggregate, &id, view_type, "aggregate")?,
            value: raw.value,
        },
        ViewType::LineChart => ViewKind::LineChart {
            x: require(raw.x, &id, view_type, "x")?,
            y: require(raw.y, &id, view_type, "y")?,
        },
        ViewType::Workload => ViewKind::Workload {
            start: require(raw.start, &id, view_type, "start")?,
            end: require(raw.end, &id, view_type, "end")?,
            effort: require(raw.effort, &id, view_type, "effort")?,
        },
        ViewType::Metric => ViewKind::Metric {
            aggregate: require(raw.aggregate, &id, view_type, "aggregate")?,
            label: raw.label,
            value: raw.value,
        },
        ViewType::Treemap => ViewKind::Treemap {
            group: require(raw.group, &id, view_type, "group")?,
            size: require(raw.size, &id, view_type, "size")?,
        },
        ViewType::Heatmap => ViewKind::Heatmap {
            x: require(raw.x, &id, view_type, "x")?,
            y: require(raw.y, &id, view_type, "y")?,
            aggregate: require(raw.aggregate, &id, view_type, "aggregate")?,
            value: raw.value,
            bucket: raw.bucket,
        },
    };

    Ok(View {
        id: raw.id,
        where_clauses: raw.where_clauses,
        kind,
    })
}

fn require<T>(
    slot: Option<T>,
    id: &str,
    view_type: ViewType,
    slot_name: &'static str,
) -> Result<T, ViewsValidationError> {
    slot.ok_or_else(|| ViewsValidationError::MissingSlot {
        id: id.to_owned(),
        view_type,
        slot: slot_name,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_single(yaml: &str) -> View {
        let mut views = parse_views(yaml).unwrap().views;
        assert_eq!(views.len(), 1);
        views.pop().unwrap()
    }

    #[test]
    fn empty_file_parses_to_empty_views() {
        let yaml = "views: []\n";
        let parsed = parse_views(yaml).unwrap();
        assert!(parsed.views.is_empty());
    }

    #[test]
    fn parse_board() {
        let view = parse_single(
            "views:\n  - id: status-board\n    type: board\n    field: status\n",
        );
        assert_eq!(view.id, "status-board");
        assert!(view.where_clauses.is_empty());
        match view.kind {
            ViewKind::Board { field } => assert_eq!(field, "status"),
            other => panic!("expected Board, got {other:?}"),
        }
    }

    #[test]
    fn parse_board_with_where() {
        let view = parse_single(
            "views:\n  - id: issues-only\n    type: board\n    field: status\n    where:\n      - \"type=issue\"\n      - \"status!=removed\"\n",
        );
        assert_eq!(view.where_clauses, vec!["type=issue", "status!=removed"]);
    }

    #[test]
    fn parse_tree() {
        let view = parse_single("views:\n  - id: h\n    type: tree\n    field: parent\n");
        assert!(matches!(view.kind, ViewKind::Tree { .. }));
    }

    #[test]
    fn parse_graph() {
        let view = parse_single("views:\n  - id: d\n    type: graph\n    field: depends_on\n");
        assert!(matches!(view.kind, ViewKind::Graph { .. }));
    }

    #[test]
    fn parse_table() {
        let view = parse_single(
            "views:\n  - id: all\n    type: table\n    columns: [id, title, status]\n",
        );
        match view.kind {
            ViewKind::Table { columns } => assert_eq!(columns, vec!["id", "title", "status"]),
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_with_group() {
        let view = parse_single(
            "views:\n  - id: roadmap\n    type: gantt\n    start: start_date\n    end: end_date\n    group: parent\n",
        );
        match view.kind {
            ViewKind::Gantt { start, end, group } => {
                assert_eq!(start, "start_date");
                assert_eq!(end, "end_date");
                assert_eq!(group.as_deref(), Some("parent"));
            }
            other => panic!("expected Gantt, got {other:?}"),
        }
    }

    #[test]
    fn parse_bar_chart() {
        let view = parse_single(
            "views:\n  - id: by-status\n    type: bar_chart\n    group_by: status\n    value: effort\n    aggregate: sum\n",
        );
        match view.kind {
            ViewKind::BarChart { group_by, value, aggregate } => {
                assert_eq!(group_by, "status");
                assert_eq!(value.as_deref(), Some("effort"));
                assert_eq!(aggregate, Aggregate::Sum);
            }
            other => panic!("expected BarChart, got {other:?}"),
        }
    }

    #[test]
    fn parse_line_chart() {
        let view = parse_single(
            "views:\n  - id: eva\n    type: line_chart\n    x: estimate\n    y: actual_effort\n",
        );
        assert!(matches!(view.kind, ViewKind::LineChart { .. }));
    }

    #[test]
    fn parse_workload() {
        let view = parse_single(
            "views:\n  - id: cap\n    type: workload\n    start: start_date\n    end: end_date\n    effort: effort\n",
        );
        assert!(matches!(view.kind, ViewKind::Workload { .. }));
    }

    #[test]
    fn parse_metric_count() {
        let view = parse_single(
            "views:\n  - id: open\n    type: metric\n    aggregate: count\n    label: Open items\n",
        );
        match view.kind {
            ViewKind::Metric { aggregate, label, value } => {
                assert_eq!(aggregate, Aggregate::Count);
                assert_eq!(label.as_deref(), Some("Open items"));
                assert!(value.is_none());
            }
            other => panic!("expected Metric, got {other:?}"),
        }
    }

    #[test]
    fn parse_treemap() {
        let view = parse_single(
            "views:\n  - id: ebm\n    type: treemap\n    group: parent\n    size: effort\n",
        );
        assert!(matches!(view.kind, ViewKind::Treemap { .. }));
    }

    #[test]
    fn parse_heatmap_with_bucket() {
        let view = parse_single(
            "views:\n  - id: activity\n    type: heatmap\n    x: end_date\n    y: assignee\n    aggregate: count\n    bucket: week\n",
        );
        match view.kind {
            ViewKind::Heatmap { bucket, .. } => assert_eq!(bucket, Some(Bucket::Week)),
            other => panic!("expected Heatmap, got {other:?}"),
        }
    }

    // ── Validation errors ──────────────────────────────────────────

    #[test]
    fn duplicate_id_rejected() {
        let yaml = "views:\n  - id: a\n    type: board\n    field: status\n  - id: a\n    type: tree\n    field: parent\n";
        let err = parse_views(yaml).unwrap_err();
        match err {
            ViewsLoadError::Validation(errors) => {
                assert!(matches!(
                    errors.as_slice(),
                    [ViewsValidationError::DuplicateId { id }] if id == "a"
                ));
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn missing_slot_rejected() {
        let yaml = "views:\n  - id: b\n    type: board\n";
        let err = parse_views(yaml).unwrap_err();
        match err {
            ViewsLoadError::Validation(errors) => {
                assert!(matches!(
                    errors.as_slice(),
                    [ViewsValidationError::MissingSlot { id, slot, .. }]
                        if id == "b" && *slot == "field"
                ));
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn unknown_slot_rejected_by_serde() {
        // `color` is not a known slot — deny_unknown_fields catches it.
        let yaml = "views:\n  - id: c\n    type: board\n    field: status\n    color: red\n";
        let err = parse_views(yaml).unwrap_err();
        assert!(matches!(err, ViewsLoadError::InvalidYaml(_)));
    }

    #[test]
    fn multiple_errors_reported_together() {
        let yaml = "views:\n  - id: x\n    type: gantt\n    start: start_date\n  - id: y\n    type: bar_chart\n    group_by: status\n";
        let err = parse_views(yaml).unwrap_err();
        match err {
            ViewsLoadError::Validation(errors) => assert_eq!(errors.len(), 2),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    // ── Round-trip example from docs/views.md ──────────────────────

    #[test]
    fn parses_full_example() {
        let yaml = r#"
views:
  - id: status-board
    type: board
    field: status
    where:
      - "type=issue"
      - "status!=removed"
  - id: hierarchy
    type: tree
    field: parent
  - id: deps
    type: graph
    field: depends_on
  - id: all-items
    type: table
    columns: [id, title, type, status, start_date, end_date]
  - id: roadmap
    type: gantt
    start: start_date
    end: end_date
    group: parent
  - id: effort-by-status
    type: bar_chart
    group_by: status
    value: effort
    aggregate: sum
  - id: estimate-vs-actual
    type: line_chart
    x: estimate
    y: actual_effort
  - id: capacity
    type: workload
    start: start_date
    end: end_date
    effort: effort
  - id: open-count
    type: metric
    aggregate: count
    label: Open items
    where: ["status=to_do,in_progress"]
  - id: effort-by-milestone
    type: treemap
    group: parent
    size: effort
  - id: activity
    type: heatmap
    x: end_date
    y: assignee
    aggregate: count
    bucket: week
"#;
        let parsed = parse_views(yaml).unwrap();
        assert_eq!(parsed.views.len(), 11);
    }
}
