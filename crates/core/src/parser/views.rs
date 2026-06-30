//! Views loader: parse `views.yaml`, validate, and produce [`Views`].
//!
//! The public API is [`parse_views`] (from a string) and [`load_views`]
//! (from disk). Semantic validation here covers:
//!
//! - Every required slot for the view type is present
//! - `id` values are unique across the file
//!
//! Cross-file checks (referenced fields exist in `schema.yaml`, field types
//! compatible with the view type) live in a separate `views_check` module and
//! are wired into `workdown validate` — see the `views-cross-file-validation`
//! and `views-validate-integration` issues.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model::views::{Aggregate, Bucket, MetricRow, View, ViewKind, ViewType, Views};
use crate::model::weekday::Weekday;

/// Default output directory written by `workdown render` when
/// `views.yaml` does not set a `directory:` key.
const DEFAULT_OUTPUT_DIR: &str = "views";

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

    let output_dir = raw
        .directory
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT_DIR));

    Ok(Views { output_dir, views })
}

/// Load a views file from disk.
pub fn load_views(path: &Path) -> Result<Views, ViewsLoadError> {
    let content = std::fs::read_to_string(path).map_err(ViewsLoadError::ReadFailed)?;
    parse_views(&content)
}

/// Build a single [`View`] from a YAML value carrying one view's slots.
///
/// The value uses the same flat shape as one entry in the `views:` list:
/// `id`, `type`, optional `where`, and the type-specific slots. Validates
/// exactly as a hand-edited file would — unknown slots are rejected
/// (`deny_unknown_fields`) and per-type required slots are enforced by
/// [`convert_view`]. Used by the view-write path to turn a UI/API view
/// definition into a validated model before persisting.
pub fn view_from_value(value: serde_yaml::Value) -> Result<View, ViewsLoadError> {
    let raw: RawView = serde_yaml::from_value(value).map_err(ViewsLoadError::InvalidYaml)?;
    convert_view(raw).map_err(|error| ViewsLoadError::Validation(vec![error]))
}

/// Serialize a [`Views`] model back to `views.yaml` text.
///
/// The inverse of [`parse_views`]: emits a file that re-parses to an
/// equal [`Views`] (covered by the round-trip test). Only slots the view
/// kind actually uses are written, empty `where` lists are omitted, and
/// the `directory:` key is emitted only when it differs from the default.
///
/// This rebuilds the file from the model, so a user's comments and key
/// ordering in the original `views.yaml` are not preserved — see the
/// `view-write-backend` design notes.
pub fn serialize_views(views: &Views) -> Result<String, serde_yaml::Error> {
    serde_yaml::to_string(&to_raw_views_file(views))
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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawViewsFile {
    /// Output directory for rendered view files, relative to project
    /// root. Optional; defaults to [`DEFAULT_OUTPUT_DIR`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    directory: Option<String>,
    views: Vec<RawView>,
}

/// Flat deserialization struct that mirrors the YAML layout. All
/// type-specific slots are optional here; [`convert_view`] enforces the
/// per-type required-slot rules. Unknown slots are rejected by serde.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawView {
    id: String,

    #[serde(rename = "type")]
    view_type: ViewType,

    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    where_clauses: Vec<String>,

    // Cross-cutting: the schema field whose value each rendered item
    // uses as its display title. Allowed on every view type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,

    // Single-field views (board / tree / graph)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    field: Option<String>,

    // Table
    #[serde(default, skip_serializing_if = "Option::is_none")]
    columns: Option<Vec<String>>,

    // Gantt / Workload
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duration: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root_link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    depth_link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<String>,

    // Bar chart / Heatmap
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    aggregate: Option<Aggregate>,

    // Metric
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metrics: Option<Vec<RawMetricRow>>,

    // Line chart / Heatmap
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    y: Option<String>,

    // Treemap
    #[serde(default, skip_serializing_if = "Option::is_none")]
    size: Option<String>,

    // Heatmap
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bucket: Option<Bucket>,

    // Workload
    #[serde(default, skip_serializing_if = "Option::is_none")]
    working_days: Option<Vec<Weekday>>,
}

/// One row inside a metric view's `metrics:` list.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RawMetricRow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    aggregate: Aggregate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(default, rename = "where", skip_serializing_if = "Vec::is_empty")]
    where_clauses: Vec<String>,
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
            columns: raw.columns.unwrap_or_default(),
        },
        ViewType::Graph => ViewKind::Graph {
            field: require(raw.field, &id, view_type, "field")?,
            group_by: raw.group_by,
        },
        ViewType::Table => ViewKind::Table {
            columns: require(raw.columns, &id, view_type, "columns")?,
        },
        ViewType::Gantt => ViewKind::Gantt {
            start: require(raw.start, &id, view_type, "start")?,
            end: raw.end,
            duration: raw.duration,
            after: raw.after,
            group: raw.group,
        },
        ViewType::GanttByInitiative => ViewKind::GanttByInitiative {
            start: require(raw.start, &id, view_type, "start")?,
            end: raw.end,
            duration: raw.duration,
            after: raw.after,
            root_link: require(raw.root_link, &id, view_type, "root_link")?,
        },
        ViewType::GanttByDepth => ViewKind::GanttByDepth {
            start: require(raw.start, &id, view_type, "start")?,
            end: raw.end,
            duration: raw.duration,
            after: raw.after,
            depth_link: require(raw.depth_link, &id, view_type, "depth_link")?,
        },
        ViewType::BarChart => ViewKind::BarChart {
            group_by: require(raw.group_by, &id, view_type, "group_by")?,
            aggregate: require(raw.aggregate, &id, view_type, "aggregate")?,
            value: raw.value,
        },
        ViewType::LineChart => ViewKind::LineChart {
            x: require(raw.x, &id, view_type, "x")?,
            y: require(raw.y, &id, view_type, "y")?,
            group: raw.group,
        },
        ViewType::Workload => ViewKind::Workload {
            start: require(raw.start, &id, view_type, "start")?,
            end: require(raw.end, &id, view_type, "end")?,
            effort: require(raw.effort, &id, view_type, "effort")?,
            working_days: raw.working_days,
        },
        ViewType::Metric => ViewKind::Metric {
            metrics: require(raw.metrics, &id, view_type, "metrics")?
                .into_iter()
                .map(|row| MetricRow {
                    label: row.label,
                    aggregate: row.aggregate,
                    value: row.value,
                    where_clauses: row.where_clauses,
                })
                .collect(),
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
        title: raw.title,
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

// ── Conversion: validated → raw (serialization) ───────────────────────

/// Build the raw, serde-serializable file mirror from a validated model.
fn to_raw_views_file(views: &Views) -> RawViewsFile {
    // Emit `directory:` only when it differs from the implicit default,
    // so files that never set it stay free of the redundant key.
    let directory = if views.output_dir.as_path() == Path::new(DEFAULT_OUTPUT_DIR) {
        None
    } else {
        Some(views.output_dir.to_string_lossy().into_owned())
    };
    RawViewsFile {
        directory,
        views: views.views.iter().map(raw_view_from).collect(),
    }
}

/// Inverse of [`convert_view`]: spread a validated [`View`] across the flat
/// raw struct, populating only the slots its kind uses.
///
/// Slots that are optional *and* empty in the model are left `None` so
/// they don't serialize (e.g. a tree with no extra columns). Slots that
/// are required for the kind are always populated — even when empty (a
/// table's `columns: []`, a metric's `metrics: []`) — so the output
/// re-parses without tripping the required-slot check.
fn raw_view_from(view: &View) -> RawView {
    let mut raw = RawView {
        id: view.id.clone(),
        view_type: view.kind.view_type(),
        where_clauses: view.where_clauses.clone(),
        title: view.title.clone(),
        field: None,
        columns: None,
        start: None,
        end: None,
        duration: None,
        after: None,
        root_link: None,
        depth_link: None,
        effort: None,
        group: None,
        group_by: None,
        value: None,
        aggregate: None,
        metrics: None,
        x: None,
        y: None,
        size: None,
        bucket: None,
        working_days: None,
    };

    match &view.kind {
        ViewKind::Board { field } => raw.field = Some(field.clone()),
        ViewKind::Tree { field, columns } => {
            raw.field = Some(field.clone());
            // Optional slot: omit when empty so we don't emit `columns: []`.
            if !columns.is_empty() {
                raw.columns = Some(columns.clone());
            }
        }
        ViewKind::Graph { field, group_by } => {
            raw.field = Some(field.clone());
            raw.group_by = group_by.clone();
        }
        ViewKind::Table { columns } => {
            // Required slot: always emit, even when empty.
            raw.columns = Some(columns.clone());
        }
        ViewKind::Gantt {
            start,
            end,
            duration,
            after,
            group,
        } => {
            raw.start = Some(start.clone());
            raw.end = end.clone();
            raw.duration = duration.clone();
            raw.after = after.clone();
            raw.group = group.clone();
        }
        ViewKind::GanttByInitiative {
            start,
            end,
            duration,
            after,
            root_link,
        } => {
            raw.start = Some(start.clone());
            raw.end = end.clone();
            raw.duration = duration.clone();
            raw.after = after.clone();
            raw.root_link = Some(root_link.clone());
        }
        ViewKind::GanttByDepth {
            start,
            end,
            duration,
            after,
            depth_link,
        } => {
            raw.start = Some(start.clone());
            raw.end = end.clone();
            raw.duration = duration.clone();
            raw.after = after.clone();
            raw.depth_link = Some(depth_link.clone());
        }
        ViewKind::BarChart {
            group_by,
            value,
            aggregate,
        } => {
            raw.group_by = Some(group_by.clone());
            raw.value = value.clone();
            raw.aggregate = Some(*aggregate);
        }
        ViewKind::LineChart { x, y, group } => {
            raw.x = Some(x.clone());
            raw.y = Some(y.clone());
            raw.group = group.clone();
        }
        ViewKind::Workload {
            start,
            end,
            effort,
            working_days,
        } => {
            raw.start = Some(start.clone());
            raw.end = Some(end.clone());
            raw.effort = Some(effort.clone());
            raw.working_days = working_days.clone();
        }
        ViewKind::Metric { metrics } => {
            // Required slot: always emit, even when empty.
            raw.metrics = Some(metrics.iter().map(raw_metric_row_from).collect());
        }
        ViewKind::Treemap { group, size } => {
            raw.group = Some(group.clone());
            raw.size = Some(size.clone());
        }
        ViewKind::Heatmap {
            x,
            y,
            value,
            aggregate,
            bucket,
        } => {
            raw.x = Some(x.clone());
            raw.y = Some(y.clone());
            raw.value = value.clone();
            raw.aggregate = Some(*aggregate);
            raw.bucket = *bucket;
        }
    }

    raw
}

fn raw_metric_row_from(row: &MetricRow) -> RawMetricRow {
    RawMetricRow {
        label: row.label.clone(),
        aggregate: row.aggregate,
        value: row.value.clone(),
        where_clauses: row.where_clauses.clone(),
    }
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
    fn empty_views_list_parses_to_empty_views() {
        let yaml = "views: []\n";
        let parsed = parse_views(yaml).unwrap();
        assert!(parsed.views.is_empty());
    }

    #[test]
    fn directory_defaults_when_omitted() {
        let yaml = "views: []\n";
        let parsed = parse_views(yaml).unwrap();
        assert_eq!(parsed.output_dir, PathBuf::from("views"));
    }

    #[test]
    fn directory_overrides_default() {
        let yaml = "directory: rendered/views\nviews: []\n";
        let parsed = parse_views(yaml).unwrap();
        assert_eq!(parsed.output_dir, PathBuf::from("rendered/views"));
    }

    #[test]
    fn empty_object_rejected() {
        // `{}` has no `views` key. The schema requires it; the parser does too.
        let err = parse_views("{}").unwrap_err();
        assert!(matches!(err, ViewsLoadError::InvalidYaml(_)), "got {err:?}");
    }

    #[test]
    fn empty_file_rejected() {
        // An empty file has no document. The schema requires `views`.
        let err = parse_views("").unwrap_err();
        assert!(matches!(err, ViewsLoadError::InvalidYaml(_)), "got {err:?}");
    }

    #[test]
    fn parse_board() {
        let view =
            parse_single("views:\n  - id: status-board\n    type: board\n    field: status\n");
        assert_eq!(view.id, "status-board");
        assert!(view.where_clauses.is_empty());
        assert!(view.title.is_none());
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
        match view.kind {
            ViewKind::Tree { field, columns } => {
                assert_eq!(field, "parent");
                assert!(columns.is_empty(), "no `columns:` slot → empty list");
            }
            other => panic!("expected Tree, got {other:?}"),
        }
    }

    #[test]
    fn parse_tree_with_columns() {
        let view = parse_single(
            "views:\n  - id: h\n    type: tree\n    field: parent\n    columns: [status, points]\n",
        );
        match view.kind {
            ViewKind::Tree { columns, .. } => assert_eq!(columns, vec!["status", "points"]),
            other => panic!("expected Tree, got {other:?}"),
        }
    }

    #[test]
    fn parse_graph() {
        let view = parse_single("views:\n  - id: d\n    type: graph\n    field: depends_on\n");
        match view.kind {
            ViewKind::Graph { field, group_by } => {
                assert_eq!(field, "depends_on");
                assert!(group_by.is_none());
            }
            other => panic!("expected Graph, got {other:?}"),
        }
    }

    #[test]
    fn parse_graph_with_group_by() {
        let view = parse_single(
            "views:\n  - id: d\n    type: graph\n    field: depends_on\n    group_by: parent\n",
        );
        match view.kind {
            ViewKind::Graph { field, group_by } => {
                assert_eq!(field, "depends_on");
                assert_eq!(group_by.as_deref(), Some("parent"));
            }
            other => panic!("expected Graph, got {other:?}"),
        }
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
            ViewKind::Gantt {
                start,
                end,
                duration,
                after,
                group,
            } => {
                assert_eq!(start, "start_date");
                assert_eq!(end.as_deref(), Some("end_date"));
                assert_eq!(duration, None);
                assert_eq!(after, None);
                assert_eq!(group.as_deref(), Some("parent"));
            }
            other => panic!("expected Gantt, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_with_duration() {
        let view = parse_single(
            "views:\n  - id: roadmap\n    type: gantt\n    start: start_date\n    duration: estimate\n",
        );
        match view.kind {
            ViewKind::Gantt {
                start,
                end,
                duration,
                after,
                group,
            } => {
                assert_eq!(start, "start_date");
                assert_eq!(end, None);
                assert_eq!(duration.as_deref(), Some("estimate"));
                assert_eq!(after, None);
                assert_eq!(group, None);
            }
            other => panic!("expected Gantt, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_with_after() {
        let view = parse_single(
            "views:\n  - id: roadmap\n    type: gantt\n    start: start_date\n    duration: estimate\n    after: depends_on\n",
        );
        match view.kind {
            ViewKind::Gantt {
                start,
                end,
                duration,
                after,
                group,
            } => {
                assert_eq!(start, "start_date");
                assert_eq!(end, None);
                assert_eq!(duration.as_deref(), Some("estimate"));
                assert_eq!(after.as_deref(), Some("depends_on"));
                assert_eq!(group, None);
            }
            other => panic!("expected Gantt, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_after_omitted_leaves_none() {
        let view = parse_single(
            "views:\n  - id: roadmap\n    type: gantt\n    start: start_date\n    end: end_date\n",
        );
        match view.kind {
            ViewKind::Gantt { after, .. } => assert_eq!(after, None),
            other => panic!("expected Gantt, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_by_initiative_with_end() {
        let view = parse_single(
            "views:\n  - id: r\n    type: gantt_by_initiative\n    start: start_date\n    end: end_date\n    root_link: parent\n",
        );
        match view.kind {
            ViewKind::GanttByInitiative {
                start,
                end,
                duration,
                after,
                root_link,
            } => {
                assert_eq!(start, "start_date");
                assert_eq!(end.as_deref(), Some("end_date"));
                assert_eq!(duration, None);
                assert_eq!(after, None);
                assert_eq!(root_link, "parent");
            }
            other => panic!("expected GanttByInitiative, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_by_initiative_with_duration() {
        let view = parse_single(
            "views:\n  - id: r\n    type: gantt_by_initiative\n    start: start_date\n    duration: estimate\n    root_link: parent\n",
        );
        match view.kind {
            ViewKind::GanttByInitiative {
                duration,
                root_link,
                ..
            } => {
                assert_eq!(duration.as_deref(), Some("estimate"));
                assert_eq!(root_link, "parent");
            }
            other => panic!("expected GanttByInitiative, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_by_initiative_with_after() {
        let view = parse_single(
            "views:\n  - id: r\n    type: gantt_by_initiative\n    start: start_date\n    duration: estimate\n    after: depends_on\n    root_link: parent\n",
        );
        match view.kind {
            ViewKind::GanttByInitiative { after, .. } => {
                assert_eq!(after.as_deref(), Some("depends_on"));
            }
            other => panic!("expected GanttByInitiative, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_by_initiative_missing_root_link_rejected() {
        let yaml = "views:\n  - id: r\n    type: gantt_by_initiative\n    start: start_date\n    end: end_date\n";
        let err = parse_views(yaml).unwrap_err();
        match err {
            ViewsLoadError::Validation(errors) => {
                assert!(matches!(
                    errors.as_slice(),
                    [ViewsValidationError::MissingSlot { id, slot, .. }]
                        if id == "r" && *slot == "root_link"
                ));
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_by_depth_with_end() {
        let view = parse_single(
            "views:\n  - id: r\n    type: gantt_by_depth\n    start: start_date\n    end: end_date\n    depth_link: parent\n",
        );
        match view.kind {
            ViewKind::GanttByDepth {
                start,
                end,
                duration,
                after,
                depth_link,
            } => {
                assert_eq!(start, "start_date");
                assert_eq!(end.as_deref(), Some("end_date"));
                assert_eq!(duration, None);
                assert_eq!(after, None);
                assert_eq!(depth_link, "parent");
            }
            other => panic!("expected GanttByDepth, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_by_depth_with_duration() {
        let view = parse_single(
            "views:\n  - id: r\n    type: gantt_by_depth\n    start: start_date\n    duration: estimate\n    depth_link: parent\n",
        );
        match view.kind {
            ViewKind::GanttByDepth {
                duration,
                depth_link,
                ..
            } => {
                assert_eq!(duration.as_deref(), Some("estimate"));
                assert_eq!(depth_link, "parent");
            }
            other => panic!("expected GanttByDepth, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_by_depth_with_after() {
        let view = parse_single(
            "views:\n  - id: r\n    type: gantt_by_depth\n    start: start_date\n    duration: estimate\n    after: depends_on\n    depth_link: parent\n",
        );
        match view.kind {
            ViewKind::GanttByDepth { after, .. } => {
                assert_eq!(after.as_deref(), Some("depends_on"));
            }
            other => panic!("expected GanttByDepth, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_by_depth_missing_depth_link_rejected() {
        let yaml = "views:\n  - id: r\n    type: gantt_by_depth\n    start: start_date\n    end: end_date\n";
        let err = parse_views(yaml).unwrap_err();
        match err {
            ViewsLoadError::Validation(errors) => {
                assert!(matches!(
                    errors.as_slice(),
                    [ViewsValidationError::MissingSlot { id, slot, .. }]
                        if id == "r" && *slot == "depth_link"
                ));
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn parse_gantt_by_initiative_missing_start_rejected() {
        let yaml = "views:\n  - id: r\n    type: gantt_by_initiative\n    end: end_date\n    root_link: parent\n";
        let err = parse_views(yaml).unwrap_err();
        match err {
            ViewsLoadError::Validation(errors) => {
                assert!(matches!(
                    errors.as_slice(),
                    [ViewsValidationError::MissingSlot { id, slot, .. }]
                        if id == "r" && *slot == "start"
                ));
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn parse_bar_chart() {
        let view = parse_single(
            "views:\n  - id: by-status\n    type: bar_chart\n    group_by: status\n    value: effort\n    aggregate: sum\n",
        );
        match view.kind {
            ViewKind::BarChart {
                group_by,
                value,
                aggregate,
            } => {
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
        match view.kind {
            ViewKind::LineChart { x, y, group } => {
                assert_eq!(x, "estimate");
                assert_eq!(y, "actual_effort");
                assert_eq!(group, None);
            }
            other => panic!("expected LineChart, got {other:?}"),
        }
    }

    #[test]
    fn parse_line_chart_with_group() {
        let view = parse_single(
            "views:\n  - id: eva\n    type: line_chart\n    x: estimate\n    y: actual_effort\n    group: assignee\n",
        );
        match view.kind {
            ViewKind::LineChart { group, .. } => assert_eq!(group.as_deref(), Some("assignee")),
            other => panic!("expected LineChart, got {other:?}"),
        }
    }

    #[test]
    fn parse_workload() {
        let view = parse_single(
            "views:\n  - id: cap\n    type: workload\n    start: start_date\n    end: end_date\n    effort: effort\n",
        );
        match view.kind {
            ViewKind::Workload { working_days, .. } => {
                assert!(working_days.is_none(), "no override means inherit config")
            }
            other => panic!("expected Workload, got {other:?}"),
        }
    }

    #[test]
    fn parse_workload_with_working_days_override() {
        let view = parse_single(
            "views:\n  - id: cap\n    type: workload\n    start: start_date\n    end: end_date\n    effort: effort\n    working_days: [monday, wednesday, friday]\n",
        );
        match view.kind {
            ViewKind::Workload { working_days, .. } => {
                let days = working_days.expect("working_days override should parse");
                assert_eq!(
                    days,
                    vec![Weekday::Monday, Weekday::Wednesday, Weekday::Friday]
                );
            }
            other => panic!("expected Workload, got {other:?}"),
        }
    }

    #[test]
    fn parse_workload_rejects_abbreviated_day() {
        // Memory rule: full day names only.
        let yaml = "views:\n  - id: cap\n    type: workload\n    start: s\n    end: e\n    effort: f\n    working_days: [mon]\n";
        assert!(parse_views(yaml).is_err());
    }

    #[test]
    fn parse_metric_count() {
        let view = parse_single(
            "views:\n  - id: open\n    type: metric\n    metrics:\n      - aggregate: count\n        label: Open items\n",
        );
        match view.kind {
            ViewKind::Metric { metrics } => {
                assert_eq!(metrics.len(), 1);
                assert_eq!(metrics[0].aggregate, Aggregate::Count);
                assert_eq!(metrics[0].label.as_deref(), Some("Open items"));
                assert!(metrics[0].value.is_none());
                assert!(metrics[0].where_clauses.is_empty());
            }
            other => panic!("expected Metric, got {other:?}"),
        }
    }

    #[test]
    fn parse_metric_multiple_rows() {
        let yaml = r#"
views:
  - id: stats
    type: metric
    metrics:
      - label: Total
        aggregate: count
      - label: In progress
        aggregate: count
        where: ["status=in_progress"]
      - label: Sum points
        aggregate: sum
        value: points
"#;
        let view = parse_single(yaml);
        match view.kind {
            ViewKind::Metric { metrics } => {
                assert_eq!(metrics.len(), 3);
                assert_eq!(metrics[0].label.as_deref(), Some("Total"));
                assert_eq!(metrics[0].aggregate, Aggregate::Count);
                assert!(metrics[0].where_clauses.is_empty());

                assert_eq!(metrics[1].label.as_deref(), Some("In progress"));
                assert_eq!(metrics[1].where_clauses, vec!["status=in_progress"]);

                assert_eq!(metrics[2].aggregate, Aggregate::Sum);
                assert_eq!(metrics[2].value.as_deref(), Some("points"));
            }
            other => panic!("expected Metric, got {other:?}"),
        }
    }

    #[test]
    fn parse_metric_empty_metrics_allowed() {
        let view = parse_single("views:\n  - id: empty\n    type: metric\n    metrics: []\n");
        match view.kind {
            ViewKind::Metric { metrics } => assert!(metrics.is_empty()),
            other => panic!("expected Metric, got {other:?}"),
        }
    }

    #[test]
    fn parse_metric_missing_metrics_rejected() {
        let yaml = "views:\n  - id: m\n    type: metric\n";
        let err = parse_views(yaml).unwrap_err();
        match err {
            ViewsLoadError::Validation(errors) => {
                assert!(matches!(
                    errors.as_slice(),
                    [ViewsValidationError::MissingSlot { id, slot, .. }]
                        if id == "m" && *slot == "metrics"
                ));
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    #[test]
    fn parse_metric_row_missing_aggregate_rejected() {
        // `aggregate` is required on each row — serde catches this as a
        // missing required field on `RawMetricRow`.
        let yaml = "views:\n  - id: m\n    type: metric\n    metrics:\n      - label: oops\n";
        let err = parse_views(yaml).unwrap_err();
        assert!(matches!(err, ViewsLoadError::InvalidYaml(_)), "got {err:?}");
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

    // ── Title slot (cross-cutting) ─────────────────────────────────

    #[test]
    fn parse_title_on_board() {
        let view = parse_single(
            "views:\n  - id: b\n    type: board\n    field: status\n    title: title\n",
        );
        assert_eq!(view.title.as_deref(), Some("title"));
    }

    #[test]
    fn parse_title_accepted_on_every_view_type() {
        // One entry per view type, each with `title: title`. Confirms the
        // slot is flat at the view level — every variant picks it up.
        let yaml = r#"
views:
  - id: v-board
    type: board
    field: status
    title: title
  - id: v-tree
    type: tree
    field: parent
    title: title
  - id: v-graph
    type: graph
    field: depends_on
    title: title
  - id: v-table
    type: table
    columns: [id, title]
    title: title
  - id: v-gantt
    type: gantt
    start: start_date
    end: end_date
    title: title
  - id: v-gantt-by-initiative
    type: gantt_by_initiative
    start: start_date
    end: end_date
    root_link: parent
    title: title
  - id: v-bar
    type: bar_chart
    group_by: status
    aggregate: count
    title: title
  - id: v-line
    type: line_chart
    x: estimate
    y: actual_effort
    title: title
  - id: v-workload
    type: workload
    start: start_date
    end: end_date
    effort: effort
    title: title
  - id: v-metric
    type: metric
    metrics:
      - aggregate: count
    title: title
  - id: v-treemap
    type: treemap
    group: parent
    size: effort
    title: title
  - id: v-heatmap
    type: heatmap
    x: end_date
    y: assignee
    aggregate: count
    title: title
"#;
        let parsed = parse_views(yaml).unwrap();
        assert_eq!(parsed.views.len(), 12);
        for view in &parsed.views {
            assert_eq!(
                view.title.as_deref(),
                Some("title"),
                "view {} did not carry the title slot",
                view.id
            );
        }
    }

    #[test]
    fn title_omitted_leaves_none() {
        let view = parse_single("views:\n  - id: v\n    type: board\n    field: status\n");
        assert!(view.title.is_none());
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
        // tree missing `field`, bar_chart missing `aggregate` — both
        // produce parse-stage MissingSlot errors that stack.
        let yaml = "views:\n  - id: x\n    type: tree\n  - id: y\n    type: bar_chart\n    group_by: status\n";
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
  - id: roadmap-by-initiative
    type: gantt_by_initiative
    start: start_date
    end: end_date
    root_link: parent
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
    metrics:
      - aggregate: count
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
        assert_eq!(parsed.views.len(), 12);
    }

    // ── Serialization round-trip ───────────────────────────────────

    /// The same broad fixture as `parses_full_example`, plus the slots
    /// that example omits (gantt_by_depth, metric `where`, workload
    /// `working_days`, heatmap with a non-default directory) so the
    /// round-trip exercises every kind and every optional slot.
    const ROUND_TRIP_FIXTURE: &str = r#"
directory: rendered/views
views:
  - id: status-board
    type: board
    field: status
    title: title
    where:
      - "type=issue"
      - "status!=removed"
  - id: hierarchy
    type: tree
    field: parent
    columns: [status, points]
  - id: bare-tree
    type: tree
    field: parent
  - id: deps
    type: graph
    field: depends_on
    group_by: parent
  - id: all-items
    type: table
    columns: [id, title, status]
  - id: roadmap
    type: gantt
    start: start_date
    duration: estimate
    after: depends_on
  - id: roadmap-by-initiative
    type: gantt_by_initiative
    start: start_date
    end: end_date
    root_link: parent
  - id: roadmap-by-depth
    type: gantt_by_depth
    start: start_date
    duration: estimate
    depth_link: parent
  - id: effort-by-status
    type: bar_chart
    group_by: status
    value: effort
    aggregate: sum
  - id: estimate-vs-actual
    type: line_chart
    x: estimate
    y: actual_effort
    group: assignee
  - id: capacity
    type: workload
    start: start_date
    end: end_date
    effort: effort
    working_days: [monday, wednesday, friday]
  - id: stats
    type: metric
    metrics:
      - label: Total
        aggregate: count
      - label: In progress
        aggregate: sum
        value: points
        where: ["status=in_progress"]
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

    #[test]
    fn serialize_then_parse_yields_equal_model() {
        let original = parse_views(ROUND_TRIP_FIXTURE).unwrap();
        let serialized = serialize_views(&original).unwrap();
        let reparsed = parse_views(&serialized).unwrap();
        assert_eq!(
            original, reparsed,
            "round-trip changed the model.\n--- serialized ---\n{serialized}"
        );
    }

    #[test]
    fn serialize_omits_default_directory() {
        let views = parse_views("views:\n  - id: b\n    type: board\n    field: status\n").unwrap();
        let serialized = serialize_views(&views).unwrap();
        assert!(
            !serialized.contains("directory:"),
            "default directory should not be emitted, got:\n{serialized}"
        );
    }

    #[test]
    fn serialize_omits_empty_where() {
        let views = parse_views("views:\n  - id: b\n    type: board\n    field: status\n").unwrap();
        let serialized = serialize_views(&views).unwrap();
        assert!(
            !serialized.contains("where:"),
            "empty where should not be emitted, got:\n{serialized}"
        );
    }

    #[test]
    fn view_from_value_builds_validated_view() {
        let value: serde_yaml::Value =
            serde_yaml::from_str("id: b\ntype: board\nfield: status\n").unwrap();
        let view = view_from_value(value).unwrap();
        assert_eq!(view.id, "b");
        assert!(matches!(view.kind, ViewKind::Board { field } if field == "status"));
    }

    #[test]
    fn view_from_value_rejects_missing_required_slot() {
        let value: serde_yaml::Value = serde_yaml::from_str("id: b\ntype: board\n").unwrap();
        let error = view_from_value(value).unwrap_err();
        assert!(matches!(error, ViewsLoadError::Validation(_)), "got {error:?}");
    }

    #[test]
    fn view_from_value_rejects_unknown_slot() {
        let value: serde_yaml::Value =
            serde_yaml::from_str("id: b\ntype: board\nfield: status\nbogus: x\n").unwrap();
        let error = view_from_value(value).unwrap_err();
        assert!(matches!(error, ViewsLoadError::InvalidYaml(_)), "got {error:?}");
    }
}
