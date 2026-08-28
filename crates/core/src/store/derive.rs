//! Derive orchestrator: evaluate every derived (item, field) unit in
//! dependency order over one unified graph.
//!
//! Nodes are the (item, field) pairs of the fields carrying any
//! derivation config; edges express "must be evaluated first": a
//! compute/when expression's inputs on the same item, a pull's source
//! field on its forward link targets, and an aggregate's direct
//! children along its `over` link. The graph owns *ordering only* —
//! the value semantics stay with the mechanisms ([`compute`],
//! [`conditional`], [`rollup`], the pull reduction below). In
//! particular an aggregate reduces over the transitive *bearer*
//! contributions (hand-written values and same-item results), never
//! over its children's already-reduced values, which keeps `count`,
//! `average`, and `median` counting bearers.
//!
//! Edges exist only where evaluation will actually read the input: an
//! item whose file already carries the field is *settled* and waits
//! for nothing (manual wins), so a hand-written anchor breaks any
//! dependency loop; and non-leaves of a derive+aggregate field never
//! wait for same-item or pull inputs the rollup makes irrelevant.
//!
//! Scheduling is a deterministic Kahn walk: among ready nodes, the
//! field earliest in reference order wins, then the smallest item id.
//! Without cross-item field dependencies this reproduces the classic
//! per-field pass order exactly — `effort → cost → budget` fills
//! front to back, and a computed leaf value is in place before the
//! same field's rollup aggregates it. Field-level loops that are
//! acyclic at the item level — `start` pulled from the dependencies'
//! `end`, `end` computed from the same item's `start` — evaluate
//! naturally: the walk follows the item graph. Nodes on a genuine,
//! unanchored dependency cycle never become ready and receive no
//! derived value; a cycle within one link field is the link cycle
//! detector's finding, while a loop only the *combination* of link
//! fields produces gets a [`ItemDiagnosticKind::DeriveCycle`] here.
//!
//! Fields whose compute/when config failed the schema-level check
//! (`compute_check`) — unknown references, type errors, cycle members
//! — arrive in `disabled_compute_fields` and skip their same-item
//! pass (an aggregate on the same field still runs), so a broken
//! config surfaces once against `schema.yaml` and never per item.
//!
//! A field coercion recorded as written-but-invalid (see
//! `conversion_failures`) is never filled: the author wrote a value,
//! and replacing a broken hand-written value with a derived one would
//! silently override the file. Its slot stays absent until the file is
//! fixed; an aggregating ancestor still passes its children's
//! contributions through, so the rest of the tree degrades gracefully.
//!
//! Completeness is judged elsewhere: the required check
//! ([`super::required`]) runs as its own phase after this one, per the
//! pipeline contract in [`super`] (ADR-012).

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use chrono::NaiveDate;
use indexmap::IndexMap;

use crate::expression::Value;
use crate::model::diagnostic::{Diagnostic, DiagnosticBody, ItemDiagnosticKind};
use crate::model::schema::{FieldDefinition, FieldType, PullConfig, Schema, Severity};
use crate::model::{FieldValue, WorkItem, WorkItemId};
use crate::walker::{target_of_link, targets_of};

use super::compute;
use super::conditional;
use super::rollup;

/// Run every derive pass. Mutates `items` in place; returns all
/// diagnostics the passes produced. `constants` are the project
/// constants from `resources.yaml`, resolved by compute expressions;
/// `evaluation_date` is what `$today` resolves to;
/// `disabled_compute_fields` names the compute configs that failed
/// `compute_check` and must not evaluate; and `conversion_failures` is
/// coercion's per-item record of written-but-invalid fields, whose
/// slots no pass may fill.
pub(crate) fn run(
    items: &mut HashMap<WorkItemId, WorkItem>,
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    schema: &Schema,
    constants: &IndexMap<String, FieldValue>,
    evaluation_date: NaiveDate,
    disabled_compute_fields: &HashSet<String>,
    conversion_failures: &HashMap<WorkItemId, HashSet<String>>,
) -> Vec<Diagnostic> {
    let derive_fields = derive_fields_in_order(schema, disabled_compute_fields);

    let mut item_ids: Vec<WorkItemId> = items.keys().cloned().collect();
    item_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    let item_count = item_ids.len();
    let item_index: HashMap<WorkItemId, usize> = item_ids
        .iter()
        .enumerate()
        .map(|(position, item_id)| (item_id.clone(), position))
        .collect();

    // Node id = field slot × item count + item position. Ascending
    // node id is exactly the evaluation priority: field reference
    // order first, then item id.
    let node_count = derive_fields.len() * item_count;
    let slot_of_field: HashMap<&str, usize> = derive_fields
        .iter()
        .enumerate()
        .map(|(slot, derive_field)| (derive_field.name, slot))
        .collect();

    // Edges are recorded only where the evaluator will actually consult
    // the input. An item whose file already carries the field is
    // *settled*: its node waits for nothing (manual wins, so evaluation
    // reads no inputs there) — which is what lets a hand-written anchor
    // break any dependency loop. A field coercion recorded as
    // written-but-invalid is treated the same way here: its pass never
    // runs (the slot is never filled), so waiting for its inputs could
    // close a loop that doesn't exist semantically. And an item the
    // same-item/pull pass is not eligible for (a non-leaf of a
    // derive+aggregate field) never waits for that pass's inputs
    // either, for the same reason.
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); node_count];
    let mut pending_inputs: Vec<usize> = vec![0; node_count];
    for (slot, derive_field) in derive_fields.iter().enumerate() {
        // Same-item edges: the expression's (or the conditions')
        // referenced fields on the same item come first.
        if derive_field.same_item_enabled {
            let mut input_slots: Vec<usize> = derive_field
                .definition
                .derived_references()
                .into_iter()
                .filter_map(|referenced| slot_of_field.get(referenced).copied())
                .filter(|input_slot| *input_slot != slot)
                .collect();
            input_slots.sort_unstable();
            input_slots.dedup();
            if !input_slots.is_empty() {
                for (position, item_id) in item_ids.iter().enumerate() {
                    let Some(item) = items.get(item_id) else {
                        continue;
                    };
                    if item.fields.contains_key(derive_field.name)
                        || conversion_failed(conversion_failures, item_id, derive_field.name)
                        || !same_item_pass_runs_on(derive_field, reverse_links, item_id)
                    {
                        continue;
                    }
                    for &input_slot in &input_slots {
                        dependents[input_slot * item_count + position]
                            .push(slot * item_count + position);
                        pending_inputs[slot * item_count + position] += 1;
                    }
                }
            }
        }
        // Pull edges: the source field on every forward link target
        // comes before the pulling item.
        if derive_field.pull_enabled {
            if let Some(pull) = &derive_field.definition.pull {
                if let Some(&source_slot) = slot_of_field.get(pull.field.as_str()) {
                    for (position, item_id) in item_ids.iter().enumerate() {
                        let Some(item) = items.get(item_id) else {
                            continue;
                        };
                        if item.fields.contains_key(derive_field.name)
                            || !same_item_pass_runs_on(derive_field, reverse_links, item_id)
                        {
                            continue;
                        }
                        let node = slot * item_count + position;
                        for target in targets_of(item, &pull.over) {
                            let Some(&target_position) = item_index.get(target) else {
                                continue;
                            };
                            let source_node = source_slot * item_count + target_position;
                            if source_node == node {
                                continue;
                            }
                            dependents[source_node].push(node);
                            pending_inputs[node] += 1;
                        }
                    }
                }
            }
        }
        // Aggregate edges: every direct child along the `over` link
        // comes before its parent (same field). A parent whose file
        // already carries the field is a bearer — it ignores its
        // children's contributions, so it doesn't wait for them.
        if let Some(over) = &derive_field.aggregate_over {
            for (position, item_id) in item_ids.iter().enumerate() {
                let Some(item) = items.get(item_id) else {
                    continue;
                };
                let Some(target) = target_of_link(item, over) else {
                    continue;
                };
                let Some(&target_position) = item_index.get(target) else {
                    continue;
                };
                if target_position == position {
                    continue;
                }
                if items
                    .get(&item_ids[target_position])
                    .is_some_and(|target_item| target_item.fields.contains_key(derive_field.name))
                {
                    continue;
                }
                dependents[slot * item_count + position].push(slot * item_count + target_position);
                pending_inputs[slot * item_count + target_position] += 1;
            }
        }
    }

    let inputs = EvaluationInputs {
        item_ids: &item_ids,
        item_index: &item_index,
        item_count,
        reverse_links,
        constants,
        today: compute::timestamp_of(evaluation_date),
        conversion_failures,
    };
    let mut state = EvaluationState {
        contributions: vec![Vec::new(); node_count],
        bearer: vec![false; node_count],
        evaluated: vec![false; node_count],
        same_item_diagnostics: vec![Vec::new(); derive_fields.len()],
    };

    // Kahn walk over the ready nodes, smallest node id first.
    let mut ready: BinaryHeap<Reverse<usize>> = (0..node_count)
        .filter(|node| pending_inputs[*node] == 0)
        .map(Reverse)
        .collect();
    while let Some(Reverse(node)) = ready.pop() {
        let slot = node / item_count;
        evaluate_node(
            &inputs,
            &derive_fields[slot],
            slot,
            node % item_count,
            items,
            &mut state,
        );
        for &dependent in &dependents[node] {
            pending_inputs[dependent] -= 1;
            if pending_inputs[dependent] == 0 {
                ready.push(Reverse(dependent));
            }
        }
    }

    // Diagnostics grouped per field in evaluation order — same-item
    // findings first (ascending item id), then the aggregate's chain
    // and coverage checks — so the report reads identically however
    // the graph interleaved the work.
    let mut diagnostics = Vec::new();
    for (slot, derive_field) in derive_fields.iter().enumerate() {
        let mut same_item = std::mem::take(&mut state.same_item_diagnostics[slot]);
        same_item.sort_by(|left, right| item_id_of(left).cmp(item_id_of(right)));
        diagnostics.append(&mut same_item);

        let Some(aggregate) = &derive_field.definition.aggregate else {
            continue;
        };
        let Some(over) = derive_field.aggregate_over.as_deref() else {
            continue;
        };
        // Bearers for the checks, flagged during the walk. The flags
        // are complete: an item with a hand-written value has no
        // incoming edges (settled nodes wait for nothing), so its node
        // always evaluates — even when it sits on a dependency loop.
        let bearer_ids: Vec<WorkItemId> = (0..item_count)
            .filter(|position| state.bearer[slot * item_count + position])
            .map(|position| item_ids[position].clone())
            .collect();
        let bearer_set: HashSet<WorkItemId> = bearer_ids.iter().cloned().collect();
        diagnostics.extend(rollup::conflict_diagnostics(
            items,
            over,
            derive_field.name,
            &bearer_ids,
            &bearer_set,
        ));
        if aggregate.error_on_missing {
            diagnostics.extend(rollup::coverage_diagnostics(
                items,
                reverse_links,
                over,
                derive_field.name,
                &bearer_set,
            ));
        }
    }

    diagnostics.extend(derive_cycle_diagnostics(
        &derive_fields,
        &item_ids,
        item_count,
        &dependents,
        &state.evaluated,
        items,
    ));

    diagnostics
}

/// One field participating in derivation, in evaluation order.
struct DeriveField<'schema> {
    name: &'schema str,
    definition: &'schema FieldDefinition,
    /// The aggregate's `over` link, resolved to a concrete field name,
    /// when the field aggregates.
    aggregate_over: Option<String>,
    /// Whether the same-item pass (compute or when) may run — false
    /// when the config failed the schema-level check.
    same_item_enabled: bool,
    /// Whether the pull pass may run — false when the config failed
    /// the schema-level check.
    pull_enabled: bool,
}

/// The fields carrying any derivation config, in evaluation order: a
/// derivation's inputs before the field that consumes them (via
/// [`field_order`]), declaration order otherwise.
fn derive_fields_in_order<'schema>(
    schema: &'schema Schema,
    disabled_compute_fields: &HashSet<String>,
) -> Vec<DeriveField<'schema>> {
    field_order(schema)
        .into_iter()
        .filter_map(|field_name| {
            let (name, definition) = schema.fields.get_key_value(&field_name)?;
            let same_item_enabled =
                definition.is_derived() && !disabled_compute_fields.contains(name.as_str());
            let pull_enabled =
                definition.pull.is_some() && !disabled_compute_fields.contains(name.as_str());
            if !same_item_enabled && !pull_enabled && definition.aggregate.is_none() {
                return None;
            }
            Some(DeriveField {
                name: name.as_str(),
                definition,
                aggregate_over: definition
                    .aggregate
                    .as_ref()
                    .map(|aggregate| aggregate.over.clone()),
                same_item_enabled,
                pull_enabled,
            })
        })
        .collect()
}

/// Whether the same-item/pull pass may run on this item: always when
/// the field doesn't aggregate, only on leaves of the aggregate's
/// `over` hierarchy otherwise (the rollup owns everything above).
/// Shared by the edge construction and [`evaluate_node`], so the
/// dependency graph never waits for a pass the evaluator would skip.
fn same_item_pass_runs_on(
    derive_field: &DeriveField<'_>,
    reverse_links: &HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    item_id: &WorkItemId,
) -> bool {
    derive_field
        .aggregate_over
        .as_deref()
        .is_none_or(|over| super::is_leaf(reverse_links, item_id, over))
}

/// Shared read-only inputs of every node evaluation.
struct EvaluationInputs<'run> {
    item_ids: &'run [WorkItemId],
    item_index: &'run HashMap<WorkItemId, usize>,
    item_count: usize,
    reverse_links: &'run HashMap<String, HashMap<WorkItemId, Vec<WorkItemId>>>,
    constants: &'run IndexMap<String, FieldValue>,
    /// The evaluation date as a midnight timestamp — what `$today`
    /// resolves to for every node.
    today: Value,
    /// Coercion's per-item record of written-but-invalid fields —
    /// slots no pass may fill.
    conversion_failures: &'run HashMap<WorkItemId, HashSet<String>>,
}

/// Whether coercion recorded this item's field as written but invalid
/// — a slot no pass may fill.
fn conversion_failed(
    conversion_failures: &HashMap<WorkItemId, HashSet<String>>,
    item_id: &WorkItemId,
    field_name: &str,
) -> bool {
    conversion_failures
        .get(item_id)
        .is_some_and(|failed_fields| failed_fields.contains(field_name))
}

/// Mutable evaluation state, indexed by node id.
struct EvaluationState {
    /// What each aggregate-field node passes to the item's parent: its
    /// own `(item position, value)` when the item is a bearer, the
    /// concatenation of its children's lists otherwise. Bearer-granular
    /// on purpose — `count`, `average`, and `median` reduce over
    /// bearers, never over already-reduced child values.
    contributions: Vec<Vec<(usize, FieldValue)>>,
    /// Whether the node's item carries an original value for the field
    /// (hand-written, or produced by the same-item pass) — the
    /// contributors and conflict candidates of the aggregate checks.
    bearer: Vec<bool>,
    /// Whether the node was reached by the Kahn walk at all — false
    /// only for nodes on a dependency cycle.
    evaluated: Vec<bool>,
    /// Compute/when findings per field slot, ordered at assembly.
    same_item_diagnostics: Vec<Vec<Diagnostic>>,
}

/// Evaluate one (item, field) node: run the same-item pass if the
/// value is absent, then the aggregate reduction if the field rolls
/// up. Writes derived values into `items` in place.
fn evaluate_node(
    inputs: &EvaluationInputs<'_>,
    derive_field: &DeriveField<'_>,
    slot: usize,
    item_position: usize,
    items: &mut HashMap<WorkItemId, WorkItem>,
    state: &mut EvaluationState,
) {
    let item_id = &inputs.item_ids[item_position];
    let node = slot * inputs.item_count + item_position;
    state.evaluated[node] = true;

    let had_value = items
        .get(item_id)
        .is_some_and(|item| item.fields.contains_key(derive_field.name));

    // Derivation pass: compute, when, or pull fills absence — on
    // leaves only when the field also aggregates (the rollup owns
    // everything above), and never where coercion recorded a
    // written-but-invalid value (the author wrote something; the file
    // must be fixed, not silently overridden).
    let mut derived_value: Option<FieldValue> = None;
    if !had_value
        && !conversion_failed(inputs.conversion_failures, item_id, derive_field.name)
        && (derive_field.same_item_enabled || derive_field.pull_enabled)
        && same_item_pass_runs_on(derive_field, inputs.reverse_links, item_id)
    {
        if let Some(item) = items.get(item_id) {
            if !derive_field.same_item_enabled {
                // Pull: mutually exclusive with compute and when.
                if let Some(pull) = &derive_field.definition.pull {
                    match evaluate_pull(pull, derive_field.definition.field_type(), item, items) {
                        PullOutcome::Value(value) => derived_value = Some(value),
                        PullOutcome::MissingInputs(missing_inputs) => {
                            if pull.error_on_missing {
                                state.same_item_diagnostics[slot].push(Diagnostic::item(
                                    Severity::Error,
                                    item.source_path.clone(),
                                    item.id.clone(),
                                    ItemDiagnosticKind::PullMissingInputs {
                                        field: derive_field.name.to_owned(),
                                        missing_inputs,
                                    },
                                ));
                            }
                        }
                        PullOutcome::Skip => {}
                    }
                }
            } else if let Some(config) = &derive_field.definition.compute {
                match compute::evaluate_for_item(
                    item,
                    derive_field.name,
                    derive_field.definition.field_type(),
                    config,
                    inputs.constants,
                    &inputs.today,
                ) {
                    compute::SameItemOutcome::Value(value) => derived_value = Some(value),
                    compute::SameItemOutcome::Report(diagnostic) => {
                        state.same_item_diagnostics[slot].push(diagnostic);
                    }
                    compute::SameItemOutcome::Skip => {}
                }
            } else if let Some(when_config) = &derive_field.definition.when {
                let (value, mut warnings) = conditional::evaluate_for_item(
                    item,
                    derive_field.name,
                    when_config,
                    inputs.constants,
                    &inputs.today,
                );
                state.same_item_diagnostics[slot].append(&mut warnings);
                derived_value = value;
            }
        }
        if let Some(value) = &derived_value {
            if let Some(item) = items.get_mut(item_id) {
                item.fields
                    .insert(derive_field.name.to_owned(), value.clone());
            }
        }
    }

    let Some(aggregate) = &derive_field.definition.aggregate else {
        return;
    };
    let Some(over) = derive_field.aggregate_over.as_deref() else {
        return;
    };

    if had_value || derived_value.is_some() {
        // A bearer: its own value is what flows to the parent; the
        // children's contributions stop here (the chain-conflict check
        // reports bearers nested under other bearers).
        state.bearer[node] = true;
        let value = derived_value.or_else(|| {
            items
                .get(item_id)
                .and_then(|item| item.fields.get(derive_field.name).cloned())
        });
        if let Some(value) = value {
            state.contributions[node] = vec![(item_position, value)];
        }
        return;
    }

    // Not a bearer: gather the children's contributions, reduce them
    // into this item's value, and pass the bearer-granular list on.
    let mut gathered: Vec<(usize, FieldValue)> = Vec::new();
    if let Some(sources) = inputs
        .reverse_links
        .get(over)
        .and_then(|by_target| by_target.get(item_id))
    {
        for source_id in sources {
            let Some(&source_position) = inputs.item_index.get(source_id) else {
                continue;
            };
            if source_position == item_position {
                continue;
            }
            gathered.extend_from_slice(
                &state.contributions[slot * inputs.item_count + source_position],
            );
        }
    }
    // Ascending bearer id, so order-sensitive reductions (float sums)
    // are deterministic.
    gathered.sort_by_key(|(bearer_position, _)| *bearer_position);

    if !gathered.is_empty()
        && !conversion_failed(inputs.conversion_failures, item_id, derive_field.name)
    {
        let values: Vec<FieldValue> = gathered.iter().map(|(_, value)| value.clone()).collect();
        if let Some(reduced) = rollup::apply_aggregate(aggregate.function, &values) {
            if let Some(item) = items.get_mut(item_id) {
                item.fields.insert(derive_field.name.to_owned(), reduced);
            }
        }
    }
    // The children's contributions pass through regardless — a broken
    // value on this item must not cut its subtree off from ancestors.
    state.contributions[node] = gathered;
}

/// The item id a same-item diagnostic is pinned to, for the per-field
/// ordering of the report.
fn item_id_of(diagnostic: &Diagnostic) -> &str {
    match &diagnostic.body {
        DiagnosticBody::Item(body) => body.item_id.as_str(),
        _ => "",
    }
}

// ── Pull evaluation ─────────────────────────────────────────────────

/// What one item's pull evaluation produced.
enum PullOutcome {
    Value(FieldValue),
    /// Linked items lacking the source field, as `target_id.field` —
    /// all-or-nothing, so any entry here means no value.
    MissingInputs(Vec<String>),
    Skip,
}

/// Read the pull's source field from every forward link target and
/// reduce the values. No targets means no value (the item is a root of
/// the pull graph and carries a manual anchor instead); an incomplete
/// target means no value either — a partial reduction would be a
/// silent guess.
fn evaluate_pull(
    pull: &PullConfig,
    declared_type: FieldType,
    item: &WorkItem,
    items: &HashMap<WorkItemId, WorkItem>,
) -> PullOutcome {
    let targets = targets_of(item, &pull.over);
    if targets.is_empty() {
        return PullOutcome::Skip;
    }

    let mut values = Vec::with_capacity(targets.len());
    let mut missing_inputs = Vec::new();
    for target_id in targets {
        match items
            .get(target_id)
            .and_then(|target| target.fields.get(&pull.field))
        {
            Some(value) => values.push(value.clone()),
            None => missing_inputs.push(format!("{}.{}", target_id.as_str(), pull.field)),
        }
    }
    if !missing_inputs.is_empty() {
        return PullOutcome::MissingInputs(missing_inputs);
    }

    match rollup::apply_aggregate(pull.function, &values) {
        // The one widening the schema-level check allows: an integer
        // reduction (count) landing in a float field.
        Some(FieldValue::Integer(value)) if declared_type == FieldType::Float => {
            PullOutcome::Value(FieldValue::Float(value as f64))
        }
        Some(reduced) => PullOutcome::Value(reduced),
        None => PullOutcome::Skip,
    }
}

// ── Cross-link cycle diagnostics ────────────────────────────────────

/// A cross-item edge's provenance: which link field it followed, and
/// in which direction (pull follows links forward, aggregates flow
/// against them). A dependency cycle whose cross-item edges all share
/// one provenance is a cycle *in that link field* — the link cycle
/// detector's finding, not ours. Only a loop that needs two different
/// provenances to close (e.g. two pull fields over two different link
/// graphs that are only jointly cyclic) is reported here.
#[derive(PartialEq, Eq, Hash)]
enum EdgeProvenance {
    PullForward(String),
    AggregateReverse(String),
}

/// Find dependency cycles among the unevaluated nodes and report the
/// ones no single link field explains.
fn derive_cycle_diagnostics(
    derive_fields: &[DeriveField<'_>],
    item_ids: &[WorkItemId],
    item_count: usize,
    dependents: &[Vec<usize>],
    evaluated: &[bool],
    items: &HashMap<WorkItemId, WorkItem>,
) -> Vec<Diagnostic> {
    let unevaluated_nodes: Vec<usize> = (0..evaluated.len())
        .filter(|node| !evaluated[*node])
        .collect();
    if unevaluated_nodes.is_empty() {
        return Vec::new();
    }

    // Adjacency restricted to the unevaluated subgraph — everything
    // evaluated is by definition not on a cycle.
    let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
    for &node in &unevaluated_nodes {
        adjacency.insert(
            node,
            dependents[node]
                .iter()
                .copied()
                .filter(|dependent| !evaluated[*dependent])
                .collect(),
        );
    }

    // Iterative depth-first walk (chains can be as long as the item
    // count), extracting each cycle once.
    #[derive(Clone, Copy, PartialEq)]
    enum VisitState {
        InProgress,
        Done,
    }
    let mut states: HashMap<usize, VisitState> = HashMap::new();
    let mut diagnostics = Vec::new();

    for &start in &unevaluated_nodes {
        if states.contains_key(&start) {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        let mut path: Vec<usize> = vec![start];
        states.insert(start, VisitState::InProgress);

        while let Some(frame) = stack.last_mut() {
            let (node, child_index) = *frame;
            let children = &adjacency[&node];
            if child_index < children.len() {
                frame.1 += 1;
                let child = children[child_index];
                match states.get(&child) {
                    None => {
                        states.insert(child, VisitState::InProgress);
                        stack.push((child, 0));
                        path.push(child);
                    }
                    Some(VisitState::InProgress) => {
                        let cycle_start = path
                            .iter()
                            .position(|on_path| *on_path == child)
                            .expect("an in-progress node is on the current path");
                        if let Some(diagnostic) = cycle_diagnostic(
                            &path[cycle_start..],
                            derive_fields,
                            item_ids,
                            item_count,
                            items,
                        ) {
                            diagnostics.push(diagnostic);
                        }
                    }
                    Some(VisitState::Done) => {}
                }
            } else {
                states.insert(node, VisitState::Done);
                stack.pop();
                path.pop();
            }
        }
    }
    diagnostics
}

/// Render one cycle (nodes in dependency order, closing edge implied)
/// as a diagnostic — or `None` when a single link field explains it.
fn cycle_diagnostic(
    cycle: &[usize],
    derive_fields: &[DeriveField<'_>],
    item_ids: &[WorkItemId],
    item_count: usize,
    items: &HashMap<WorkItemId, WorkItem>,
) -> Option<Diagnostic> {
    let mut provenances: HashSet<EdgeProvenance> = HashSet::new();
    for (edge_index, &from_node) in cycle.iter().enumerate() {
        let to_node = cycle[(edge_index + 1) % cycle.len()];
        let from_position = from_node % item_count;
        let to_position = to_node % item_count;
        if from_position == to_position {
            continue; // same-item edge: no link field involved
        }
        let from_slot = from_node / item_count;
        let to_slot = to_node / item_count;
        if from_slot == to_slot {
            // A same-slot edge is an aggregate-reverse edge, so the slot
            // always carries an `over`; no aggregate means no such edge.
            if let Some(over) = derive_fields[to_slot].aggregate_over.clone() {
                provenances.insert(EdgeProvenance::AggregateReverse(over));
            }
        } else if let Some(pull) = &derive_fields[to_slot].definition.pull {
            provenances.insert(EdgeProvenance::PullForward(pull.over.clone()));
        }
    }
    if provenances.len() < 2 {
        return None; // one link field's cycle: the cycle detector's finding
    }

    let mut chain: Vec<String> = cycle
        .iter()
        .map(|node| {
            format!(
                "{}.{}",
                item_ids[node % item_count].as_str(),
                derive_fields[node / item_count].name
            )
        })
        .collect();
    chain.push(chain[0].clone());

    let first_item_id = &item_ids[cycle[0] % item_count];
    let source_path = items.get(first_item_id)?.source_path.clone();
    Some(Diagnostic::item(
        Severity::Error,
        source_path,
        first_item_id.clone(),
        ItemDiagnosticKind::DeriveCycle { chain },
    ))
}

/// Schema fields in evaluation order: a derivation's inputs (compute
/// expression or `when:` condition references) before the field that
/// consumes them; declaration order otherwise. Cycles are not descended
/// into (compute_check reports them), so the walk always terminates.
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
    if let Some(field_definition) = schema.fields.get(field_name) {
        // A pull's source field counts as a reference for the ordering
        // walk, so the report groups sources before their consumers.
        // It is NOT part of `derived_references` — a pull reads other
        // items, so a field-level loop through it (start ↔ end) is
        // legitimate; the walk's visited set simply stops there.
        let pull_source = field_definition
            .pull
            .as_ref()
            .map(|pull| pull.field.as_str());
        for referenced in field_definition
            .derived_references()
            .into_iter()
            .chain(pull_source)
        {
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

    /// The fixed evaluation date every derive test runs under.
    fn test_evaluation_date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 1, 8).unwrap()
    }

    fn run_derive(
        items: &mut HashMap<WorkItemId, WorkItem>,
        schema_yaml: &str,
        resources_yaml: &str,
    ) -> Vec<Diagnostic> {
        let schema = parse_schema(schema_yaml).expect("test schema must parse");
        let resources = parse_resources(resources_yaml).expect("test resources must parse");
        let reverse_links = reverse_links_of(items);
        // Mirror Store::load_with_resources: check-failed compute
        // fields are skipped and the required check follows the derive
        // passes as its own phase, exactly as in production. In-memory
        // items never went through coercion, so there are no
        // conversion failures to carry.
        let disabled_compute_fields = crate::compute_check::failed_fields(&schema, &resources);
        let conversion_failures = HashMap::new();
        let mut diagnostics = run(
            items,
            &reverse_links,
            &schema,
            &resources.constants,
            test_evaluation_date(),
            &disabled_compute_fields,
            &conversion_failures,
        );
        diagnostics.extend(super::super::required::check(
            items,
            &reverse_links,
            &schema,
            &disabled_compute_fields,
            &conversion_failures,
        ));
        diagnostics
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
      over: parent
  duration:
    type: duration
    aggregate:
      function: sum
      over: parent
  end_date:
    type: date
    compute: start_date + duration
    aggregate:
      function: max
      over: parent
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
      over: parent
  duration:
    type: duration
    aggregate:
      function: sum
      over: parent
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
    fn check_failed_compute_stays_quiet_even_with_error_on_missing() {
        // The typo'd reference is a schema-level finding (reported once
        // by compute_check); the items must not each repeat it as a
        // missing-input error.
        let schema_yaml = "\
fields:
  start_date:
    type: date
  end_date:
    type: date
    compute:
      expression: strat_date + duration
      error_on_missing: true
";
        let mut items = HashMap::from([item("task", vec![("start_date", date(2026, 1, 5))])]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "task", "end_date"), None);
    }

    #[test]
    fn required_field_with_check_failed_compute_reports_plain_missing_required() {
        // The check's schema diagnostic carries the cause; the item
        // reports its blank value without guessing at inputs of an
        // expression that never ran.
        let schema_yaml = "\
fields:
  end_date:
    type: date
    required: true
    compute: strat_date + duration
";
        let mut items = HashMap::from([item("task", vec![])]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            item_kinds(&diagnostics)[0],
            ItemDiagnosticKind::MissingRequired { field } if field == "end_date"
        ));
    }

    // ── Pull fields ───────────────────────────────────────────────────

    fn links_value(ids: &[&str]) -> FieldValue {
        FieldValue::Links(
            ids.iter()
                .map(|id| WorkItemId::from((*id).to_owned()))
                .collect(),
        )
    }

    /// The forward-scheduling schema: manual input is `depends_on` +
    /// `duration` everywhere, plus a hand-written `start` on roots.
    const FORWARD_SCHEDULING_SCHEMA: &str = "\
fields:
  depends_on:
    type: links
    allow_cycles: false
  duration:
    type: duration
  start:
    type: date
    pull:
      over: depends_on
      field: end
      function: max
  end:
    type: date
    compute: start + duration
";

    #[test]
    fn pull_chain_schedules_forward_from_the_root_anchor() {
        // a (anchored Jan 5, 7d) → b (7d) → c (7d): starts and ends
        // must cascade down the dependency chain.
        let mut items = HashMap::from([
            item(
                "a",
                vec![("start", date(2026, 1, 5)), ("duration", duration_days(7))],
            ),
            item(
                "b",
                vec![
                    ("depends_on", links_value(&["a"])),
                    ("duration", duration_days(7)),
                ],
            ),
            item(
                "c",
                vec![
                    ("depends_on", links_value(&["b"])),
                    ("duration", duration_days(7)),
                ],
            ),
        ]);
        let diagnostics = run_derive(&mut items, FORWARD_SCHEDULING_SCHEMA, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "a", "end"), Some(&date(2026, 1, 12)));
        assert_eq!(field(&items, "b", "start"), Some(&date(2026, 1, 12)));
        assert_eq!(field(&items, "b", "end"), Some(&date(2026, 1, 19)));
        assert_eq!(field(&items, "c", "start"), Some(&date(2026, 1, 19)));
        assert_eq!(field(&items, "c", "end"), Some(&date(2026, 1, 26)));
    }

    #[test]
    fn pull_takes_the_max_over_all_dependencies() {
        // c depends on both a (ends Jan 12) and b (ends Jan 26): its
        // start is the later end.
        let mut items = HashMap::from([
            item(
                "a",
                vec![("start", date(2026, 1, 5)), ("duration", duration_days(7))],
            ),
            item(
                "b",
                vec![("start", date(2026, 1, 19)), ("duration", duration_days(7))],
            ),
            item(
                "c",
                vec![
                    ("depends_on", links_value(&["a", "b"])),
                    ("duration", duration_days(7)),
                ],
            ),
        ]);
        let diagnostics = run_derive(&mut items, FORWARD_SCHEDULING_SCHEMA, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "c", "start"), Some(&date(2026, 1, 26)));
    }

    #[test]
    fn pull_manual_value_wins() {
        let mut items = HashMap::from([
            item(
                "a",
                vec![("start", date(2026, 1, 5)), ("duration", duration_days(7))],
            ),
            item(
                "b",
                vec![
                    ("depends_on", links_value(&["a"])),
                    ("duration", duration_days(7)),
                    ("start", date(2026, 3, 1)),
                ],
            ),
        ]);
        run_derive(&mut items, FORWARD_SCHEDULING_SCHEMA, "");
        assert_eq!(field(&items, "b", "start"), Some(&date(2026, 3, 1)));
    }

    #[test]
    fn pull_incomplete_dependency_is_all_or_nothing_and_silent() {
        // a has no duration → no end; b depends on a → no start, no
        // end, and no diagnostics without the opt-in flag.
        let mut items = HashMap::from([
            item("a", vec![("start", date(2026, 1, 5))]),
            item(
                "b",
                vec![
                    ("depends_on", links_value(&["a"])),
                    ("duration", duration_days(7)),
                ],
            ),
        ]);
        let diagnostics = run_derive(&mut items, FORWARD_SCHEDULING_SCHEMA, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "b", "start"), None);
        assert_eq!(field(&items, "b", "end"), None);
    }

    #[test]
    fn pull_partial_inputs_yield_nothing_even_when_one_dependency_is_complete() {
        // c depends on a (complete) and b (no end): a partial max
        // would be a silently-too-early start — must stay absent.
        let mut items = HashMap::from([
            item(
                "a",
                vec![("start", date(2026, 1, 5)), ("duration", duration_days(7))],
            ),
            item("b", vec![]),
            item(
                "c",
                vec![
                    ("depends_on", links_value(&["a", "b"])),
                    ("duration", duration_days(7)),
                ],
            ),
        ]);
        run_derive(&mut items, FORWARD_SCHEDULING_SCHEMA, "");
        assert_eq!(field(&items, "c", "start"), None);
    }

    #[test]
    fn pull_missing_input_with_error_on_missing_names_the_dependency() {
        let schema_yaml = "\
fields:
  depends_on:
    type: links
    allow_cycles: false
  end:
    type: date
  start:
    type: date
    pull:
      over: depends_on
      field: end
      function: max
      error_on_missing: true
";
        let mut items = HashMap::from([
            item("a", vec![]),
            item("b", vec![("depends_on", links_value(&["a"]))]),
        ]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Error);
        assert!(matches!(
            item_kinds(&diagnostics)[0],
            ItemDiagnosticKind::PullMissingInputs { field, missing_inputs }
                if field == "start" && missing_inputs == &vec!["a.end".to_owned()]
        ));
    }

    #[test]
    fn required_pull_field_flags_unanchored_roots_and_names_incomplete_dependencies() {
        let schema_yaml = "\
fields:
  depends_on:
    type: links
    allow_cycles: false
  duration:
    type: duration
  start:
    type: date
    required: true
    pull:
      over: depends_on
      field: end
      function: max
  end:
    type: date
    compute: start + duration
";
        // root: no manual start (unanchored). blocked: depends on
        // root, whose end never materializes.
        let mut items = HashMap::from([
            item("blocked", vec![("depends_on", links_value(&["root"]))]),
            item("root", vec![("duration", duration_days(7))]),
        ]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert_eq!(diagnostics.len(), 2);
        assert!(matches!(
            item_kinds(&diagnostics)[0],
            ItemDiagnosticKind::PullMissingInputs { field, missing_inputs }
                if field == "start" && missing_inputs == &vec!["root.end".to_owned()]
        ));
        assert!(matches!(
            item_kinds(&diagnostics)[1],
            ItemDiagnosticKind::MissingRequired { field } if field == "start"
        ));
    }

    #[test]
    fn pull_from_a_milestone_reads_its_aggregated_end() {
        // Milestone m's end is the max of its children's computed
        // ends; task t depends on m and must start when m ends —
        // a pull → rollup → compute chain across items.
        let schema_yaml = "\
fields:
  parent:
    type: link
    allow_cycles: false
  depends_on:
    type: links
    allow_cycles: false
  duration:
    type: duration
  start:
    type: date
    pull:
      over: depends_on
      field: end
      function: max
  end:
    type: date
    compute: start + duration
    aggregate:
      function: max
      over: parent
";
        let mut items = HashMap::from([
            item(
                "child-a",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("m".to_owned()))),
                    ("start", date(2026, 1, 5)),
                    ("duration", duration_days(7)),
                ],
            ),
            item(
                "child-b",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("m".to_owned()))),
                    ("start", date(2026, 1, 12)),
                    ("duration", duration_days(7)),
                ],
            ),
            item("m", vec![]),
            item(
                "t",
                vec![
                    ("depends_on", links_value(&["m"])),
                    ("duration", duration_days(7)),
                ],
            ),
        ]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "m", "end"), Some(&date(2026, 1, 19)));
        assert_eq!(field(&items, "t", "start"), Some(&date(2026, 1, 19)));
        assert_eq!(field(&items, "t", "end"), Some(&date(2026, 1, 26)));
    }

    #[test]
    fn pull_combined_with_aggregate_fills_leaves_only() {
        // start pulls on leaves; the milestone's start is min of its
        // children — not a pull over its own (absent) dependencies.
        let schema_yaml = "\
fields:
  parent:
    type: link
    allow_cycles: false
  depends_on:
    type: links
    allow_cycles: false
  duration:
    type: duration
  start:
    type: date
    pull:
      over: depends_on
      field: end
      function: max
    aggregate:
      function: min
      over: parent
  end:
    type: date
    compute: start + duration
    aggregate:
      function: max
      over: parent
";
        let mut items = HashMap::from([
            item(
                "child-a",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("m".to_owned()))),
                    ("start", date(2026, 1, 5)),
                    ("duration", duration_days(7)),
                ],
            ),
            item(
                "child-b",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("m".to_owned()))),
                    ("depends_on", links_value(&["child-a"])),
                    ("duration", duration_days(7)),
                ],
            ),
            item("m", vec![]),
        ]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        // child-b starts when child-a ends.
        assert_eq!(field(&items, "child-b", "start"), Some(&date(2026, 1, 12)));
        // Milestone: min of children's starts, max of children's ends.
        assert_eq!(field(&items, "m", "start"), Some(&date(2026, 1, 5)));
        assert_eq!(field(&items, "m", "end"), Some(&date(2026, 1, 19)));
    }

    #[test]
    fn pull_count_widens_into_a_float_field() {
        let schema_yaml = "\
fields:
  depends_on:
    type: links
    allow_cycles: false
  weight:
    type: integer
  dependency_weight_count:
    type: float
    pull:
      over: depends_on
      field: weight
      function: count
";
        let mut items = HashMap::from([
            item("a", vec![("weight", FieldValue::Integer(1))]),
            item("b", vec![("weight", FieldValue::Integer(2))]),
            item("c", vec![("depends_on", links_value(&["a", "b"]))]),
        ]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(
            field(&items, "c", "dependency_weight_count"),
            Some(&FieldValue::Float(2.0))
        );
    }

    /// Two pull fields over two different link fields: `f` pulls `g`
    /// over `first_link`, `g` pulls `f` over `second_link` — only the
    /// combination of the two link graphs can loop.
    const JOINTLY_CYCLIC_SCHEMA: &str = "\
fields:
  first_link:
    type: links
    allow_cycles: false
  second_link:
    type: links
    allow_cycles: false
  f:
    type: integer
    pull:
      over: first_link
      field: g
      function: sum
  g:
    type: integer
    pull:
      over: second_link
      field: f
      function: sum
";

    #[test]
    fn jointly_cyclic_link_fields_report_a_derive_cycle() {
        // Each link graph alone is acyclic (a → b, b → a via
        // *different* fields), so no link cycle exists — only the
        // combination loops. One DeriveCycle diagnostic; unrelated
        // items evaluate.
        let mut items = HashMap::from([
            item("a", vec![("first_link", links_value(&["b"]))]),
            item("b", vec![("second_link", links_value(&["a"]))]),
            // An unrelated, complete pair still evaluates.
            item("x", vec![("g", FieldValue::Integer(3))]),
            item("y", vec![("first_link", links_value(&["x"]))]),
        ]);
        let diagnostics = run_derive(&mut items, JOINTLY_CYCLIC_SCHEMA, "");

        let cycle_chains: Vec<&Vec<String>> = item_kinds(&diagnostics)
            .into_iter()
            .filter_map(|kind| match kind {
                ItemDiagnosticKind::DeriveCycle { chain } => Some(chain),
                _ => None,
            })
            .collect();
        assert_eq!(
            cycle_chains.len(),
            1,
            "expected exactly one derive-cycle diagnostic, got: {diagnostics:?}"
        );
        let chain = cycle_chains[0];
        assert_eq!(chain.first(), chain.last());
        assert!(chain.contains(&"a.f".to_owned()) && chain.contains(&"b.g".to_owned()));
        assert_eq!(field(&items, "y", "f"), Some(&FieldValue::Integer(3)));
        assert_eq!(field(&items, "a", "f"), None);
        assert_eq!(field(&items, "b", "g"), None);
    }

    #[test]
    fn manual_anchor_breaks_a_jointly_cyclic_loop() {
        // Same jointly-cyclic wiring, but `a.f` is hand-written: the
        // settled node waits for nothing, so the loop never forms and
        // b.g = sum(a.f) is derivable — no diagnostic, no missing value.
        let mut items = HashMap::from([
            item(
                "a",
                vec![
                    ("first_link", links_value(&["b"])),
                    ("f", FieldValue::Integer(10)),
                ],
            ),
            item("b", vec![("second_link", links_value(&["a"]))]),
        ]);
        let diagnostics = run_derive(&mut items, JOINTLY_CYCLIC_SCHEMA, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "a", "f"), Some(&FieldValue::Integer(10)));
        assert_eq!(field(&items, "b", "g"), Some(&FieldValue::Integer(10)));
    }

    #[test]
    fn milestone_with_own_depends_on_does_not_deadlock() {
        // Milestones never run their own pull/compute — their children
        // own their values — so their nodes must not wait for those
        // inputs either. Here the phantom waits ("m.start waits for
        // z.end", "m.end waits for m.start") would close a loop across
        // the two milestones that doesn't exist semantically: every
        // value derives from x's anchor, front to back (x → m → w → z).
        let schema_yaml = "\
fields:
  parent:
    type: link
    allow_cycles: false
  depends_on:
    type: links
    allow_cycles: false
  duration:
    type: duration
  start:
    type: date
    pull:
      over: depends_on
      field: end
      function: max
    aggregate:
      function: min
      over: parent
  end:
    type: date
    compute: start + duration
    aggregate:
      function: max
      over: parent
";
        let mut items = HashMap::from([
            item(
                "x",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("m".to_owned()))),
                    ("start", date(2026, 1, 5)),
                    ("duration", duration_days(7)),
                ],
            ),
            // A milestone's own depends_on is meaningful for rules but
            // never feeds its own pulled start.
            item("m", vec![("depends_on", links_value(&["z"]))]),
            item(
                "w",
                vec![
                    ("parent", FieldValue::Link(WorkItemId::from("z".to_owned()))),
                    ("depends_on", links_value(&["m"])),
                    ("duration", duration_days(7)),
                ],
            ),
            item("z", vec![]),
        ]);
        let diagnostics = run_derive(&mut items, schema_yaml, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "m", "start"), Some(&date(2026, 1, 5)));
        assert_eq!(field(&items, "m", "end"), Some(&date(2026, 1, 12)));
        assert_eq!(field(&items, "w", "start"), Some(&date(2026, 1, 12)));
        assert_eq!(field(&items, "w", "end"), Some(&date(2026, 1, 19)));
        assert_eq!(field(&items, "z", "start"), Some(&date(2026, 1, 12)));
        assert_eq!(field(&items, "z", "end"), Some(&date(2026, 1, 19)));
    }

    #[test]
    fn pull_over_a_link_cycle_stays_silent_here() {
        // a and b depend on each other through ONE link field — that
        // is the link cycle detector's finding, not a DeriveCycle.
        let mut items = HashMap::from([
            item(
                "a",
                vec![
                    ("depends_on", links_value(&["b"])),
                    ("duration", duration_days(7)),
                ],
            ),
            item(
                "b",
                vec![
                    ("depends_on", links_value(&["a"])),
                    ("duration", duration_days(7)),
                ],
            ),
        ]);
        let diagnostics = run_derive(&mut items, FORWARD_SCHEDULING_SCHEMA, "");

        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
        assert_eq!(field(&items, "a", "start"), None);
        assert_eq!(field(&items, "b", "start"), None);
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
