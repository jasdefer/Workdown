//! `--delta` on `duration` and `date` fields.
//!
//! Both share the same operand shape (signed seconds, parsed from a
//! duration literal like `1w 2d`) and the same precondition shape
//! (existing parseable value). They differ in the storage format
//! (duration string vs `YYYY-MM-DD`) and the arithmetic.

use std::collections::HashMap;

use super::{ComputedMutation, SetError};

/// Reject `--delta` when the duration field is absent or its current
/// value isn't a parseable duration literal.
pub(super) fn require_existing_duration(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match frontmatter.get(field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "delta",
            field: field.to_owned(),
        }),
        Some(serde_yaml::Value::String(string))
            if crate::model::duration::parse_duration(string).is_ok() =>
        {
            Ok(())
        }
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "duration string (e.g. '1w 2d', '-3h')",
        }),
    }
}

/// Reject `--delta` when the date field is absent or its current value
/// isn't a parseable `YYYY-MM-DD` date.
pub(super) fn require_existing_date(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match frontmatter.get(field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "delta",
            field: field.to_owned(),
        }),
        Some(serde_yaml::Value::String(string))
            if chrono::NaiveDate::parse_from_str(string, "%Y-%m-%d").is_ok() =>
        {
            Ok(())
        }
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "delta",
            field: field.to_owned(),
            expected: "date (YYYY-MM-DD)",
        }),
    }
}

pub(super) fn compute_duration_delta(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    delta_seconds: i64,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    let current_string = previous_value
        .as_ref()
        .and_then(|value| value.as_str())
        .expect("precondition ensures existing duration string");
    let current_seconds = crate::model::duration::parse_duration(current_string)
        .expect("precondition ensures parseable duration");
    let new_seconds = current_seconds.saturating_add(delta_seconds);
    let new_string = crate::model::duration::format_duration_seconds(new_seconds);
    let new_value = serde_yaml::Value::String(new_string);
    new_frontmatter.insert(field.to_owned(), new_value.clone());
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: Some(new_value),
        write_needed: true,
        info_messages: Vec::new(),
    }
}

pub(super) fn compute_date_delta(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    delta_seconds: i64,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    let current_string = previous_value
        .as_ref()
        .and_then(|value| value.as_str())
        .expect("precondition ensures existing date string");
    let current_date = chrono::NaiveDate::parse_from_str(current_string, "%Y-%m-%d")
        .expect("precondition ensures parseable date");
    let new_date = current_date
        .checked_add_signed(chrono::Duration::seconds(delta_seconds))
        .expect("date arithmetic must fit chrono's NaiveDate range");
    let new_value = serde_yaml::Value::String(new_date.format("%Y-%m-%d").to_string());
    new_frontmatter.insert(field.to_owned(), new_value.clone());
    ComputedMutation {
        new_frontmatter,
        previous_value,
        new_value: Some(new_value),
        write_needed: true,
        info_messages: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use crate::model::WorkItemId;
    use crate::operations::set::*;

    // ── Delta: duration ──────────────────────────────────────────────

    #[test]
    fn delta_on_duration_adds_seconds() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nestimate: 2d\n---\n",
        );

        // +1d = 86_400 seconds
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(86_400)),
        )
        .unwrap();

        let new_string = outcome.new_value.unwrap();
        assert_eq!(new_string.as_str().unwrap(), "3d");
    }

    #[test]
    fn delta_on_duration_with_negative_subtracts() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\nestimate: 1w\n---\n",
        );

        // -3d = -259_200 seconds. 1w - 3d = 4d.
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(-259_200)),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "4d");
    }

    #[test]
    fn delta_on_absent_duration_field_returns_requires_existing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "estimate",
            SetOperation::Duration(DurationMode::Delta(3600)),
        );

        assert!(matches!(
            result,
            Err(SetError::MutationRequiresExistingValue { .. })
        ));
    }

    // ── Delta: date ──────────────────────────────────────────────────

    #[test]
    fn delta_on_date_adds_duration() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ndue_date: '2026-05-14'\n---\n",
        );

        // +1w
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(604_800)),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "2026-05-21");
    }

    #[test]
    fn delta_on_date_with_negative_subtracts_duration() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ndue_date: '2026-05-14'\n---\n",
        );

        // -3d
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(-259_200)),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_str().unwrap(), "2026-05-11");
    }

    #[test]
    fn delta_on_absent_date_field_returns_requires_existing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "due_date",
            SetOperation::Date(DateMode::Delta(86_400)),
        );

        assert!(matches!(
            result,
            Err(SetError::MutationRequiresExistingValue { .. })
        ));
    }

    #[test]
    fn date_delta_on_integer_field_returns_mode_not_valid() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: 3\n---\n",
        );

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Date(DateMode::Delta(86_400)),
        );

        assert!(matches!(
            result,
            Err(SetError::ModeNotValidForFieldType { .. })
        ));
    }
}
