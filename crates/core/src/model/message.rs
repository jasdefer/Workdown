//! House style for user-facing message text.
//!
//! Every validation finding renders exactly once, into
//! [`Diagnostic::message`](super::diagnostic::Diagnostic::message), and
//! that one string is what both `workdown validate` and the web banner
//! show. The wording rules therefore live here rather than being
//! rediscovered in each `Display` arm:
//!
//! - **Lowercase prose, no trailing period.** Messages are list items,
//!   not sentences in a paragraph.
//! - **Identifiers in single quotes** — `field 'status'`, `view 'board'`.
//!   Not backticks: these strings land in a terminal, where a backtick
//!   is a literal character rather than markup. (The Markdown renderers
//!   under `crates/cli/src/render` write into `.md` files, where
//!   backticks *are* markup — a different surface, not this style.)
//! - **Never debug formatting.** `{:?}` on a `Vec` or an `Option` prints
//!   Rust syntax at someone who did not write any Rust: `["open",
//!   "done"]`, `Some(1.0)`. Values go through [`one_of`] or
//!   [`quoted_list`]; an absent value gets prose or gets left out of the
//!   message entirely.
//! - **One glyph per concept.** Reference chains join with [`chain`], so
//!   a cycle reads the same whether it was found among items, computed
//!   fields, or derived values.
//!
//! The two list helpers are deliberately phrased and bounded
//! differently. [`one_of`] describes what *would* be accepted and
//! truncates, because a 40-entry `people` section would otherwise bury
//! the message it belongs to. [`quoted_list`] names what an item
//! actually got wrong and never truncates, because every member of that
//! list is something the reader has to go and fix.

/// How many members [`one_of`] names before it counts the rest. Long
/// enough for a realistic `status` or `type`, short enough that a
/// 40-entry `people` section doesn't bury the message.
const MAX_LISTED_VALUES: usize = 8;

/// The one glyph a reference chain is joined with.
const CHAIN_ARROW: &str = " \u{2192} ";

/// Phrase an accepted-value set as `one of: a, b, c`, counting the tail
/// once the list gets long. Reads as the complement of a negation —
/// `'x' is not one of: …` — so the caller supplies the negation.
///
/// Members print **in the order given**. A schema's `values:` list and
/// the color palette are authored orders that carry information (a
/// status workflow's sequence, the spectrum) and alphabetizing them
/// throws it away; a caller whose set genuinely has no order — a
/// `HashSet` of resource entries widened by the values items hold —
/// sorts before calling, so that its message is at least stable.
pub fn one_of<S: AsRef<str>>(values: &[S]) -> String {
    if values.is_empty() {
        return "one of this field's values (it declares none)".to_owned();
    }

    let shown = values
        .iter()
        .take(MAX_LISTED_VALUES)
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join(", ");

    if values.len() <= MAX_LISTED_VALUES {
        return format!("one of: {shown}");
    }
    let rest = values.len() - MAX_LISTED_VALUES;
    format!("one of: {shown}, … ({rest} more)")
}

/// Phrase identifiers as a quoted, comma-separated list: `'a', 'b'`.
/// Never truncated — see the module doc.
pub fn quoted_list<S: AsRef<str>>(values: &[S]) -> String {
    values
        .iter()
        .map(|value| format!("'{}'", value.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Join a reference chain — a link cycle, a compute dependency path —
/// with the single arrow glyph the messages use.
pub fn chain<S: AsRef<str>>(ids: &[S]) -> String {
    ids.iter()
        .map(AsRef::as_ref)
        .collect::<Vec<_>>()
        .join(CHAIN_ARROW)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_set_is_listed_in_full() {
        assert_eq!(
            one_of(&["open", "in_progress", "done"]),
            "one of: open, in_progress, done"
        );
    }

    #[test]
    fn the_order_given_is_the_order_printed() {
        // The whole point of taking a slice rather than a set: an
        // authored order survives the trip into the message.
        assert_eq!(
            one_of(&["red", "orange", "yellow"]),
            "one of: red, orange, yellow"
        );
    }

    #[test]
    fn a_long_set_is_truncated_with_a_count() {
        let values: Vec<String> = (1..=12).map(|index| format!("value-{index:02}")).collect();
        let described = one_of(&values);
        assert!(described.starts_with("one of: value-01, "), "{described}");
        assert!(described.ends_with("… (4 more)"), "{described}");
    }

    #[test]
    fn a_set_at_the_cap_is_not_truncated() {
        let values: Vec<String> = (1..=MAX_LISTED_VALUES).map(|i| i.to_string()).collect();
        assert!(!one_of(&values).contains("more)"));
    }

    #[test]
    fn an_empty_set_says_so_in_prose() {
        let empty: [&str; 0] = [];
        assert_eq!(
            one_of(&empty),
            "one of this field's values (it declares none)"
        );
    }

    #[test]
    fn offending_values_are_quoted_and_never_truncated() {
        let values: Vec<String> = (1..=12).map(|index| format!("v{index}")).collect();
        let listed = quoted_list(&values);
        assert!(listed.starts_with("'v1', 'v2', "), "{listed}");
        assert!(listed.ends_with("'v12'"), "{listed}");
    }

    #[test]
    fn a_chain_joins_with_one_arrow_glyph() {
        assert_eq!(chain(&["a", "b", "a"]), "a → b → a");
    }
}
