//! `workdown render` — writes view files to `views/<id>.md`.
//!
//! Orchestration: loads the project via `core::load_project`, dispatches
//! each view to the matching renderer, writes the result to disk. The
//! actual Markdown formatting lives in `crate::render`.
//!
//! Error policy (per project decisions):
//! - Missing `views.yaml` → info log, exit 0.
//! - Per-item load errors → warn, continue with what loaded.
//! - Per-view `views_check` *errors* → warn, skip that view. A
//!   warning-severity finding is reported but the view still renders.
//! - Unknown view id (single-view mode) → hard error.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use workdown_core::model::calendar::WorkingCalendar;
use workdown_core::model::config::Config;
use workdown_core::model::diagnostic::Diagnostic;
use workdown_core::model::schema::Schema;
use workdown_core::model::views::{rendered_view_path, View, Views};
use workdown_core::project::load_project;
use workdown_core::store::Store;
use workdown_core::view_data::{self, CheckedView, ViewData};

use crate::cli::output;
use crate::render;

pub fn run_render(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    view_id: Option<&str>,
    as_of: Option<chrono::NaiveDate>,
) -> anyhow::Result<ExitCode> {
    let project = load_project(config, project_root, config_path, as_of)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Order preserved: store diagnostics first (broken links, missing
    // fields), then cycles + rules, then views.
    output::surface_diagnostics(&project.diagnostics);

    // When any computed field reads `$today`, the rendered output is a
    // function of the calendar, not just the repository — say so, so a
    // surprising diff on an untouched repo has its explanation attached.
    if schema_references_today(&project.schema) {
        output::info(&format!(
            "output depends on the current date (evaluated as of {}); pin with --as-of for reproducible renders",
            project.evaluation_date.format("%Y-%m-%d"),
        ));
    }

    let Some(views) = project.views.as_ref() else {
        let views_path = project_root.join(&config.paths.views);
        tracing::info!(path = %views_path.display(), "no views.yaml — nothing to render");
        return Ok(ExitCode::SUCCESS);
    };

    // Fill unset display roles from `defaults.display` in config.yaml.
    // Applied after validation so diagnostics keep pointing at what the
    // user actually wrote in views.yaml.
    let views = views
        .clone()
        .with_display_defaults(&config.defaults.display);
    let views = &views;

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
            views,
            id,
            &project.diagnostics,
            &project.store,
            &project.schema,
            &project.calendar,
            &output_dir,
            &link_base,
        ),
        None => render_all(
            views,
            &project.diagnostics,
            &project.store,
            &project.schema,
            &project.calendar,
            &output_dir,
            &link_base,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_single(
    views: &Views,
    view_id: &str,
    diagnostics: &[Diagnostic],
    store: &Store,
    schema: &workdown_core::model::schema::Schema,
    calendar: &WorkingCalendar,
    output_dir: &Path,
    link_base: &str,
) -> anyhow::Result<ExitCode> {
    let view = views
        .views
        .iter()
        .find(|view| view.id == view_id)
        .ok_or_else(|| anyhow::anyhow!("no view with id '{view_id}' in views.yaml"))?;

    let Some(checked) = CheckedView::new(view, diagnostics) else {
        anyhow::bail!("view '{}' failed validation — see warnings above", view.id);
    };

    let view_data = view_data::extract(checked, store, schema, calendar);
    emit_unplaced_warnings(view, &view_data);
    let description = render::description::description_for(view);
    let markdown = render_view_data(&view_data, link_base, &description);

    ensure_output_dir(output_dir)?;
    let path = write_view_file(output_dir, &view.id, &markdown)?;
    output::success(&format!("Wrote {}", path.display()));
    Ok(ExitCode::SUCCESS)
}

fn render_all(
    views: &Views,
    diagnostics: &[Diagnostic],
    store: &Store,
    schema: &workdown_core::model::schema::Schema,
    calendar: &WorkingCalendar,
    output_dir: &Path,
    link_base: &str,
) -> anyhow::Result<ExitCode> {
    if views.views.is_empty() {
        tracing::info!("views.yaml has no entries — nothing to render");
        return Ok(ExitCode::SUCCESS);
    }

    let renderable: Vec<CheckedView<'_>> = views
        .views
        .iter()
        .filter_map(|view| CheckedView::new(view, diagnostics))
        .collect();

    if renderable.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    ensure_output_dir(output_dir)?;

    for checked in renderable {
        let view = checked.view();
        let view_data = view_data::extract(checked, store, schema, calendar);
        emit_unplaced_warnings(view, &view_data);
        let description = render::description::description_for(view);
        let markdown = render_view_data(&view_data, link_base, &description);
        let path = write_view_file(output_dir, &view.id, &markdown)?;
        output::success(&format!("Wrote {}", path.display()));
    }

    Ok(ExitCode::SUCCESS)
}

/// Dispatch a `ViewData` to the matching renderer. Each renderer that
/// shows item-level marks takes `link_base` to build relative links
/// from the rendered file back to `workdown-items/<id>.md`; renderers
/// that only emit aggregates or use Mermaid bar labels (`gantt`,
/// `gantt_by_*`, `graph`, `metric`) ignore it.
fn render_view_data(view_data: &ViewData, link_base: &str, description: &str) -> String {
    match view_data {
        ViewData::Board(data) => render::board::render_board(data, link_base, description),
        ViewData::Tree(data) => render::tree::render_tree(data, link_base, description),
        ViewData::Graph(data) => render::graph::render_graph(data, description),
        ViewData::Table(data) => render::table::render_table(data, link_base, description),
        ViewData::Gantt(data) => render::gantt::render_gantt(data, description),
        ViewData::GanttByDepth(data) => {
            render::gantt_by_depth::render_gantt_by_depth(data, description)
        }
        ViewData::GanttByInitiative(data) => {
            render::gantt_by_initiative::render_gantt_by_initiative(data, description)
        }
        ViewData::Metric(data) => render::metric::render_metric(data, description),
        ViewData::Treemap(data) => render::treemap::render_treemap(data, link_base, description),
        ViewData::LineChart(data) => {
            render::line_chart::render_line_chart(data, link_base, description)
        }
        ViewData::BarChart(data) => {
            render::bar_chart::render_bar_chart(data, link_base, description)
        }
        ViewData::Heatmap(data) => render::heatmap::render_heatmap(data, link_base, description),
        ViewData::Workload(data) => render::workload::render_workload(data, link_base, description),
    }
}

/// Surface any unplaced items from a view's extraction as CLI warnings.
///
/// The renderer already includes a footer in the rendered Markdown for
/// users who open the file; this is the parallel terminal-side notice
/// so it doesn't go unnoticed when running `workdown render` in CI or
/// pre-commit. Exhaustive over [`ViewData`] so a new view kind fails
/// compilation here and decides deliberately whether it has unplaced
/// items to warn about; board/table/tree/graph place every
/// filter-matched item by construction.
fn emit_unplaced_warnings(view: &View, view_data: &ViewData) {
    let count = match view_data {
        ViewData::Gantt(data) => data.unplaced.len(),
        ViewData::GanttByDepth(data) => data.unplaced.len(),
        ViewData::GanttByInitiative(data) => data.unplaced.len(),
        ViewData::Metric(data) => data.rows.iter().map(|row| row.unplaced.len()).sum(),
        ViewData::Treemap(data) => data.unplaced.len(),
        ViewData::LineChart(data) => data.unplaced.len(),
        ViewData::BarChart(data) => data.unplaced.len(),
        ViewData::Heatmap(data) => data.unplaced.len(),
        ViewData::Workload(data) => data.unplaced.len(),
        ViewData::Board(_) | ViewData::Table(_) | ViewData::Tree(_) | ViewData::Graph(_) => 0,
    };
    if count > 0 {
        output::warning(&format!(
            "view '{}': {} dropped — see footer",
            view.id,
            render::markdown::pluralize(count, "item"),
        ));
    }
}

/// Whether any computed field's expression reads `$today` — the static
/// signal that this project's derived values depend on the clock.
fn schema_references_today(schema: &Schema) -> bool {
    schema
        .fields
        .values()
        .filter_map(|field_definition| field_definition.compute.as_ref())
        .any(|config| config.expression.references_today())
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
    // The naming rule lives in core (`rendered_view_path`) so the
    // view-write housekeeping that removes stale files after a delete or
    // rename can never disagree with where this writes.
    let path = rendered_view_path(output_dir, view_id);
    std::fs::write(&path, markdown)
        .map_err(|e| anyhow::anyhow!("failed to write '{}': {e}", path.display()))?;
    Ok(path)
}
