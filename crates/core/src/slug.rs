//! Kebab-case slug derivation.
//!
//! Shared so a work item's filename slug (`workdown add`) and a view's id
//! (created from the UI) follow the *same* rule — one definition, no drift.
//! The result is always a valid id per [`crate::model::work_item::is_valid_id`].

use crate::model::work_item::is_valid_id;

/// A name that can't be reduced to a valid id (e.g. it has no alphanumeric
/// characters). Callers map this onto their own error type.
#[derive(Debug, thiserror::Error)]
#[error("cannot derive a valid id from '{input}': {reason}")]
pub struct SlugError {
    pub input: String,
    pub reason: String,
}

/// Convert a human name into a valid kebab-case id.
///
/// Rules: lowercase, non-alphanumeric replaced with hyphens, consecutive
/// hyphens collapsed, leading/trailing hyphens stripped. Leading digits are
/// preserved (`is_valid_id` accepts digit-first ids). Fails when nothing
/// alphanumeric remains.
pub fn slugify(input: &str) -> Result<String, SlugError> {
    let replaced: String = input
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();

    // Collapse consecutive hyphens.
    let mut collapsed = String::with_capacity(replaced.len());
    let mut previous_was_hyphen = false;
    for character in replaced.chars() {
        if character == '-' {
            if !previous_was_hyphen {
                collapsed.push('-');
            }
            previous_was_hyphen = true;
        } else {
            collapsed.push(character);
            previous_was_hyphen = false;
        }
    }

    let trimmed = collapsed.trim_start_matches('-').trim_end_matches('-');

    if trimmed.is_empty() || !is_valid_id(trimmed) {
        return Err(SlugError {
            input: input.to_owned(),
            reason: "must contain at least one alphanumeric character".to_owned(),
        });
    }

    Ok(trimmed.to_owned())
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_simple_title() {
        assert_eq!(slugify("My Cool Task").unwrap(), "my-cool-task");
    }

    #[test]
    fn slugify_special_characters() {
        assert_eq!(slugify("Fix Bug #123!").unwrap(), "fix-bug-123");
    }

    #[test]
    fn slugify_extra_spaces_and_symbols() {
        assert_eq!(slugify("  Hello,  World!  ").unwrap(), "hello-world");
    }

    #[test]
    fn slugify_preserves_leading_digits() {
        assert_eq!(slugify("123 Task").unwrap(), "123-task");
    }

    #[test]
    fn slugify_only_special_characters_fails() {
        assert!(slugify("###!!!").is_err());
    }

    #[test]
    fn slugify_only_digits_succeeds() {
        assert_eq!(slugify("12345").unwrap(), "12345");
    }

    #[test]
    fn slugify_preserves_internal_digits() {
        assert_eq!(slugify("Task 42 Done").unwrap(), "task-42-done");
    }
}
