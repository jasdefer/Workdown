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
//! [`Condition`] when it is a single comparison on a *local* field, or an
//! IN filter (`field=a,b`) on one local field — which folds back to a
//! multi-value `Equal` condition so a multi-select round-trips. Everything
//! else (other boolean trees, cross-field ORs, regex written as `field/…/`
//! that isn't a lone comparison, cross-relation references like
//! `parent.status`) falls back to [`Clause::Raw`] — consistent with
//! [`crate::schema_data`] keeping cross-relation filters in the raw hatch.

use serde::{Deserialize, Serialize};

use crate::query::parse::parse_where;
use crate::query::types::{Comparison, FieldReference, Operator, Predicate};

// ── Wire types ───────────────────────────────────────────────────────

/// A single guided filter condition: one local field, one operator, and an
/// optional value (absent for the presence checks `is_set` / `is_not_set`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
pub struct Condition {
    pub field: String,
    pub operator: Operator,
    /// Absent for the presence checks `is_set` / `is_not_set`. `#[serde(default)]`
    /// lets a request omit it; it serializes as `null` (the codebase's
    /// convention for optional wire fields), not skipped.
    #[serde(default)]
    pub value: Option<String>,
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
/// (that is how the parser stores it), so it is appended directly.
/// `Equal` assumes a comma-free value — a comma would re-parse as IN; the
/// guided builder never produces a multi-value `Equal` (those are raw).
pub fn serialize_condition(condition: &Condition) -> String {
    let field = &condition.field;
    let value = condition.value.as_deref().unwrap_or("");
    match condition.operator {
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
    }
}

/// Serialize a list of clauses to the `where:` strings persisted in
/// `views.yaml`. Raw clauses pass through unchanged.
pub fn clauses_to_strings(clauses: &[Clause]) -> Vec<String> {
    clauses
        .iter()
        .map(|clause| match clause {
            Clause::Comparison(condition) => serialize_condition(condition),
            Clause::Raw { raw } => raw.clone(),
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
            .unwrap_or_else(|| Clause::Raw { raw: raw.to_owned() }),
        Err(_) => Clause::Raw { raw: raw.to_owned() },
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
                })
            }
            _ => None,
        },
        // `field=a,b` parses to an Or of same-field equals; fold it back to
        // one multi-value `Equal` condition so a multi-select round-trips.
        Predicate::Or(branches) => condition_from_or(branches),
        Predicate::And(_) => None,
    }
}

/// Fold an `Or` whose branches are all `field = value` on the *same* local
/// field into a single `Equal` condition whose value is the comma-joined
/// list. Any other `Or` shape (mixed fields, non-equal operators,
/// cross-relation) returns `None` → raw.
fn condition_from_or(branches: &[Predicate]) -> Option<Condition> {
    if branches.is_empty() {
        return None;
    }
    let mut field: Option<String> = None;
    let mut values = Vec::with_capacity(branches.len());
    for branch in branches {
        let Predicate::Comparison(comparison) = branch else {
            return None;
        };
        if comparison.operator != Operator::Equal {
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
        operator: Operator::Equal,
        value: Some(values.join(",")),
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
        ];
        for condition in cases {
            let serialized = serialize_condition(&condition);
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
        assert_eq!(serialize_condition(&condition), "title/^fix-.*/i");
    }

    #[test]
    fn serialize_is_not_set_uses_bang_prefix() {
        let condition = comparison("assignee", Operator::IsNotSet, None);
        assert_eq!(serialize_condition(&condition), "!assignee?");
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
    fn decompose_in_syntax_folds_to_multi_value_condition() {
        // `status=open,in_progress` (an Or of same-field equals) folds back
        // into one multi-value `Equal` condition for the multi-select.
        assert_eq!(
            decompose_clause("status=open,in_progress"),
            Clause::Comparison(comparison(
                "status",
                Operator::Equal,
                Some("open,in_progress")
            ))
        );
    }

    #[test]
    fn in_syntax_round_trips() {
        let condition = comparison("status", Operator::Equal, Some("open,in_progress,done"));
        let serialized = serialize_condition(&condition);
        assert_eq!(serialized, "status=open,in_progress,done");
        assert_eq!(decompose_clause(&serialized), Clause::Comparison(condition));
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
            Clause::Raw {
                raw: "status=open,in_progress".to_owned(),
            },
        ];
        assert_eq!(
            clauses_to_strings(&clauses),
            vec!["status=open".to_owned(), "status=open,in_progress".to_owned()]
        );
    }

    #[test]
    fn decompose_clauses_round_trips_a_persisted_list() {
        let raws = vec![
            "status=open".to_owned(),
            "title~fix".to_owned(),
            "parent.status=done".to_owned(), // cross-relation → raw
        ];
        let clauses = decompose_clauses(&raws);
        // Serializing the decomposed list reproduces the original strings.
        assert_eq!(clauses_to_strings(&clauses), raws);
    }
}
