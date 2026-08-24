//! The load pipeline's required-field check: one check, after the
//! fill-in phase.
//!
//! Runs once every mechanism that could supply a value has had its
//! chance, so a derived value is held to the same standard as a
//! hand-written one and this check never needs to predict *which*
//! items a mechanism could fill — the two hand-mirrored mechanism
//! lists that prediction once required are gone (see ADR-012).
//!
//! The check stays silent in two situations. Where coercion recorded a
//! conversion failure, the invalid-value diagnostic already stands and
//! "missing" would be false — without that record, "written but
//! invalid" and "never written" are indistinguishable here, both being
//! absent keys. And where an incomplete pull has already been reported
//! by the pull pass itself (`error_on_missing`), repeating the same
//! inputs would say the same thing twice.
//!
//! Findings name the actual cause where a fill mechanism can explain
//! the blank — a computed field's absent inputs, a pull's incomplete
//! link targets, a conditional field's unmatched branches — and fall
//! back to the plain missing-required message otherwise. Reporting
//! order is item-first (ascending item id, schema declaration order
//! within an item): users fix files, not schema fields.

use std::collections::{HashMap, HashSet};

use crate::model::diagnostic::{Diagnostic, ItemDiagnosticKind};
use crate::model::schema::{FieldDefinition, FillMechanism, Schema, Severity};
use crate::model::{WorkItem, WorkItemId};
use crate::walker::targets_of;

use super::compute;
use super::conditional;
use super::rollup;

/// What the check concluded about one blank required field.
enum Finding {
    /// Report this diagnostic against the item.
    Report(ItemDiagnosticKind),
    /// An earlier pass already reported the same cause against the
    /// same item — stay silent rather than say it twice.
    AlreadyReported,
}

/// Check every required field on every item, after the fill-in phase.
///
/// `disabled_compute_fields` names the fields whose derivation config
/// failed the schema-level check and never ran; `conversion_failures`
/// is coercion's per-item record of fields that were written but
/// failed conversion.
pub(crate) fn check(
    items: &HashMap<WorkItemId, WorkItem>,
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    schema: &Schema,
    disabled_compute_fields: &HashSet<String>,
    conversion_failures: &HashMap<WorkItemId, HashSet<String>>,
) -> Vec<Diagnostic> {
    let mut item_ids: Vec<&WorkItemId> = items.keys().collect();
    item_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));

    let mut diagnostics = Vec::new();
    for item_id in item_ids {
        let Some(item) = items.get(item_id) else {
            continue;
        };
        for (field_name, field_definition) in &schema.fields {
            if !field_definition.required {
                continue;
            }
            // `id` is identity, always projected into the field map by
            // coercion, so it never trips the absence test below.
            if item.fields.contains_key(field_name) {
                continue;
            }
            if conversion_failures
                .get(item_id)
                .is_some_and(|failed_fields| failed_fields.contains(field_name))
            {
                // Written but invalid: the invalid-value diagnostic
                // already stands, and "missing" would be false.
                continue;
            }
            match finding(
                item,
                items,
                reverse_links,
                field_name,
                field_definition,
                disabled_compute_fields,
            ) {
                Finding::Report(kind) => diagnostics.push(Diagnostic::item(
                    Severity::Error,
                    item.source_path.clone(),
                    item.id.clone(),
                    kind,
                )),
                Finding::AlreadyReported => {}
            }
        }
    }
    diagnostics
}

/// Decide what one blank required field gets reported as, naming the
/// cause where a fill mechanism can explain the blank.
fn finding(
    item: &WorkItem,
    items: &HashMap<WorkItemId, WorkItem>,
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    field_name: &str,
    field_definition: &FieldDefinition,
    disabled_compute_fields: &HashSet<String>,
) -> Finding {
    let generic = ItemDiagnosticKind::MissingRequired {
        field: field_name.to_owned(),
    };

    // A schema-check-failed config never ran; the check's schema
    // diagnostic carries the cause, so the item reports the plain
    // blank without guessing at inputs of a pass that never happened.
    if disabled_compute_fields.contains(field_name) {
        return Finding::Report(generic);
    }

    // On a non-leaf of an aggregating field the same-item/pull pass
    // never runs — the rollup owns everything above the leaves, and it
    // found nothing to aggregate. There is no per-input cause to name.
    let mechanisms = field_definition.fill_mechanisms();
    let has_same_item_or_pull = mechanisms.iter().any(|mechanism| match mechanism {
        FillMechanism::Compute | FillMechanism::Pull | FillMechanism::When => true,
        FillMechanism::Aggregate => false,
    });
    let same_item_pass_skipped = has_same_item_or_pull
        && field_definition
            .aggregate
            .as_ref()
            .is_some_and(|aggregate| {
                let over = aggregate
                    .over
                    .as_deref()
                    .unwrap_or(rollup::DEFAULT_OVER_FIELD);
                !compute::is_leaf(reverse_links, &item.id, over)
            });
    if same_item_pass_skipped {
        return Finding::Report(generic);
    }

    // Per-mechanism cause, first one that has something to say wins.
    // The match is exhaustive on purpose: a new mechanism must decide
    // here what its blank-but-required story is before this compiles.
    for mechanism in mechanisms {
        let cause = match mechanism {
            // Cross-item with no same-item source: the rollup found no
            // bearers below. Nothing per-item to name.
            FillMechanism::Aggregate => None,
            FillMechanism::Compute => field_definition.compute.as_ref().and_then(|config| {
                let missing_inputs = compute::missing_inputs(item, config);
                if missing_inputs.is_empty() {
                    None
                } else {
                    Some(ItemDiagnosticKind::ComputeMissingInputs {
                        field: field_name.to_owned(),
                        missing_inputs,
                    })
                }
            }),
            FillMechanism::Pull => {
                let Some(pull) = &field_definition.pull else {
                    continue;
                };
                let missing_inputs = pull_missing_inputs(item, pull, items);
                if missing_inputs.is_empty() {
                    // No incomplete link target — an unanchored root
                    // (or a manual anchor simply not written yet).
                    None
                } else if pull.error_on_missing {
                    // The pull pass already reported these very inputs
                    // against this item — `error_on_missing` is what
                    // asked it to. Repeating it here would say the same
                    // thing twice; adding the generic missing-required
                    // message instead would say it twice less usefully.
                    return Finding::AlreadyReported;
                } else {
                    Some(ItemDiagnosticKind::PullMissingInputs {
                        field: field_name.to_owned(),
                        missing_inputs,
                    })
                }
            }
            FillMechanism::When => field_definition.when.as_ref().map(|when_config| {
                ItemDiagnosticKind::WhenUnmatched {
                    field: field_name.to_owned(),
                    missing_inputs: conditional::missing_inputs(item, when_config),
                }
            }),
        };
        if let Some(kind) = cause {
            return Finding::Report(kind);
        }
    }

    Finding::Report(generic)
}

/// The pull's link targets that have no source value, as
/// `target_id.field` — the cause the required-field diagnostic names.
fn pull_missing_inputs(
    item: &WorkItem,
    pull: &crate::model::schema::PullConfig,
    items: &HashMap<WorkItemId, WorkItem>,
) -> Vec<String> {
    targets_of(item, &pull.over)
        .into_iter()
        .filter(|target_id| {
            !items
                .get(*target_id)
                .is_some_and(|target| target.fields.contains_key(&pull.field))
        })
        .map(|target_id| format!("{}.{}", target_id.as_str(), pull.field))
        .collect()
}
