//! `--toggle` on `boolean` fields.

use std::collections::HashMap;

use super::{ComputedMutation, SetError};

/// Reject `--toggle` when the field is absent or the current value
/// isn't a real boolean.
pub(super) fn require_existing(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match frontmatter.get(field) {
        None => Err(SetError::MutationRequiresExistingValue {
            mode: "toggle",
            field: field.to_owned(),
        }),
        Some(serde_yaml::Value::Bool(_)) => Ok(()),
        Some(_) => Err(SetError::MutationCurrentValueMalformed {
            mode: "toggle",
            field: field.to_owned(),
            expected: "boolean",
        }),
    }
}

pub(super) fn compute_toggle(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    let current = previous_value
        .as_ref()
        .and_then(|value| value.as_bool())
        .expect("precondition ensures existing boolean value");
    let new_value = serde_yaml::Value::Bool(!current);
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

    // ── Toggle: boolean ──────────────────────────────────────────────

    #[test]
    fn toggle_flips_boolean_from_false_to_true() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\narchived: false\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "archived",
            SetOperation::Boolean(BooleanMode::Toggle),
        )
        .unwrap();

        assert_eq!(outcome.previous_value.unwrap().as_bool().unwrap(), false);
        assert_eq!(outcome.new_value.unwrap().as_bool().unwrap(), true);
        let file = read_item(&root, "task-1");
        assert!(file.contains("archived: true"));
    }

    #[test]
    fn toggle_flips_boolean_from_true_to_false() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\narchived: true\n---\n",
        );

        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "archived",
            SetOperation::Boolean(BooleanMode::Toggle),
        )
        .unwrap();

        assert_eq!(outcome.new_value.unwrap().as_bool().unwrap(), false);
    }

    #[test]
    fn toggle_on_absent_field_returns_requires_existing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "archived",
            SetOperation::Boolean(BooleanMode::Toggle),
        );

        assert!(matches!(
            result,
            Err(SetError::MutationRequiresExistingValue { ref mode, ref field })
                if *mode == "toggle" && field == "archived"
        ));
    }

    #[test]
    fn toggle_on_non_boolean_field_returns_mode_not_valid() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Boolean(BooleanMode::Toggle),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::ModeNotValidForFieldType { ref mode, ref field, .. }
                if *mode == "toggle" && field == "status"
        ));
    }

    #[test]
    fn toggle_on_malformed_boolean_returns_malformed_error() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        // `archived: yes` — YAML parses this as a string, not a bool.
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\narchived: yes\n---\n",
        );

        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "archived",
            SetOperation::Boolean(BooleanMode::Toggle),
        );

        assert!(matches!(
            result,
            Err(SetError::MutationCurrentValueMalformed { ref mode, ref field, .. })
                if *mode == "toggle" && field == "archived"
        ));
    }
}
