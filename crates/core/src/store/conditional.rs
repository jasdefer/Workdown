//! Conditional evaluation: one field's `when:` branches on one item.
//!
//! The choose-by-condition sibling of [`super::compute`]: the branches
//! evaluate top to bottom and the first condition that holds supplies
//! the value. A branch whose condition *cannot be answered* — a
//! referenced field is absent on this item — simply does not match,
//! and evaluation falls through to the next branch, mirroring how the
//! rule engine skips comparisons on absent operands. The config's
//! `default` catches everything that fell through; without one the
//! field stays unset, and the deferred required check in
//! [`super::derive`] reports it when the field is `required`.
//!
//! Scheduling — which items evaluate, that a hand-written frontmatter
//! value always wins, and the leaves-only restriction when the field
//! also aggregates — is the derive orchestrator's job; this module
//! only answers "what value do the branches pick for this item".

use indexmap::IndexMap;

use crate::expression::{evaluate, EvaluateError, Value};
use crate::model::diagnostic::{Diagnostic, ItemDiagnosticKind};
use crate::model::schema::{Severity, WhenConfig};
use crate::model::{FieldValue, WorkItem};

use super::compute::ItemValueContext;

/// Evaluate the `when:` branches of `field_name` on one item. Returns
/// the value the first matching branch (or the config's `default`)
/// picked, plus warnings for branches whose condition failed at
/// runtime on this item's actual values (arithmetic inside the
/// comparison overflowed, …) — those branches are skipped and
/// evaluation keeps falling through.
pub(super) fn evaluate_for_item(
    item: &WorkItem,
    field_name: &str,
    config: &WhenConfig,
    constants: &IndexMap<String, FieldValue>,
    today: &Value,
) -> (Option<FieldValue>, Vec<Diagnostic>) {
    let context = ItemValueContext {
        fields: &item.fields,
        constants,
        today: today.clone(),
    };

    let mut warnings = Vec::new();
    let mut chosen: Option<FieldValue> = None;
    for (index, branch) in config.branches.iter().enumerate() {
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
            // Real runtime failures on this item's actual values.
            Err(runtime_failure) => {
                warnings.push(Diagnostic::item(
                    Severity::Warning,
                    item.source_path.clone(),
                    item.id.clone(),
                    ItemDiagnosticKind::WhenConditionFailed {
                        field: field_name.to_owned(),
                        branch_number: index + 1,
                        detail: runtime_failure.to_string(),
                    },
                ));
            }
        }
    }

    (chosen.or_else(|| config.default.clone()), warnings)
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
