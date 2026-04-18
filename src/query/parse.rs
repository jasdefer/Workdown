//! Parser for `--where` CLI expressions.
//!
//! Each `--where` flag carries a single expression string. Multiple flags
//! are combined into [`Predicate::And`] by the command layer — this parser
//! handles one expression at a time.

use crate::query::types::{Comparison, FieldReference, Operator, Predicate};

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
}

// ── Public API ──────────────────────────────────────────────────────

/// Parse a single `--where` expression into a [`Predicate`].
///
/// # Syntax
///
/// | Form | Example | Meaning |
/// |------|---------|---------|
/// | Equality | `status=open` | field equals value |
/// | IN | `status=open,in_progress` | field equals any value |
/// | Not-equal | `status!=done` | field does not equal value |
/// | Greater | `points>3` | numeric/lexicographic greater-than |
/// | Less | `points<10` | numeric/lexicographic less-than |
/// | Greater-or-equal | `points>=3` | numeric/lexicographic >= |
/// | Less-or-equal | `points<=10` | numeric/lexicographic <= |
/// | Contains | `title~login` | substring (strings) or membership (lists) |
/// | Regex | `title/^fix-.*/i` | regex match (optional `i` flag) |
/// | Is-set | `assignee?` | field is present |
/// | Is-not-set | `!assignee?` | field is absent |
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
            return Ok(Predicate::Not(Box::new(Predicate::Comparison(Comparison {
                field: build_field_ref(field_name),
                operator: Operator::IsSet,
                value: String::new(),
            }))));
        }
    }

    // 2. IsSet: field?
    if let Some(field_name) = trimmed.strip_suffix('?') {
        let field_name = field_name.trim();
        validate_field_name(field_name, trimmed)?;
        return Ok(Predicate::Comparison(Comparison {
            field: build_field_ref(field_name),
            operator: Operator::IsSet,
            value: String::new(),
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
                value: value.to_owned(),
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

            // IN syntax: status=open,in_progress → Or of Equals
            if operator == Operator::Equal && value.contains(',') {
                let values: Vec<&str> = value.split(',').collect();
                let comparisons = values
                    .into_iter()
                    .map(|individual_value| {
                        Predicate::Comparison(Comparison {
                            field: build_field_ref(field_name),
                            operator: Operator::Equal,
                            value: individual_value.trim().to_owned(),
                        })
                    })
                    .collect();
                return Ok(Predicate::Or(comparisons));
            }

            return Ok(Predicate::Comparison(Comparison {
                field: build_field_ref(field_name),
                operator,
                value: value.to_owned(),
            }));
        }
    }

    Err(QueryParseError::UnknownOperator {
        raw: trimmed.to_owned(),
    })
}

// ── Helpers ─────────────────────────────────────────────────────────

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

    // Validate the field name (reject related fields).
    validate_field_name(field_name, input)?;

    // Validate the regex pattern by compiling it.
    let test_pattern = if flags == "i" {
        format!("(?i){pattern}")
    } else {
        pattern.to_owned()
    };
    if let Err(error) = regex::Regex::new(&test_pattern) {
        return Err(QueryParseError::InvalidRegex {
            pattern: pattern.to_owned(),
            reason: error.to_string(),
        });
    }

    // Store the full /pattern/flags form so the evaluator can reconstruct it.
    let stored_value = format!("/{pattern}/{flags}");

    Ok(Some(Predicate::Comparison(Comparison {
        field: build_field_ref(field_name),
        operator: Operator::Matches,
        value: stored_value,
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
        assert_eq!(comparison.value, "open");
    }

    // ── Not-equal ───────────────────────────────────────────────

    #[test]
    fn parse_not_equal() {
        let predicate = parse_where("status!=done").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "status");
        assert_eq!(comparison.operator, Operator::NotEqual);
        assert_eq!(comparison.value, "done");
    }

    // ── Numeric comparisons ─────────────────────────────────────

    #[test]
    fn parse_greater_than() {
        let predicate = parse_where("points>3").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "points");
        assert_eq!(comparison.operator, Operator::GreaterThan);
        assert_eq!(comparison.value, "3");
    }

    #[test]
    fn parse_less_than() {
        let predicate = parse_where("points<10").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "points");
        assert_eq!(comparison.operator, Operator::LessThan);
        assert_eq!(comparison.value, "10");
    }

    #[test]
    fn parse_greater_or_equal() {
        let predicate = parse_where("points>=3").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "points");
        assert_eq!(comparison.operator, Operator::GreaterOrEqual);
        assert_eq!(comparison.value, "3");
    }

    #[test]
    fn parse_less_or_equal() {
        let predicate = parse_where("points<=10").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "points");
        assert_eq!(comparison.operator, Operator::LessOrEqual);
        assert_eq!(comparison.value, "10");
    }

    // ── Contains ────────────────────────────────────────────────

    #[test]
    fn parse_contains() {
        let predicate = parse_where("title~login").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "title");
        assert_eq!(comparison.operator, Operator::Contains);
        assert_eq!(comparison.value, "login");
    }

    // ── Regex ───────────────────────────────────────────────────

    #[test]
    fn parse_regex_without_flags() {
        let predicate = parse_where("title/^fix-.*/").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "title");
        assert_eq!(comparison.operator, Operator::Matches);
        assert_eq!(comparison.value, "/^fix-.*/");
    }

    #[test]
    fn parse_regex_with_case_insensitive_flag() {
        let predicate = parse_where("title/^fix-.*/i").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "title");
        assert_eq!(comparison.operator, Operator::Matches);
        assert_eq!(comparison.value, "/^fix-.*/i");
    }

    #[test]
    fn parse_regex_invalid_pattern() {
        let result = parse_where("title/[invalid/");
        assert!(matches!(result, Err(QueryParseError::InvalidRegex { .. })));
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

    // ── IN syntax ───────────────────────────────────────────────

    #[test]
    fn parse_in_syntax() {
        let predicate = parse_where("status=open,in_progress").unwrap();
        match &predicate {
            Predicate::Or(predicates) => {
                assert_eq!(predicates.len(), 2);
                let first = as_comparison(&predicates[0]);
                assert_eq!(field_name(first), "status");
                assert_eq!(first.operator, Operator::Equal);
                assert_eq!(first.value, "open");
                let second = as_comparison(&predicates[1]);
                assert_eq!(field_name(second), "status");
                assert_eq!(second.operator, Operator::Equal);
                assert_eq!(second.value, "in_progress");
            }
            other => panic!("expected Or, got {other:?}"),
        }
    }

    #[test]
    fn parse_in_syntax_three_values() {
        let predicate = parse_where("status=open,in_progress,done").unwrap();
        match &predicate {
            Predicate::Or(predicates) => assert_eq!(predicates.len(), 3),
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
        assert_eq!(comparison.value, "open");
    }

    #[test]
    fn parse_related_field_is_set() {
        let predicate = parse_where("parent.status?").unwrap();
        let comparison = as_comparison(&predicate);
        assert!(matches!(&comparison.field, FieldReference::Related { .. }));
        assert_eq!(comparison.operator, Operator::IsSet);
    }

    #[test]
    fn parse_related_field_in_syntax() {
        let predicate = parse_where("parent.status=open,done").unwrap();
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

    #[test]
    fn parse_whitespace_around_operator() {
        let predicate = parse_where(" status = open ").unwrap();
        let comparison = as_comparison(&predicate);
        assert_eq!(field_name(comparison), "status");
        assert_eq!(comparison.value, "open");
    }
}
