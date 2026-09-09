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
