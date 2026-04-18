//! Rule engine: evaluate schema rules against loaded work items.
//!
//! The public API is [`evaluate`], called from the validate command.
//! It returns diagnostics for rule violations and count constraint failures.

pub(crate) mod assertion;
pub mod condition;

use crate::model::diagnostic::{Diagnostic, DiagnosticKind};
use crate::model::schema::{Condition, ConditionOperator, Schema};
use crate::model::{FieldValue, WorkItem};
use crate::resolve::{resolve_field_ref, ResolvedValues};
use crate::store::Store;

use self::assertion::check_assertion;
use self::condition::eval_condition;

// ── Public API ──────────────────────────────────────────────────────

/// Evaluate all schema rules against the store. Returns diagnostics for
/// violations (both per-item `RuleViolation` and collection-wide
/// `CountViolation`).
pub fn evaluate(store: &Store, schema: &Schema) -> Vec<Diagnostic> {
    let ctx = EvalContext::new(store, schema);
    let mut diagnostics = Vec::new();

    for rule in &schema.rules {
        // Phase 1: Find items matching this rule's conditions.
        let matching: Vec<&WorkItem> = store
            .all_items()
            .filter(|item| matches_all_conditions(item, &rule.match_conditions, &ctx))
            .collect();

        // Phase 2: Check require assertions on each matching item.
        for item in &matching {
            for (field_ref, assertion) in &rule.require {
                if let Some(detail) = check_assertion(item, field_ref, assertion, &ctx) {
                    let detail = match &rule.description {
                        Some(description) => format!("{description} — {detail}"),
                        None => detail,
                    };
                    diagnostics.push(Diagnostic {
                        severity: rule.severity,
                        kind: DiagnosticKind::RuleViolation {
                            item_id: item.id.clone(),
                            rule: rule.name.clone(),
                            detail,
                        },
                    });
                }
            }
        }

        // Phase 3: Check collection-wide count constraint.
        if let Some(ref count) = rule.count {
            let matching_count = matching.len();
            let violated = count.min.is_some_and(|min| matching_count < min as usize)
                || count.max.is_some_and(|max| matching_count > max as usize);
            if violated {
                diagnostics.push(Diagnostic {
                    severity: rule.severity,
                    kind: DiagnosticKind::CountViolation {
                        rule: rule.name.clone(),
                        count: matching_count,
                        min: count.min,
                        max: count.max,
                    },
                });
            }
        }
    }

    diagnostics
}

// ── Evaluation context ──────────────────────────────────────────────

/// Shared context for rule evaluation, threading store and schema
/// through all evaluation functions.
pub(crate) struct EvalContext<'a> {
    pub store: &'a Store,
    pub schema: &'a Schema,
}

impl<'a> EvalContext<'a> {
    pub fn new(store: &'a Store, schema: &'a Schema) -> Self {
        Self { store, schema }
    }
}

// ── Condition matching ──────────────────────────────────────────────

/// Check if an item matches ALL conditions in the match section (AND logic).
fn matches_all_conditions(
    item: &WorkItem,
    conditions: &indexmap::IndexMap<String, Condition>,
    ctx: &EvalContext,
) -> bool {
    conditions
        .iter()
        .all(|(field_ref, condition)| eval_condition_on_item(item, field_ref, condition, ctx))
}

/// Evaluate a single condition against an item, resolving the field reference
/// and handling quantifiers for one-to-many relationships.
fn eval_condition_on_item(
    item: &WorkItem,
    field_ref: &str,
    condition: &Condition,
    ctx: &EvalContext,
) -> bool {
    let resolved = resolve_field_ref(item, field_ref, ctx.schema, ctx.store);
    eval_condition_on_resolved(&resolved, condition)
}

/// Evaluate a condition against resolved values, handling quantifiers.
///
/// - `Single`: delegates to [`eval_condition`].
/// - `Many` with explicit quantifier: applies quantifier logic.
/// - `Many` without quantifier: defaults to `all` semantics.
///
/// Quantifier semantics on empty sets:
/// - `all` → true (vacuously true)
/// - `any` → false
/// - `none` → true
fn eval_condition_on_resolved(resolved: &ResolvedValues, condition: &Condition) -> bool {
    match resolved {
        ResolvedValues::Single(value) => eval_condition(*value, condition),
        ResolvedValues::Many(values) => {
            // Check for explicit quantifiers in the condition.
            if let Condition::Operator(operator) = condition {
                return eval_quantifiers_on_many(values, operator);
            }
            // No quantifier on a Many result: default to `all` semantics.
            values.iter().all(|value| eval_condition(*value, condition))
        }
    }
}

/// Evaluate quantifier operators (all/any/none) on a list of resolved values.
///
/// If no quantifiers are present, falls back to `all` semantics applied to
/// the non-quantifier parts of the operator.
fn eval_quantifiers_on_many(
    values: &[Option<&FieldValue>],
    operator: &ConditionOperator,
) -> bool {
    let has_quantifier =
        operator.all.is_some() || operator.any.is_some() || operator.none.is_some();

    if has_quantifier {
        if let Some(ref inner) = operator.all {
            if !values.iter().all(|value| eval_condition(*value, inner)) {
                return false;
            }
        }
        if let Some(ref inner) = operator.any {
            if !values.iter().any(|value| eval_condition(*value, inner)) {
                return false;
            }
        }
        if let Some(ref inner) = operator.none {
            if values.iter().any(|value| eval_condition(*value, inner)) {
                return false;
            }
        }

        // Also check non-quantifier operators if present (is_set, not).
        // These apply to each value individually with `all` semantics.
        let has_non_quantifier = operator.is_set.is_some() || operator.not.is_some();
        if has_non_quantifier {
            let scalar_operator = ConditionOperator {
                not: operator.not.clone(),
                is_set: operator.is_set,
                all: None,
                any: None,
                none: None,
            };
            let scalar_condition = Condition::Operator(scalar_operator);
            if !values
                .iter()
                .all(|value| eval_condition(*value, &scalar_condition))
            {
                return false;
            }
        }

        true
    } else {
        // No quantifiers — default to `all` semantics for the whole operator.
        let condition = Condition::Operator(operator.clone());
        values
            .iter()
            .all(|value| eval_condition(*value, &condition))
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{
        Assertion, ConditionValue, CountConstraint, FieldDefinition, FieldTypeConfig, NegationValue,
        Rule, Severity,
    };
    use indexmap::IndexMap;
    use std::fs;
    use std::path::PathBuf;

    fn test_schema_with_rules(rules: Vec<Rule>) -> Schema {
        let mut fields = IndexMap::new();
        fields.insert(
            "title".to_owned(),
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        );
        let mut status = FieldDefinition::new(FieldTypeConfig::Choice {
            values: vec![
                "backlog".into(),
                "open".into(),
                "in_progress".into(),
                "done".into(),
            ],
        });
        status.required = true;
        fields.insert("status".to_owned(), status);
        fields.insert(
            "type_field".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Choice {
                values: vec!["task".into(), "bug".into(), "epic".into()],
            }),
        );
        fields.insert(
            "assignee".to_owned(),
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        );
        fields.insert(
            "priority".to_owned(),
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        );
        fields.insert(
            "parent".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: Some(false),
                inverse: Some("children".into()),
            }),
        );
        fields.insert(
            "depends_on".to_owned(),
            FieldDefinition::new(FieldTypeConfig::Links {
                allow_cycles: Some(false),
                inverse: Some("dependents".into()),
            }),
        );
        let inverse_table = Schema::build_inverse_table(&fields);
        Schema { fields, rules, inverse_table }
    }

    fn setup_items(items: Vec<(&str, &str)>) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        for (name, content) in items {
            fs::write(path.join(name), content).unwrap();
        }
        (dir, path)
    }

    // ── inverse_table ──────────────────────────────────────────

    #[test]
    fn inverse_table_built_from_schema() {
        let schema = test_schema_with_rules(vec![]);
        assert_eq!(schema.inverse_table.get("children"), Some(&"parent".to_owned()));
        assert_eq!(schema.inverse_table.get("dependents"), Some(&"depends_on".to_owned()));
        assert_eq!(schema.inverse_table.get("nonexistent"), None);
    }

    // ── L2: Cross-field rules ───────────────────────────────────

    #[test]
    fn l2_rule_violation() {
        let rule = Rule {
            name: "in-progress-needs-assignee".into(),
            description: Some("Must have assignee when in progress".into()),
            severity: Severity::Error,
            match_conditions: {
                let mut conditions = IndexMap::new();
                conditions.insert(
                    "status".into(),
                    Condition::Equals(ConditionValue::String("in_progress".into())),
                );
                conditions
            },
            require: {
                let mut assertions = IndexMap::new();
                assertions.insert("assignee".into(), Assertion::Required);
                assertions
            },
            count: None,
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![(
            "task-a.md",
            "---\nstatus: in_progress\n---\n",
        )]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        let rule_violations: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| matches!(&diagnostic.kind, DiagnosticKind::RuleViolation { .. }))
            .collect();
        assert_eq!(rule_violations.len(), 1);
        assert_eq!(rule_violations[0].severity, Severity::Error);
    }

    #[test]
    fn l2_rule_passes_when_condition_not_met() {
        let rule = Rule {
            name: "in-progress-needs-assignee".into(),
            description: None,
            severity: Severity::Error,
            match_conditions: {
                let mut conditions = IndexMap::new();
                conditions.insert(
                    "status".into(),
                    Condition::Equals(ConditionValue::String("in_progress".into())),
                );
                conditions
            },
            require: {
                let mut assertions = IndexMap::new();
                assertions.insert("assignee".into(), Assertion::Required);
                assertions
            },
            count: None,
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![(
            "task-a.md",
            "---\nstatus: open\n---\n",
        )]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        assert!(diagnostics
            .iter()
            .all(|diagnostic| !matches!(&diagnostic.kind, DiagnosticKind::RuleViolation { .. })));
    }

    #[test]
    fn l2_rule_passes_when_assertion_satisfied() {
        let rule = Rule {
            name: "in-progress-needs-assignee".into(),
            description: None,
            severity: Severity::Error,
            match_conditions: {
                let mut conditions = IndexMap::new();
                conditions.insert(
                    "status".into(),
                    Condition::Equals(ConditionValue::String("in_progress".into())),
                );
                conditions
            },
            require: {
                let mut assertions = IndexMap::new();
                assertions.insert("assignee".into(), Assertion::Required);
                assertions
            },
            count: None,
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![(
            "task-a.md",
            "---\nstatus: in_progress\nassignee: alice\n---\n",
        )]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        assert!(diagnostics
            .iter()
            .all(|diagnostic| !matches!(&diagnostic.kind, DiagnosticKind::RuleViolation { .. })));
    }

    #[test]
    fn rule_no_match_applies_to_all() {
        let rule = Rule {
            name: "all-need-title".into(),
            description: None,
            severity: Severity::Warning,
            match_conditions: IndexMap::new(),
            require: {
                let mut assertions = IndexMap::new();
                assertions.insert("title".into(), Assertion::Required);
                assertions
            },
            count: None,
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![
            ("task-a.md", "---\ntitle: A\nstatus: open\n---\n"),
            ("task-b.md", "---\nstatus: open\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        let violations: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| matches!(&diagnostic.kind, DiagnosticKind::RuleViolation { .. }))
            .collect();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].severity, Severity::Warning);
    }

    // ── L4: Count constraints ───────────────────────────────────

    #[test]
    fn l4_count_violation() {
        let rule = Rule {
            name: "wip-limit".into(),
            description: None,
            severity: Severity::Error,
            match_conditions: {
                let mut conditions = IndexMap::new();
                conditions.insert(
                    "status".into(),
                    Condition::Equals(ConditionValue::String("in_progress".into())),
                );
                conditions
            },
            require: IndexMap::new(),
            count: Some(CountConstraint {
                min: None,
                max: Some(1),
            }),
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![
            ("task-a.md", "---\nstatus: in_progress\n---\n"),
            ("task-b.md", "---\nstatus: in_progress\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        assert!(diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.kind,
            DiagnosticKind::CountViolation { rule, count, max, .. }
            if rule == "wip-limit" && *count == 2 && *max == Some(1)
        )));
    }

    #[test]
    fn l4_count_passes() {
        let rule = Rule {
            name: "wip-limit".into(),
            description: None,
            severity: Severity::Error,
            match_conditions: {
                let mut conditions = IndexMap::new();
                conditions.insert(
                    "status".into(),
                    Condition::Equals(ConditionValue::String("in_progress".into())),
                );
                conditions
            },
            require: IndexMap::new(),
            count: Some(CountConstraint {
                min: None,
                max: Some(5),
            }),
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![(
            "task-a.md",
            "---\nstatus: in_progress\n---\n",
        )]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        assert!(!diagnostics
            .iter()
            .any(|diagnostic| matches!(&diagnostic.kind, DiagnosticKind::CountViolation { .. })));
    }

    // ── L3: Relationship-based ──────────────────────────────────

    #[test]
    fn l3_forward_link_rule() {
        let rule = Rule {
            name: "parent-not-backlog".into(),
            description: None,
            severity: Severity::Error,
            match_conditions: {
                let mut conditions = IndexMap::new();
                conditions.insert(
                    "status".into(),
                    Condition::Equals(ConditionValue::String("in_progress".into())),
                );
                conditions
            },
            require: {
                let mut assertions = IndexMap::new();
                assertions.insert(
                    "parent.status".into(),
                    Assertion::Operator(crate::model::schema::AssertionOperator {
                        required: None,
                        forbidden: None,
                        values: None,
                        not: Some(NegationValue::Single(ConditionValue::String(
                            "backlog".into(),
                        ))),
                        eq_field: None,
                        lt_field: None,
                        lte_field: None,
                        gt_field: None,
                        gte_field: None,
                        min_count: None,
                        max_count: None,
                    }),
                );
                assertions
            },
            count: None,
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![
            ("epic.md", "---\nstatus: backlog\n---\n"),
            ("task-a.md", "---\nstatus: in_progress\nparent: epic\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        assert!(diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.kind,
            DiagnosticKind::RuleViolation { rule, .. } if rule == "parent-not-backlog"
        )));
    }

    #[test]
    fn l3_inverse_quantifier_all() {
        // Rule: match items where all children have status "done",
        // require that the item itself has status "done".
        let rule = Rule {
            name: "close-parent".into(),
            description: None,
            severity: Severity::Warning,
            match_conditions: {
                let mut conditions = IndexMap::new();
                conditions.insert(
                    "children.status".into(),
                    Condition::Operator(ConditionOperator {
                        all: Some(Box::new(Condition::Equals(ConditionValue::String(
                            "done".into(),
                        )))),
                        any: None,
                        none: None,
                        not: None,
                        is_set: None,
                    }),
                );
                conditions
            },
            require: {
                let mut assertions = IndexMap::new();
                assertions.insert(
                    "status".into(),
                    Assertion::Operator(crate::model::schema::AssertionOperator {
                        required: None,
                        forbidden: None,
                        values: Some(vec![ConditionValue::String("done".into())]),
                        not: None,
                        eq_field: None,
                        lt_field: None,
                        lte_field: None,
                        gt_field: None,
                        gte_field: None,
                        min_count: None,
                        max_count: None,
                    }),
                );
                assertions
            },
            count: None,
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![
            ("epic.md", "---\nstatus: open\n---\n"),
            ("child-a.md", "---\nstatus: done\nparent: epic\n---\n"),
            ("child-b.md", "---\nstatus: done\nparent: epic\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        // Epic matches (all children done) but its status is "open" not "done"
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.kind,
            DiagnosticKind::RuleViolation { rule, item_id, .. }
            if rule == "close-parent" && item_id == "epic"
        )));
    }

    #[test]
    fn quantifier_all_vacuously_true_for_no_children() {
        // Items with no children: "all children done" is vacuously true.
        let rule = Rule {
            name: "close-parent".into(),
            description: None,
            severity: Severity::Warning,
            match_conditions: {
                let mut conditions = IndexMap::new();
                conditions.insert(
                    "children.status".into(),
                    Condition::Operator(ConditionOperator {
                        all: Some(Box::new(Condition::Equals(ConditionValue::String(
                            "done".into(),
                        )))),
                        any: None,
                        none: None,
                        not: None,
                        is_set: None,
                    }),
                );
                conditions
            },
            require: {
                let mut assertions = IndexMap::new();
                assertions.insert(
                    "status".into(),
                    Assertion::Operator(crate::model::schema::AssertionOperator {
                        required: None,
                        forbidden: None,
                        values: Some(vec![ConditionValue::String("done".into())]),
                        not: None,
                        eq_field: None,
                        lt_field: None,
                        lte_field: None,
                        gt_field: None,
                        gte_field: None,
                        min_count: None,
                        max_count: None,
                    }),
                );
                assertions
            },
            count: None,
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![
            // This item has no children — "all children done" is vacuously true
            ("leaf.md", "---\nstatus: open\n---\n"),
        ]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        // The leaf matches the rule (vacuously) so the require should fire
        assert!(diagnostics.iter().any(|diagnostic| matches!(
            &diagnostic.kind,
            DiagnosticKind::RuleViolation { item_id, .. } if item_id == "leaf"
        )));
    }

    // ── Severity propagation ────────────────────────────────────

    #[test]
    fn warning_severity_propagated() {
        let rule = Rule {
            name: "soft-check".into(),
            description: None,
            severity: Severity::Warning,
            match_conditions: IndexMap::new(),
            require: {
                let mut assertions = IndexMap::new();
                assertions.insert("assignee".into(), Assertion::Required);
                assertions
            },
            count: None,
        };

        let schema = test_schema_with_rules(vec![rule]);
        let (_dir, path) = setup_items(vec![(
            "task-a.md",
            "---\nstatus: open\n---\n",
        )]);
        let store = Store::load(&path, &schema).unwrap();
        let diagnostics = evaluate(&store, &schema);

        let violations: Vec<_> = diagnostics
            .iter()
            .filter(|diagnostic| matches!(&diagnostic.kind, DiagnosticKind::RuleViolation { .. }))
            .collect();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].severity, Severity::Warning);
    }
}
