//! Condition evaluation: check whether a field value satisfies a condition.
//!
//! This module is designed to be reusable by the query command (issue #15).
//! The core function [`eval_condition`] takes a value and a condition and
//! returns a boolean — no store or schema context needed.

use crate::model::schema::{Condition, ConditionOperator, ConditionValue, NegationValue};
use crate::model::FieldValue;

// ── Public API ──────────────────────────────────────────────────────

/// Evaluate a condition against a field value.
///
/// Returns `true` if the value satisfies the condition.
///
/// **Null handling:** A condition on a `None` value evaluates to `false`,
/// with one exception: `{ is_set: false }` explicitly matches null/absent
/// fields.
pub fn eval_condition(value: Option<&FieldValue>, condition: &Condition) -> bool {
    match condition {
        Condition::Equals(cv) => match value {
            Some(fv) => field_value_matches(fv, cv),
            None => false,
        },
        Condition::OneOf(cvs) => match value {
            Some(fv) => cvs.iter().any(|cv| field_value_matches(fv, cv)),
            None => false,
        },
        Condition::Operator(op) => eval_operator(value, op),
    }
}

// ── Operator evaluation ─────────────────────────────────────────────

/// Evaluate a condition operator object. All specified operators must be
/// satisfied (AND logic).
fn eval_operator(value: Option<&FieldValue>, op: &ConditionOperator) -> bool {
    // is_set is checked first — it's the only operator that can match null.
    if let Some(expected) = op.is_set {
        let is_set = value.is_some();
        if is_set != expected {
            return false;
        }
        // If is_set: false and value is None, this passed. But if there are
        // other operators they need a value, and null would fail them. If
        // is_set: false is the only operator, return true here.
        if !expected && value.is_none() {
            // Only return true if no other operators are set.
            let has_others = op.not.is_some()
                || op.all.is_some()
                || op.any.is_some()
                || op.none.is_some();
            if !has_others {
                return true;
            }
            // Other operators + null = false (contradicts is_set: false + not/etc.)
            return false;
        }
    }

    // All remaining operators require a non-null value.
    // If value is None and we haven't returned from is_set above, it's false.
    if value.is_none() {
        return false;
    }
    let fv = value.unwrap();

    if let Some(ref neg) = op.not {
        if matches_negation(fv, neg) {
            return false;
        }
    }

    // Quantifiers (all/any/none) are handled at a higher level by
    // eval_condition_on_resolved. When we reach here, it means the
    // condition is being evaluated on a single value (Single resolved).
    // For single values, quantifiers don't apply — skip them.
    // (The parser validates that quantifiers only appear on one-to-many refs.)

    true
}

/// Check if a field value matches a negation (i.e., is one of the disallowed values).
fn matches_negation(fv: &FieldValue, neg: &NegationValue) -> bool {
    match neg {
        NegationValue::Single(cv) => field_value_matches(fv, cv),
        NegationValue::Multiple(cvs) => cvs.iter().any(|cv| field_value_matches(fv, cv)),
    }
}

// ── Value comparison ────────────────────────────────────────────────

/// Check if a typed field value matches a condition value.
///
/// **Membership semantics for Multichoice/List:** `ConditionValue::String("x")`
/// on a `Multichoice(["x", "y"])` or `List(["x", "y"])` returns `true` if
/// the condition value is *contained in* the list.
pub(crate) fn field_value_matches(fv: &FieldValue, cv: &ConditionValue) -> bool {
    match (fv, cv) {
        // String-like field types match string condition values
        (FieldValue::String(s), ConditionValue::String(cs)) => s == cs,
        (FieldValue::Choice(s), ConditionValue::String(cs)) => s == cs,
        (FieldValue::Date(s), ConditionValue::String(cs)) => s == cs,
        (FieldValue::Link(s), ConditionValue::String(cs)) => s == cs,

        // Multichoice/List: membership check
        (FieldValue::Multichoice(vals), ConditionValue::String(cs)) => vals.iter().any(|v| v == cs),
        (FieldValue::List(vals), ConditionValue::String(cs)) => vals.iter().any(|v| v == cs),
        (FieldValue::Links(vals), ConditionValue::String(cs)) => vals.iter().any(|v| v == cs),

        // Numeric field types match numeric condition values
        (FieldValue::Integer(i), ConditionValue::Number(n)) => *i as f64 == *n,
        (FieldValue::Float(f), ConditionValue::Number(n)) => *f == *n,

        // Boolean
        (FieldValue::Boolean(b), ConditionValue::Bool(cb)) => b == cb,

        // Type mismatches never match
        _ => false,
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Equals ──────────────────────────────────────────────────

    #[test]
    fn equals_string_matches() {
        let cond = Condition::Equals(ConditionValue::String("open".into()));
        assert!(eval_condition(
            Some(&FieldValue::String("open".into())),
            &cond
        ));
    }

    #[test]
    fn equals_string_no_match() {
        let cond = Condition::Equals(ConditionValue::String("open".into()));
        assert!(!eval_condition(
            Some(&FieldValue::String("closed".into())),
            &cond
        ));
    }

    #[test]
    fn equals_choice_matches() {
        let cond = Condition::Equals(ConditionValue::String("in_progress".into()));
        assert!(eval_condition(
            Some(&FieldValue::Choice("in_progress".into())),
            &cond
        ));
    }

    #[test]
    fn equals_date_matches() {
        let cond = Condition::Equals(ConditionValue::String("2026-01-01".into()));
        assert!(eval_condition(
            Some(&FieldValue::Date("2026-01-01".into())),
            &cond
        ));
    }

    #[test]
    fn equals_integer_matches() {
        let cond = Condition::Equals(ConditionValue::Number(42.0));
        assert!(eval_condition(Some(&FieldValue::Integer(42)), &cond));
    }

    #[test]
    fn equals_float_matches() {
        let cond = Condition::Equals(ConditionValue::Number(3.14));
        assert!(eval_condition(Some(&FieldValue::Float(3.14)), &cond));
    }

    #[test]
    fn equals_boolean_matches() {
        let cond = Condition::Equals(ConditionValue::Bool(true));
        assert!(eval_condition(Some(&FieldValue::Boolean(true)), &cond));
    }

    #[test]
    fn equals_null_is_false() {
        let cond = Condition::Equals(ConditionValue::String("open".into()));
        assert!(!eval_condition(None, &cond));
    }

    #[test]
    fn equals_type_mismatch_is_false() {
        let cond = Condition::Equals(ConditionValue::String("42".into()));
        assert!(!eval_condition(Some(&FieldValue::Integer(42)), &cond));
    }

    // ── Multichoice/List membership ──────────────────────────────

    #[test]
    fn equals_multichoice_membership() {
        let cond = Condition::Equals(ConditionValue::String("backend".into()));
        let val = FieldValue::Multichoice(vec!["backend".into(), "frontend".into()]);
        assert!(eval_condition(Some(&val), &cond));
    }

    #[test]
    fn equals_multichoice_no_membership() {
        let cond = Condition::Equals(ConditionValue::String("devops".into()));
        let val = FieldValue::Multichoice(vec!["backend".into(), "frontend".into()]);
        assert!(!eval_condition(Some(&val), &cond));
    }

    #[test]
    fn equals_list_membership() {
        let cond = Condition::Equals(ConditionValue::String("rust".into()));
        let val = FieldValue::List(vec!["rust".into(), "go".into()]);
        assert!(eval_condition(Some(&val), &cond));
    }

    // ── OneOf ───────────────────────────────────────────────────

    #[test]
    fn one_of_matches() {
        let cond = Condition::OneOf(vec![
            ConditionValue::String("open".into()),
            ConditionValue::String("in_progress".into()),
        ]);
        assert!(eval_condition(
            Some(&FieldValue::Choice("open".into())),
            &cond
        ));
    }

    #[test]
    fn one_of_no_match() {
        let cond = Condition::OneOf(vec![
            ConditionValue::String("open".into()),
            ConditionValue::String("in_progress".into()),
        ]);
        assert!(!eval_condition(
            Some(&FieldValue::Choice("done".into())),
            &cond
        ));
    }

    #[test]
    fn one_of_null_is_false() {
        let cond = Condition::OneOf(vec![ConditionValue::String("open".into())]);
        assert!(!eval_condition(None, &cond));
    }

    #[test]
    fn one_of_list_any_member() {
        let cond = Condition::OneOf(vec![
            ConditionValue::String("rust".into()),
            ConditionValue::String("python".into()),
        ]);
        let val = FieldValue::List(vec!["go".into(), "python".into()]);
        assert!(eval_condition(Some(&val), &cond));
    }

    // ── Operator: is_set ────────────────────────────────────────

    #[test]
    fn is_set_true_with_value() {
        let cond = Condition::Operator(ConditionOperator {
            is_set: Some(true),
            not: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(eval_condition(
            Some(&FieldValue::String("hello".into())),
            &cond
        ));
    }

    #[test]
    fn is_set_true_with_null() {
        let cond = Condition::Operator(ConditionOperator {
            is_set: Some(true),
            not: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(!eval_condition(None, &cond));
    }

    #[test]
    fn is_set_false_with_null() {
        let cond = Condition::Operator(ConditionOperator {
            is_set: Some(false),
            not: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(eval_condition(None, &cond));
    }

    #[test]
    fn is_set_false_with_value() {
        let cond = Condition::Operator(ConditionOperator {
            is_set: Some(false),
            not: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(!eval_condition(
            Some(&FieldValue::String("hello".into())),
            &cond
        ));
    }

    // ── Operator: not ───────────────────────────────────────────

    #[test]
    fn not_single_no_match() {
        let cond = Condition::Operator(ConditionOperator {
            not: Some(NegationValue::Single(ConditionValue::String(
                "backlog".into(),
            ))),
            is_set: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(eval_condition(
            Some(&FieldValue::Choice("open".into())),
            &cond
        ));
    }

    #[test]
    fn not_single_matches_disallowed() {
        let cond = Condition::Operator(ConditionOperator {
            not: Some(NegationValue::Single(ConditionValue::String(
                "backlog".into(),
            ))),
            is_set: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(!eval_condition(
            Some(&FieldValue::Choice("backlog".into())),
            &cond
        ));
    }

    #[test]
    fn not_null_is_false() {
        let cond = Condition::Operator(ConditionOperator {
            not: Some(NegationValue::Single(ConditionValue::String(
                "backlog".into(),
            ))),
            is_set: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(!eval_condition(None, &cond));
    }

    #[test]
    fn not_multiple() {
        let cond = Condition::Operator(ConditionOperator {
            not: Some(NegationValue::Multiple(vec![
                ConditionValue::String("backlog".into()),
                ConditionValue::String("closed".into()),
            ])),
            is_set: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(eval_condition(
            Some(&FieldValue::Choice("open".into())),
            &cond
        ));
        assert!(!eval_condition(
            Some(&FieldValue::Choice("backlog".into())),
            &cond
        ));
    }

    // ── Combined operators (AND) ────────────────────────────────

    #[test]
    fn combined_is_set_and_not() {
        let cond = Condition::Operator(ConditionOperator {
            is_set: Some(true),
            not: Some(NegationValue::Single(ConditionValue::String(
                "backlog".into(),
            ))),
            all: None,
            any: None,
            none: None,
        });
        assert!(eval_condition(
            Some(&FieldValue::Choice("open".into())),
            &cond
        ));
        assert!(!eval_condition(
            Some(&FieldValue::Choice("backlog".into())),
            &cond
        ));
        assert!(!eval_condition(None, &cond));
    }
}
