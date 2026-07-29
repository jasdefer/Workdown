//! Structured filter clauses — the shape the UI exchanges, and its
//! conversion to and from the `where:` clause-string grammar.
//!
//! The filter-editor UI never reads or writes clause syntax. It works in
//! terms of a [`Clause`]: either a guided [`Condition`] it renders as
//! field / operator / value pickers, or a [`Clause::Raw`] string it treats
//! as opaque (the escape hatch, and anything the guided builder can't
//! represent). This module owns *both* directions of the conversion —
//! [`serialize_condition`] (structured → clause string) and
//! [`decompose_clause`] (clause string → structured) — so the grammar has
//! a single home in `core`, round-trip-tested together. The wire types
//! carry the `ts_rs` derive so `gen_types` emits matching TypeScript.
//!
//! Scope mirrors the guided builder's: a clause decomposes to a
//! [`Condition`] when it is a single comparison on a *local* field, or a
//! membership test (`field in a,b` / `field not in a,b`) on one local field —
//! which the parser desugars into a flat same-field `Or` / `And` and this
//! module folds back, so a multi-select round-trips. Everything else (other
//! boolean trees, cross-field ORs, regex written as `field/…/` that isn't a
//! lone comparison, cross-relation references like `parent.status`) falls
//! back to [`Clause::Raw`] — consistent with [`crate::schema_data`] keeping
//! cross-relation filters in the raw hatch.

use serde::{Deserialize, Serialize};

use crate::query::parse::parse_where;
use crate::query::types::{Comparison, FieldReference, Operator, Predicate};

// ── Wire types ───────────────────────────────────────────────────────

/// A single guided filter condition: one local field, one operator, and its
/// operand — a scalar `value` for most operators, a `values` list for `in` /
/// `not in`, and neither for the presence checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Condition {
    pub field: String,
    pub operator: Operator,
    /// The scalar operand. Absent for the presence checks `is_set` /
    /// `is_not_set` and for the list-valued operators, which use
    /// [`Condition::values`]. `#[serde(default)]` lets a request omit it; it
    /// serializes as `null` (the codebase's convention for optional wire
    /// fields), not skipped.
    #[serde(default)]
    pub value: Option<String>,
    /// The operand list for `in` / `not in`, empty for every other operator.
    /// Kept separate from [`Condition::value`] so the comma-join never enters
    /// the data model: members travel as members, and a literal comma inside
    /// one is simply a member containing a comma.
    #[serde(default)]
    pub values: Vec<String>,
}

/// Why a [`Condition`] cannot be turned into a clause string.
///
/// The guided builder cannot construct these — it picks the operand widget
/// from the operator — so a mismatch means a malformed request rather than a
/// user-authored file problem, and the write endpoint rejects it outright
/// instead of saving with a warning.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConditionError {
    #[error("operator '{operator}' takes a list of values in 'values', not a scalar 'value'")]
    ScalarOnListOperator { operator: &'static str },

    #[error("operator '{operator}' takes a scalar 'value', not a list in 'values'")]
    ListOnScalarOperator { operator: &'static str },

    #[error("operator '{operator}' requires at least one non-empty value")]
    EmptyValueList { operator: &'static str },
}

/// One clause of a view's filter in the UI's vocabulary: a guided
/// [`Condition`], or a raw clause string the UI treats as opaque.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Clause {
    /// A guided field/operator/value condition.
    Comparison(Condition),
    /// A raw clause string — the escape hatch, or anything the guided
    /// builder can't represent. Passed through verbatim, validated by the
    /// server like a hand-written clause.
    Raw { raw: String },
}

// ── Structured → string ──────────────────────────────────────────────

/// Serialize a guided condition into a `where:` clause string in the
/// grammar [`parse_where`] accepts.
///
/// For `Matches`, `value` already carries the full `/pattern/flags` form
/// (that is how the parser stores it), so it is appended directly. `In` /
/// `NotIn` join their `values` with commas — the only place the comma-join
/// happens, and only on the way out to the clause string.
///
/// Fails when the operand does not match the operator's arity; see
/// [`ConditionError`].
pub fn serialize_condition(condition: &Condition) -> Result<String, ConditionError> {
    let field = &condition.field;
    let operator = condition.operator;

    if operator.is_list_valued() {
        if condition.value.is_some() {
            return Err(ConditionError::ScalarOnListOperator {
                operator: operator.token(),
            });
        }
        if condition.values.is_empty()
            || condition.values.iter().any(|value| value.trim().is_empty())
        {
            return Err(ConditionError::EmptyValueList {
                operator: operator.token(),
            });
        }
        let members = condition.values.join(",");
        return Ok(format!("{field} {} {members}", operator.token()));
    }

    if !condition.values.is_empty() {
        return Err(ConditionError::ListOnScalarOperator {
            operator: operator.token(),
        });
    }

    let value = condition.value.as_deref().unwrap_or("");
    Ok(match operator {
        Operator::Equal => format!("{field}={value}"),
        Operator::NotEqual => format!("{field}!={value}"),
        Operator::GreaterThan => format!("{field}>{value}"),
        Operator::LessThan => format!("{field}<{value}"),
        Operator::GreaterOrEqual => format!("{field}>={value}"),
        Operator::LessOrEqual => format!("{field}<={value}"),
        Operator::Contains => format!("{field}~{value}"),
        // `value` is the stored `/pattern/flags`; `field` + value reproduces
        // the `field/pattern/flags` source form.
        Operator::Matches => format!("{field}{value}"),
        Operator::IsSet => format!("{field}?"),
        Operator::IsNotSet => format!("!{field}?"),
        Operator::In | Operator::NotIn => unreachable!("handled above"),
    })
}

/// Serialize a list of clauses to the `where:` strings persisted in
/// `views.yaml`. Raw clauses pass through unchanged.
pub fn clauses_to_strings(clauses: &[Clause]) -> Result<Vec<String>, ConditionError> {
    clauses
        .iter()
        .map(|clause| match clause {
            Clause::Comparison(condition) => serialize_condition(condition),
            Clause::Raw { raw } => Ok(raw.clone()),
        })
        .collect()
}

// ── String → structured ──────────────────────────────────────────────

/// Turn a stored clause string into the UI's [`Clause`] shape: a guided
/// [`Clause::Comparison`] when it is a single comparison on a local field,
/// otherwise [`Clause::Raw`].
///
/// An unparseable clause also becomes [`Clause::Raw`] — the editor shows it
/// as raw text and the server's validation reports the problem, rather than
/// this conversion failing.
pub fn decompose_clause(raw: &str) -> Clause {
    match parse_where(raw) {
        Ok(predicate) => condition_from_predicate(&predicate)
            .map(Clause::Comparison)
            .unwrap_or_else(|| Clause::Raw {
                raw: raw.to_owned(),
            }),
        Err(_) => Clause::Raw {
            raw: raw.to_owned(),
        },
    }
}

/// Decompose every clause string in a list.
pub fn decompose_clauses(raws: &[String]) -> Vec<Clause> {
    raws.iter().map(|raw| decompose_clause(raw)).collect()
}

/// Recognize the two predicate shapes a guided row maps to: a bare
/// comparison, and `Not(IsSet)` (the `!field?` source form folded back to
/// the `IsNotSet` operator). Everything else returns `None` → raw.
fn condition_from_predicate(predicate: &Predicate) -> Option<Condition> {
    match predicate {
        Predicate::Comparison(comparison) => condition_from_comparison(comparison),
        Predicate::Not(inner) => match inner.as_ref() {
            Predicate::Comparison(comparison) if comparison.operator == Operator::IsSet => {
                Some(Condition {
                    field: local_field(&comparison.field)?,
                    operator: Operator::IsNotSet,
                    value: None,
                    values: Vec::new(),
                })
            }
            _ => None,
        },
        // The two shapes `query::parse` desugars a membership test into.
        Predicate::Or(branches) => condition_from_membership(branches, Operator::In),
        Predicate::And(branches) => condition_from_membership(branches, Operator::NotIn),
    }
}

/// Fold the flat, same-field predicate a membership test desugars to back into
/// one condition: an `Or` of `field = value` becomes `in`, an `And` of
/// `field != value` becomes `not in`.
///
/// Any other shape — mixed fields, the wrong comparison operator, a
/// cross-relation reference, a nested tree — returns `None` and the clause
/// stays raw. That includes a hand-written `And` that happens to mix
/// operators, which is exactly the conservative behavior the raw hatch exists
/// for.
fn condition_from_membership(branches: &[Predicate], operator: Operator) -> Option<Condition> {
    if branches.is_empty() {
        return None;
    }
    let member_operator = if operator == Operator::NotIn {
        Operator::NotEqual
    } else {
        Operator::Equal
    };

    let mut field: Option<String> = None;
    let mut values = Vec::with_capacity(branches.len());
    for branch in branches {
        let Predicate::Comparison(comparison) = branch else {
            return None;
        };
        if comparison.operator != member_operator {
            return None;
        }
        let name = local_field(&comparison.field)?;
        match &field {
            None => field = Some(name),
            Some(existing) if *existing == name => {}
            Some(_) => return None, // mixed fields → raw
        }
        values.push(comparison.value.clone());
    }
    Some(Condition {
        field: field?,
        operator,
        value: None,
        values,
    })
}

fn condition_from_comparison(comparison: &Comparison) -> Option<Condition> {
    let field = local_field(&comparison.field)?;
    let value = match comparison.operator {
        Operator::IsSet | Operator::IsNotSet => None,
        _ => Some(comparison.value.clone()),
    };
    Some(Condition {
        field,
        operator: comparison.operator,
        value,
        values: Vec::new(),
    })
}

/// Guided rows are local-field only; a cross-relation reference
/// (`parent.status`) stays in the raw escape hatch.
fn local_field(field: &FieldReference) -> Option<String> {
    match field {
        FieldReference::Local(name) => Some(name.clone()),
        FieldReference::Related { .. } => None,
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn comparison(field: &str, operator: Operator, value: Option<&str>) -> Condition {
        Condition {
            field: field.to_owned(),
            operator,
            value: value.map(str::to_owned),
            values: Vec::new(),
        }
    }

    fn membership(field: &str, operator: Operator, values: &[&str]) -> Condition {
        Condition {
            field: field.to_owned(),
            operator,
            value: None,
            values: values.iter().map(|value| (*value).to_owned()).collect(),
        }
    }

    /// Every guided operator survives a structured → string → structured
    /// round-trip unchanged.
    #[test]
    fn round_trip_every_operator() {
        let cases = [
            comparison("status", Operator::Equal, Some("open")),
            comparison("status", Operator::NotEqual, Some("done")),
            comparison("points", Operator::GreaterThan, Some("3")),
            comparison("points", Operator::LessThan, Some("10")),
            comparison("points", Operator::GreaterOrEqual, Some("3")),
            comparison("points", Operator::LessOrEqual, Some("10")),
            comparison("title", Operator::Contains, Some("login")),
            comparison("title", Operator::Matches, Some("/^fix-.*/i")),
            comparison("assignee", Operator::IsSet, None),
            comparison("assignee", Operator::IsNotSet, None),
            membership("status", Operator::In, &["open", "in_progress"]),
            membership("status", Operator::NotIn, &["done", "removed"]),
            // Arity 1 must not collapse to `=` / `!=` on the way back.
            membership("status", Operator::In, &["open"]),
            membership("status", Operator::NotIn, &["done"]),
        ];
        for condition in cases {
            let serialized = serialize_condition(&condition).unwrap();
            let decomposed = decompose_clause(&serialized);
            assert_eq!(
                decomposed,
                Clause::Comparison(condition.clone()),
                "round-trip failed for {condition:?} via '{serialized}'"
            );
        }
    }

    #[test]
    fn serialize_matches_reproduces_source_form() {
        let condition = comparison("title", Operator::Matches, Some("/^fix-.*/i"));
        assert_eq!(serialize_condition(&condition).unwrap(), "title/^fix-.*/i");
    }

    #[test]
    fn serialize_is_not_set_uses_bang_prefix() {
        let condition = comparison("assignee", Operator::IsNotSet, None);
        assert_eq!(serialize_condition(&condition).unwrap(), "!assignee?");
    }

    #[test]
    fn serialize_membership_joins_members_with_commas() {
        let condition = membership("status", Operator::In, &["open", "in_progress"]);
        assert_eq!(
            serialize_condition(&condition).unwrap(),
            "status in open,in_progress"
        );
        let condition = membership("status", Operator::NotIn, &["done", "removed"]);
        assert_eq!(
            serialize_condition(&condition).unwrap(),
            "status not in done,removed"
        );
    }

    /// A comma in an `=` value is a comma, not a hidden list.
    #[test]
    fn equal_value_containing_a_comma_round_trips_literally() {
        let condition = comparison("title", Operator::Equal, Some("bug, crash"));
        let serialized = serialize_condition(&condition).unwrap();
        assert_eq!(serialized, "title=bug, crash");
        assert_eq!(decompose_clause(&serialized), Clause::Comparison(condition));
    }

    // ── Operand / operator arity mismatches ─────────────────────────

    #[test]
    fn serialize_rejects_operand_arity_mismatches() {
        let scalar_on_list = Condition {
            field: "status".to_owned(),
            operator: Operator::In,
            value: Some("open".to_owned()),
            values: vec!["open".to_owned()],
        };
        assert_eq!(
            serialize_condition(&scalar_on_list),
            Err(ConditionError::ScalarOnListOperator { operator: "in" })
        );

        let list_on_scalar = Condition {
            field: "status".to_owned(),
            operator: Operator::Equal,
            value: Some("open".to_owned()),
            values: vec!["open".to_owned()],
        };
        assert_eq!(
            serialize_condition(&list_on_scalar),
            Err(ConditionError::ListOnScalarOperator { operator: "=" })
        );

        for values in [vec![], vec!["open".to_owned(), String::new()]] {
            let empty = Condition {
                field: "status".to_owned(),
                operator: Operator::NotIn,
                value: None,
                values,
            };
            assert_eq!(
                serialize_condition(&empty),
                Err(ConditionError::EmptyValueList { operator: "not in" })
            );
        }
    }

    // ── Decomposition: simple comparisons → guided ──────────────────

    #[test]
    fn decompose_simple_equality() {
        assert_eq!(
            decompose_clause("status=open"),
            Clause::Comparison(comparison("status", Operator::Equal, Some("open")))
        );
    }

    #[test]
    fn decompose_is_set_has_no_value() {
        assert_eq!(
            decompose_clause("assignee?"),
            Clause::Comparison(comparison("assignee", Operator::IsSet, None))
        );
    }

    #[test]
    fn decompose_is_not_set_folds_not_isset_to_operator() {
        assert_eq!(
            decompose_clause("!assignee?"),
            Clause::Comparison(comparison("assignee", Operator::IsNotSet, None))
        );
    }

    // ── Decomposition: complex → raw ────────────────────────────────

    #[test]
    fn decompose_membership_folds_to_one_condition() {
        assert_eq!(
            decompose_clause("status in open,in_progress"),
            Clause::Comparison(membership("status", Operator::In, &["open", "in_progress"]))
        );
        assert_eq!(
            decompose_clause("status not in done,removed"),
            Clause::Comparison(membership("status", Operator::NotIn, &["done", "removed"]))
        );
    }

    /// A hand-written boolean tree that isn't the shape a membership test
    /// desugars to stays raw rather than being coerced into a guided row.
    #[test]
    fn decompose_mixed_operator_tree_falls_back_to_raw() {
        // Not reachable through the grammar today (one clause carries one
        // operator), but the fold must not assume that.
        let mixed = Predicate::And(vec![
            Predicate::Comparison(Comparison {
                field: FieldReference::Local("status".to_owned()),
                operator: Operator::NotEqual,
                value: "done".to_owned(),
            }),
            Predicate::Comparison(Comparison {
                field: FieldReference::Local("status".to_owned()),
                operator: Operator::Equal,
                value: "open".to_owned(),
            }),
        ]);
        assert_eq!(condition_from_predicate(&mixed), None);
    }

    #[test]
    fn decompose_cross_relation_falls_back_to_raw() {
        // The guided builder is local-field only.
        assert_eq!(
            decompose_clause("parent.status=open"),
            Clause::Raw {
                raw: "parent.status=open".to_owned()
            }
        );
    }

    #[test]
    fn decompose_unparseable_falls_back_to_raw() {
        assert_eq!(
            decompose_clause("this is not a filter"),
            Clause::Raw {
                raw: "this is not a filter".to_owned()
            }
        );
    }

    // ── clauses_to_strings ──────────────────────────────────────────

    #[test]
    fn clauses_to_strings_mixes_guided_and_raw() {
        let clauses = vec![
            Clause::Comparison(comparison("status", Operator::Equal, Some("open"))),
            Clause::Comparison(membership("type", Operator::In, &["milestone", "epic"])),
            Clause::Raw {
                raw: "parent.status=done".to_owned(),
            },
        ];
        assert_eq!(
            clauses_to_strings(&clauses).unwrap(),
            vec![
                "status=open".to_owned(),
                "type in milestone,epic".to_owned(),
                "parent.status=done".to_owned()
            ]
        );
    }

    #[test]
    fn clauses_to_strings_propagates_a_malformed_condition() {
        let clauses = vec![Clause::Comparison(Condition {
            field: "status".to_owned(),
            operator: Operator::In,
            value: None,
            values: Vec::new(),
        })];
        assert!(clauses_to_strings(&clauses).is_err());
    }

    #[test]
    fn decompose_clauses_round_trips_a_persisted_list() {
        let raws = vec![
            "status=open".to_owned(),
            "title~fix".to_owned(),
            "type in milestone,epic".to_owned(),
            "status not in done,removed".to_owned(),
            "parent.status=done".to_owned(), // cross-relation → raw
        ];
        let clauses = decompose_clauses(&raws);
        // Serializing the decomposed list reproduces the original strings.
        assert_eq!(clauses_to_strings(&clauses).unwrap(), raws);
    }
}
