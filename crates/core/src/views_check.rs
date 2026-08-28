//! Cross-file validation for `views.yaml`.
//!
//! Catches bad view configs at `workdown validate` time rather than at
//! render time: field references that don't resolve against `schema.yaml`,
//! slot/type mismatches (e.g. `tree.field` pointing at a `choice`),
//! malformed `where:` expressions, and a handful of cross-slot constraints.
//!
//! After [`evaluate`] returns no errors, every field name referenced by
//! `views.yaml` is either present in `schema.fields`, is a recognized
//! relation name (forward link/links field name, or an inverse name from
//! `schema.inverse_table`), or is the virtual `"id"` field in a slot that
//! accepts it — text display roles (resolved specially at extraction) and
//! `where:` clauses. Structural slots (board `field`, chart grouping, axes,
//! date windows, aggregate values) reject it: the id is unique per item,
//! so grouping or plotting by it degenerates into one group per item.
//! The id *is* projected into `item.fields` at load, so such a view would
//! technically render — the rejection is policy against a meaningless
//! configuration, and renderers can rely on validated views never naming
//! `id` in a structural slot.
//!
//! Which field types each structural slot accepts is not written here: it is
//! [`crate::model::view_slots`], which the web UI's create form also reads
//! (as generated TypeScript) so the fields it offers are the fields this
//! module accepts. Cross-slot rules — a gantt's input modes, a heatmap's
//! bucket needing a date axis, `count` forbidding a `value` — stay here.
//!
//! The companion helper [`parse_errors_to_diagnostics`] converts load-time
//! errors from [`crate::parser::views`] into the same diagnostic stream,
//! so `workdown validate` can report them instead of aborting.

use std::path::Path;

use crate::display_check::{check_display_roles, RoleViolation};
use crate::model::diagnostic::{
    ConfigDiagnosticKind, Diagnostic, FileDiagnosticKind, ViewLocation,
};
use crate::model::resources::Resources;
use crate::model::schema::{
    is_relation_anchor, FieldDefinition, FieldType, FieldTypeConfig, Schema, Severity,
};
use crate::model::view_slots::{self, SlotSpec};
use crate::model::views::{Aggregate, MetricRow, View, ViewKind, Views};
use crate::parser::views::{ViewsLoadError, ViewsValidationError};
use crate::query::parse::parse_where;
use crate::query::types::{FieldReference, Predicate};
use crate::store::Store;
use crate::where_check;

// ── Public API ──────────────────────────────────────────────────────

/// Shared validation state threaded through every helper.
///
/// Carries the schema (referenced by all field-type checks) and the
/// `views.yaml` path (set as `source_path` on every emitted diagnostic).
struct ViewCheckContext<'a> {
    schema: &'a Schema,
    /// Resources and items are needed only by the operand checks in
    /// [`crate::where_check`] — a `resource:`-backed field's entries and
    /// the work item ids are option sets that live outside the schema.
    resources: &'a Resources,
    store: &'a Store,
    views_path: &'a Path,
}

impl ViewCheckContext<'_> {
    /// Construct a Config-scope diagnostic using this context's `views_path`.
    fn error(&self, kind: ConfigDiagnosticKind) -> Diagnostic {
        Diagnostic::config(Severity::Error, self.views_path.to_path_buf(), kind)
    }

    /// As [`ViewCheckContext::error`], for a finding that leaves the view
    /// renderable. See this module's severity contract.
    fn warning(&self, kind: ConfigDiagnosticKind) -> Diagnostic {
        Diagnostic::config(Severity::Warning, self.views_path.to_path_buf(), kind)
    }
}

/// Which view — and which metric row, when inside one — the current
/// check is running in. Pairs with a slot name to make the
/// [`ViewLocation`] a diagnostic carries.
///
/// This is the parameter that lets one check serve both loci: a helper
/// takes a locus rather than a bare view id, so the same code emits a
/// view-level finding or a `metrics[i]` one depending on where it was
/// called from.
#[derive(Clone, Copy)]
struct SlotLocus<'a> {
    view_id: &'a str,
    metric_index: Option<usize>,
}

impl<'a> SlotLocus<'a> {
    /// The view itself.
    fn view(view_id: &'a str) -> Self {
        Self {
            view_id,
            metric_index: None,
        }
    }

    /// The same view, narrowed to one row of its `metrics:` list.
    fn metric_row(self, metric_index: usize) -> Self {
        Self {
            metric_index: Some(metric_index),
            ..self
        }
    }

    /// Name a slot at this locus.
    fn at(self, slot: &'static str) -> ViewLocation {
        match self.metric_index {
            Some(metric_index) => ViewLocation::metric_row(self.view_id, metric_index, slot),
            None => ViewLocation::view(self.view_id, slot),
        }
    }
}

/// Run all cross-file checks on a parsed `views.yaml` against a schema.
///
/// Returns one [`Diagnostic`] per problem found; does not stop at the first.
///
/// Severity carries meaning for callers: an [`Severity::Error`] describes a
/// view that cannot produce output, and `workdown render` and the server's
/// per-view endpoint both skip such a view. A [`Severity::Warning`] describes
/// a view that renders fine but is probably not what its author meant — a
/// `where:` operand that can never match, say — and must not suppress the
/// view. Both callers filter on severity for exactly that reason.
pub fn evaluate(
    views: &Views,
    schema: &Schema,
    resources: &Resources,
    store: &Store,
    views_path: &Path,
) -> Vec<Diagnostic> {
    let ctx = ViewCheckContext {
        schema,
        resources,
        store,
        views_path,
    };
    let mut out = Vec::new();
    for view in &views.views {
        check_view(view, &ctx, &mut out);
        check_display(view, &ctx, &mut out);
        check_where_clauses(
            &ctx,
            SlotLocus::view(view.id.as_str()),
            &view.where_clauses,
            &mut out,
        );
    }
    out
}

/// Load `views.yaml` from disk, run cross-file checks, and return both
/// the parsed views and every diagnostic produced — parsing the file
/// exactly once so callers that also need the [`Views`] (the project
/// loader) don't have to re-read it.
///
/// Returns `(None, [])` when the file is absent — `views.yaml` is
/// optional. On I/O or YAML-parse failure returns `(None, diagnostics)`
/// with the load error routed through [`parse_errors_to_diagnostics`].
/// On a successful parse returns `(Some(views), diagnostics)`, where the
/// diagnostics are the semantic check results (often empty).
pub fn load_and_check(
    views_path: &Path,
    schema: &Schema,
    resources: &Resources,
    store: &Store,
) -> (Option<Views>, Vec<Diagnostic>) {
    if !views_path.exists() {
        return (None, Vec::new());
    }
    match crate::parser::views::load_views(views_path) {
        Ok(views) => {
            let diagnostics = evaluate(&views, schema, resources, store, views_path);
            (Some(views), diagnostics)
        }
        Err(err) => (None, parse_errors_to_diagnostics(err, views_path)),
    }
}

/// Convert a [`ViewsLoadError`] from the views parser into a list of
/// diagnostics pointed at `views_path`.
///
/// `ReadFailed` and `InvalidYaml` become a single file-scope diagnostic
/// (the detail carries the serde line/column or I/O message). `Validation`
/// expands into one config-scope diagnostic per semantic error.
pub fn parse_errors_to_diagnostics(err: ViewsLoadError, views_path: &Path) -> Vec<Diagnostic> {
    match err {
        ViewsLoadError::ReadFailed(io) => vec![Diagnostic::file(
            Severity::Error,
            views_path.to_path_buf(),
            FileDiagnosticKind::ReadError {
                detail: io.to_string(),
            },
        )],
        ViewsLoadError::InvalidYaml(yaml) => vec![Diagnostic::file(
            Severity::Error,
            views_path.to_path_buf(),
            FileDiagnosticKind::ReadError {
                detail: yaml.to_string(),
            },
        )],
        ViewsLoadError::Validation(errors) => errors
            .into_iter()
            .map(|err| {
                Diagnostic::config(
                    Severity::Error,
                    views_path.to_path_buf(),
                    validation_error_to_kind(err),
                )
            })
            .collect(),
    }
}

// ── Validation-error → ConfigDiagnosticKind ──────────────────────────

fn validation_error_to_kind(err: ViewsValidationError) -> ConfigDiagnosticKind {
    match err {
        ViewsValidationError::DuplicateId { id } => {
            ConfigDiagnosticKind::ViewDuplicateId { view_id: id }
        }
        ViewsValidationError::MissingSlot {
            id,
            view_type,
            slot,
        } => ConfigDiagnosticKind::ViewMissingSlot {
            view_id: id,
            view_type,
            slot,
        },
        ViewsValidationError::LegacyDisplaySlot {
            id,
            slot,
            replacement,
        } => ConfigDiagnosticKind::ViewLegacyDisplaySlot {
            view_id: id,
            slot,
            replacement,
        },
    }
}

// ── Per-view checks ──────────────────────────────────────────────────

fn check_view(view: &View, ctx: &ViewCheckContext, out: &mut Vec<Diagnostic>) {
    let locus = SlotLocus::view(view.id.as_str());

    match &view.kind {
        ViewKind::Board { field } => {
            check_slot(ctx, locus, view_slots::BOARD_FIELD, field, out);
        }
        ViewKind::Tree { field } => {
            check_slot(ctx, locus, view_slots::TREE_FIELD, field, out);
        }
        ViewKind::Graph { field, group_by } => {
            check_graph_field(ctx, locus, field, out);
            if let Some(group_by) = group_by {
                check_link_slot(ctx, locus, view_slots::GRAPH_GROUP_BY, group_by, out);
            }
        }
        // Table has no structural slots — its columns come from the
        // `fields` display role, checked in `check_display`.
        ViewKind::Table => {}
        ViewKind::Gantt {
            start,
            end,
            duration,
            after,
            group,
        } => {
            check_gantt_input_modes(
                ctx,
                locus,
                start,
                end.as_deref(),
                duration.as_deref(),
                after.as_deref(),
                out,
            );
            if let Some(group) = group {
                check_slot(ctx, locus, view_slots::GANTT_GROUP, group, out);
            }
        }
        ViewKind::GanttByInitiative {
            start,
            end,
            duration,
            after,
            root_link,
        } => {
            check_gantt_input_modes(
                ctx,
                locus,
                start,
                end.as_deref(),
                duration.as_deref(),
                after.as_deref(),
                out,
            );
            check_link_slot(ctx, locus, view_slots::GANTT_ROOT_LINK, root_link, out);
        }
        ViewKind::GanttByDepth {
            start,
            end,
            duration,
            after,
            depth_link,
        } => {
            check_gantt_input_modes(
                ctx,
                locus,
                start,
                end.as_deref(),
                duration.as_deref(),
                after.as_deref(),
                out,
            );
            check_link_slot(ctx, locus, view_slots::GANTT_DEPTH_LINK, depth_link, out);
        }
        ViewKind::BarChart {
            group_by,
            value,
            aggregate,
        } => {
            check_slot(ctx, locus, view_slots::BAR_CHART_GROUP_BY, group_by, out);
            if let Some(value) = value {
                check_aggregate_value_slot(ctx, locus, value, *aggregate, out);
            }
        }
        ViewKind::LineChart { x, y, group } => {
            check_slot(ctx, locus, view_slots::LINE_CHART_X, x, out);
            check_slot(ctx, locus, view_slots::LINE_CHART_Y, y, out);
            if let Some(group) = group {
                check_slot(ctx, locus, view_slots::LINE_CHART_GROUP, group, out);
            }
        }
        ViewKind::Workload {
            start,
            end,
            effort,
            working_days: _,
        } => {
            check_slot(ctx, locus, view_slots::WORKLOAD_START, start, out);
            check_slot(ctx, locus, view_slots::WORKLOAD_END, end, out);
            check_slot(ctx, locus, view_slots::WORKLOAD_EFFORT, effort, out);
        }
        ViewKind::Metric { metrics } => {
            for (metric_index, row) in metrics.iter().enumerate() {
                check_metric_row(ctx, locus.metric_row(metric_index), row, out);
            }
        }
        ViewKind::Treemap { group, size } => {
            check_slot(ctx, locus, view_slots::TREEMAP_GROUP, group, out);
            check_slot(ctx, locus, view_slots::TREEMAP_SIZE, size, out);
        }
        ViewKind::Heatmap {
            x,
            y,
            value,
            aggregate,
            bucket,
        } => {
            check_slot(ctx, locus, view_slots::HEATMAP_X, x, out);
            check_slot(ctx, locus, view_slots::HEATMAP_Y, y, out);
            if let Some(value) = value {
                check_aggregate_value_slot(ctx, locus, value, *aggregate, out);
            }
            if bucket.is_some() && !has_date_axis(ctx.schema, x, y) {
                out.push(ctx.error(ConfigDiagnosticKind::ViewBucketWithoutDateAxis {
                    view_id: locus.view_id.to_owned(),
                }));
            }
        }
    }
}

// ── Display roles (cross-cutting) ────────────────────────────────────

/// Check the view's display-role field references. The rules live in
/// [`crate::display_check`] — shared with `config_check`, which applies
/// them to `defaults.display` in `config.yaml` — and each violation is
/// wrapped into a view-scoped diagnostic here.
fn check_display(view: &View, ctx: &ViewCheckContext, out: &mut Vec<Diagnostic>) {
    let locus = SlotLocus::view(view.id.as_str());
    for violation in check_display_roles(&view.display, ctx.schema) {
        let kind = match violation {
            RoleViolation::UnknownField { role, field_name } => {
                ConfigDiagnosticKind::ViewUnknownField {
                    location: locus.at(role.view_slot()),
                    field_name,
                }
            }
            RoleViolation::TypeMismatch {
                role,
                field_name,
                actual_type,
                expected,
            } => ConfigDiagnosticKind::ViewFieldTypeMismatch {
                location: locus.at(role.view_slot()),
                field_name,
                actual_type,
                expected: expected.to_owned(),
            },
        };
        out.push(ctx.error(kind));
    }
}

// ── Slot helper ──────────────────────────────────────────────────────

/// Check one slot's field reference against its [`SlotSpec`]. Emits:
/// - [`ConfigDiagnosticKind::ViewVirtualIdNotAllowed`] for the virtual
///   `"id"` — every `check_slot` caller is a structural slot, and the
///   id is unique per item, so grouping or plotting by it is
///   meaningless (text display roles resolve `id` specially and are
///   checked in `display_check`, not here),
/// - [`ConfigDiagnosticKind::ViewUnknownField`] if `field_name` isn't defined in
///   `schema.fields`,
/// - [`ConfigDiagnosticKind::ViewFieldTypeMismatch`] if the slot accepts a
///   list of types and the field's isn't in it.
///
/// A slot whose `accepts` is empty takes any field, so the check is
/// existence-only — the bar chart's `group_by`, the heatmap's `x`/`y`.
fn check_slot(
    ctx: &ViewCheckContext,
    locus: SlotLocus,
    slot: SlotSpec,
    field_name: &str,
    out: &mut Vec<Diagnostic>,
) {
    if field_name == "id" {
        out.push(ctx.error(ConfigDiagnosticKind::ViewVirtualIdNotAllowed {
            location: locus.at(slot.name),
        }));
        return;
    }

    let Some(def) = ctx.schema.fields.get(field_name) else {
        out.push(ctx.error(ConfigDiagnosticKind::ViewUnknownField {
            location: locus.at(slot.name),
            field_name: field_name.to_owned(),
        }));
        return;
    };

    if slot.accepts.is_empty() {
        return;
    }

    let actual = def.field_type();
    if !slot.accepts.contains(&actual) {
        out.push(ctx.error(ConfigDiagnosticKind::ViewFieldTypeMismatch {
            location: locus.at(slot.name),
            field_name: field_name.to_owned(),
            actual_type: actual,
            expected: view_slots::describe(slot.accepts),
        }));
    }
}

// ── Graph field helper ───────────────────────────────────────────────

/// Graph-specific slot check: accepts a direct Link/Links field, or an
/// inverse name (declared via `inverse:` on a link/links field and thus
/// present in `schema.inverse_table`). Inverse names resolve to their
/// original field at extraction time; the underlying data is the same.
fn check_graph_field(
    ctx: &ViewCheckContext,
    locus: SlotLocus,
    field_name: &str,
    out: &mut Vec<Diagnostic>,
) {
    if field_name == "id" {
        out.push(ctx.error(ConfigDiagnosticKind::ViewVirtualIdNotAllowed {
            location: locus.at("field"),
        }));
        return;
    }

    let slot = view_slots::GRAPH_FIELD;
    if let Some(def) = ctx.schema.fields.get(field_name) {
        let actual = def.field_type();
        if !slot.accepts.contains(&actual) {
            out.push(ctx.error(ConfigDiagnosticKind::ViewFieldTypeMismatch {
                location: locus.at(slot.name),
                field_name: field_name.to_owned(),
                actual_type: actual,
                expected: view_slots::describe(slot.accepts),
            }));
        }
        return;
    }

    if ctx.schema.inverse_table.contains_key(field_name) {
        return;
    }

    out.push(ctx.error(ConfigDiagnosticKind::ViewUnknownField {
        location: locus.at("field"),
        field_name: field_name.to_owned(),
    }));
}

// ── Link-slot helper ─────────────────────────────────────────────────

/// Validates a slot that drives an upward chain walk (`group_by`, `after`,
/// `root_link`, `depth_link`).
///
/// All four require:
/// - the field exists in the schema (not an inverse name);
/// - the field's type is one the slot accepts — `link` for the
///   single-target walks, `link` or `links` for `after`;
/// - cycles are explicitly disabled (`allow_cycles: false`).
///
/// Each rule has its own diagnostic so the error message points at the
/// actual constraint violated. Which link types the slot takes comes from
/// its [`SlotSpec`], so there is no arity parameter to keep aligned with
/// the table — a link slot that accepts `links` says so in
/// [`crate::model::view_slots`].
fn check_link_slot(
    ctx: &ViewCheckContext,
    locus: SlotLocus,
    slot: SlotSpec,
    field_name: &str,
    out: &mut Vec<Diagnostic>,
) {
    // Without this, `id` falls into the unknown-field arm below — a
    // misleading message for a field that does exist, just not here.
    if field_name == "id" {
        out.push(ctx.error(ConfigDiagnosticKind::ViewVirtualIdNotAllowed {
            location: locus.at(slot.name),
        }));
        return;
    }

    let Some(def) = ctx.schema.fields.get(field_name) else {
        if ctx.schema.inverse_table.contains_key(field_name) {
            out.push(ctx.error(ConfigDiagnosticKind::ViewSlotInverseNotAllowed {
                location: locus.at(slot.name),
                field_name: field_name.to_owned(),
            }));
        } else {
            out.push(ctx.error(ConfigDiagnosticKind::ViewUnknownField {
                location: locus.at(slot.name),
                field_name: field_name.to_owned(),
            }));
        }
        return;
    };

    let actual = def.field_type();
    if !slot.accepts.contains(&actual) {
        out.push(ctx.error(ConfigDiagnosticKind::ViewFieldTypeMismatch {
            location: locus.at(slot.name),
            field_name: field_name.to_owned(),
            actual_type: actual,
            expected: view_slots::describe(slot.accepts),
        }));
        return;
    }

    let allow_cycles = match &def.type_config {
        FieldTypeConfig::Link { allow_cycles, .. }
        | FieldTypeConfig::Links { allow_cycles, .. } => *allow_cycles,
        // Unreachable: every type a link slot accepts is one of the two
        // above, and the check before this one has returned otherwise.
        _ => return,
    };

    if allow_cycles != Some(false) {
        out.push(ctx.error(ConfigDiagnosticKind::ViewSlotCyclic {
            location: locus.at(slot.name),
            field_name: field_name.to_owned(),
        }));
    }
}
// ── Gantt input-mode helper ──────────────────────────────────────────

/// Validate the `start` slot and the cross-slot input-mode rules shared
/// by [`ViewKind::Gantt`] and [`ViewKind::GanttByInitiative`]. Three
/// valid combinations:
///   (start, end)         — bar window read directly
///   (start, duration)    — end computed as start + duration
///   (start, after, dur)  — start anchored on predecessors,
///                          end computed as start + duration
/// Anything else is rejected. When a combination is invalid we still
/// type-check whatever fields are present so the user gets all the
/// actionable feedback in one pass.
fn check_gantt_input_modes(
    ctx: &ViewCheckContext,
    locus: SlotLocus,
    start: &str,
    end: Option<&str>,
    duration: Option<&str>,
    after: Option<&str>,
    out: &mut Vec<Diagnostic>,
) {
    check_slot(ctx, locus, view_slots::GANTT_START, start, out);
    if let Some(after_field) = after {
        if end.is_some() {
            out.push(
                ctx.error(ConfigDiagnosticKind::ViewGanttAfterWithEndConflict {
                    view_id: locus.view_id.to_owned(),
                }),
            );
        }
        if duration.is_none() {
            out.push(
                ctx.error(ConfigDiagnosticKind::ViewGanttAfterRequiresDuration {
                    view_id: locus.view_id.to_owned(),
                }),
            );
        }
        check_link_slot(ctx, locus, view_slots::GANTT_AFTER, after_field, out);
        if let Some(duration) = duration {
            check_slot(ctx, locus, view_slots::GANTT_DURATION, duration, out);
        }
    } else {
        match (end, duration) {
            (Some(_), Some(_)) => out.push(ctx.error(
                ConfigDiagnosticKind::ViewGanttEndAndDurationConflict {
                    view_id: locus.view_id.to_owned(),
                },
            )),
            (None, None) => out.push(ctx.error(
                ConfigDiagnosticKind::ViewGanttEndOrDurationRequired {
                    view_id: locus.view_id.to_owned(),
                },
            )),
            (Some(end), None) => {
                check_slot(ctx, locus, view_slots::GANTT_END, end, out);
            }
            (None, Some(duration)) => {
                check_slot(ctx, locus, view_slots::GANTT_DURATION, duration, out);
            }
        }
    }
}

// ── Aggregate value-slot helper ──────────────────────────────────────

/// Verify an aggregate's `value` slot: the field exists, isn't the
/// virtual `id`, and has a type the chosen aggregate can combine.
///
/// Which types each function accepts is
/// [`view_slots::aggregate_value_types`]; the web UI's create form reads
/// the same table, so the fields it offers are the fields this accepts.
///
/// Shared by every locus carrying an aggregate — the bar chart's and
/// heatmap's `value` slot, and each row of a metric view — so the rule is
/// enforced in exactly one place. Which locus is reported comes from
/// `locus`; the caller decides, the rule does not.
///
/// Called only when a `value` is actually set, which is why `count`
/// lands on [`ConfigDiagnosticKind::ViewCountAggregateWithValue`] here:
/// `count` counts items, so a value field is meaningless rather than
/// mistyped. That verdict is reported alone — checking the type of a
/// slot that shouldn't be there at all would only add noise.
fn check_aggregate_value_slot(
    ctx: &ViewCheckContext,
    locus: SlotLocus,
    field_name: &str,
    aggregate: Aggregate,
    out: &mut Vec<Diagnostic>,
) {
    if aggregate == Aggregate::Count {
        out.push(
            ctx.error(ConfigDiagnosticKind::ViewCountAggregateWithValue {
                location: locus.at("value"),
            }),
        );
        return;
    }
    if field_name == "id" {
        out.push(ctx.error(ConfigDiagnosticKind::ViewVirtualIdNotAllowed {
            location: locus.at("value"),
        }));
        return;
    }
    let Some(def) = ctx.schema.fields.get(field_name) else {
        out.push(ctx.error(ConfigDiagnosticKind::ViewUnknownField {
            location: locus.at("value"),
            field_name: field_name.to_owned(),
        }));
        return;
    };
    let actual = def.field_type();
    if !view_slots::aggregate_value_types(aggregate).contains(&actual) {
        out.push(ctx.error(ConfigDiagnosticKind::ViewAggregateTypeMismatch {
            location: locus.at("value"),
            aggregate,
            actual_type: actual,
        }));
    }
}

// ── Metric row helper ────────────────────────────────────────────────

/// Validate one row of a metric view.
///
/// A row carries the same two things a view does — an aggregate over a
/// `value` field, and a `where:` filter — so it runs the view's own two
/// checks, with the locus already narrowed to this row by the caller.
/// Every diagnostic they emit is pinned to the row by that locus, with
/// no row-specific variant involved.
fn check_metric_row(
    ctx: &ViewCheckContext,
    locus: SlotLocus,
    row: &MetricRow,
    out: &mut Vec<Diagnostic>,
) {
    if let Some(value) = &row.value {
        check_aggregate_value_slot(ctx, locus, value, row.aggregate, out);
    }
    check_where_clauses(ctx, locus, &row.where_clauses, out);
}

// ── Heatmap bucket-coupling helper ───────────────────────────────────

/// Does at least one of the two axis fields resolve to a `date` field in the schema?
fn has_date_axis(schema: &Schema, x: &str, y: &str) -> bool {
    is_date_field(schema.fields.get(x)) || is_date_field(schema.fields.get(y))
}

fn is_date_field(def: Option<&FieldDefinition>) -> bool {
    matches!(def.map(|d| d.field_type()), Some(FieldType::Date))
}

// ── Where-clause checks ──────────────────────────────────────────────

/// Validate a list of `where:` expressions: each one parses, every field
/// it references resolves, and every operand it compares against is
/// reachable.
///
/// Shared by the view's own `where:` and a metric row's, so the two
/// cannot drift; `locus` decides which one a finding names.
fn check_where_clauses(
    ctx: &ViewCheckContext,
    locus: SlotLocus,
    where_clauses: &[String],
    out: &mut Vec<Diagnostic>,
) {
    for raw in where_clauses {
        match parse_where(raw) {
            Ok(predicate) => {
                walk_predicate(&predicate, locus, ctx, out);
                // Operand checking runs after the field walk and never
                // instead of it: an operand judged against a field that
                // doesn't exist would be noise on top of the real error.
                for violation in
                    where_check::check_predicate(&predicate, ctx.schema, ctx.resources, ctx.store)
                {
                    out.push(ctx.warning(ConfigDiagnosticKind::ViewWhereUnknownValue {
                        location: locus.at("where"),
                        raw: raw.clone(),
                        field_name: violation.field.clone(),
                        detail: violation.detail(),
                    }));
                }
            }
            Err(err) => out.push(ctx.error(ConfigDiagnosticKind::ViewWhereParseError {
                location: locus.at("where"),
                raw: raw.clone(),
                detail: err.to_string(),
            })),
        }
    }
}

fn walk_predicate(
    predicate: &Predicate,
    locus: SlotLocus,
    ctx: &ViewCheckContext,
    out: &mut Vec<Diagnostic>,
) {
    match predicate {
        Predicate::Comparison(comparison) => {
            check_where_field_ref(&comparison.field, locus, ctx, out)
        }
        Predicate::And(inner) | Predicate::Or(inner) => {
            for p in inner {
                walk_predicate(p, locus, ctx, out);
            }
        }
        Predicate::Not(inner) => walk_predicate(inner, locus, ctx, out),
    }
}

fn check_where_field_ref(
    field_ref: &FieldReference,
    locus: SlotLocus,
    ctx: &ViewCheckContext,
    out: &mut Vec<Diagnostic>,
) {
    match field_ref {
        FieldReference::Local(name) => {
            if name == "id" {
                return;
            }
            if !ctx.schema.fields.contains_key(name) {
                out.push(ctx.error(ConfigDiagnosticKind::ViewUnknownField {
                    location: locus.at("where"),
                    field_name: name.clone(),
                }));
            }
        }
        FieldReference::Related { relation, .. } => {
            if is_relation_anchor(relation, &ctx.schema.fields) {
                return;
            }
            out.push(ctx.error(ConfigDiagnosticKind::ViewUnknownField {
                location: locus.at("where"),
                field_name: relation.clone(),
            }));
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
