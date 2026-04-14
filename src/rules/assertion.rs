//! Assertion evaluation: check whether a field value satisfies an assertion.
//!
//! The core function [`check_assertion`] returns `None` when the assertion
//! passes, or `Some(detail)` with a human-readable violation message.

use std::cmp::Ordering;

use crate::model::schema::{Assertion, AssertionOperator, ConditionValue, NegationValue};
use crate::model::{FieldValue, WorkItem};

use super::condition::field_value_matches;
use super::resolve::{resolve_field_ref, resolve_related_items, ResolvedValues};
use super::EvalContext;

// ── Public API ──────────────────────────────────────────────────────

/// Check whether an assertion holds for a given field reference on an item.
///
/// Returns `None` if the assertion passes, or `Some(detail)` with a
/// human-readable message explaining the violation.
pub(crate) fn check_assertion(
    item: &WorkItem,
    field_ref: &str,
    assertion: &Assertion,
    ctx: &EvalContext,
) -> Option<String> {
    match assertion {
        Assertion::Required => check_required(item, field_ref, ctx),
        Assertion::Forbidden => check_forbidden(item, field_ref, ctx),
        Assertion::Operator(op) => check_operator(item, field_ref, op, ctx),
    }
}

// ── Simple assertions ───────────────────────────────────────────────

fn check_required(item: &WorkItem, field_ref: &str, ctx: &EvalContext) -> Option<String> {
    let resolved = resolve_field_ref(item, field_ref, ctx);
    match resolved {
        ResolvedValues::Single(None) => Some(format!("'{field_ref}' is required")),
        ResolvedValues::Single(Some(_)) => None,
        ResolvedValues::Many(values) => {
            if values.is_empty() {
                Some(format!("'{field_ref}' is required"))
            } else {
                None
            }
        }
    }
}

fn check_forbidden(item: &WorkItem, field_ref: &str, ctx: &EvalContext) -> Option<String> {
    let resolved = resolve_field_ref(item, field_ref, ctx);
    match resolved {
        ResolvedValues::Single(None) => None,
        ResolvedValues::Single(Some(_)) => Some(format!("'{field_ref}' is forbidden")),
        ResolvedValues::Many(values) => {
            if values.is_empty() {
                None
            } else {
                Some(format!("'{field_ref}' is forbidden"))
            }
        }
    }
}

// ── Operator assertions ─────────────────────────────────────────────

fn check_operator(
    item: &WorkItem,
    field_ref: &str,
    op: &AssertionOperator,
    ctx: &EvalContext,
) -> Option<String> {
    // required/forbidden operators
    if op.required == Some(true) {
        if let Some(detail) = check_required(item, field_ref, ctx) {
            return Some(detail);
        }
    }
    if op.forbidden == Some(true) {
        if let Some(detail) = check_forbidden(item, field_ref, ctx) {
            return Some(detail);
        }
    }

    // min_count / max_count — bare relationship count
    if op.min_count.is_some() || op.max_count.is_some() {
        let related = resolve_related_items(item, field_ref, ctx);
        let count = related.len();

        if let Some(min) = op.min_count {
            if count < min as usize {
                return Some(format!(
                    "'{field_ref}' count {count} is below minimum {min}"
                ));
            }
        }
        if let Some(max) = op.max_count {
            if count > max as usize {
                return Some(format!(
                    "'{field_ref}' count {count} exceeds maximum {max}"
                ));
            }
        }
    }

    let resolved = resolve_field_ref(item, field_ref, ctx);

    // Collect values to check (both Single and Many)
    let values_to_check: Vec<Option<&FieldValue>> = match &resolved {
        ResolvedValues::Single(v) => vec![*v],
        ResolvedValues::Many(vs) => vs.clone(),
    };

    // values assertion — all values must be in the allowed set
    if let Some(ref allowed) = op.values {
        for fv in values_to_check.iter().flatten() {
            if !allowed.iter().any(|cv| field_value_matches(fv, cv)) {
                return Some(format!(
                    "'{field_ref}' value '{}' is not one of {:?}",
                    format_field_value(fv),
                    format_condition_values(allowed),
                ));
            }
        }
    }

    // not assertion — no value may match the negation
    if let Some(ref neg) = op.not {
        for fv in values_to_check.iter().flatten() {
            let matches = match neg {
                NegationValue::Single(cv) => field_value_matches(fv, cv),
                NegationValue::Multiple(cvs) => {
                    cvs.iter().any(|cv| field_value_matches(fv, cv))
                }
            };
            if matches {
                return Some(format!(
                    "'{field_ref}' value '{}' is not allowed",
                    format_field_value(fv),
                ));
            }
        }
    }

    // Field-to-field comparisons — skip when either operand is null.
    if let Some(ref other_ref) = op.eq_field {
        if let Some(detail) = check_field_comparison(item, field_ref, other_ref, Ordering::Equal, "==", ctx) {
            return Some(detail);
        }
    }
    if let Some(ref other_ref) = op.lt_field {
        if let Some(detail) = check_field_ordering(item, field_ref, other_ref, |ord| ord == Ordering::Less, "<", ctx) {
            return Some(detail);
        }
    }
    if let Some(ref other_ref) = op.lte_field {
        if let Some(detail) = check_field_ordering(item, field_ref, other_ref, |ord| ord != Ordering::Greater, "<=", ctx) {
            return Some(detail);
        }
    }
    if let Some(ref other_ref) = op.gt_field {
        if let Some(detail) = check_field_ordering(item, field_ref, other_ref, |ord| ord == Ordering::Greater, ">", ctx) {
            return Some(detail);
        }
    }
    if let Some(ref other_ref) = op.gte_field {
        if let Some(detail) = check_field_ordering(item, field_ref, other_ref, |ord| ord != Ordering::Less, ">=", ctx) {
            return Some(detail);
        }
    }

    None
}

// ── Field-to-field comparison ───────────────────────────────────────

/// Compare two field values for equality. Returns a violation detail if
/// they are not equal, or `None` if they are equal or either is null.
fn check_field_comparison(
    item: &WorkItem,
    field_ref: &str,
    other_ref: &str,
    _expected: Ordering,
    op_str: &str,
    ctx: &EvalContext,
) -> Option<String> {
    let a = resolve_single_value(item, field_ref, ctx)?;
    let b = resolve_single_value(item, other_ref, ctx)?;

    match compare_field_values(a, b) {
        Some(Ordering::Equal) => None,
        Some(_) => Some(format!("'{field_ref}' must {op_str} '{other_ref}'")),
        None => None, // incompatible types — skip
    }
}

/// Compare two field values with an ordering predicate. Returns a violation
/// detail if the predicate fails, or `None` if it passes or either is null.
fn check_field_ordering(
    item: &WorkItem,
    field_ref: &str,
    other_ref: &str,
    predicate: impl Fn(Ordering) -> bool,
    op_str: &str,
    ctx: &EvalContext,
) -> Option<String> {
    let a = resolve_single_value(item, field_ref, ctx)?;
    let b = resolve_single_value(item, other_ref, ctx)?;

    match compare_field_values(a, b) {
        Some(ord) if predicate(ord) => None,
        Some(_) => Some(format!("'{field_ref}' must be {op_str} '{other_ref}'")),
        None => None, // incompatible types — skip
    }
}

/// Resolve a field reference to a single value. Returns `None` if the
/// field is absent (skipping the comparison per null-handling rules).
fn resolve_single_value<'a>(
    item: &'a WorkItem,
    field_ref: &str,
    ctx: &'a EvalContext<'a>,
) -> Option<&'a FieldValue> {
    match resolve_field_ref(item, field_ref, ctx) {
        ResolvedValues::Single(v) => v,
        ResolvedValues::Many(vs) => vs.into_iter().next().flatten(),
    }
}

/// Compare two field values for ordering.
///
/// Supports Integer, Float, Date, String, Boolean.
/// Does NOT support Choice, Multichoice, List, Link, Links (returns None).
/// Cross-type Integer vs Float is promoted to f64.
pub(crate) fn compare_field_values(a: &FieldValue, b: &FieldValue) -> Option<Ordering> {
    match (a, b) {
        (FieldValue::Integer(a), FieldValue::Integer(b)) => Some(a.cmp(b)),
        (FieldValue::Float(a), FieldValue::Float(b)) => a.partial_cmp(b),
        (FieldValue::Integer(a), FieldValue::Float(b)) => (*a as f64).partial_cmp(b),
        (FieldValue::Float(a), FieldValue::Integer(b)) => a.partial_cmp(&(*b as f64)),
        (FieldValue::Date(a), FieldValue::Date(b)) => Some(a.cmp(b)),
        (FieldValue::String(a), FieldValue::String(b)) => Some(a.cmp(b)),
        (FieldValue::Boolean(a), FieldValue::Boolean(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

// ── Formatting helpers ──────────────────────────────────────────────

fn format_field_value(fv: &FieldValue) -> String {
    match fv {
        FieldValue::String(s) | FieldValue::Choice(s) | FieldValue::Date(s) => s.clone(),
        FieldValue::Link(id) => id.to_string(),
        FieldValue::Integer(i) => i.to_string(),
        FieldValue::Float(f) => f.to_string(),
        FieldValue::Boolean(b) => b.to_string(),
        FieldValue::Multichoice(v) | FieldValue::List(v) => {
            format!("[{}]", v.join(", "))
        }
        FieldValue::Links(ids) => {
            let strs: Vec<&str> = ids.iter().map(|id| id.as_str()).collect();
            format!("[{}]", strs.join(", "))
        }
    }
}

fn format_condition_values(cvs: &[ConditionValue]) -> Vec<String> {
    cvs.iter()
        .map(|cv| match cv {
            ConditionValue::String(s) => s.clone(),
            ConditionValue::Number(n) => n.to_string(),
            ConditionValue::Bool(b) => b.to_string(),
        })
        .collect()
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── compare_field_values ────────────────────────────────────

    #[test]
    fn compare_integers() {
        assert_eq!(
            compare_field_values(&FieldValue::Integer(1), &FieldValue::Integer(2)),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_field_values(&FieldValue::Integer(5), &FieldValue::Integer(5)),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn compare_floats() {
        assert_eq!(
            compare_field_values(&FieldValue::Float(1.0), &FieldValue::Float(2.0)),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn compare_int_float_cross() {
        assert_eq!(
            compare_field_values(&FieldValue::Integer(1), &FieldValue::Float(1.0)),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_field_values(&FieldValue::Float(2.5), &FieldValue::Integer(3)),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn compare_dates() {
        assert_eq!(
            compare_field_values(
                &FieldValue::Date("2026-01-01".into()),
                &FieldValue::Date("2026-06-15".into())
            ),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn compare_strings() {
        assert_eq!(
            compare_field_values(
                &FieldValue::String("alpha".into()),
                &FieldValue::String("beta".into())
            ),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn compare_booleans() {
        assert_eq!(
            compare_field_values(&FieldValue::Boolean(false), &FieldValue::Boolean(true)),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn compare_choice_returns_none() {
        assert_eq!(
            compare_field_values(
                &FieldValue::Choice("a".into()),
                &FieldValue::Choice("b".into())
            ),
            None
        );
    }

    #[test]
    fn compare_incompatible_types_returns_none() {
        assert_eq!(
            compare_field_values(&FieldValue::Integer(1), &FieldValue::String("1".into())),
            None
        );
    }
}
