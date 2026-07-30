//! Tokenizer for compute expressions.
//!
//! Produces a flat token list from the source string. The
//! multi-character subtleties: identifiers (with `true` / `false`
//! reserved), numeric literals (integer or fractional, no exponents),
//! quoted string literals (no escapes), two-character comparison
//! operators, and dollar references — `$` is legal solely as the exact
//! keyword `$today` or the start of the exact shape `$constants.<name>`.

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
    /// A quoted string literal (the content, quotes stripped).
    StringLiteral(String),
    /// The reserved word `true`.
    True,
    /// The reserved word `false`.
    False,
    Plus,
    Minus,
    Star,
    Slash,
    LeftParen,
    RightParen,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
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

    #[error("string literal starting at column {column} is missing its closing '\"'")]
    UnterminatedString { column: usize },

    #[error(
        "'{found}' at column {column} is not an operator — equality is '==', inequality is '!='"
    )]
    IncompleteComparison { found: char, column: usize },
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
            b'=' | b'!' => {
                if bytes.get(position + 1) == Some(&b'=') {
                    let kind = if byte == b'=' {
                        TokenKind::EqualEqual
                    } else {
                        TokenKind::BangEqual
                    };
                    tokens.push(Token {
                        kind,
                        span: Span::new(position, position + 2),
                    });
                    position += 2;
                } else {
                    return Err(LexError::IncompleteComparison {
                        found: byte as char,
                        column: position + 1,
                    });
                }
            }
            b'<' | b'>' => {
                let is_or_equal = bytes.get(position + 1) == Some(&b'=');
                let kind = match (byte, is_or_equal) {
                    (b'<', false) => TokenKind::Less,
                    (b'<', true) => TokenKind::LessEqual,
                    (b'>', false) => TokenKind::Greater,
                    (b'>', true) => TokenKind::GreaterEqual,
                    _ => unreachable!("outer match narrowed byte to < or >"),
                };
                let length = if is_or_equal { 2 } else { 1 };
                tokens.push(Token {
                    kind,
                    span: Span::new(position, position + length),
                });
                position += length;
            }
            b'"' => {
                let (token, next) = lex_string_literal(source, position)?;
                tokens.push(token);
                position = next;
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
                // `true` / `false` are reserved words, never field names.
                let kind = match &source[position..end] {
                    "true" => TokenKind::True,
                    "false" => TokenKind::False,
                    word => TokenKind::Identifier(word.to_owned()),
                };
                tokens.push(Token {
                    kind,
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

/// Lex a `"…"` string literal starting at the `"` at `start`. No escape
/// sequences: the literal runs to the next `"`, so a value containing a
/// double quote is not expressible — acceptable for the choice values
/// and color names literals exist to name.
fn lex_string_literal(source: &str, start: usize) -> Result<(Token, usize), LexError> {
    let bytes = source.as_bytes();
    let content_start = start + 1;
    let mut end = content_start;
    while end < bytes.len() && bytes[end] != b'"' {
        end += 1;
    }
    if end >= bytes.len() {
        return Err(LexError::UnterminatedString { column: start + 1 });
    }
    Ok((
        Token {
            kind: TokenKind::StringLiteral(source[content_start..end].to_owned()),
            span: Span::new(start, end + 1),
        },
        end + 1,
    ))
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
    fn lexes_comparison_operators() {
        assert_eq!(
            kinds("a == b != c < d <= e > f >= g"),
            vec![
                TokenKind::Identifier("a".to_owned()),
                TokenKind::EqualEqual,
                TokenKind::Identifier("b".to_owned()),
                TokenKind::BangEqual,
                TokenKind::Identifier("c".to_owned()),
                TokenKind::Less,
                TokenKind::Identifier("d".to_owned()),
                TokenKind::LessEqual,
                TokenKind::Identifier("e".to_owned()),
                TokenKind::Greater,
                TokenKind::Identifier("f".to_owned()),
                TokenKind::GreaterEqual,
                TokenKind::Identifier("g".to_owned()),
            ]
        );
    }

    #[test]
    fn single_equals_and_bang_are_errors_with_guidance() {
        for (source, expected_char) in [("a = b", '='), ("a ! b", '!')] {
            let error = lex(source).unwrap_err();
            assert_eq!(
                error,
                LexError::IncompleteComparison {
                    found: expected_char,
                    column: 3
                },
                "source {source:?}"
            );
        }
    }

    #[test]
    fn lexes_string_literal_without_escapes() {
        assert_eq!(
            kinds("status == \"in progress\""),
            vec![
                TokenKind::Identifier("status".to_owned()),
                TokenKind::EqualEqual,
                TokenKind::StringLiteral("in progress".to_owned()),
            ]
        );
        let tokens = lex("\"done\"").unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 6));
    }

    #[test]
    fn empty_string_literal_lexes() {
        assert_eq!(kinds("\"\""), vec![TokenKind::StringLiteral(String::new())]);
    }

    #[test]
    fn unterminated_string_is_an_error() {
        assert_eq!(
            lex("status == \"done").unwrap_err(),
            LexError::UnterminatedString { column: 11 }
        );
    }

    #[test]
    fn true_and_false_are_reserved_words() {
        assert_eq!(
            kinds("flag == true"),
            vec![
                TokenKind::Identifier("flag".to_owned()),
                TokenKind::EqualEqual,
                TokenKind::True,
            ]
        );
        assert_eq!(kinds("false"), vec![TokenKind::False]);
        // Word boundary: an identifier merely starting with the word
        // stays an identifier.
        assert_eq!(
            kinds("truthy"),
            vec![TokenKind::Identifier("truthy".to_owned())]
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
