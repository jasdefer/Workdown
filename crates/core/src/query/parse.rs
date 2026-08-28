//! Parser for `--where` CLI expressions.
//!
//! Each `--where` flag carries a single expression string. Multiple flags
//! are combined into [`Predicate::And`] by the command layer — this parser
//! handles one expression at a time.

use crate::query::types::{Comparison, FieldReference, Operand, Operator, Predicate, QueryRegex};

// ── Error ───────────────────────────────────────────────────────────

/// Errors produced when parsing a `--where` expression.
#[derive(Debug, thiserror::Error)]
pub enum QueryParseError {
    #[error("empty filter expression")]
    Empty,

    #[error("cannot parse filter expression: '{raw}'")]
    UnknownOperator { raw: String },

    #[error("invalid regex '/{pattern}/': {reason}")]
    InvalidRegex { pattern: String, reason: String },

    #[error(
        "'{raw}': the value list after '{operator}' must not be empty or contain empty members"
    )]
    EmptyValueList { raw: String, operator: &'static str },
}

// ── Public API ──────────────────────────────────────────────────────

/// Parse a single `--where` expression into a [`Predicate`].
///
/// # Syntax
///
/// | Form | Example | Meaning |
/// |------|---------|---------|
/// | Equality | `status=open` | field equals value, commas included |
/// | Not-equal | `status!=done` | field does not equal value |
/// | In | `status in open,in_progress` | field equals any listed value |
/// | Not-in | `status not in done,removed` | field equals none of them |
/// | Greater | `points>3` | numeric/lexicographic greater-than |
/// | Less | `points<10` | numeric/lexicographic less-than |
/// | Greater-or-equal | `points>=3` | numeric/lexicographic >= |
/// | Less-or-equal | `points<=10` | numeric/lexicographic <= |
/// | Contains | `title~login` | substring (strings) or membership (lists) |
/// | Regex | `title/^fix-.*/i` | regex match (optional `i` flag) |
/// | Is-set | `assignee?` | field is present |
/// | Is-not-set | `!assignee?` | field is absent |
///
/// # Fields
///
/// Any field defined in `schema.yaml`, plus `id`. The id is addressable
/// like any other field — it is projected into every item's field map at
/// load (see `store::coerce::coerce_fields`) — and compares as a string,
/// so the full string operator set applies: `id=alpha`, `id in a,b`,
/// `id/^auth-/`. It is never absent, so `id?` always holds. A single dot
/// traverses one relation, on either side: `parent.status`, `parent.id`,
/// `children.id`.
///
/// # Operator precedence
///
/// An expression is split at the first operator token found, scanning the
/// punctuation operators before the word operators `in` / `not in`. That
/// order matters because `in` is the only token made of letters, and letters
/// occur inside field names and values: `title=a in b` must split at `=`
/// (field `title`, value `a in b`), not at ` in ` (field `title=a`). The word
/// tokens are whitespace-delimited for the same reason — a field named
/// `sprint` contains the letters `in` — and ` not in ` is tested before ` in `
/// so that `status not in done` does not yield the field name `status not`.
///
/// One consequence is accepted: on a string field, `title in review` now
/// reads as membership rather than as a literal. It was a parse error before,
/// so nothing changes meaning.
///
/// # Desugaring
///
/// `in` and `not in` do not reach the evaluator. `status in a,b` becomes
/// `Or(status=a, status=b)` and `status not in a,b` becomes
/// `And(status!=a, status!=b)`; [`crate::query::clause`] folds both shapes
/// back into a single condition for the guided builder. The n-ary predicate
/// is produced even for a one-member list, so that a round-trip through the
/// clause string cannot silently downgrade `in` to `=`.
pub fn parse_where(input: &str) -> Result<Predicate, QueryParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(QueryParseError::Empty);
    }

    // 1. IsNotSet: !field?
    if let Some(inner) = trimmed.strip_prefix('!') {
        if let Some(field_name) = inner.strip_suffix('?') {
            let field_name = field_name.trim();
            validate_field_name(field_name, trimmed)?;
            return Ok(Predicate::Not(Box::new(Predicate::Comparison(
                Comparison {
                    field: build_field_ref(field_name),
                    operator: Operator::IsSet,
                    operand: Operand::Value(String::new()),
                },
            ))));
        }
    }

    // 2. IsSet: field?
    if let Some(field_name) = trimmed.strip_suffix('?') {
        let field_name = field_name.trim();
        validate_field_name(field_name, trimmed)?;
        return Ok(Predicate::Comparison(Comparison {
            field: build_field_ref(field_name),
            operator: Operator::IsSet,
            operand: Operand::Value(String::new()),
        }));
    }

    // 3. Regex: field/pattern/flags
    if let Some(result) = try_parse_regex(trimmed)? {
        return Ok(result);
    }

    // 4. Two-char operators: !=, >=, <=
    for (token, operator) in [
        ("!=", Operator::NotEqual),
        (">=", Operator::GreaterOrEqual),
        ("<=", Operator::LessOrEqual),
    ] {
        if let Some(position) = trimmed.find(token) {
            let field_name = trimmed[..position].trim();
            let value = trimmed[position + 2..].trim();
            validate_field_name(field_name, trimmed)?;
            return Ok(Predicate::Comparison(Comparison {
                field: build_field_ref(field_name),
                operator,
                operand: Operand::Value(value.to_owned()),
            }));
        }
    }

    // 5. Single-char operators: =, >, <, ~
    for (token, operator) in [
        ('=', Operator::Equal),
        ('>', Operator::GreaterThan),
        ('<', Operator::LessThan),
        ('~', Operator::Contains),
    ] {
        if let Some(position) = trimmed.find(token) {
            let field_name = trimmed[..position].trim();
            let value = trimmed[position + 1..].trim();
            validate_field_name(field_name, trimmed)?;
            return Ok(Predicate::Comparison(Comparison {
                field: build_field_ref(field_name),
                operator,
                operand: Operand::Value(value.to_owned()),
            }));
        }
    }

    // 6. Word operators, only once every punctuation operator has missed.
    //    ` not in` before ` in`, or the longer token's own ` in` matches first
    //    and the field name absorbs the `not`.
    for (token, operator) in [(" not in", Operator::NotIn), (" in", Operator::In)] {
        if let Some(position) = find_word_operator(trimmed, token) {
            let field_name = trimmed[..position].trim();
            validate_field_name(field_name, trimmed)?;
            let values = parse_value_list(&trimmed[position + token.len()..], trimmed, token)?;
            return Ok(desugar_membership(field_name, operator, &values));
        }
    }

    Err(QueryParseError::UnknownOperator {
        raw: trimmed.to_owned(),
    })
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Locate a word operator: the token must be followed by whitespace or by the
/// end of the expression.
///
/// The leading space in the token keeps a field name that merely *contains* the
/// letters (`sprint`) from being split; requiring a boundary after it keeps a
/// field name that merely *starts* with them (`status in_progress`, which has
/// no operator at all) from matching either. Accepting end-of-input is what
/// lets `status in` report a missing value list rather than a generic parse
/// failure — the input is trimmed before the scan, so a typed trailing space is
/// already gone by now.
fn find_word_operator(input: &str, token: &str) -> Option<usize> {
    let mut search_from = 0;
    while let Some(offset) = input[search_from..].find(token) {
        let position = search_from + offset;
        let after = position + token.len();
        if input[after..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
        {
            return Some(position);
        }
        search_from = position + 1;
    }
    None
}

/// Split a comma-separated value list, trimming each member. Rejects an empty
/// list and empty members: neither can ever match, so both are typos rather
/// than intent (a trailing comma being the likely source).
///
/// There is no escaping, so a literal comma inside one member is not
/// representable — the raw hatch covers that case.
fn parse_value_list(
    rest: &str,
    raw: &str,
    token: &'static str,
) -> Result<Vec<String>, QueryParseError> {
    let trimmed = rest.trim();
    let empty = || QueryParseError::EmptyValueList {
        raw: raw.to_owned(),
        operator: token.trim(),
    };
    if trimmed.is_empty() {
        return Err(empty());
    }
    let mut values = Vec::new();
    for member in trimmed.split(',') {
        let member = member.trim();
        if member.is_empty() {
            return Err(empty());
        }
        values.push(member.to_owned());
    }
    Ok(values)
}

/// Rewrite a membership test into the predicates the evaluator understands:
/// `in` as an `Or` of equals, `not in` as an `And` of not-equals.
///
/// The two are equivalent under the evaluator's absent-value rule (a field
/// with no value satisfies the negative comparisons and fails the positive
/// ones), so this is a shape choice: the `And` form mirrors the `Or` form as a
/// flat, same-field list, which is what [`crate::query::clause`] folds back.
///
/// Always n-ary, including for one member — see [`parse_where`].
fn desugar_membership(field_name: &str, operator: Operator, values: &[String]) -> Predicate {
    let comparisons = values
        .iter()
        .map(|value| {
            Predicate::Comparison(Comparison {
                field: build_field_ref(field_name),
                operator: if operator == Operator::NotIn {
                    Operator::NotEqual
                } else {
                    Operator::Equal
                },
                operand: Operand::Value(value.clone()),
            })
        })
        .collect();
    if operator == Operator::NotIn {
        Predicate::And(comparisons)
    } else {
        Predicate::Or(comparisons)
    }
}

/// Check that a field name is non-empty.
fn validate_field_name(field_name: &str, raw: &str) -> Result<(), QueryParseError> {
    if field_name.is_empty() {
        return Err(QueryParseError::UnknownOperator {
            raw: raw.to_owned(),
        });
    }
    Ok(())
}

/// Build a [`FieldReference`] from a validated field name. A single dot
/// splits the name into `relation.field` (forward link, forward links, or
/// inverse — resolved at evaluation time); anything else is a local field.
fn build_field_ref(field_name: &str) -> FieldReference {
    match field_name.split_once('.') {
        Some((relation, field)) => FieldReference::Related {
            relation: relation.to_owned(),
            field: field.to_owned(),
        },
        None => FieldReference::Local(field_name.to_owned()),
    }
}

/// Try to parse a regex expression: `field/pattern/` or `field/pattern/i`.
/// Returns `None` if the input doesn't match the regex syntax.
fn try_parse_regex(input: &str) -> Result<Option<Predicate>, QueryParseError> {
    // Find the first `/` — everything before it is the field name.
    let first_slash = match input.find('/') {
        Some(position) => position,
        None => return Ok(None),
    };

    let field_name = input[..first_slash].trim();
    if field_name.is_empty() {
        return Ok(None);
    }

    let after_first_slash = &input[first_slash + 1..];

    // Find the closing `/`.
    let closing_slash = match after_first_slash.rfind('/') {
        Some(position) => position,
        None => return Ok(None), // No closing slash — not regex syntax
    };

    let pattern = &after_first_slash[..closing_slash];
    let flags = &after_first_slash[closing_slash + 1..];

    // Validate flags: only `i` is allowed.
    if !flags.is_empty() && flags != "i" {
        return Ok(None);
    }

    // The field name must be non-empty; related fields (`parent.title/…/`)
    // are allowed and evaluate against the related item's value.
    validate_field_name(field_name, input)?;

    let regex = QueryRegex::new(pattern, flags).map_err(|error| QueryParseError::InvalidRegex {
        pattern: pattern.to_owned(),
        reason: error.to_string(),
    })?;

    Ok(Some(Predicate::Comparison(Comparison {
        field: build_field_ref(field_name),
        operator: Operator::Matches,
        operand: Operand::Regex(regex),
    })))
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: unwrap a Comparison from a Predicate.
    fn as_comparison(predicate: &Predicate) -> &Comparison {
        match predicate {
            Predicate::Comparison(comparison) => comparison,
            other => panic!("expected Comparison, got {other:?}"),
        }
    }

    fn field_name(comparison: &Comparison) -> String {
        match &comparison.field {
            FieldReference::Local(name) => name.clone(),
            FieldReference::Related { relation, field } => format!("{relation}.{field}"),
        }
    }

    // ── Equality ────────────────────────────────────────────────

    #[test]
    fn parse_equality() {
        let predicate = parse_where("status=open").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "status");
        assert_eq!(comparison.operator, Operator::Equal);
        assert_eq!(comparison.operand.text(), "open");
    }

    // ── Not-equal ───────────────────────────────────────────────

    #[test]
    fn parse_not_equal() {
        let predicate = parse_where("status!=done").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "status");
        assert_eq!(comparison.operator, Operator::NotEqual);
        assert_eq!(comparison.operand.text(), "done");
    }

    // ── Numeric comparisons ─────────────────────────────────────

    #[test]
    fn parse_greater_than() {
        let predicate = parse_where("points>3").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "points");
        assert_eq!(comparison.operator, Operator::GreaterThan);
        assert_eq!(comparison.operand.text(), "3");
    }

    #[test]
    fn parse_less_than() {
        let predicate = parse_where("points<10").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "points");
        assert_eq!(comparison.operator, Operator::LessThan);
        assert_eq!(comparison.operand.text(), "10");
    }

    #[test]
    fn parse_greater_or_equal() {
        let predicate = parse_where("points>=3").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "points");
        assert_eq!(comparison.operator, Operator::GreaterOrEqual);
        assert_eq!(comparison.operand.text(), "3");
    }

    #[test]
    fn parse_less_or_equal() {
        let predicate = parse_where("points<=10").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "points");
        assert_eq!(comparison.operator, Operator::LessOrEqual);
        assert_eq!(comparison.operand.text(), "10");
    }

    // ── Contains ────────────────────────────────────────────────

    #[test]
    fn parse_contains() {
        let predicate = parse_where("title~login").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "title");
        assert_eq!(comparison.operator, Operator::Contains);
        assert_eq!(comparison.operand.text(), "login");
    }

    // ── Regex ───────────────────────────────────────────────────

    #[test]
    fn parse_regex_without_flags() {
        let predicate = parse_where("title/^fix-.*/").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "title");
        assert_eq!(comparison.operator, Operator::Matches);
        assert_eq!(comparison.operand.text(), "/^fix-.*/");
    }

    #[test]
    fn parse_regex_with_case_insensitive_flag() {
        let predicate = parse_where("title/^fix-.*/i").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "title");
        assert_eq!(comparison.operator, Operator::Matches);
        assert_eq!(comparison.operand.text(), "/^fix-.*/i");
    }

    #[test]
    fn parse_regex_invalid_pattern() {
        let result = parse_where("title/[invalid/");
        assert!(matches!(result, Err(QueryParseError::InvalidRegex { .. })));
    }

    /// A regex on a related field is intended, like every other operator on
    /// a related field — the dot traverses, the pattern tests the target.
    #[test]
    fn parse_regex_on_related_field() {
        let predicate = parse_where("parent.title/^fix-/i").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(
            comparison.field,
            FieldReference::Related {
                relation: "parent".to_owned(),
                field: "title".to_owned(),
            }
        );
        assert_eq!(comparison.operator, Operator::Matches);
        assert_eq!(comparison.operand.text(), "/^fix-/i");
    }

    // ── IsSet / IsNotSet ────────────────────────────────────────

    #[test]
    fn parse_is_set() {
        let predicate = parse_where("assignee?").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "assignee");
        assert_eq!(comparison.operator, Operator::IsSet);
    }

    #[test]
    fn parse_is_not_set() {
        let predicate = parse_where("!assignee?").unwrap();
        match &predicate {
            Predicate::Not(inner) => {
                let comparison = as_comparison(inner);
                assert_eq!(field_name(comparison), "assignee");
                assert_eq!(comparison.operator, Operator::IsSet);
            }
            other => panic!("expected Not(Comparison), got {other:?}"),
        }
    }

    // ── In / not in ─────────────────────────────────────────────

    #[test]
    fn parse_in_desugars_to_or_of_equals() {
        let predicate = parse_where("status in open,in_progress").unwrap();
        match &predicate {
            Predicate::Or(predicates) => {
                assert_eq!(predicates.len(), 2);
                let first = as_comparison(&predicates[0]);
                assert_eq!(field_name(first), "status");
                assert_eq!(first.operator, Operator::Equal);
                assert_eq!(first.operand.text(), "open");
                let second = as_comparison(&predicates[1]);
                assert_eq!(field_name(second), "status");
                assert_eq!(second.operator, Operator::Equal);
                assert_eq!(second.operand.text(), "in_progress");
            }
            other => panic!("expected Or, got {other:?}"),
        }
    }

    #[test]
    fn parse_not_in_desugars_to_and_of_not_equals() {
        let predicate = parse_where("status not in done,removed").unwrap();
        match &predicate {
            Predicate::And(predicates) => {
                assert_eq!(predicates.len(), 2);
                for (sub, expected) in predicates.iter().zip(["done", "removed"]) {
                    let comparison = as_comparison(sub);
                    assert_eq!(field_name(comparison), "status");
                    assert_eq!(comparison.operator, Operator::NotEqual);
                    assert_eq!(comparison.operand.text(), expected);
                }
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn parse_in_three_values() {
        let predicate = parse_where("status in open,in_progress,done").unwrap();
        match &predicate {
            Predicate::Or(predicates) => assert_eq!(predicates.len(), 3),
            other => panic!("expected Or, got {other:?}"),
        }
    }

    /// A one-member list still produces the n-ary predicate, so a round-trip
    /// through the clause string cannot downgrade `in` to `=`.
    #[test]
    fn parse_in_single_value_stays_n_ary() {
        match parse_where("status in open").unwrap() {
            Predicate::Or(predicates) => assert_eq!(predicates.len(), 1),
            other => panic!("expected Or, got {other:?}"),
        }
        match parse_where("status not in open").unwrap() {
            Predicate::And(predicates) => assert_eq!(predicates.len(), 1),
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn parse_in_trims_members() {
        let predicate = parse_where("status in open , in_progress").unwrap();
        match &predicate {
            Predicate::Or(predicates) => {
                assert_eq!(as_comparison(&predicates[0]).operand.text(), "open");
                assert_eq!(as_comparison(&predicates[1]).operand.text(), "in_progress");
            }
            other => panic!("expected Or, got {other:?}"),
        }
    }

    #[test]
    fn parse_in_rejects_empty_and_partial_lists() {
        for input in [
            "status in ",
            "status in",
            "status in open,",
            "status in ,",
            "status not in ",
            "status not in",
        ] {
            assert!(
                matches!(
                    parse_where(input),
                    Err(QueryParseError::EmptyValueList { .. })
                ),
                "expected EmptyValueList for '{input}'"
            );
        }
    }

    /// The token needs a boundary on both sides: `status in_progress` names no
    /// operator at all and must not be read as membership.
    #[test]
    fn parse_word_operator_requires_a_trailing_boundary() {
        assert!(matches!(
            parse_where("status in_progress"),
            Err(QueryParseError::UnknownOperator { .. })
        ));
    }

    // ── `=` is always literal ───────────────────────────────────

    /// The whole point of the `in` operator: a comma in an `=` value is data,
    /// not a hidden OR.
    #[test]
    fn parse_equality_with_comma_is_literal() {
        let predicate = parse_where("title=bug, crash").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "title");
        assert_eq!(comparison.operator, Operator::Equal);
        assert_eq!(comparison.operand.text(), "bug, crash");
    }

    /// `=` and `!=` agree about what a comma means.
    #[test]
    fn parse_not_equal_with_comma_is_literal() {
        let comparison_predicate = parse_where("title!=bug, crash").unwrap();
        let comparison = as_comparison(&comparison_predicate);
        assert_eq!(comparison.operator, Operator::NotEqual);
        assert_eq!(comparison.operand.text(), "bug, crash");
    }

    // ── Word-operator precedence ────────────────────────────────

    /// A punctuation operator wins, so the `in` inside a value is data.
    #[test]
    fn parse_punctuation_operator_beats_word_operator() {
        let predicate = parse_where("title=a in b").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "title");
        assert_eq!(comparison.operator, Operator::Equal);
        assert_eq!(comparison.operand.text(), "a in b");
    }

    /// The token is whitespace-delimited, so a field name containing the
    /// letters `in` is not split apart.
    #[test]
    fn parse_field_name_containing_in_is_not_split() {
        let predicate = parse_where("sprint=3").unwrap();
        assert_eq!(field_name(as_comparison(&predicate)), "sprint");

        let predicate = parse_where("sprint in 3,4").unwrap();
        match &predicate {
            Predicate::Or(predicates) => {
                assert_eq!(field_name(as_comparison(&predicates[0])), "sprint");
            }
            other => panic!("expected Or, got {other:?}"),
        }
    }

    /// ` not in ` is matched before ` in `, or the field name absorbs `not`.
    #[test]
    fn parse_not_in_does_not_leave_not_in_the_field_name() {
        match parse_where("status not in done").unwrap() {
            Predicate::And(predicates) => {
                assert_eq!(field_name(as_comparison(&predicates[0])), "status");
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn parse_in_is_case_sensitive() {
        // One spelling only; `IN` is not an operator, so there is nothing to
        // split on.
        assert!(matches!(
            parse_where("status IN open,done"),
            Err(QueryParseError::UnknownOperator { .. })
        ));
    }

    #[test]
    fn parse_related_field_in() {
        let predicate = parse_where("parent.status in open,done").unwrap();
        match &predicate {
            Predicate::Or(predicates) => {
                assert_eq!(predicates.len(), 2);
                for sub in predicates {
                    let comparison = as_comparison(sub);
                    assert!(matches!(&comparison.field, FieldReference::Related { .. }));
                }
            }
            other => panic!("expected Or, got {other:?}"),
        }
    }

    // ── Edge cases ──────────────────────────────────────────────

    #[test]
    fn parse_empty_input() {
        assert!(matches!(parse_where(""), Err(QueryParseError::Empty)));
    }

    #[test]
    fn parse_whitespace_only() {
        assert!(matches!(parse_where("  "), Err(QueryParseError::Empty)));
    }

    #[test]
    fn parse_no_operator() {
        assert!(matches!(
            parse_where("justtext"),
            Err(QueryParseError::UnknownOperator { .. })
        ));
    }

    #[test]
    fn parse_related_field_equality() {
        let predicate = parse_where("parent.status=open").unwrap();
        let comparison = as_comparison(&predicate);
        match &comparison.field {
            FieldReference::Related { relation, field } => {
                assert_eq!(relation, "parent");
                assert_eq!(field, "status");
            }
            other => panic!("expected Related, got {other:?}"),
        }
        assert_eq!(comparison.operator, Operator::Equal);
        assert_eq!(comparison.operand.text(), "open");
    }

    #[test]
    fn parse_related_field_is_set() {
        let predicate = parse_where("parent.status?").unwrap();
        let comparison = as_comparison(&predicate);
        assert!(matches!(&comparison.field, FieldReference::Related { .. }));
        assert_eq!(comparison.operator, Operator::IsSet);
    }

    #[test]
    fn parse_whitespace_around_operator() {
        let predicate = parse_where(" status = open ").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "status");
        assert_eq!(comparison.operand.text(), "open");
    }
}
