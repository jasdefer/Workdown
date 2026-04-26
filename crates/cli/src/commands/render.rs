//! `workdown render` — writes view files to `views/<id>.md`.
//!
//! Orchestration only: loads schema, items, and views.yaml; runs cross-file
//! validation; dispatches each view to the matching renderer; writes the
//! result to disk. The actual Markdown formatting lives in `crate::render`.
//!
//! Error policy (per project decisions):
//! - Missing `views.yaml` → info log, exit 0.
//! - Per-item load errors → warn, continue with what loaded.
//! - Per-view `views_check` failures → warn, skip that view.
//! - Unimplemented renderer, bulk mode → warn, skip.
//! - Unimplemented renderer, single-view mode → hard error.
//! - Unknown view id (single-view mode) → hard error.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use workdown_core::model::config::Config;
use workdown_core::model::diagnostic::{Diagnostic, DiagnosticKind};
use workdown_core::model::views::{View, Views};
use workdown_core::parser;
use workdown_core::store::Store;
use workdown_core::view_data::{self, ViewData};
use workdown_core::views_check;

use crate::cli::output;
use crate::render;

pub fn run_render(
    config: &Config,
    project_root: &Path,
    view_id: Option<&str>,
) -> anyhow::Result<ExitCode> {
    let schema_path = project_root.join(&config.schema);
    let items_path = project_root.join(&config.paths.work_items);
    let views_path = project_root.join(&config.paths.views);

    let schema = parser::schema::load_schema(&schema_path)
        .map_err(|e| anyhow::anyhow!("failed to load schema: {e}"))?;

    let store = Store::load(&items_path, &schema)
        .map_err(|e| anyhow::anyhow!("failed to read items directory: {e}"))?;
    for diagnostic in store.diagnostics() {
        output::warning(&diagnostic.to_string());
    }

    if !views_path.exists() {
        tracing::info!(path = %views_path.display(), "no views.yaml — nothing to render");
        return Ok(ExitCode::SUCCESS);
    }

    let views = parser::views::load_views(&views_path)
        .map_err(|e| anyhow::anyhow!("failed to load views: {e}"))?;

    let views_check_diagnostics = views_check::evaluate(&views, &schema);
    let invalid_view_ids = invalid_view_ids(&views_check_diagnostics);
    for diagnostic in &views_check_diagnostics {
        output::warning(&diagnostic.to_string());
    }

    // Climb out of the output directory back to project root, then down
    // into the work items dir. Each component of `output_dir` adds one
    // `../` so nested output paths (e.g. `rendered/views`) still produce
    // working links.
    let depth = views.output_dir.components().count();
    let link_base = format!(
        "{}{}",
        "../".repeat(depth),
        config.paths.work_items.display()
    );
    let output_dir = project_root.join(&views.output_dir);

    match view_id {
        Some(id) => render_single(
            &views,
            id,
            &invalid_view_ids,
            &store,
            &schema,
            &output_dir,
            &link_base,
        ),
        None => render_all(
            &views,
            &invalid_view_ids,
            &store,
            &schema,
            &output_dir,
            &link_base,
        ),
    }
}

fn render_single(
    views: &Views,
    view_id: &str,
    invalid_view_ids: &HashSet<String>,
    store: &Store,
    schema: &workdown_core::model::schema::Schema,
    output_dir: &Path,
    link_base: &str,
) -> anyhow::Result<ExitCode> {
    let view = views
        .views
        .iter()
        .find(|view| view.id == view_id)
        .ok_or_else(|| anyhow::anyhow!("no view with id '{view_id}' in views.yaml"))?;

    if invalid_view_ids.contains(&view.id) {
        anyhow::bail!("view '{}' failed validation — see warnings above", view.id);
    }

    let view_data = view_data::extract(view, store, schema);
    let markdown = render_view_data(&view_data, link_base).ok_or_else(|| {
        anyhow::anyhow!(
            "renderer for view type '{}' not yet implemented",
            view.kind.view_type()
        )
    })?;

    ensure_output_dir(output_dir)?;
    let path = write_view_file(output_dir, &view.id, &markdown)?;
    output::success(&format!("Wrote {}", path.display()));
    Ok(ExitCode::SUCCESS)
}

fn render_all(
    views: &Views,
    invalid_view_ids: &HashSet<String>,
    store: &Store,
    schema: &workdown_core::model::schema::Schema,
    output_dir: &Path,
    link_base: &str,
) -> anyhow::Result<ExitCode> {
    if views.views.is_empty() {
        tracing::info!("views.yaml has no entries — nothing to render");
        return Ok(ExitCode::SUCCESS);
    }

    let renderable: Vec<&View> = views
        .views
        .iter()
        .filter(|view| !invalid_view_ids.contains(&view.id))
        .collect();

    if renderable.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    ensure_output_dir(output_dir)?;

    for view in renderable {
        let view_data = view_data::extract(view, store, schema);
        match render_view_data(&view_data, link_base) {
            Some(markdown) => {
                let path = write_view_file(output_dir, &view.id, &markdown)?;
                output::success(&format!("Wrote {}", path.display()));
            }
            None => {
                output::warning(&format!(
                    "view '{}': renderer for type '{}' not yet implemented — skipped",
                    view.id,
                    view.kind.view_type()
                ));
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

/// Dispatch a `ViewData` to the matching renderer.
///
/// Returns `None` for view types whose renderer is not yet implemented;
/// callers decide whether that's fatal (single-view mode) or skippable
/// (bulk mode). Each new renderer moves one arm from the fallthrough
/// into the match.
fn render_view_data(view_data: &ViewData, link_base: &str) -> Option<String> {
    match view_data {
        ViewData::Board(data) => Some(render::board::render_board(data, link_base)),
        ViewData::Tree(data) => Some(render::tree::render_tree(data, link_base)),
        ViewData::Graph(data) => Some(render::graph::render_graph(data)),
        ViewData::Table(data) => Some(render::table::render_table(data, link_base)),
        ViewData::BarChart(_)
        | ViewData::Gantt(_)
        | ViewData::Heatmap(_)
        | ViewData::LineChart(_)
        | ViewData::Metric(_)
        | ViewData::Treemap(_)
        | ViewData::Workload(_) => None,
    }
}

/// Extract the set of view ids that failed `views_check` validation.
///
/// Every view-level `DiagnosticKind` variant carries a `view_id` field;
/// we union them so callers can filter `views.views` in one pass.
fn invalid_view_ids(diagnostics: &[Diagnostic]) -> HashSet<String> {
    diagnostics
        .iter()
        .filter_map(|diagnostic| match &diagnostic.kind {
            DiagnosticKind::ViewDuplicateId { view_id }
            | DiagnosticKind::ViewMissingSlot { view_id, .. }
            | DiagnosticKind::ViewUnknownField { view_id, .. }
            | DiagnosticKind::ViewFieldTypeMismatch { view_id, .. }
            | DiagnosticKind::ViewWhereParseError { view_id, .. }
            | DiagnosticKind::ViewBucketWithoutDateAxis { view_id }
            | DiagnosticKind::ViewCountAggregateWithValue { view_id }
            | DiagnosticKind::ViewAggregateTypeMismatch { view_id, .. }
            | DiagnosticKind::ViewGroupByCyclic { view_id, .. }
            | DiagnosticKind::ViewGroupByInverseNotAllowed { view_id, .. } => Some(view_id.clone()),
            _ => None,
        })
        .collect()
}

fn ensure_output_dir(output_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(output_dir).map_err(|e| {
        anyhow::anyhow!(
            "failed to create output directory '{}': {e}",
            output_dir.display()
        )
    })
}

fn write_view_file(output_dir: &Path, view_id: &str, markdown: &str) -> anyhow::Result<PathBuf> {
    let path = output_dir.join(format!("{view_id}.md"));
    std::fs::write(&path, markdown)
        .map_err(|e| anyhow::anyhow!("failed to write '{}': {e}", path.display()))?;
    Ok(path)
}
