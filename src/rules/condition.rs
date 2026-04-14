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
        Condition::Equals(condition_value) => match value {
            Some(field_value) => field_value_matches(field_value, condition_value),
            None => false,
        },
        Condition::OneOf(condition_values) => match value {
            Some(field_value) => condition_values
                .iter()
                .any(|condition_value| field_value_matches(field_value, condition_value)),
            None => false,
        },
        Condition::Operator(operator) => eval_operator(value, operator),
    }
}

// ── Operator evaluation ─────────────────────────────────────────────

/// Evaluate a condition operator object. All specified operators must be
/// satisfied (AND logic).
fn eval_operator(value: Option<&FieldValue>, operator: &ConditionOperator) -> bool {
    // is_set is checked first — it's the only operator that can match null.
    if let Some(expected) = operator.is_set {
        let is_set = value.is_some();
        if is_set != expected {
            return false;
        }
        // If is_set: false and value is None, this passed. But if there are
        // other operators they need a value, and null would fail them. If
        // is_set: false is the only operator, return true here.
        if !expected && value.is_none() {
            // Only return true if no other operators are set.
            let has_others = operator.not.is_some()
                || operator.all.is_some()
                || operator.any.is_some()
                || operator.none.is_some();
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
    let field_value = value.unwrap();

    if let Some(ref negation) = operator.not {
        if matches_negation(field_value, negation) {
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
fn matches_negation(field_value: &FieldValue, negation: &NegationValue) -> bool {
    match negation {
        NegationValue::Single(condition_value) => {
            field_value_matches(field_value, condition_value)
        }
        NegationValue::Multiple(condition_values) => condition_values
            .iter()
            .any(|condition_value| field_value_matches(field_value, condition_value)),
    }
}

// ── Value comparison ────────────────────────────────────────────────

/// Check if a typed field value matches a condition value.
///
/// **Membership semantics for Multichoice/List:** `ConditionValue::String("x")`
/// on a `Multichoice(["x", "y"])` or `List(["x", "y"])` returns `true` if
/// the condition value is *contained in* the list.
pub(crate) fn field_value_matches(
    field_value: &FieldValue,
    condition_value: &ConditionValue,
) -> bool {
    match (field_value, condition_value) {
        // String-like field types match string condition values
        (FieldValue::String(value), ConditionValue::String(expected)) => value == expected,
        (FieldValue::Choice(value), ConditionValue::String(expected)) => value == expected,
        (FieldValue::Date(value), ConditionValue::String(expected)) => value == expected,
        (FieldValue::Link(value), ConditionValue::String(expected)) => {
            value.as_str() == expected
        }

        // Multichoice/List: membership check
        (FieldValue::Multichoice(values), ConditionValue::String(expected)) => {
            values.iter().any(|value| value == expected)
        }
        (FieldValue::List(values), ConditionValue::String(expected)) => {
            values.iter().any(|value| value == expected)
        }
        (FieldValue::Links(values), ConditionValue::String(expected)) => {
            values.iter().any(|value| value.as_str() == expected.as_str())
        }

        // Numeric field types match numeric condition values
        (FieldValue::Integer(value), ConditionValue::Number(expected)) => {
            *value as f64 == *expected
        }
        (FieldValue::Float(value), ConditionValue::Number(expected)) => value == expected,

        // Boolean
        (FieldValue::Boolean(value), ConditionValue::Bool(expected)) => value == expected,

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
        let condition =Condition::Equals(ConditionValue::String("open".into()));
        assert!(eval_condition(
            Some(&FieldValue::String("open".into())),
            &condition
        ));
    }

    #[test]
    fn equals_string_no_match() {
        let condition =Condition::Equals(ConditionValue::String("open".into()));
        assert!(!eval_condition(
            Some(&FieldValue::String("closed".into())),
            &condition
        ));
    }

    #[test]
    fn equals_choice_matches() {
        let condition =Condition::Equals(ConditionValue::String("in_progress".into()));
        assert!(eval_condition(
            Some(&FieldValue::Choice("in_progress".into())),
            &condition
        ));
    }

    #[test]
    fn equals_date_matches() {
        let condition =Condition::Equals(ConditionValue::String("2026-01-01".into()));
        assert!(eval_condition(
            Some(&FieldValue::Date("2026-01-01".into())),
            &condition
        ));
    }

    #[test]
    fn equals_integer_matches() {
        let condition =Condition::Equals(ConditionValue::Number(42.0));
        assert!(eval_condition(Some(&FieldValue::Integer(42)), &condition));
    }

    #[test]
    fn equals_float_matches() {
        let condition =Condition::Equals(ConditionValue::Number(3.14));
        assert!(eval_condition(Some(&FieldValue::Float(3.14)), &condition));
    }

    #[test]
    fn equals_boolean_matches() {
        let condition =Condition::Equals(ConditionValue::Bool(true));
        assert!(eval_condition(Some(&FieldValue::Boolean(true)), &condition));
    }

    #[test]
    fn equals_null_is_false() {
        let condition =Condition::Equals(ConditionValue::String("open".into()));
        assert!(!eval_condition(None, &condition));
    }

    #[test]
    fn equals_type_mismatch_is_false() {
        let condition =Condition::Equals(ConditionValue::String("42".into()));
        assert!(!eval_condition(Some(&FieldValue::Integer(42)), &condition));
    }

    // ── Multichoice/List membership ──────────────────────────────

    #[test]
    fn equals_multichoice_membership() {
        let condition =Condition::Equals(ConditionValue::String("backend".into()));
        let value =FieldValue::Multichoice(vec!["backend".into(), "frontend".into()]);
        assert!(eval_condition(Some(&value), &condition));
    }

    #[test]
    fn equals_multichoice_no_membership() {
        let condition =Condition::Equals(ConditionValue::String("devops".into()));
        let value =FieldValue::Multichoice(vec!["backend".into(), "frontend".into()]);
        assert!(!eval_condition(Some(&value), &condition));
    }

    #[test]
    fn equals_list_membership() {
        let condition =Condition::Equals(ConditionValue::String("rust".into()));
        let value =FieldValue::List(vec!["rust".into(), "go".into()]);
        assert!(eval_condition(Some(&value), &condition));
    }

    // ── OneOf ───────────────────────────────────────────────────

    #[test]
    fn one_of_matches() {
        let condition =Condition::OneOf(vec![
            ConditionValue::String("open".into()),
            ConditionValue::String("in_progress".into()),
        ]);
        assert!(eval_condition(
            Some(&FieldValue::Choice("open".into())),
            &condition
        ));
    }

    #[test]
    fn one_of_no_match() {
        let condition =Condition::OneOf(vec![
            ConditionValue::String("open".into()),
            ConditionValue::String("in_progress".into()),
        ]);
        assert!(!eval_condition(
            Some(&FieldValue::Choice("done".into())),
            &condition
        ));
    }

    #[test]
    fn one_of_null_is_false() {
        let condition =Condition::OneOf(vec![ConditionValue::String("open".into())]);
        assert!(!eval_condition(None, &condition));
    }

    #[test]
    fn one_of_list_any_member() {
        let condition =Condition::OneOf(vec![
            ConditionValue::String("rust".into()),
            ConditionValue::String("python".into()),
        ]);
        let value =FieldValue::List(vec!["go".into(), "python".into()]);
        assert!(eval_condition(Some(&value), &condition));
    }

    // ── Operator: is_set ────────────────────────────────────────

    #[test]
    fn is_set_true_with_value() {
        let condition =Condition::Operator(ConditionOperator {
            is_set: Some(true),
            not: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(eval_condition(
            Some(&FieldValue::String("hello".into())),
            &condition
        ));
    }

    #[test]
    fn is_set_true_with_null() {
        let condition =Condition::Operator(ConditionOperator {
            is_set: Some(true),
            not: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(!eval_condition(None, &condition));
    }

    #[test]
    fn is_set_false_with_null() {
        let condition =Condition::Operator(ConditionOperator {
            is_set: Some(false),
            not: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(eval_condition(None, &condition));
    }

    #[test]
    fn is_set_false_with_value() {
        let condition =Condition::Operator(ConditionOperator {
            is_set: Some(false),
            not: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(!eval_condition(
            Some(&FieldValue::String("hello".into())),
            &condition
        ));
    }

    // ── Operator: not ───────────────────────────────────────────

    #[test]
    fn not_single_no_match() {
        let condition =Condition::Operator(ConditionOperator {
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
            &condition
        ));
    }

    #[test]
    fn not_single_matches_disallowed() {
        let condition =Condition::Operator(ConditionOperator {
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
            &condition
        ));
    }

    #[test]
    fn not_null_is_false() {
        let condition =Condition::Operator(ConditionOperator {
            not: Some(NegationValue::Single(ConditionValue::String(
                "backlog".into(),
            ))),
            is_set: None,
            all: None,
            any: None,
            none: None,
        });
        assert!(!eval_condition(None, &condition));
    }

    #[test]
    fn not_multiple() {
        let condition =Condition::Operator(ConditionOperator {
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
            &condition
        ));
        assert!(!eval_condition(
            Some(&FieldValue::Choice("backlog".into())),
            &condition
        ));
    }

    // ── Combined operators (AND) ────────────────────────────────

    #[test]
    fn combined_is_set_and_not() {
        let condition =Condition::Operator(ConditionOperator {
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
            &condition
        ));
        assert!(!eval_condition(
            Some(&FieldValue::Choice("backlog".into())),
            &condition
        ));
        assert!(!eval_condition(None, &condition));
    }
}
