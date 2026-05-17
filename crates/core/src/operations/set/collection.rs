//! `--append` / `--remove` modes on collection-shaped fields
//! (`list`, `links`, `multichoice`).

use std::collections::HashMap;

use super::{CollectionMode, ComputedMutation};

pub(super) fn compute(
    mut new_frontmatter: HashMap<String, serde_yaml::Value>,
    field: &str,
    mode: CollectionMode,
    previous_value: Option<serde_yaml::Value>,
) -> ComputedMutation {
    match mode {
        CollectionMode::Append(values) => {
            let current_sequence = current_value_as_sequence(previous_value.as_ref());
            let (new_sequence, info_messages) = append_to_sequence(current_sequence, values, field);
            let new_value = serde_yaml::Value::Sequence(new_sequence);
            new_frontmatter.insert(field.to_owned(), new_value.clone());
            ComputedMutation {
                new_frontmatter,
                previous_value,
                new_value: Some(new_value),
                // Append always writes — duplicate-append is intentional
                // (decision 4 in cli-set-modes), surfaced via info_messages.
                write_needed: true,
                info_messages,
            }
        }
        CollectionMode::Remove(values) => {
            let current_sequence = current_value_as_sequence(previous_value.as_ref());
            let (new_sequence, info_messages) =
                remove_from_sequence(current_sequence.clone(), values, field);
            // Skip the write when the sequence is unchanged — covers
            // both "remove from absent field" and "remove value that
            // wasn't there". Keeps the file byte-identical when nothing
            // happened on disk.
            let write_needed = current_sequence != new_sequence;
            let new_value = if write_needed {
                let value = serde_yaml::Value::Sequence(new_sequence);
                new_frontmatter.insert(field.to_owned(), value.clone());
                Some(value)
            } else {
                previous_value.clone()
            };
            ComputedMutation {
                new_frontmatter,
                previous_value,
                new_value,
                write_needed,
                info_messages,
            }
        }
    }
}

/// Normalize a possibly-absent, possibly-scalar field value into a
/// `Vec<Value>` ready for collection-mode arithmetic.
///
/// Scalar promotion handles a hand-edited file where a `list`/`links`/
/// `multichoice` field accidentally holds a single scalar — we treat it
/// as a one-element sequence so the operation still produces a clean
/// sequence on disk. The coerce pass on reload will reconcile.
fn current_value_as_sequence(previous_value: Option<&serde_yaml::Value>) -> Vec<serde_yaml::Value> {
    match previous_value {
        Some(serde_yaml::Value::Sequence(sequence)) => sequence.clone(),
        Some(value) => vec![value.clone()],
        None => Vec::new(),
    }
}

/// Append each value to the end of `current`, flagging duplicates via
/// an info message but appending them anyway (decision 4 in
/// cli-set-modes — honors the literal request, lets the user notice).
fn append_to_sequence(
    mut current: Vec<serde_yaml::Value>,
    values_to_append: Vec<serde_yaml::Value>,
    field: &str,
) -> (Vec<serde_yaml::Value>, Vec<String>) {
    let mut info_messages = Vec::new();
    for value in values_to_append {
        if current.contains(&value) {
            info_messages.push(format!(
                "value {} was already present in '{}'",
                format_value_for_info(&value),
                field
            ));
        }
        current.push(value);
    }
    (current, info_messages)
}

/// Remove every occurrence of each value from `current`. Values that
/// weren't there emit an info message (decision 3 in cli-set-modes:
/// "remove all" semantics across `list`/`links`/`multichoice`).
fn remove_from_sequence(
    mut current: Vec<serde_yaml::Value>,
    values_to_remove: Vec<serde_yaml::Value>,
    field: &str,
) -> (Vec<serde_yaml::Value>, Vec<String>) {
    let mut info_messages = Vec::new();
    for value in values_to_remove {
        let before_length = current.len();
        current.retain(|element| element != &value);
        if current.len() == before_length {
            info_messages.push(format!(
                "value {} was not present in '{}'",
                format_value_for_info(&value),
                field
            ));
        }
    }
    (current, info_messages)
}

/// Compact rendering of a value for inclusion in an info message.
/// Strings are quoted; other scalars are stringified plainly; complex
/// shapes (rare in collection elements) fall back to single-line YAML.
fn format_value_for_info(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(string) => format!("'{string}'"),
        serde_yaml::Value::Bool(boolean) => boolean.to_string(),
        serde_yaml::Value::Number(number) => number.to_string(),
        serde_yaml::Value::Null => "(null)".to_owned(),
        _ => serde_yaml::to_string(value)
            .unwrap_or_default()
            .trim()
            .to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use crate::model::WorkItemId;
    use crate::operations::set::*;

    // ── Collection modes: append ─────────────────────────────────────

    #[test]
    fn append_to_list_appends_in_order() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth]\n---\n",
        );

        let appended = vec![serde_yaml::Value::String("backend".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let new_sequence = outcome.new_value.unwrap();
        let elements = new_sequence.as_sequence().unwrap();
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].as_str().unwrap(), "auth");
        assert_eq!(elements[1].as_str().unwrap(), "backend");
        assert!(outcome.info_messages.is_empty());
        assert!(!outcome.mutation_caused_warning);
    }

    #[test]
    fn append_to_absent_field_creates_sequence() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("qa".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        assert!(outcome.previous_value.is_none());
        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "qa");
    }

    #[test]
    fn append_duplicate_writes_and_emits_info() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth, qa]\n---\n",
        );

        let appended = vec![serde_yaml::Value::String("qa".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 3);
        assert_eq!(sequence[2].as_str().unwrap(), "qa");
        assert_eq!(outcome.info_messages.len(), 1);
        assert!(outcome.info_messages[0].contains("'qa'"));
        assert!(outcome.info_messages[0].contains("already present"));
        // Duplicate append is intentional and never flips exit code.
        assert!(!outcome.mutation_caused_warning);
    }

    #[test]
    fn append_multi_value_in_order() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth]\n---\n",
        );

        let appended = vec![
            serde_yaml::Value::String("backend".to_owned()),
            serde_yaml::Value::String("qa".to_owned()),
        ];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        let names: Vec<&str> = sequence.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(names, vec!["auth", "backend", "qa"]);
    }

    #[test]
    fn append_on_links_field_works() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");
        write_item(&root, "alice", "---\ntitle: Alice\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("alice".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "assignees",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "alice");
    }

    #[test]
    fn append_on_multichoice_field_works() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("bug".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "labels",
            SetOperation::Collection(CollectionMode::Append(appended)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "bug");
        assert!(!outcome.mutation_caused_warning);
    }

    // ── Collection modes: remove ─────────────────────────────────────

    #[test]
    fn remove_value_removes_all_occurrences() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth, backend, auth]\n---\n",
        );

        let to_remove = vec![serde_yaml::Value::String("auth".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "backend");
        assert!(outcome.info_messages.is_empty());
    }

    #[test]
    fn remove_absent_value_emits_info() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth]\n---\n",
        );

        let to_remove = vec![serde_yaml::Value::String("qa".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        )
        .unwrap();

        // Sequence unchanged → write skipped, file byte-identical.
        let file_after = read_item(&root, "task-1");
        assert!(file_after.contains("tags:"));
        assert!(file_after.contains("auth"));
        assert!(!file_after.contains("qa"));

        assert_eq!(outcome.info_messages.len(), 1);
        assert!(outcome.info_messages[0].contains("'qa'"));
        assert!(outcome.info_messages[0].contains("not present"));
        assert!(!outcome.mutation_caused_warning);
    }

    #[test]
    fn remove_from_absent_field_is_noop_with_info() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        let original = "---\ntitle: Task 1\nstatus: open\n---\nbody\n";
        write_item(&root, "task-1", original);

        let to_remove = vec![serde_yaml::Value::String("qa".to_owned())];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        )
        .unwrap();

        // File untouched byte-for-byte; field stays absent.
        assert_eq!(read_item(&root, "task-1"), original);
        assert!(outcome.previous_value.is_none());
        assert_eq!(outcome.info_messages.len(), 1);
    }

    #[test]
    fn remove_multi_value_with_some_absent() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\ntags: [auth, backend]\n---\n",
        );

        let to_remove = vec![
            serde_yaml::Value::String("auth".to_owned()),
            serde_yaml::Value::String("qa".to_owned()),
        ];
        let outcome = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "tags",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        )
        .unwrap();

        let elements = outcome.new_value.unwrap();
        let sequence = elements.as_sequence().unwrap();
        assert_eq!(sequence.len(), 1);
        assert_eq!(sequence[0].as_str().unwrap(), "backend");
        assert_eq!(outcome.info_messages.len(), 1);
        assert!(outcome.info_messages[0].contains("'qa'"));
    }

    // ── Collection modes: mode-type validity ─────────────────────────

    #[test]
    fn append_on_choice_field_returns_mode_not_valid_error() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("done".to_owned())];
        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "status",
            SetOperation::Collection(CollectionMode::Append(appended)),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::ModeNotValidForFieldType { ref mode, ref field, .. }
                if *mode == "append" && field == "status"
        ));
        let message = error.to_string();
        assert!(message.contains("--append"));
        assert!(message.contains("'status'"));
        assert!(message.contains("choice"));
    }

    #[test]
    fn remove_on_integer_field_returns_mode_not_valid_error() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task 1\nstatus: open\npoints: 3\n---\n",
        );

        let to_remove = vec![serde_yaml::Value::String("3".to_owned())];
        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "points",
            SetOperation::Collection(CollectionMode::Remove(to_remove)),
        );

        let error = result.unwrap_err();
        assert!(matches!(
            error,
            SetError::ModeNotValidForFieldType { ref mode, ref field, .. }
                if *mode == "remove" && field == "points"
        ));
    }

    #[test]
    fn append_on_link_singular_field_returns_mode_not_valid_error() {
        // `parent: link` is single-valued — collection modes must reject.
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task 1\nstatus: open\n---\n");

        let appended = vec![serde_yaml::Value::String("other-task".to_owned())];
        let result = run_set(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "parent",
            SetOperation::Collection(CollectionMode::Append(appended)),
        );

        assert!(matches!(
            result,
            Err(SetError::ModeNotValidForFieldType { .. })
        ));
    }
}
