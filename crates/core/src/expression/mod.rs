//! Compute expressions: the language behind a field's `compute:` config.
//!
//! An expression combines fields of the same item, project constants, and
//! numeric literals with `+ - * /` and parentheses:
//!
//! ```text
//! start_date + duration
//! effort * $constants.daily_rate
//! effort / duration
//! ```
//!
//! The grammar is deliberately tiny — no conditionals, no functions, no
//! duration or date literals (named quantities belong in `constants` in
//! `resources.yaml`). What keeps expressions honest is the closed type
//! algebra behind [`check_types`]: every operator/operand-type pairing
//! either has a defined result type (`date - date → duration`,
//! `duration / duration → float`) or is a load-time error.
//!
//! Pipeline: [`parse_expression`] turns the source string into an
//! [`Expression`] tree (positions preserved as [`Span`]s so later passes
//! can point errors into the source), then [`check_types`] resolves
//! references through a caller-supplied [`TypeContext`] and infers the
//! result type. Parsing needs no project state; type checking needs the
//! schema *and* resources (constant types), which is why the two are
//! separate passes. Evaluation against a concrete item lives with the
//! store's derive pass, not here.

mod ast;
mod lexer;
mod parser;
mod typecheck;

pub use ast::{BinaryOperator, Expression, Span};
pub use lexer::LexError;
pub use parser::{parse_expression, ParseExpressionError};
pub use typecheck::{
    check_types, ExpressionType, ExpressionTypeError, ReferenceResolution, TypeContext,
};
