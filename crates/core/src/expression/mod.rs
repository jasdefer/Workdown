//! Compute expressions: the language behind a field's `compute:` config.
//!
//! An expression combines fields of the same item, project constants,
//! the evaluation date, and numeric literals with `+ - * /` and
//! parentheses:
//!
//! ```text
//! start_date + duration
//! effort * $constants.daily_rate
//! end_date - $today
//! status == "done"
//! end_date - start_date >= duration
//! ```
//!
//! The grammar is deliberately tiny — no logical combinators, no
//! functions, no duration or date literals (named quantities belong in
//! `constants` in `resources.yaml`). String literals are always quoted,
//! so a typo'd field name stays an unknown-field error instead of
//! silently becoming text; `true` / `false` are reserved words. At most
//! one comparison per expression — `a < b < c` is a parse error.
//! `$today` is the one value entering from outside the repository: a
//! date, resolved once per run by the caller and injected through the
//! evaluation context (see ADR-010). What keeps expressions honest is
//! the closed type algebra behind [`check_types`]: every
//! operator/operand-type pairing either has a defined result type
//! (`date - date → duration`, `date > date → boolean`) or is a
//! load-time error.
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
mod evaluate;
mod lexer;
mod parser;
mod typecheck;

pub use ast::{BinaryOperator, ComparisonOperator, Expression, Span};
pub use evaluate::{evaluate, EvaluateError, Value, ValueContext};
pub use lexer::LexError;
pub use parser::{parse_expression, ParseExpressionError};
pub use typecheck::{
    check_types, ExpressionType, ExpressionTypeError, ReferenceResolution, TypeContext,
};
