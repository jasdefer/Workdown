//! `--delta` on `integer` / `float` fields.

use std::collections::HashMap;

use super::{ComputedMutation, SetError};

/// Reject `--delta` when the field is absent or the current value isn't
/// a number we can parse. Hard error — the file is not written.
pub(super) fn require_existing(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match frontmatter.get(field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "delta",
            field: field.to_owned(),
        }),
        Some(value) if value.as_i64().is_some() || value.as_f64().is_some() => Ok(()),
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "number",
        }),
    }
}

pub(super) fn compute_delta(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    delta: serde_yaml::Number,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    // Preconditions guarantee `previous_value` is `Some(Number)`.
    let new_value = apply_numeric_delta(
        previous_value
            .as_ref()
            .expect("precondition ensures existing numeric value"),
        &delta,
    );
    new_frontmatter.insert(field.to_owned(), new_value.clone());
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: Some(new_value),
        write_needed: true,
        info_messages: Vec::new(),
    }
}

/// Add a signed delta to a numeric field's current value, preserving
/// the field's int/float typing.
///
/// Float arithmetic kicks in only when either operand is itself a
/// float; pure-integer adds stay as i64 so the on-disk YAML reads as
/// `points: 8` and not `points: 8.0`.
fn apply_numeric_delta(
    current: &serde_yaml::Value,
    delta: &serde_yaml::Number,
) -> serde_yaml::Value {
    let current_number = match current {
        serde_yaml::Value::Number(number) => number,
        _ => unreachable!("preconditions ensure numeric current value"),
    };

    let use_float = delta.is_f64() || current_number.is_f64();
    if use_float {
        let a = current_number
            .as_f64()
            .expect("number coerces to f64 unless infinitely large");
        let b = delta.as_f64().expect("delta coerces to f64");
        serde_yaml::Value::Number(serde_yaml::Number::from(a + b))
    } else {
        let a = current_number.as_i64().expect("integer number stays i64");
        let b = delta.as_i64().expect("integer delta stays i64");
        serde_yaml::Value::Number(serde_yaml::Number::from(a.saturating_add(b)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use crate::model::WorkItemId;
    use crate::operations::set::*;

    // ── Delta: numeric ───────────────────────────────────────────────

    #[test]
    fn delta_on_integer_adds_value() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: 5\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
        )
        .unwrap();

        assert_eq!(outcome.previous_value.unwrap().as_i64().unwrap(), 5);
        assert_eq!(outcome.new_value.unwrap().as_i64().unwrap(), 8);
        let file = read_item(&root, "task-1");
        assert!(file.contains("points: 8"));
    }

    #[test]
    fn delta_on_integer_with_negative_subtracts() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: 5\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(-3_i64))),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_i64().unwrap(), 2);
    }

    #[test]
    fn delta_on_float_adds_value() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nvelocity: 2.5\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "velocity",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(1.5_f64))),
        )
        .unwrap();

        assert!((outcome.new_value.unwrap().as_f64().unwrap() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn delta_on_absent_numeric_field_returns_requires_existing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::MutationRequiresExistingValue { ref mode, ref field }
                if *mode == "delta" && field == "points"
        ));
    }

    #[test]
    fn delta_on_malformed_numeric_returns_malformed_error() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: high\n---\n",
        );

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(3))),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::MutationCurrentValueMalformed { ref mode, ref field, .. }
                if *mode == "delta" && field == "points"
        ));
    }

    #[test]
    fn numeric_delta_on_choice_field_returns_mode_not_valid() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Numeric(NumericMode::Delta(serde_yaml::Number::from(1))),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::ModeNotValidForFieldType { ref mode, ref field, .. }
                if *mode == "delta" && field == "status"
        ));
    }
}
