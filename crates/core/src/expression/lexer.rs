//! Tokenizer for compute expressions.
//!
//! Produces a flat token list from the source string. The only
//! multi-character subtleties are identifiers, numeric literals (integer
//! or fractional, no exponents), and dollar references — `$` is legal
//! solely as the exact keyword `$today` or the start of the exact shape
//! `$constants.<name>`.

use super::ast::Span;

/// One lexical token with its source range.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Token {
    pub(super) kind: TokenKind,
    pub(super) span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum TokenKind {
    /// A field name, e.g. `start_date`.
    Identifier(String),
    /// A constant reference, e.g. `$constants.daily_rate` (the name only).
    Constant(String),
    /// The evaluation date keyword, `$today`.
    Today,
    /// A whole-number literal.
    Integer(i64),
    /// A fractional literal.
    Float(f64),
    Plus,
    Minus,
    Star,
    Slash,
    LeftParen,
    RightParen,
}

/// Errors produced while tokenizing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LexError {
    #[error("unexpected character '{character}' at column {column}")]
    UnexpectedCharacter { character: char, column: usize },

    #[error(
        "'$' at column {column} must be '$today' or start a constant reference of the form $constants.<name>"
    )]
    MalformedDollarReference { column: usize },

    #[error("number '{literal}' at column {column} is out of range")]
    NumberOutOfRange { literal: String, column: usize },

    #[error("number '{literal}' at column {column} is malformed (a fraction needs digits after the dot)")]
    MalformedNumber { literal: String, column: usize },
}

/// The exact prefix a constant reference must carry.
const CONSTANT_PREFIX: &str = "constants.";

/// The keyword (after `$`) that references the evaluation date.
const TODAY_KEYWORD: &str = "today";

pub(super) fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut position = 0;

    while position < bytes.len() {
        let byte = bytes[position];
        match byte {
            b' ' | b'\t' | b'\n' | b'\r' => {
                position += 1;
            }
            b'+' => {
                tokens.push(single(TokenKind::Plus, position));
                position += 1;
            }
            b'-' => {
                tokens.push(single(TokenKind::Minus, position));
                position += 1;
            }
            b'*' => {
                tokens.push(single(TokenKind::Star, position));
                position += 1;
            }
            b'/' => {
                tokens.push(single(TokenKind::Slash, position));
                position += 1;
            }
            b'(' => {
                tokens.push(single(TokenKind::LeftParen, position));
                position += 1;
            }
            b')' => {
                tokens.push(single(TokenKind::RightParen, position));
                position += 1;
            }
            b'0'..=b'9' => {
                let (token, next) = lex_number(source, position)?;
                tokens.push(token);
                position = next;
            }
            b'$' => {
                let (token, next) = lex_dollar_reference(source, position)?;
                tokens.push(token);
                position = next;
            }
            _ if is_identifier_start(byte) => {
                let end = identifier_end(bytes, position);
                tokens.push(Token {
                    kind: TokenKind::Identifier(source[position..end].to_owned()),
                    span: Span::new(position, end),
                });
                position = end;
            }
            _ => {
                // Take a whole char, not a byte, so multi-byte characters
                // render correctly in the message.
                let character = source[position..].chars().next().expect("in bounds");
                return Err(LexError::UnexpectedCharacter {
                    character,
                    column: position + 1,
                });
            }
        }
    }

    Ok(tokens)
}

fn single(kind: TokenKind, position: usize) -> Token {
    Token {
        kind,
        span: Span::new(position, position + 1),
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// End (exclusive) of the identifier starting at `start`.
fn identifier_end(bytes: &[u8], start: usize) -> usize {
    let mut end = start + 1;
    while end < bytes.len() && is_identifier_continue(bytes[end]) {
        end += 1;
    }
    end
}

/// Lex an integer or fractional literal starting at `start`. No leading
/// sign (the parser handles unary minus) and no exponent notation.
fn lex_number(source: &str, start: usize) -> Result<(Token, usize), LexError> {
    let bytes = source.as_bytes();
    let mut end = start;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }

    let is_fraction = end < bytes.len() && bytes[end] == b'.';
    if is_fraction {
        end += 1;
        let fraction_start = end;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
        }
        if end == fraction_start {
            return Err(LexError::MalformedNumber {
                literal: source[start..end].to_owned(),
                column: start + 1,
            });
        }
    }

    let literal = &source[start..end];
    let span = Span::new(start, end);
    let kind = if is_fraction {
        let value = literal
            .parse::<f64>()
            .map_err(|_| LexError::NumberOutOfRange {
                literal: literal.to_owned(),
                column: start + 1,
            })?;
        TokenKind::Float(value)
    } else {
        let value = literal
            .parse::<i64>()
            .map_err(|_| LexError::NumberOutOfRange {
                literal: literal.to_owned(),
                column: start + 1,
            })?;
        TokenKind::Integer(value)
    };

    Ok((Token { kind, span }, end))
}

/// Lex a dollar reference starting at the `$` at `start`: the `$today`
/// keyword, or a `$constants.<name>` constant reference.
fn lex_dollar_reference(source: &str, start: usize) -> Result<(Token, usize), LexError> {
    let malformed = || LexError::MalformedDollarReference { column: start + 1 };

    let after_dollar = start + 1;
    let rest = &source[after_dollar..];
    let bytes = source.as_bytes();

    // `$today` must end at a word boundary so `$todays` stays an error
    // instead of lexing as `$today` plus a stray identifier.
    if rest.starts_with(TODAY_KEYWORD) {
        let end = after_dollar + TODAY_KEYWORD.len();
        if end >= bytes.len() || !is_identifier_continue(bytes[end]) {
            return Ok((
                Token {
                    kind: TokenKind::Today,
                    span: Span::new(start, end),
                },
                end,
            ));
        }
    }

    if !rest.starts_with(CONSTANT_PREFIX) {
        return Err(malformed());
    }

    let name_start = after_dollar + CONSTANT_PREFIX.len();
    if name_start >= bytes.len() || !is_identifier_start(bytes[name_start]) {
        return Err(malformed());
    }

    let end = identifier_end(bytes, name_start);
    Ok((
        Token {
            kind: TokenKind::Constant(source[name_start..end].to_owned()),
            span: Span::new(start, end),
        },
        end,
    ))
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source)
            .unwrap()
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_fields_operators_and_whitespace() {
        assert_eq!(
            kinds("start_date + duration"),
            vec![
                TokenKind::Identifier("start_date".to_owned()),
                TokenKind::Plus,
                TokenKind::Identifier("duration".to_owned()),
            ]
        );
    }

    #[test]
    fn lexes_all_operators_and_parens() {
        assert_eq!(
            kinds("(a - b) * c / d"),
            vec![
                TokenKind::LeftParen,
                TokenKind::Identifier("a".to_owned()),
                TokenKind::Minus,
                TokenKind::Identifier("b".to_owned()),
                TokenKind::RightParen,
                TokenKind::Star,
                TokenKind::Identifier("c".to_owned()),
                TokenKind::Slash,
                TokenKind::Identifier("d".to_owned()),
            ]
        );
    }

    #[test]
    fn lexes_integer_and_float_literals() {
        assert_eq!(
            kinds("2 * 1.5"),
            vec![
                TokenKind::Integer(2),
                TokenKind::Star,
                TokenKind::Float(1.5),
            ]
        );
    }

    #[test]
    fn lexes_constant_reference() {
        assert_eq!(
            kinds("effort * $constants.daily_rate"),
            vec![
                TokenKind::Identifier("effort".to_owned()),
                TokenKind::Star,
                TokenKind::Constant("daily_rate".to_owned()),
            ]
        );
    }

    #[test]
    fn constant_span_covers_dollar_to_name_end() {
        let tokens = lex("$constants.rate").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 15));
    }

    #[test]
    fn spans_are_byte_ranges_into_the_source() {
        let tokens = lex("ab + c").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 2));
        assert_eq!(tokens[1].span, Span::new(3, 4));
        assert_eq!(tokens[2].span, Span::new(5, 6));
    }

    #[test]
    fn dollar_without_today_or_constants_prefix_is_an_error() {
        for source in ["$rate", "$const.rate", "$constants", "$constants.", "$"] {
            let error = lex(source).unwrap_err();
            assert!(
                matches!(error, LexError::MalformedDollarReference { column: 1 }),
                "source {source:?} produced {error:?}"
            );
        }
    }

    #[test]
    fn lexes_today_keyword() {
        assert_eq!(
            kinds("end_date - $today"),
            vec![
                TokenKind::Identifier("end_date".to_owned()),
                TokenKind::Minus,
                TokenKind::Today,
            ]
        );
    }

    #[test]
    fn today_span_covers_dollar_to_keyword_end() {
        let tokens = lex("$today").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 6));
    }

    #[test]
    fn today_requires_a_word_boundary() {
        // `$todays` must not lex as `$today` plus a stray identifier.
        for source in ["$todays", "$today_", "$today2"] {
            let error = lex(source).unwrap_err();
            assert!(
                matches!(error, LexError::MalformedDollarReference { column: 1 }),
                "source {source:?} produced {error:?}"
            );
        }
    }

    #[test]
    fn today_followed_by_an_operator_lexes_cleanly() {
        assert_eq!(
            kinds("$today+1"),
            vec![TokenKind::Today, TokenKind::Plus, TokenKind::Integer(1)]
        );
    }

    #[test]
    fn number_with_trailing_dot_is_an_error() {
        let error = lex("1. + a").unwrap_err();
        assert!(matches!(error, LexError::MalformedNumber { .. }));
    }

    #[test]
    fn integer_overflow_is_an_error() {
        let error = lex("99999999999999999999").unwrap_err();
        assert!(matches!(error, LexError::NumberOutOfRange { .. }));
    }

    #[test]
    fn unexpected_character_reports_column() {
        let error = lex("a % b").unwrap_err();
        assert_eq!(
            error,
            LexError::UnexpectedCharacter {
                character: '%',
                column: 3
            }
        );
    }

    #[test]
    fn empty_source_lexes_to_no_tokens() {
        assert!(lex("").unwrap().is_empty());
        assert!(lex("   ").unwrap().is_empty());
    }
}
