//! Conditional pass: evaluate one field's `when:` branches per item.
//!
//! The choose-by-condition sibling of [`super::compute`]: for each item
//! that doesn't already carry the field (a hand-written frontmatter
//! value always wins), the branches evaluate top to bottom and the
//! first condition that holds supplies the value. A branch whose
//! condition *cannot be answered* — a referenced field is absent on
//! this item — simply does not match, and evaluation falls through to
//! the next branch, mirroring how the rule engine skips comparisons on
//! absent operands. The config's `default` catches everything that fell
//! through; without one the field stays unset, and the deferred
//! required check in [`super::derive`] reports it when the field is
//! `required`.
//!
//! Composition mirrors compute exactly: with an `aggregate:` on the
//! same field the pass is restricted to leaves of the rollup hierarchy,
//! and derived values are never written back to files.

use std::collections::HashMap;

use chrono::NaiveDate;
use indexmap::IndexMap;

use crate::expression::{evaluate, EvaluateError, Value};
use crate::model::diagnostic::{Diagnostic, ItemDiagnosticKind};
use crate::model::schema::{Severity, WhenConfig};
use crate::model::{FieldValue, WorkItem, WorkItemId};

use super::compute::{is_leaf, timestamp_of, ItemValueContext};

/// One `when:`-configured field, resolved by the derive orchestrator —
/// the conditional counterpart to [`super::compute::ComputeFieldSpec`].
pub(super) struct WhenFieldSpec<'a> {
    pub(super) name: &'a str,
    pub(super) config: &'a WhenConfig,
    /// The aggregate's resolved `over` link when the field also
    /// aggregates — restricting the pass to leaves of that hierarchy.
    pub(super) leaves_only_over: Option<String>,
}

/// Evaluate `spec`'s branches for every eligible item, writing results
/// into `items`. `evaluation_date` is what `$today` resolves to.
pub(super) fn run_for_field(
    items: &mut HashMap<WorkItemId, WorkItem>,
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    constants: &IndexMap<String, FieldValue>,
    evaluation_date: NaiveDate,
    spec: &WhenFieldSpec<'_>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let today = timestamp_of(evaluation_date);

    // Sorted for deterministic diagnostic order, like the other passes.
    let mut item_ids: Vec<WorkItemId> = items.keys().cloned().collect();
    item_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    for item_id in item_ids {
        let Some(item) = items.get(&item_id) else {
            continue;
        };
        if item.fields.contains_key(spec.name) {
            continue; // manual value wins; when fills only absence
        }
        if let Some(over) = &spec.leaves_only_over {
            if !is_leaf(reverse_links, &item_id, over) {
                continue; // non-leaf of a when+aggregate field: rollup's job
            }
        }

        let context = ItemValueContext {
            fields: &item.fields,
            constants,
            today: today.clone(),
        };

        let mut chosen: Option<FieldValue> = None;
        for (index, branch) in spec.config.branches.iter().enumerate() {
            match evaluate(&branch.condition, &context) {
                Ok(Value::Boolean(true)) => {
                    chosen = Some(branch.value.clone());
                    break;
                }
                Ok(Value::Boolean(false)) => {}
                // Non-boolean results were rejected by the schema-level
                // check; a config that reaches this pass never produces
                // them. Skip defensively.
                Ok(_) => {}
                // An absent input means the condition cannot be answered:
                // the branch does not match, evaluation falls through.
                // InvalidOperation is schema-level and already reported.
                Err(EvaluateError::MissingInput { .. }) | Err(EvaluateError::InvalidOperation) => {}
                // Real runtime failures on this item's actual values
                // (arithmetic inside the comparison overflowed, …):
                // warn, skip the branch, keep falling through.
                Err(runtime_failure) => {
                    diagnostics.push(Diagnostic::item(
                        Severity::Warning,
                        item.source_path.clone(),
                        item_id.clone(),
                        ItemDiagnosticKind::WhenConditionFailed {
                            field: spec.name.to_owned(),
                            branch_number: index + 1,
                            detail: runtime_failure.to_string(),
                        },
                    ));
                }
            }
        }

        let value = chosen.or_else(|| spec.config.default.clone());
        if let Some(value) = value {
            if let Some(item) = items.get_mut(&item_id) {
                item.fields.insert(spec.name.to_owned(), value);
            }
        }
    }
}

/// The condition inputs absent on `item`, across all branches —
/// deduplicated, in source order. Feeds the required-field diagnostic
/// so it names why branches could not be answered.
pub(super) fn missing_inputs(item: &WorkItem, config: &WhenConfig) -> Vec<String> {
    let mut missing = Vec::new();
    for branch in &config.branches {
        for reference in branch.condition.field_references() {
            if !item.fields.contains_key(reference) && !missing.iter().any(|seen| seen == reference)
            {
                missing.push(reference.to_owned());
            }
        }
    }
    missing
}
