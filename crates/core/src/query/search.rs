//! Text search: find items by what they say, not by what a field holds.
//!
//! A [`SearchTerm`] is looked for in an item's *searchable text* — its
//! body, its id, and the value of every field of type `string`. The set
//! is chosen by type, so no field name is wired in: a title is searched
//! because it is a `string`, a status is not because it is a `choice`,
//! which keeps `--search done` from returning every finished item.
//!
//! This is deliberately not part of the `where` grammar
//! ([`crate::query::parse`]): a where clause compares one field, a search
//! asks about the item as a whole and has its own matching rules. Only
//! `workdown query` builds search terms; views have no slot for them.
//!
//! # Matching
//!
//! - [`SearchTerm::text`] — a literal substring, ignoring case.
//! - [`SearchTerm::regex`] — a regular expression, case-sensitive unless
//!   the pattern says `(?i)`, with `^` and `$` matching at line
//!   boundaries, since a body has many lines.
//!
//! Several terms are ANDed. Each term may be found in a different place,
//! so one can hit the title and another the body.

use crate::model::field_value::format_field_value;
use crate::model::schema::{FieldType, Schema};
use crate::model::WorkItem;

// ── Search term ─────────────────────────────────────────────────────

/// One thing to look for in an item's searchable text.
#[derive(Debug, Clone)]
pub struct SearchTerm {
    matcher: Matcher,
}

#[derive(Debug, Clone)]
enum Matcher {
    /// The text, lowercased once here so each haystack is lowercased and
    /// compared without touching the needle again.
    Text {
        lowercase_text: String,
    },
    Regex(regex::Regex),
}

/// Why a search term cannot be built.
#[derive(Debug, thiserror::Error)]
pub enum SearchTermError {
    /// An empty term is found in every text, so it would silently select
    /// everything.
    #[error("the search text must not be empty")]
    Empty,

    #[error("invalid regex: {reason}")]
    InvalidRegex { reason: String },
}

impl SearchTerm {
    /// A literal text, matched as a substring ignoring case. Whitespace is
    /// part of the text: `user login` looks for that phrase.
    pub fn text(text: &str) -> Result<Self, SearchTermError> {
        if text.is_empty() {
            return Err(SearchTermError::Empty);
        }
        Ok(Self {
            matcher: Matcher::Text {
                lowercase_text: text.to_lowercase(),
            },
        })
    }

    /// A regular expression. Case-sensitive; `^` and `$` match at the
    /// start and end of every line.
    pub fn regex(pattern: &str) -> Result<Self, SearchTermError> {
        if pattern.is_empty() {
            return Err(SearchTermError::Empty);
        }
        let compiled = regex::RegexBuilder::new(pattern)
            .multi_line(true)
            .build()
            .map_err(|error| SearchTermError::InvalidRegex {
                reason: error.to_string(),
            })?;
        Ok(Self {
            matcher: Matcher::Regex(compiled),
        })
    }

    fn is_found_in(&self, haystack: &str) -> bool {
        match &self.matcher {
            Matcher::Text { lowercase_text } => haystack.to_lowercase().contains(lowercase_text),
            Matcher::Regex(compiled) => compiled.is_match(haystack),
        }
    }
}

// ── Evaluation ──────────────────────────────────────────────────────

/// Whether every term is found somewhere in the item's searchable text.
/// No terms at all matches every item.
pub fn matches_search(item: &WorkItem, terms: &[SearchTerm], schema: &Schema) -> bool {
    if terms.is_empty() {
        return true;
    }
    let texts = searchable_texts(item, schema);
    terms
        .iter()
        .all(|term| texts.iter().any(|text| term.is_found_in(text)))
}

/// The texts a search looks in: the body, the id, and each `string`
/// field the item has a value for. Kept apart rather than joined, so a
/// term cannot match across the seam between two of them.
fn searchable_texts(item: &WorkItem, schema: &Schema) -> Vec<String> {
    let mut texts = vec![item.body.clone(), item.id.as_str().to_owned()];
    for (field_name, definition) in &schema.fields {
        if definition.field_type() != FieldType::String {
            continue;
        }
        if let Some(value) = item.fields.get(field_name) {
            texts.push(format_field_value(value));
        }
    }
    texts
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn text(term: &str) -> SearchTerm {
        SearchTerm::text(term).unwrap()
    }

    fn regex(pattern: &str) -> SearchTerm {
        SearchTerm::regex(pattern).unwrap()
    }

    #[test]
    fn text_matches_a_substring_ignoring_case() {
        assert!(text("LOGIN").is_found_in("Fix the login flow"));
        assert!(text("login").is_found_in("Fix the Login flow"));
        assert!(text("ogi").is_found_in("login"));
        assert!(!text("logout").is_found_in("Fix the login flow"));
    }

    #[test]
    fn text_ignores_case_beyond_ascii() {
        assert!(text("übersicht").is_found_in("Die ÜBERSICHT fehlt"));
    }

    #[test]
    fn text_is_literal_including_spaces_and_regex_characters() {
        assert!(text("user login").is_found_in("the user login page"));
        assert!(!text("user login").is_found_in("the user's login page"));
        assert!(text("/api/users/").is_found_in("GET /api/users/ fails"));
        assert!(text("a.c").is_found_in("a.c"));
        assert!(!text("a.c").is_found_in("abc"));
    }

    #[test]
    fn regex_is_case_sensitive_unless_the_pattern_says_otherwise() {
        assert!(regex("log(in|out)").is_found_in("fix logout"));
        assert!(!regex("login").is_found_in("Fix Login"));
        assert!(regex("(?i)login").is_found_in("Fix Login"));
    }

    #[test]
    fn regex_anchors_match_at_line_boundaries() {
        let body = "Intro.\n\n## Decisions\n\nNone yet.\n";
        assert!(regex("^## Decisions$").is_found_in(body));
        assert!(!regex("^Decisions").is_found_in(body));
    }

    #[test]
    fn empty_terms_are_refused() {
        assert!(matches!(SearchTerm::text(""), Err(SearchTermError::Empty)));
        assert!(matches!(SearchTerm::regex(""), Err(SearchTermError::Empty)));
    }

    #[test]
    fn an_invalid_regex_is_refused_with_the_reason() {
        let error = SearchTerm::regex("log(in").unwrap_err();
        assert!(matches!(error, SearchTermError::InvalidRegex { .. }));
        assert!(error.to_string().starts_with("invalid regex: "));
    }
}
