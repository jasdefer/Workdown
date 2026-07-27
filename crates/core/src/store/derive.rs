//! Derive orchestrator: run the compute and rollup passes per field, in
//! dependency order.
//!
//! Fields evaluate in topological order over compute-reference edges
//! (an expression's inputs before the field itself), so a chain like
//! `effort → cost → budget` fills front to back, and a computed leaf
//! value is in place before the same field's rollup aggregates it.
//! Within one field, compute runs before aggregate. Cycles — already
//! reported as schema diagnostics by `compute_check` — are simply not
//! descended into; the fields involved end up without derived values.
//!
//! Ends with the deferred required check that coercion skipped for
//! derivable fields: an item still blank on a `required` aggregate
//! field gets the classic `MissingRequired`; on a `required` computed
//! field it gets `ComputeMissingInputs` naming the actual inputs that
//! were absent — the real cause, one step before the symptom.

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::model::diagnostic::{Diagnostic, ItemDiagnosticKind};
use crate::model::schema::{Schema, Severity};
use crate::model::{FieldValue, WorkItem, WorkItemId};

use super::compute;
use super::rollup;

/// Run every derive pass. Mutates `items` in place; returns all
/// diagnostics the passes produced. `constants` are the project
/// constants from `resources.yaml`, resolved by compute expressions.
pub(crate) fn run(
    items: &mut HashMap<WorkItemId, WorkItem>,
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    schema: &Schema,
    constants: &IndexMap<String, FieldValue>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for field_name in field_order(schema) {
        let field_definition = &schema.fields[&field_name];

        if let Some(config) = &field_definition.compute {
            let leaves_only_over = field_definition.aggregate.as_ref().map(|aggregate| {
                aggregate
                    .over
                    .clone()
                    .unwrap_or_else(|| rollup::DEFAULT_OVER_FIELD.to_owned())
            });
            compute::run_for_field(
                items,
                reverse_links,
                constants,
                &field_name,
                field_definition.field_type(),
                config,
                leaves_only_over.as_deref(),
                &mut diagnostics,
            );
        }

        if let Some(aggregate) = &field_definition.aggregate {
            let spec = rollup::AggregateFieldSpec {
                name: field_name.clone(),
                function: aggregate.function,
                over: aggregate
                    .over
                    .clone()
                    .unwrap_or_else(|| rollup::DEFAULT_OVER_FIELD.to_owned()),
                error_on_missing: aggregate.error_on_missing,
            };
            rollup::run_for_field(items, reverse_links, &spec, &mut diagnostics);
        }
    }

    required_check(items, schema, &mut diagnostics);
    diagnostics
}

/// Deferred required check for derivable fields (coercion skipped them —
/// the derive passes may have filled them in).
fn required_check(
    items: &HashMap<WorkItemId, WorkItem>,
    schema: &Schema,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (field_name, field_definition) in &schema.fields {
        if !field_definition.required
            || (field_definition.aggregate.is_none() && field_definition.compute.is_none())
        {
            continue;
        }
        let mut missing: Vec<(&WorkItemId, &WorkItem)> = items
            .iter()
            .filter(|(_, item)| !item.fields.contains_key(field_name))
            .collect();
        missing.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

        for (item_id, item) in missing {
            // For a computed field, name the absent inputs — the actual
            // cause — instead of just the blank output. Falls back to
            // the classic message when the inputs were all present (a
            // non-leaf of a compute+aggregate field with no children).
            let kind = match &field_definition.compute {
                Some(config) => {
                    let missing_inputs = compute::missing_inputs(item, config);
                    if missing_inputs.is_empty() {
                        ItemDiagnosticKind::MissingRequired {
                            field: field_name.clone(),
                        }
                    } else {
                        ItemDiagnosticKind::ComputeMissingInputs {
                            field: field_name.clone(),
                            missing_inputs,
                        }
                    }
                }
                None => ItemDiagnosticKind::MissingRequired {
                    field: field_name.clone(),
                },
            };
            diagnostics.push(Diagnostic::item(
                Severity::Error,
                item.source_path.clone(),
                item_id.clone(),
                kind,
            ));
        }
    }
}

/// Schema fields in evaluation order: an expression's inputs before the
/// field that consumes them; declaration order otherwise. Cycles are
/// not descended into (compute_check reports them), so the walk always
/// terminates.
fn field_order(schema: &Schema) -> Vec<String> {
    let mut order = Vec::with_capacity(schema.fields.len());
    let mut visited: HashSet<&str> = HashSet::new();

    for field_name in schema.fields.keys() {
        visit(schema, field_name, &mut visited, &mut order);
    }
    order
}

fn visit<'a>(
    schema: &'a Schema,
    field_name: &'a str,
    visited: &mut HashSet<&'a str>,
    order: &mut Vec<String>,
) {
    if !visited.insert(field_name) {
        return; // already placed, or currently on the path (cycle)
    }
    if let Some(config) = schema
        .fields
        .get(field_name)
        .and_then(|field_definition| field_definition.compute.as_ref())
    {
        for referenced in config.expression.field_references() {
            if let Some((key, _)) = schema.fields.get_key_value(referenced) {
                visit(schema, key, visited, order);
            }
        }
    }
    order.push(field_name.to_owned());
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::diagnostic::DiagnosticBody;
    use crate::parser::resources::parse_resources;
    use crate::parser::schema::parse_schema;
    use chrono::NaiveDate;
    use std::path::PathBuf;

    // ── Fixture helpers ───────────────────────────────────────────────

    fn item(id: &str, fields: Vec<(&str, FieldValue)>) -> (WorkItemId, WorkItem) {
        let item_id = WorkItemId::from(id.to_owned());
        let item = WorkItem {
            id: item_id.clone(),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
            body: String::new(),
            source_path: PathBuf::from(format!("{id}.md")),
        };
        (item_id, item)
    }

    fn date(y: i32, m: u32, d: u32) -> FieldValue {
        FieldValue::Date(NaiveDate::from_ymd_opt(y, m, d).unwrap())
    }

    fn duration_days(days: i64) -> FieldValue {
        FieldValue::Duration(days * 86_400)
    }

    /// Build reverse links for `parent` values present in the items.
    fn reverse_links_of(
        items: &HashMap<WorkItemId, WorkItem>,
    ) -> HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>> {
        let mut reverse_links: HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>> =
            HashMap::new();
        for (id, item) in items {
            if let Some(FieldValue::Link(target)) = item.fields.get("parent") {
                reverse_links
                    .entry("parent".to_owned())
                    .or_default()
                    .entry(target.clone())
                    .or_default()
                    .push(id.clone());
            }
        }
        reverse_links
    }

    fn run_derive(
        items: &mut HashMap<WorkItemId, WorkItem>,
        schema_yaml: &str,
        resources_yaml: &str,
    ) -> Vec<Diagnostic> {
        let schema = parse_schema(schema_yaml).expect("test schema must parse");
        let resources = parse_resources(resources_yaml).expect("test resources must parse");
        let reverse_links = reverse_links_of(items);
        run(items, &reverse_links, &schema, &resources.constants)
    }

    fn field<'a>(
        items: &'a HashMap<WorkItemId, WorkItem>,
        id: &str,
        name: &str,
    ) -> Option<&'a FieldValue> {
        items[&WorkItemId::from(id.to_owned())].fields.get(name)
    }

    const SCHEDULING_SCHEMA: &str = "\
fields:
  parent:
    type: link
    allow_cycles: false
  start_date:
    type: date
    aggregate:
      function: min
  duration:
    type: duration
    aggregate:
      function: sum
  end_date:
    type: date
    compute: start_date + duration
    aggregate:
      function: max
";

    // ── The motivating scenario ───────────────────────────────────────

    #[test]
    fn leaves_compute_and_the_parent_aggregates_across_the_gap() {
        // Task A: Jan 5 + 1w → Jan 12. Task B: Jan 19 + 1w → Jan 26.
        // Milestone M must get max(children) = Jan 26 — NOT its own
        // rolled-up start + duration (Jan 5 + 2w = Jan 19), which is
        // blind to the idle week between A and B.
        let mut items = HashMap::from([
            item(
                "task-a",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("m".to_owned()))),
                    ("start_date", date(2026, 1, 5)),
                    ("duration", duration_days(7)),
                ],
            ),
            item(
                "task-b",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("m".to_owned()))),
                    ("start_date", date(2026, 1, 19)),
                    ("duration", duration_days(7)),
                ],
            ),
            item("m", vec![]),
        ]);
        let diagnostics = run_derive(&mut items, SCHEDULING_SCHEMA, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(
            field(&items, "task-a", "end_date"),
            Some(&date(2026, 1, 12))
        );
        assert_eq!(
            field(&items, "task-b", "end_date"),
            Some(&date(2026, 1, 26))
        );
        assert_eq!(field(&items, "m", "end_date"), Some(&date(2026, 1, 26)));
        // The rolled-up inputs still exist on M — they're just not what
        // its end_date derives from.
        assert_eq!(field(&items, "m", "start_date"), Some(&date(2026, 1, 5)));
        assert_eq!(field(&items, "m", "duration"), Some(&duration_days(14)));
    }

    #[test]
    fn manual_value_wins_over_compute() {
        let mut items = HashMap::from([item(
            "task",
            vec![
                ("start_date", date(2026, 1, 5)),
                ("duration", duration_days(7)),
                ("end_date", date(2026, 3, 1)),
            ],
        )]);
        run_derive(&mut items, SCHEDULING_SCHEMA, "");
        assert_eq!(field(&items, "task", "end_date"), Some(&date(2026, 3, 1)));
    }

    #[test]
    fn compute_only_field_consumes_rolled_up_inputs() {
        // flow_efficiency has no aggregate — it computes everywhere its
        // inputs resolve, including on the milestone, where effort and
        // duration are themselves rolled up: sum / sum, not an average
        // of children's ratios.
        let schema_yaml = "\
fields:
  parent:
    type: link
    allow_cycles: false
  effort:
    type: duration
    aggregate:
      function: sum
  duration:
    type: duration
    aggregate:
      function: sum
  flow_efficiency:
    type: float
    compute: effort / duration
";
        let mut items = HashMap::from([
            item(
                "task-a",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("m".to_owned()))),
                    ("effort", FieldValue::Duration(6 * 3_600)),
                    ("duration", duration_days(2)),
                ],
            ),
            item(
                "task-b",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("m".to_owned()))),
                    ("effort", FieldValue::Duration(18 * 3_600)),
                    ("duration", duration_days(2)),
                ],
            ),
            item("m", vec![]),
        ]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        // Milestone: (6h + 18h) / 4d = 24h / 96h = 0.25.
        assert_eq!(
            field(&items, "m", "flow_efficiency"),
            Some(&FieldValue::Float(0.25))
        );
    }

    #[test]
    fn computed_chain_fills_front_to_back_regardless_of_declaration_order() {
        // budget is declared before cost, which is declared before its
        // input — the topological order must still fill effort → cost →
        // budget.
        let schema_yaml = "\
fields:
  budget:
    type: duration
    compute: cost * 2
  cost:
    type: duration
    compute: effort * $constants.daily_rate
  effort:
    type: duration
";
        let resources_yaml = "\
constants:
  daily_rate:
    type: float
    value: 3
";
        let mut items = HashMap::from([item("task", vec![("effort", FieldValue::Duration(100))])]);
        let diagnostics = run_derive(&mut items, schema_yaml, resources_yaml);

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(
            field(&items, "task", "cost"),
            Some(&FieldValue::Duration(300))
        );
        assert_eq!(
            field(&items, "task", "budget"),
            Some(&FieldValue::Duration(600))
        );
    }

    // ── Failure modes ─────────────────────────────────────────────────

    fn item_kinds(diagnostics: &[Diagnostic]) -> Vec<&ItemDiagnosticKind> {
        diagnostics
            .iter()
            .filter_map(|diagnostic| match &diagnostic.body {
                DiagnosticBody::Item(body) => Some(&body.kind),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn missing_input_is_silent_without_the_flag() {
        let mut items = HashMap::from([item("task", vec![("start_date", date(2026, 1, 5))])]);
        let diagnostics = run_derive(&mut items, SCHEDULING_SCHEMA, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "task", "end_date"), None);
    }

    #[test]
    fn missing_input_with_error_on_missing_names_the_inputs() {
        let schema_yaml = "\
fields:
  start_date:
    type: date
  duration:
    type: duration
  end_date:
    type: date
    compute:
      expression: start_date + duration
      error_on_missing: true
";
        let mut items = HashMap::from([item("task", vec![("start_date", date(2026, 1, 5))])]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(matches!(
            item_kinds(&diagnostics)[0],
            ItemDiagnosticKind::ComputeMissingInputs { field, missing_inputs }
                if field == "end_date" && missing_inputs == &vec!["duration".to_owned()]
        ));
    }

    #[test]
    fn required_computed_field_left_blank_names_the_inputs_as_error() {
        let schema_yaml = "\
fields:
  spent:
    type: duration
  estimate:
    type: duration
  remaining:
    type: duration
    required: true
    compute: estimate - spent
";
        let mut items = HashMap::from([item("task", vec![("estimate", duration_days(2))])]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(matches!(
            item_kinds(&diagnostics)[0],
            ItemDiagnosticKind::ComputeMissingInputs { field, missing_inputs }
                if field == "remaining" && missing_inputs == &vec!["spent".to_owned()]
        ));
    }

    #[test]
    fn division_by_zero_is_a_warning_and_the_field_stays_absent() {
        let schema_yaml = "\
fields:
  spent:
    type: duration
  estimate:
    type: duration
  percent_done:
    type: float
    compute: spent / estimate
";
        let mut items = HashMap::from([item(
            "task",
            vec![
                ("spent", FieldValue::Duration(3_600)),
                ("estimate", FieldValue::Duration(0)),
            ],
        )]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(matches!(
            item_kinds(&diagnostics)[0],
            ItemDiagnosticKind::ComputeFailed { field, detail }
                if field == "percent_done" && detail.contains("division by zero")
        ));
        assert_eq!(field(&items, "task", "percent_done"), None);
    }

    #[test]
    fn expression_cycle_terminates_with_no_derived_values() {
        // compute_check reports the cycle as a schema diagnostic; the
        // derive pass must simply not loop and produce nothing.
        let schema_yaml = "\
fields:
  a:
    type: float
    compute: b * 1.0
  b:
    type: float
    compute: a * 1.0
";
        let mut items = HashMap::from([item("task", vec![])]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "task", "a"), None);
        assert_eq!(field(&items, "task", "b"), None);
    }

    #[test]
    fn rounding_mode_applies_to_the_computed_date() {
        let schema_yaml = "\
fields:
  start_date:
    type: date
  effort:
    type: duration
  finish:
    type: date
    compute:
      expression: start_date + effort
      round: ceil
";
        // 1d 4h of effort: floor/nearest would say Jan 6; ceil says the
        // work spills into Jan 7.
        let mut items = HashMap::from([item(
            "task",
            vec![
                ("start_date", date(2026, 1, 5)),
                ("effort", FieldValue::Duration(86_400 + 4 * 3_600)),
            ],
        )]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "task", "finish"), Some(&date(2026, 1, 7)));
    }
}
