//! Aggregate rollup: walk the configured `over` link upward from each
//! manual-bearing item, collect contributions on its non-manual ancestors,
//! and reduce them via the field's aggregate function.
//!
//! A single up-walk pass simultaneously emits chain-conflict diagnostics
//! (encountering a second manual setter aborts that walk) and accumulates
//! values for the apply pass. An optional coverage pass surfaces
//! `error_on_missing` diagnostics for tree-leaves with no covering value.
//!
//! Computed values are written back into `WorkItem.fields` and become
//! indistinguishable from manually-set values for downstream consumers.
//! `Store::load` runs this once per load on a freshly-coerced state, so we
//! never have to track per-field provenance.
//!
//! Cycles are guarded by a per-walk visited set; the cycle detector emits
//! its own diagnostic separately.

use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};

use crate::model::diagnostic::{Diagnostic, DiagnosticKind};
use crate::model::schema::{AggregateFunction, Schema, Severity};
use crate::model::{FieldValue, WorkItem, WorkItemId};

/// Link field walked when an aggregate config doesn't set `over`.
const DEFAULT_OVER_FIELD: &str = "parent";

// ── Public entry ────────────────────────────────────────────────────

/// Run the rollup over every aggregate-configured field in the schema.
/// Mutates `items` in place; returns chain-conflict and (when configured)
/// missing-value diagnostics.
pub(crate) fn run(
    items: &mut HashMap<WorkItemId, WorkItem>,
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    schema: &Schema,
) -> Vec<Diagnostic> {
    let specs: Vec<AggregateFieldSpec> = schema
        .fields
        .iter()
        .filter_map(|(name, field_def)| {
            field_def.aggregate.as_ref().map(|cfg| AggregateFieldSpec {
                name: name.clone(),
                function: cfg.function,
                over: cfg
                    .over
                    .clone()
                    .unwrap_or_else(|| DEFAULT_OVER_FIELD.to_owned()),
                error_on_missing: cfg.error_on_missing,
            })
        })
        .collect();

    let mut diagnostics = Vec::new();
    for spec in &specs {
        run_for_field(items, reverse_links, spec, &mut diagnostics);
    }

    // Post-compute required check: for any field that is both `required:
    // true` and aggregate-configured, every item must end up with a value
    // (manual or computed). Coercion deferred this check to here.
    for (field_name, field_def) in &schema.fields {
        if !field_def.required || field_def.aggregate.is_none() {
            continue;
        }
        let mut missing: Vec<WorkItemId> = items
            .iter()
            .filter_map(|(id, item)| (!item.fields.contains_key(field_name)).then(|| id.clone()))
            .collect();
        missing.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        for item_id in missing {
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                kind: DiagnosticKind::MissingRequired {
                    item_id,
                    field: field_name.clone(),
                },
            });
        }
    }

    diagnostics
}

struct AggregateFieldSpec {
    name: String,
    function: AggregateFunction,
    over: String,
    error_on_missing: bool,
}

// ── Per-field pass ──────────────────────────────────────────────────

fn run_for_field(
    items: &mut HashMap<WorkItemId, WorkItem>,
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    spec: &AggregateFieldSpec,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Snapshot of items that manually set this field, sorted by id for
    // deterministic diagnostic order.
    let mut manual_items: Vec<(WorkItemId, FieldValue)> = items
        .iter()
        .filter_map(|(id, item)| {
            item.fields
                .get(&spec.name)
                .cloned()
                .map(|value| (id.clone(), value))
        })
        .collect();
    manual_items.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    let manual_set: HashSet<WorkItemId> = manual_items.iter().map(|(id, _)| id.clone()).collect();

    let mut accumulators: HashMap<WorkItemId, Vec<FieldValue>> = HashMap::new();

    // Up-walk pass: contribute each manual-bearing item's value to its
    // non-manual ancestors, stopping (with a chain-conflict diagnostic)
    // at the first manual-bearing ancestor.
    for (manual_id, manual_value) in &manual_items {
        let mut visited: HashSet<WorkItemId> = HashSet::new();
        visited.insert(manual_id.clone());

        let mut current = parent_of(items, manual_id, &spec.over);
        while let Some(ancestor_id) = current {
            if !visited.insert(ancestor_id.clone()) {
                // Cycle. Cycle detector handles the diagnostic; just stop.
                break;
            }
            if manual_set.contains(&ancestor_id) {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    kind: DiagnosticKind::AggregateChainConflict {
                        field: spec.name.clone(),
                        item_id: manual_id.clone(),
                        conflicting_ancestor_id: ancestor_id,
                    },
                });
                break;
            }
            accumulators
                .entry(ancestor_id.clone())
                .or_default()
                .push(manual_value.clone());
            current = parent_of(items, &ancestor_id, &spec.over);
        }
    }

    // Apply pass: reduce each accumulator and write into the item.
    let mut sorted: Vec<(WorkItemId, Vec<FieldValue>)> = accumulators.into_iter().collect();
    sorted.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    for (item_id, values) in sorted {
        if let Some(reduced) = apply_aggregate(spec.function, &values) {
            if let Some(item) = items.get_mut(&item_id) {
                item.fields.insert(spec.name.clone(), reduced);
            }
        }
    }

    // Coverage pass: only when error_on_missing is set.
    if spec.error_on_missing {
        let mut leaves: Vec<WorkItemId> = items
            .keys()
            .filter(|id| is_tree_leaf(reverse_links, id, &spec.over))
            .cloned()
            .collect();
        leaves.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        for leaf_id in leaves {
            if !covered(items, &leaf_id, &spec.over, &manual_set) {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    kind: DiagnosticKind::AggregateMissingValue {
                        field: spec.name.clone(),
                        leaf_id,
                    },
                });
            }
        }
    }
}

// ── Helpers: tree navigation ────────────────────────────────────────

/// Read `over_field` on `item_id` and follow it as a Link.
fn parent_of(
    items: &HashMap<WorkItemId, WorkItem>,
    item_id: &WorkItemId,
    over_field: &str,
) -> Option<WorkItemId> {
    items
        .get(item_id)
        .and_then(|item| item.fields.get(over_field))
        .and_then(|value| match value {
            FieldValue::Link(target) => Some(target.clone()),
            _ => None,
        })
}

/// True if no item references `item_id` as its `over_field` target —
/// nothing has it as their parent in the rollup hierarchy.
fn is_tree_leaf(
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    item_id: &WorkItemId,
    over_field: &str,
) -> bool {
    reverse_links
        .get(over_field)
        .and_then(|by_target| by_target.get(item_id))
        .is_none_or(|sources| sources.is_empty())
}

/// True if `start` itself or any ancestor on its `over_field` chain is in
/// `manual_set`. Cycle-safe via visited tracking.
fn covered(
    items: &HashMap<WorkItemId, WorkItem>,
    start: &WorkItemId,
    over_field: &str,
    manual_set: &HashSet<WorkItemId>,
) -> bool {
    if manual_set.contains(start) {
        return true;
    }
    let mut visited: HashSet<WorkItemId> = HashSet::new();
    visited.insert(start.clone());
    let mut current = parent_of(items, start, over_field);
    while let Some(ancestor) = current {
        if !visited.insert(ancestor.clone()) {
            return false;
        }
        if manual_set.contains(&ancestor) {
            return true;
        }
        current = parent_of(items, &ancestor, over_field);
    }
    false
}

// ── Aggregate application ───────────────────────────────────────────
//
// Inputs are guaranteed homogeneous (all Integer, or all Float, or all
// Date, or all Boolean) because the schema-time check rejects
// function/type mismatches and coercion ensures every manual value
// matches the field's declared type. Functions inspect the first value
// to choose the integer/float branch.

fn apply_aggregate(function: AggregateFunction, values: &[FieldValue]) -> Option<FieldValue> {
    if values.is_empty() {
        return None;
    }
    match function {
        AggregateFunction::Count => Some(FieldValue::Integer(values.len() as i64)),
        AggregateFunction::Sum => sum(values),
        AggregateFunction::Min => extremum(values, true),
        AggregateFunction::Max => extremum(values, false),
        AggregateFunction::Average => average(values),
        AggregateFunction::Median => median(values),
        AggregateFunction::All => boolean_reduce(values, |bools| bools.iter().all(|b| *b)),
        AggregateFunction::Any => boolean_reduce(values, |bools| bools.iter().any(|b| *b)),
        AggregateFunction::None => boolean_reduce(values, |bools| !bools.iter().any(|b| *b)),
    }
}

fn sum(values: &[FieldValue]) -> Option<FieldValue> {
    match values.first()? {
        FieldValue::Integer(_) => {
            let total: i64 = values.iter().filter_map(as_i64).sum();
            Some(FieldValue::Integer(total))
        }
        FieldValue::Float(_) => {
            let total: f64 = values.iter().filter_map(as_f64).sum();
            Some(FieldValue::Float(total))
        }
        FieldValue::Duration(_) => {
            // Saturating add prevents panic on overflow at i64::MAX.
            let total: i64 = values
                .iter()
                .filter_map(as_duration_seconds)
                .fold(0i64, i64::saturating_add);
            Some(FieldValue::Duration(total))
        }
        _ => None,
    }
}

fn extremum(values: &[FieldValue], pick_min: bool) -> Option<FieldValue> {
    match values.first()? {
        FieldValue::Integer(_) => {
            let nums: Vec<i64> = values.iter().filter_map(as_i64).collect();
            let chosen = if pick_min {
                *nums.iter().min().unwrap()
            } else {
                *nums.iter().max().unwrap()
            };
            Some(FieldValue::Integer(chosen))
        }
        FieldValue::Float(_) => {
            let nums: Vec<f64> = values.iter().filter_map(as_f64).collect();
            let chosen = if pick_min {
                nums.iter().copied().fold(f64::INFINITY, f64::min)
            } else {
                nums.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            };
            Some(FieldValue::Float(chosen))
        }
        FieldValue::Date(_) => {
            let dates: Vec<NaiveDate> = values.iter().filter_map(as_date).collect();
            let chosen = if pick_min {
                *dates.iter().min().unwrap()
            } else {
                *dates.iter().max().unwrap()
            };
            Some(FieldValue::Date(chosen))
        }
        FieldValue::Duration(_) => {
            let nums: Vec<i64> = values.iter().filter_map(as_duration_seconds).collect();
            let chosen = if pick_min {
                *nums.iter().min().unwrap()
            } else {
                *nums.iter().max().unwrap()
            };
            Some(FieldValue::Duration(chosen))
        }
        _ => None,
    }
}

/// `average` returns Float for numeric inputs (a true mean can be
/// fractional even on integer fields), a midpoint Date for date inputs,
/// and a truncated Duration (integer division on canonical seconds) for
/// duration inputs. The truncation choice keeps round-trip behavior
/// integer-clean — sub-second precision isn't part of the input grammar.
fn average(values: &[FieldValue]) -> Option<FieldValue> {
    match values.first()? {
        FieldValue::Integer(_) | FieldValue::Float(_) => {
            let nums: Vec<f64> = values.iter().filter_map(as_f64).collect();
            let avg = nums.iter().sum::<f64>() / nums.len() as f64;
            Some(FieldValue::Float(avg))
        }
        FieldValue::Date(_) => {
            let dates: Vec<NaiveDate> = values.iter().filter_map(as_date).collect();
            let sum_days: i64 = dates
                .iter()
                .map(|date| date.num_days_from_ce() as i64)
                .sum();
            let avg_days = sum_days / dates.len() as i64;
            NaiveDate::from_num_days_from_ce_opt(avg_days as i32).map(FieldValue::Date)
        }
        FieldValue::Duration(_) => {
            let nums: Vec<i64> = values.iter().filter_map(as_duration_seconds).collect();
            // i128 sum prevents overflow when summing many large values.
            let sum: i128 = nums.iter().map(|n| *n as i128).sum();
            let avg = sum / nums.len() as i128;
            Some(FieldValue::Duration(avg as i64))
        }
        _ => None,
    }
}

/// `median` always returns Float for numeric inputs (even-length input
/// produces a fractional midpoint). For Duration, returns Duration with
/// even-length midpoint truncated to integer seconds.
fn median(values: &[FieldValue]) -> Option<FieldValue> {
    if let Some(FieldValue::Duration(_)) = values.first() {
        let mut nums: Vec<i64> = values.iter().filter_map(as_duration_seconds).collect();
        if nums.is_empty() {
            return None;
        }
        nums.sort();
        let n = nums.len();
        let median = if n % 2 == 1 {
            nums[n / 2]
        } else {
            // i128 sum to avoid overflow on extreme values.
            ((nums[n / 2 - 1] as i128 + nums[n / 2] as i128) / 2) as i64
        };
        return Some(FieldValue::Duration(median));
    }

    let mut nums: Vec<f64> = values.iter().filter_map(as_f64).collect();
    if nums.is_empty() {
        return None;
    }
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = nums.len();
    let median = if n % 2 == 1 {
        nums[n / 2]
    } else {
        (nums[n / 2 - 1] + nums[n / 2]) / 2.0
    };
    Some(FieldValue::Float(median))
}

fn boolean_reduce(values: &[FieldValue], reducer: impl Fn(&[bool]) -> bool) -> Option<FieldValue> {
    let bools: Vec<bool> = values.iter().filter_map(as_bool).collect();
    if bools.is_empty() {
        return None;
    }
    Some(FieldValue::Boolean(reducer(&bools)))
}

fn as_f64(value: &FieldValue) -> Option<f64> {
    match value {
        FieldValue::Integer(i) => Some(*i as f64),
        FieldValue::Float(f) => Some(*f),
        _ => None,
    }
}

fn as_i64(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Integer(i) => Some(*i),
        _ => None,
    }
}

fn as_duration_seconds(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Duration(seconds) => Some(*seconds),
        _ => None,
    }
}

fn as_date(value: &FieldValue) -> Option<NaiveDate> {
    match value {
        FieldValue::Date(d) => Some(*d),
        _ => None,
    }
}

fn as_bool(value: &FieldValue) -> Option<bool> {
    match value {
        FieldValue::Boolean(b) => Some(*b),
        _ => None,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{
        AggregateConfig, AggregateFunction, FieldDefinition, FieldTypeConfig,
    };
    use indexmap::IndexMap;
    use std::path::PathBuf;

    // ── apply_aggregate (table-driven) ──────────────────────────────

    fn int(n: i64) -> FieldValue {
        FieldValue::Integer(n)
    }
    fn float(n: f64) -> FieldValue {
        FieldValue::Float(n)
    }
    fn date(y: i32, m: u32, d: u32) -> FieldValue {
        FieldValue::Date(NaiveDate::from_ymd_opt(y, m, d).unwrap())
    }
    fn boolean(b: bool) -> FieldValue {
        FieldValue::Boolean(b)
    }
    fn duration(seconds: i64) -> FieldValue {
        FieldValue::Duration(seconds)
    }

    #[test]
    fn apply_aggregate_table() {
        struct Case {
            label: &'static str,
            function: AggregateFunction,
            inputs: Vec<FieldValue>,
            expected: Option<FieldValue>,
        }

        let cases = vec![
            // ── Integer ─────────────────────────────────────────────
            Case {
                label: "integer sum",
                function: AggregateFunction::Sum,
                inputs: vec![int(1), int(2), int(4)],
                expected: Some(int(7)),
            },
            Case {
                label: "integer min",
                function: AggregateFunction::Min,
                inputs: vec![int(3), int(1), int(2)],
                expected: Some(int(1)),
            },
            Case {
                label: "integer max",
                function: AggregateFunction::Max,
                inputs: vec![int(3), int(1), int(2)],
                expected: Some(int(3)),
            },
            Case {
                label: "integer average",
                function: AggregateFunction::Average,
                inputs: vec![int(2), int(4)],
                expected: Some(float(3.0)),
            },
            Case {
                label: "integer median odd",
                function: AggregateFunction::Median,
                inputs: vec![int(5), int(1), int(3)],
                expected: Some(float(3.0)),
            },
            Case {
                label: "integer median even",
                function: AggregateFunction::Median,
                inputs: vec![int(1), int(2), int(3), int(4)],
                expected: Some(float(2.5)),
            },
            Case {
                label: "integer count",
                function: AggregateFunction::Count,
                inputs: vec![int(1), int(2), int(3)],
                expected: Some(int(3)),
            },
            // ── Float ───────────────────────────────────────────────
            Case {
                label: "float sum",
                function: AggregateFunction::Sum,
                inputs: vec![float(0.5), float(1.5)],
                expected: Some(float(2.0)),
            },
            Case {
                label: "float min",
                function: AggregateFunction::Min,
                inputs: vec![float(2.5), float(1.5)],
                expected: Some(float(1.5)),
            },
            Case {
                label: "float max",
                function: AggregateFunction::Max,
                inputs: vec![float(2.5), float(1.5)],
                expected: Some(float(2.5)),
            },
            Case {
                label: "float average",
                function: AggregateFunction::Average,
                inputs: vec![float(1.0), float(2.0), float(3.0)],
                expected: Some(float(2.0)),
            },
            Case {
                label: "float median",
                function: AggregateFunction::Median,
                inputs: vec![float(1.0), float(3.0)],
                expected: Some(float(2.0)),
            },
            Case {
                label: "float count",
                function: AggregateFunction::Count,
                inputs: vec![float(1.0), float(2.0)],
                expected: Some(int(2)),
            },
            // ── Date ────────────────────────────────────────────────
            Case {
                label: "date min",
                function: AggregateFunction::Min,
                inputs: vec![date(2026, 5, 1), date(2026, 1, 1), date(2026, 3, 1)],
                expected: Some(date(2026, 1, 1)),
            },
            Case {
                label: "date max",
                function: AggregateFunction::Max,
                inputs: vec![date(2026, 5, 1), date(2026, 1, 1), date(2026, 3, 1)],
                expected: Some(date(2026, 5, 1)),
            },
            Case {
                label: "date average midpoint",
                function: AggregateFunction::Average,
                inputs: vec![date(2026, 1, 1), date(2026, 1, 5)],
                expected: Some(date(2026, 1, 3)),
            },
            // ── Duration ────────────────────────────────────────────
            // Canonical seconds: 1d=86_400, 2d=172_800, 3d=259_200.
            Case {
                label: "duration sum",
                function: AggregateFunction::Sum,
                inputs: vec![duration(86_400), duration(172_800), duration(259_200)],
                expected: Some(duration(518_400)), // 6 days
            },
            Case {
                label: "duration min",
                function: AggregateFunction::Min,
                inputs: vec![duration(259_200), duration(86_400), duration(172_800)],
                expected: Some(duration(86_400)),
            },
            Case {
                label: "duration max",
                function: AggregateFunction::Max,
                inputs: vec![duration(259_200), duration(86_400), duration(172_800)],
                expected: Some(duration(259_200)),
            },
            Case {
                label: "duration average",
                function: AggregateFunction::Average,
                inputs: vec![duration(86_400), duration(172_800), duration(259_200)],
                expected: Some(duration(172_800)),
            },
            Case {
                label: "duration average truncates",
                function: AggregateFunction::Average,
                // (1 + 2) / 2 = 1 (truncated, not 1.5)
                inputs: vec![duration(1), duration(2)],
                expected: Some(duration(1)),
            },
            Case {
                label: "duration median odd",
                function: AggregateFunction::Median,
                inputs: vec![duration(86_400), duration(259_200), duration(172_800)],
                expected: Some(duration(172_800)),
            },
            Case {
                label: "duration median even truncates",
                function: AggregateFunction::Median,
                // midpoint of (1, 2) = (1+2)/2 = 1 (truncated)
                inputs: vec![duration(1), duration(2)],
                expected: Some(duration(1)),
            },
            Case {
                label: "duration count",
                function: AggregateFunction::Count,
                inputs: vec![duration(86_400), duration(172_800)],
                expected: Some(int(2)),
            },
            Case {
                label: "duration sum with negatives",
                function: AggregateFunction::Sum,
                inputs: vec![duration(86_400), duration(-86_400), duration(3_600)],
                expected: Some(duration(3_600)),
            },
            // ── Boolean ─────────────────────────────────────────────
            Case {
                label: "boolean all true",
                function: AggregateFunction::All,
                inputs: vec![boolean(true), boolean(true)],
                expected: Some(boolean(true)),
            },
            Case {
                label: "boolean all mixed",
                function: AggregateFunction::All,
                inputs: vec![boolean(true), boolean(false)],
                expected: Some(boolean(false)),
            },
            Case {
                label: "boolean any true",
                function: AggregateFunction::Any,
                inputs: vec![boolean(false), boolean(true)],
                expected: Some(boolean(true)),
            },
            Case {
                label: "boolean any false",
                function: AggregateFunction::Any,
                inputs: vec![boolean(false), boolean(false)],
                expected: Some(boolean(false)),
            },
            Case {
                label: "boolean none all-false",
                function: AggregateFunction::None,
                inputs: vec![boolean(false), boolean(false)],
                expected: Some(boolean(true)),
            },
            Case {
                label: "boolean none any-true",
                function: AggregateFunction::None,
                inputs: vec![boolean(false), boolean(true)],
                expected: Some(boolean(false)),
            },
            Case {
                label: "boolean count",
                function: AggregateFunction::Count,
                inputs: vec![boolean(true), boolean(false), boolean(true)],
                expected: Some(int(3)),
            },
            // ── Empty inputs ────────────────────────────────────────
            Case {
                label: "empty sum",
                function: AggregateFunction::Sum,
                inputs: vec![],
                expected: None,
            },
            Case {
                label: "empty average",
                function: AggregateFunction::Average,
                inputs: vec![],
                expected: None,
            },
        ];

        for case in cases {
            let actual = apply_aggregate(case.function, &case.inputs);
            assert_eq!(
                actual, case.expected,
                "case '{}': got {:?}, expected {:?}",
                case.label, actual, case.expected
            );
        }
    }

    // ── Integration: end-to-end rollup ──────────────────────────────

    /// Build a minimal schema with `parent: link` and one aggregate field.
    fn schema_with_aggregate(
        field_name: &str,
        type_config: FieldTypeConfig,
        function: AggregateFunction,
        over: Option<&str>,
        error_on_missing: bool,
    ) -> Schema {
        let mut fields = IndexMap::new();
        fields.insert(
            "parent".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: Some(false),
                inverse: Some("children".into()),
            }),
        );
        // Optional secondary link field for "custom over" tests.
        fields.insert(
            "epic".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: Some(false),
                inverse: Some("epic_children".into()),
            }),
        );
        let mut def = FieldDefinition::new(type_config);
        def.aggregate = Some(AggregateConfig {
            function,
            error_on_missing,
            over: over.map(str::to_owned),
        });
        fields.insert(field_name.to_owned(), def);

        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    /// Build a WorkItem with the given fields. `parent` and `epic` are
    /// inserted as Links if non-empty.
    fn item(id: &str, parent: Option<&str>, value: Option<FieldValue>) -> WorkItem {
        item_with(id, parent, None, "effort", value)
    }

    fn item_with(
        id: &str,
        parent: Option<&str>,
        epic: Option<&str>,
        field_name: &str,
        value: Option<FieldValue>,
    ) -> WorkItem {
        let mut fields = HashMap::new();
        if let Some(p) = parent {
            fields.insert(
                "parent".to_owned(),
                FieldValue::Link(WorkItemId::from(p.to_owned())),
            );
        }
        if let Some(e) = epic {
            fields.insert(
                "epic".to_owned(),
                FieldValue::Link(WorkItemId::from(e.to_owned())),
            );
        }
        if let Some(v) = value {
            fields.insert(field_name.to_owned(), v);
        }
        WorkItem {
            id: WorkItemId::from(id.to_owned()),
            fields,
            body: String::new(),
            source_path: PathBuf::from(format!("{id}.md")),
        }
    }

    /// Build reverse_links from the items.
    fn build_reverse_links(
        items: &HashMap<WorkItemId, WorkItem>,
    ) -> HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>> {
        let mut reverse_links: HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>> =
            HashMap::new();
        for item in items.values() {
            for (field_name, value) in &item.fields {
                if let FieldValue::Link(target) = value {
                    reverse_links
                        .entry(field_name.clone())
                        .or_default()
                        .entry(target.clone())
                        .or_default()
                        .push(item.id.clone());
                }
            }
        }
        reverse_links
    }

    fn map_of(items: Vec<WorkItem>) -> HashMap<WorkItemId, WorkItem> {
        items.into_iter().map(|i| (i.id.clone(), i)).collect()
    }

    #[test]
    fn aggregates_leaf_values_to_parent() {
        // root <- a, b
        let schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            None,
            false,
        );
        let mut items = map_of(vec![
            item("root", None, None),
            item("a", Some("root"), Some(int(2))),
            item("b", Some("root"), Some(int(3))),
        ]);
        let reverse_links = build_reverse_links(&items);
        let diagnostics = run(&mut items, &reverse_links, &schema);

        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics: {diagnostics:#?}"
        );
        assert_eq!(items["root"].fields.get("effort"), Some(&int(5)));
        // Leaves still hold their manual values.
        assert_eq!(items["a"].fields.get("effort"), Some(&int(2)));
        assert_eq!(items["b"].fields.get("effort"), Some(&int(3)));
    }

    #[test]
    fn intermediate_manual_value_acts_as_value_provider() {
        // root <- mid (manual) <- leaf (manual would conflict; leave blank)
        let schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            None,
            false,
        );
        let mut items = map_of(vec![
            item("root", None, None),
            item("mid", Some("root"), Some(int(7))),
            item("leaf", Some("mid"), None),
        ]);
        let reverse_links = build_reverse_links(&items);
        let diagnostics = run(&mut items, &reverse_links, &schema);

        assert!(diagnostics.is_empty());
        // mid keeps its manual value; root aggregates from mid (the value-provider).
        assert_eq!(items["mid"].fields.get("effort"), Some(&int(7)));
        assert_eq!(items["root"].fields.get("effort"), Some(&int(7)));
        assert!(items["leaf"].fields.get("effort").is_none());
    }

    #[test]
    fn chain_conflict_emits_diagnostic() {
        // root (manual=10) <- leaf (manual=4): same chain, both set
        let schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            None,
            false,
        );
        let mut items = map_of(vec![
            item("root", None, Some(int(10))),
            item("leaf", Some("root"), Some(int(4))),
        ]);
        let reverse_links = build_reverse_links(&items);
        let diagnostics = run(&mut items, &reverse_links, &schema);

        assert_eq!(diagnostics.len(), 1);
        let kind = &diagnostics[0].kind;
        assert!(
            matches!(
                kind,
                DiagnosticKind::AggregateChainConflict {
                    field, item_id, conflicting_ancestor_id
                } if field == "effort"
                    && item_id.as_str() == "leaf"
                    && conflicting_ancestor_id.as_str() == "root"
            ),
            "unexpected diagnostic: {kind:#?}"
        );
        // Both keep their manual values (root not overwritten by aggregation).
        assert_eq!(items["root"].fields.get("effort"), Some(&int(10)));
        assert_eq!(items["leaf"].fields.get("effort"), Some(&int(4)));
    }

    #[test]
    fn siblings_in_different_chains_no_conflict() {
        // root_a (manual) <- leaf_a; root_b (manual) <- leaf_b
        let schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            None,
            false,
        );
        let mut items = map_of(vec![
            item("root_a", None, Some(int(1))),
            item("root_b", None, Some(int(2))),
        ]);
        let reverse_links = build_reverse_links(&items);
        let diagnostics = run(&mut items, &reverse_links, &schema);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn missing_value_with_error_on_missing_emits_diagnostic() {
        // root <- a (manual=5), b (no value): b is a tree-leaf with no covering ancestor.
        let schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            None,
            true,
        );
        let mut items = map_of(vec![
            item("root", None, None),
            item("a", Some("root"), Some(int(5))),
            item("b", Some("root"), None),
        ]);
        let reverse_links = build_reverse_links(&items);
        let diagnostics = run(&mut items, &reverse_links, &schema);

        // b is missing; a is covered by its own manual; root isn't a leaf.
        let missing: Vec<_> = diagnostics
            .iter()
            .filter_map(|d| match &d.kind {
                DiagnosticKind::AggregateMissingValue { leaf_id, .. } => Some(leaf_id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(missing, vec!["b"]);
    }

    #[test]
    fn missing_value_covered_by_intermediate_ancestor() {
        // root <- mid (manual) <- leaf (no value): leaf is covered by mid.
        let schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            None,
            true,
        );
        let mut items = map_of(vec![
            item("root", None, None),
            item("mid", Some("root"), Some(int(9))),
            item("leaf", Some("mid"), None),
        ]);
        let reverse_links = build_reverse_links(&items);
        let diagnostics = run(&mut items, &reverse_links, &schema);
        assert!(
            diagnostics.is_empty(),
            "leaf should be covered by mid: {diagnostics:#?}"
        );
    }

    #[test]
    fn custom_over_field_walks_correct_link() {
        // a, b have epic: root; root holds the aggregate.
        // parent links go elsewhere (or are absent) and must be ignored.
        let schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            Some("epic"),
            false,
        );
        let mut items = map_of(vec![
            item_with("root", None, None, "effort", None),
            item_with("a", None, Some("root"), "effort", Some(int(2))),
            item_with("b", None, Some("root"), "effort", Some(int(3))),
        ]);
        let reverse_links = build_reverse_links(&items);
        let diagnostics = run(&mut items, &reverse_links, &schema);

        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        assert_eq!(items["root"].fields.get("effort"), Some(&int(5)));
    }

    #[test]
    fn required_aggregate_missing_on_leaf_emits_diagnostic() {
        // root <- a (no value): a is a leaf with no manual and no descendants;
        // required+aggregate means it must end up with a value.
        let mut schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            None,
            false,
        );
        schema.fields.get_mut("effort").unwrap().required = true;

        let mut items = map_of(vec![
            item("root", None, None),
            item("a", Some("root"), None),
        ]);
        let reverse_links = build_reverse_links(&items);
        let diagnostics = run(&mut items, &reverse_links, &schema);

        let missing_ids: Vec<&str> = diagnostics
            .iter()
            .filter_map(|d| match &d.kind {
                DiagnosticKind::MissingRequired { item_id, field } if field == "effort" => {
                    Some(item_id.as_str())
                }
                _ => None,
            })
            .collect();
        // Both root and a end up without a value for effort → both flagged.
        assert_eq!(missing_ids, vec!["a", "root"]);
    }

    #[test]
    fn required_aggregate_filled_by_compute_no_diagnostic() {
        // root has no manual but children supply values → root is filled.
        let mut schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            None,
            false,
        );
        schema.fields.get_mut("effort").unwrap().required = true;

        let mut items = map_of(vec![
            item("root", None, None),
            item("a", Some("root"), Some(int(2))),
            item("b", Some("root"), Some(int(3))),
        ]);
        let reverse_links = build_reverse_links(&items);
        let diagnostics = run(&mut items, &reverse_links, &schema);

        let missing: Vec<_> = diagnostics
            .iter()
            .filter(|d| matches!(d.kind, DiagnosticKind::MissingRequired { .. }))
            .collect();
        assert!(missing.is_empty(), "{missing:#?}");
        assert_eq!(items["root"].fields.get("effort"), Some(&int(5)));
    }

    #[test]
    fn cycle_in_over_does_not_loop() {
        // a -> b -> a (cycle); both have manual values.
        let schema = schema_with_aggregate(
            "effort",
            FieldTypeConfig::Integer {
                min: None,
                max: None,
            },
            AggregateFunction::Sum,
            None,
            false,
        );
        let mut items = map_of(vec![
            item("a", Some("b"), Some(int(1))),
            item("b", Some("a"), Some(int(2))),
        ]);
        let reverse_links = build_reverse_links(&items);
        // If the visited guard fails this hangs; we just need it to return.
        let _ = run(&mut items, &reverse_links, &schema);
    }
}
