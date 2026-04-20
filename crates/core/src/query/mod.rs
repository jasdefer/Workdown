//! Query engine: filtering, sorting, and listing work items.
//!
//! This module is the shared library layer for querying work items.
//! The CLI `query` command is one consumer; future commands (board, tree,
//! graph) reuse the engine programmatically.

pub mod engine;
pub mod eval;
pub mod format;
pub mod parse;
pub mod sort;
pub mod types;
