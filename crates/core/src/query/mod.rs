//! Query engine: filtering, sorting, and listing work items.
//!
//! This module is the shared library layer for querying work items.
//! The CLI `query` command is one consumer; every view's `where:` filter
//! is the other, so a view and a query share one filter semantics.

pub mod clause;
pub mod engine;
pub mod eval;
pub mod format;
pub mod parse;
pub mod sort;
pub mod types;
