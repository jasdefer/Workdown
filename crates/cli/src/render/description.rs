//! One-line view descriptions for rendered Markdown files.
//!
//! Builds a short prose caption that goes between a rendered view's
//! `# Heading` and its content (chart, table, list). The caption tells
//! a reader what the view is showing — including the schema field names
//! it draws from — without having to flip back to `views.yaml`.
//!
//! Strings are computed from the view configuration, so renaming a
//! field in `views.yaml` is reflected in rendered descriptions the next
//! time `workdown render` runs.
//!
//! Renderers that don't need a description (e.g. `table`, where column
//! headers already convey the field names) can be omitted from this
//! module — callers tolerate an empty string.

use workdown_core::model::views::{View, ViewKind};

/// Build the caption for a view, ready to be emitted right under the
/// rendered `# Heading`. Returns an empty string for view kinds that
/// don't need a description (currently only `table`).
pub fn description_for(view: &View) -> String {
    match &view.kind {
        ViewKind::Board { field } => {
            format!("Cards grouped into columns by `{field}`.")
        }
        ViewKind::Tree { field } => {
            format!("Hierarchical outline following `{field}` upward to roots.")
        }
        ViewKind::Graph { field, group_by } => match group_by {
            Some(group) => {
                format!("Directed graph of items connected through `{field}`, nested by `{group}`.")
            }
            None => format!("Directed graph of items connected through `{field}`."),
        },
        ViewKind::Table { .. } | ViewKind::Metric { .. } => String::new(),
        ViewKind::Gantt {
            start,
            end,
            duration,
            after,
            group,
        } => {
            let prefix = gantt_input_mode_prefix(
                start,
                end.as_deref(),
                duration.as_deref(),
                after.as_deref(),
            );
            let mut out = format!("{prefix}.");
            if let Some(group) = group {
                out.pop();
                out.push_str(&format!(", grouped by `{group}`."));
            }
            out
        }
        ViewKind::GanttByInitiative {
            start,
            end,
            duration,
            after,
            root_link,
        } => {
            let prefix = gantt_input_mode_prefix(
                start,
                end.as_deref(),
                duration.as_deref(),
                after.as_deref(),
            );
            format!("{prefix}, partitioned by top-level ancestor in `{root_link}` — one chart per initiative.")
        }
        ViewKind::GanttByDepth {
            start,
            end,
            duration,
            after,
            depth_link,
        } => {
            let prefix = gantt_input_mode_prefix(
                start,
                end.as_deref(),
                duration.as_deref(),
                after.as_deref(),
            );
            format!("{prefix}, partitioned by depth in `{depth_link}` — one chart per level (0 = roots, 1 = children, ...).")
        }
        ViewKind::Treemap { group, size } => {
            format!("Hierarchical breakdown of `{size}` summed up the `{group}` chain.")
        }
        // Renderers below are not yet implemented; descriptions land when
        // their renderers do.
        ViewKind::BarChart { .. }
        | ViewKind::LineChart { .. }
        | ViewKind::Workload { .. }
        | ViewKind::Heatmap { .. } => String::new(),
    }
}

/// Pick the input-mode phrase shared by every gantt-family renderer.
///
/// Three modes, mirroring the validated combinations in `views_check`:
/// `start+end`, `start+duration`, `start+after+duration`. Caller appends
/// any partition or grouping suffix.
fn gantt_input_mode_prefix(
    start: &str,
    end: Option<&str>,
    duration: Option<&str>,
    after: Option<&str>,
) -> String {
    match (end, duration, after) {
        (Some(end), None, None) => {
            format!("Timeline of items from `{start}` to `{end}`")
        }
        (None, Some(duration), None) => {
            format!("Timeline of items starting at `{start}` for `{duration}` each")
        }
        (None, Some(duration), Some(after)) => {
            format!(
                "Timeline of items starting at `max({start}, predecessor end)` for `{duration}` each; predecessors from `{after}`"
            )
        }
        // Other combinations are rejected by `views_check`; reachable only
        // if validation was bypassed. Best-effort fallback.
        _ => format!("Timeline of items anchored at `{start}`"),
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use workdown_core::model::views::{View, ViewKind};

    fn view(kind: ViewKind) -> View {
        View {
            id: "v".into(),
            where_clauses: vec![],
            title: None,
            kind,
        }
    }

    #[test]
    fn board_describes_grouping_field() {
        let v = view(ViewKind::Board {
            field: "status".into(),
        });
        assert_eq!(
            description_for(&v),
            "Cards grouped into columns by `status`."
        );
    }

    #[test]
    fn tree_describes_link_field() {
        let v = view(ViewKind::Tree {
            field: "parent".into(),
        });
        assert_eq!(
            description_for(&v),
            "Hierarchical outline following `parent` upward to roots."
        );
    }

    #[test]
    fn graph_without_group_by_omits_nesting_phrase() {
        let v = view(ViewKind::Graph {
            field: "depends_on".into(),
            group_by: None,
        });
        assert_eq!(
            description_for(&v),
            "Directed graph of items connected through `depends_on`."
        );
    }

    #[test]
    fn graph_with_group_by_appends_nesting_phrase() {
        let v = view(ViewKind::Graph {
            field: "depends_on".into(),
            group_by: Some("parent".into()),
        });
        assert_eq!(
            description_for(&v),
            "Directed graph of items connected through `depends_on`, nested by `parent`."
        );
    }

    #[test]
    fn table_returns_empty() {
        let v = view(ViewKind::Table {
            columns: vec!["id".into(), "title".into()],
        });
        assert_eq!(description_for(&v), "");
    }

    #[test]
    fn gantt_start_end_mode() {
        let v = view(ViewKind::Gantt {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            group: None,
        });
        assert_eq!(
            description_for(&v),
            "Timeline of items from `start_date` to `end_date`."
        );
    }

    #[test]
    fn gantt_start_duration_mode() {
        let v = view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("duration".into()),
            after: None,
            group: None,
        });
        assert_eq!(
            description_for(&v),
            "Timeline of items starting at `start_date` for `duration` each."
        );
    }

    #[test]
    fn gantt_after_mode() {
        let v = view(ViewKind::Gantt {
            start: "start_date".into(),
            end: None,
            duration: Some("duration".into()),
            after: Some("depends_on".into()),
            group: None,
        });
        assert_eq!(
            description_for(&v),
            "Timeline of items starting at `max(start_date, predecessor end)` for `duration` each; predecessors from `depends_on`."
        );
    }

    #[test]
    fn gantt_with_group_appends_grouped_by() {
        let v = view(ViewKind::Gantt {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            group: Some("type".into()),
        });
        assert_eq!(
            description_for(&v),
            "Timeline of items from `start_date` to `end_date`, grouped by `type`."
        );
    }

    #[test]
    fn gantt_by_initiative_partition_phrase() {
        let v = view(ViewKind::GanttByInitiative {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            root_link: "parent".into(),
        });
        assert_eq!(
            description_for(&v),
            "Timeline of items from `start_date` to `end_date`, partitioned by top-level ancestor in `parent` — one chart per initiative."
        );
    }

    #[test]
    fn treemap_describes_size_and_group_fields() {
        let v = view(ViewKind::Treemap {
            group: "parent".into(),
            size: "effort".into(),
        });
        assert_eq!(
            description_for(&v),
            "Hierarchical breakdown of `effort` summed up the `parent` chain."
        );
    }

    #[test]
    fn gantt_by_depth_partition_phrase() {
        let v = view(ViewKind::GanttByDepth {
            start: "start_date".into(),
            end: Some("end_date".into()),
            duration: None,
            after: None,
            depth_link: "parent".into(),
        });
        assert_eq!(
            description_for(&v),
            "Timeline of items from `start_date` to `end_date`, partitioned by depth in `parent` — one chart per level (0 = roots, 1 = children, ...)."
        );
    }
}
