//! Recursive-descent parser for compute expressions.
//!
//! Grammar (standard arithmetic precedence, `*` `/` over `+` `-`, all
//! left-associative):
//!
//! ```text
//! expression → term  (('+' | '-') term)*
//! term       → unary (('*' | '/') unary)*
//! unary      → '-' unary | primary
//! primary    → integer | float | field | constant | '(' expression ')'
//! ```

use super::ast::{BinaryOperator, Expression, Span};
use super::lexer::{lex, LexError, Token, TokenKind};

/// Errors produced while parsing an expression string.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ParseExpressionError {
    #[error(transparent)]
    Lex(#[from] LexError),

    #[error("expression is empty")]
    Empty,

    #[error("expected {expected} at column {column}, found '{found}'")]
    UnexpectedToken {
        expected: &'static str,
        found: String,
        column: usize,
    },

    #[error("expression ends early — expected {expected}")]
    UnexpectedEnd { expected: &'static str },
}

/// Parse an expression string into an [`Expression`] tree.
pub fn parse_expression(source: &str) -> Result<Expression, ParseExpressionError> {
    let tokens = lex(source)?;
    if tokens.is_empty() {
        return Err(ParseExpressionError::Empty);
    }

    let mut parser = Parser {
        source,
        tokens,
        position: 0,
    };
    let expression = parser.expression()?;

    if let Some(token) = parser.peek() {
        return Err(parser.unexpected("end of expression", token.clone()));
    }
    Ok(expression)
}

struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    position: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn unexpected(&self, expected: &'static str, token: Token) -> ParseExpressionError {
        ParseExpressionError::UnexpectedToken {
            expected,
            found: self.source[token.span.start..token.span.end].to_owned(),
            column: token.span.column(),
        }
    }

    /// `expression → term (('+' | '-') term)*`
    fn expression(&mut self) -> Result<Expression, ParseExpressionError> {
        let mut left = self.term()?;
        while let Some(token) = self.peek() {
            let operator = match token.kind {
                TokenKind::Plus => BinaryOperator::Add,
                TokenKind::Minus => BinaryOperator::Subtract,
                _ => break,
            };
            self.advance();
            let right = self.term()?;
            left = binary(operator, left, right);
        }
        Ok(left)
    }

    /// `term → unary (('*' | '/') unary)*`
    fn term(&mut self) -> Result<Expression, ParseExpressionError> {
        let mut left = self.unary()?;
        while let Some(token) = self.peek() {
            let operator = match token.kind {
                TokenKind::Star => BinaryOperator::Multiply,
                TokenKind::Slash => BinaryOperator::Divide,
                _ => break,
            };
            self.advance();
            let right = self.unary()?;
            left = binary(operator, left, right);
        }
        Ok(left)
    }

    /// `unary → '-' unary | primary`
    fn unary(&mut self) -> Result<Expression, ParseExpressionError> {
        if let Some(token) = self.peek() {
            if token.kind == TokenKind::Minus {
                let minus_span = token.span;
                self.advance();
                let operand = self.unary()?;
                let span = minus_span.merge(operand.span());
                return Ok(Expression::Negate {
                    operand: Box::new(operand),
                    span,
                });
            }
        }
        self.primary()
    }

    /// `primary → integer | float | field | constant | '$today' | '(' expression ')'`
    fn primary(&mut self) -> Result<Expression, ParseExpressionError> {
        const EXPECTED: &str = "a field name, constant, '$today', number, or '('";

        let Some(token) = self.advance() else {
            return Err(ParseExpressionError::UnexpectedEnd { expected: EXPECTED });
        };

        match token.kind {
            TokenKind::Identifier(name) => Ok(Expression::FieldReference {
                name,
                span: token.span,
            }),
            TokenKind::Constant(name) => Ok(Expression::ConstantReference {
                name,
                span: token.span,
            }),
            TokenKind::Today => Ok(Expression::TodayReference { span: token.span }),
            TokenKind::Integer(value) => Ok(Expression::IntegerLiteral {
                value,
                span: token.span,
            }),
            TokenKind::Float(value) => Ok(Expression::FloatLiteral {
                value,
                span: token.span,
            }),
            TokenKind::LeftParen => {
                let inner = self.expression()?;
                match self.advance() {
                    Some(closing) if closing.kind == TokenKind::RightParen => {
                        // The parenthesized span covers both parens, so an
                        // error on the whole group underlines the group.
                        Ok(respan(inner, token.span.merge(closing.span)))
                    }
                    Some(other) => Err(self.unexpected("')'", other)),
                    None => Err(ParseExpressionError::UnexpectedEnd { expected: "')'" }),
                }
            }
            _ => Err(self.unexpected(EXPECTED, token)),
        }
    }
}

fn binary(operator: BinaryOperator, left: Expression, right: Expression) -> Expression {
    let span = left.span().merge(right.span());
    Expression::Binary {
        operator,
        left: Box::new(left),
        right: Box::new(right),
        span,
    }
}

/// Widen `expression`'s span to `span` (used to make a parenthesized group
/// carry the parens' range).
fn respan(expression: Expression, span: Span) -> Expression {
    match expression {
        Expression::FieldReference { name, .. } => Expression::FieldReference { name, span },
        Expression::ConstantReference { name, .. } => Expression::ConstantReference { name, span },
        Expression::TodayReference { .. } => Expression::TodayReference { span },
        Expression::IntegerLiteral { value, .. } => Expression::IntegerLiteral { value, span },
        Expression::FloatLiteral { value, .. } => Expression::FloatLiteral { value, span },
        Expression::Negate { operand, .. } => Expression::Negate { operand, span },
        Expression::Binary {
            operator,
            left,
            right,
            ..
        } => Expression::Binary {
            operator,
            left,
            right,
            span,
        },
    }
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Render a tree as a compact s-expression for readable assertions.
    fn render(expression: &Expression) -> String {
        match expression {
            Expression::FieldReference { name, .. } => name.clone(),
            Expression::ConstantReference { name, .. } => format!("${name}"),
            Expression::TodayReference { .. } => "$today".to_owned(),
            Expression::IntegerLiteral { value, .. } => value.to_string(),
            Expression::FloatLiteral { value, .. } => value.to_string(),
            Expression::Negate { operand, .. } => format!("(neg {})", render(operand)),
            Expression::Binary {
                operator,
                left,
                right,
                ..
            } => format!("({operator} {} {})", render(left), render(right)),
        }
    }

    fn parsed(source: &str) -> String {
        render(&parse_expression(source).unwrap())
    }

    #[test]
    fn parses_simple_addition() {
        assert_eq!(parsed("start_date + duration"), "(+ start_date duration)");
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(parsed("a + b * c"), "(+ a (* b c))");
        assert_eq!(parsed("a * b + c"), "(+ (* a b) c)");
    }

    #[test]
    fn same_precedence_is_left_associative() {
        assert_eq!(parsed("a - b - c"), "(- (- a b) c)");
        assert_eq!(parsed("a / b / c"), "(/ (/ a b) c)");
    }

    #[test]
    fn parentheses_override_precedence() {
        assert_eq!(parsed("(a + b) * c"), "(* (+ a b) c)");
    }

    #[test]
    fn parses_constants_and_literals() {
        assert_eq!(
            parsed("effort * $constants.daily_rate * 1.1"),
            "(* (* effort $daily_rate) 1.1)"
        );
    }

    #[test]
    fn parses_today_reference() {
        assert_eq!(parsed("end_date - $today"), "(- end_date $today)");
        let expression = parse_expression("$today").unwrap();
        assert_eq!(expression.span(), Span::new(0, 6));
        assert!(expression.references_today());
        assert!(expression.field_references().is_empty());
    }

    #[test]
    fn references_today_sees_through_nesting() {
        assert!(parse_expression("-(a + $today)")
            .unwrap()
            .references_today());
        assert!(!parse_expression("a + b").unwrap().references_today());
    }

    #[test]
    fn unary_minus_parses_and_nests() {
        assert_eq!(parsed("-effort"), "(neg effort)");
        assert_eq!(parsed("a * -b"), "(* a (neg b))");
        assert_eq!(parsed("--a"), "(neg (neg a))");
    }

    #[test]
    fn unary_minus_binds_tighter_than_binary_operators() {
        assert_eq!(parsed("-a + b"), "(+ (neg a) b)");
    }

    #[test]
    fn binary_span_covers_both_operands() {
        let expression = parse_expression("ab + cd").unwrap();
        assert_eq!(expression.span(), Span::new(0, 7));
    }

    #[test]
    fn parenthesized_span_covers_the_parens() {
        let expression = parse_expression("(a + b) * c").unwrap();
        let Expression::Binary { left, .. } = &expression else {
            panic!("expected binary");
        };
        assert_eq!(left.span(), Span::new(0, 7));
    }

    #[test]
    fn empty_expression_is_an_error() {
        assert_eq!(
            parse_expression("").unwrap_err(),
            ParseExpressionError::Empty
        );
        assert_eq!(
            parse_expression("  ").unwrap_err(),
            ParseExpressionError::Empty
        );
    }

    #[test]
    fn missing_operand_is_an_error() {
        assert!(matches!(
            parse_expression("a +").unwrap_err(),
            ParseExpressionError::UnexpectedEnd { .. }
        ));
    }

    #[test]
    fn missing_closing_paren_is_an_error() {
        assert!(matches!(
            parse_expression("(a + b").unwrap_err(),
            ParseExpressionError::UnexpectedEnd { expected: "')'" }
        ));
    }

    #[test]
    fn dangling_operator_is_an_error() {
        let error = parse_expression("a + * b").unwrap_err();
        assert!(matches!(
            error,
            ParseExpressionError::UnexpectedToken { column: 5, .. }
        ));
    }

    #[test]
    fn trailing_garbage_is_an_error() {
        let error = parse_expression("a b").unwrap_err();
        assert!(matches!(
            error,
            ParseExpressionError::UnexpectedToken {
                expected: "end of expression",
                column: 3,
                ..
            }
        ));
    }

    #[test]
    fn adjacent_operands_without_operator_are_an_error() {
        assert!(parse_expression("2 3").is_err());
    }

    #[test]
    fn lex_error_passes_through() {
        assert!(matches!(
            parse_expression("a % b").unwrap_err(),
            ParseExpressionError::Lex(_)
        ));
    }
}
