//! `--toggle` on `boolean` fields.

use std::collections::HashMap;

use super::{current_value, ComputedMutation, SetError};

/// Reject `--toggle` when the field is absent or the current value
/// isn't a real boolean. There is no "flip nothing".
pub(super) fn require_existing(
    frontmatter: &HashMap<String, serde_yaml::Value>,
    field: &str,
) -> Result<(), SetError> {
    match current_value(frontmatter, field) {
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
