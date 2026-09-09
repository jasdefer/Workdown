//! The one date grammar: `YYYY-MM-DD`.
//!
//! Every place that turns user-written text into a calendar date —
//! frontmatter coercion, resource constants, rule conditions, the
//! filter evaluator and its operand checker, the `set` date delta —
//! goes through [`parse_date`], so they all accept exactly the same
//! strings. chrono's `%m` and `%d` take one or two digits, so an
//! unpadded `2026-3-1` is March 1st everywhere, not a malformed date
//! in one place and a valid one in another.

use chrono::NaiveDate;

/// The strftime pattern behind [`parse_date`], and the shape every
/// renderer prints a date in.
pub const DATE_FORMAT: &str = "%Y-%m-%d";

/// Parse `text` as a `YYYY-MM-DD` calendar date. `None` when it is not
/// one; the caller decides whether that is an error, a warning, or a
/// filter that matches nothing.
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text, DATE_FORMAT).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_and_unpadded_components_are_the_same_date() {
        let march_first = NaiveDate::from_ymd_opt(2026, 3, 1);
        assert_eq!(parse_date("2026-03-01"), march_first);
        assert_eq!(parse_date("2026-3-1"), march_first);
    }

    #[test]
    fn other_shapes_are_not_dates() {
        for text in [
            "03/01/2026",
            "2026-03-15T00:00",
            "yesterday",
            "2026-13-45",
            "",
        ] {
            assert_eq!(parse_date(text), None, "{text:?}");
        }
    }
}
