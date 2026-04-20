//! Rule types: validated and raw rule definitions, severity, and count constraints.
//!
//! These types represent the `rules:` section of `schema.yaml`.
//! They are data only — the rule engine that *executes* them lives in [`crate::rules`].

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::assertion::Assertion;
use super::condition::Condition;

// ── Rules ─────────────────────────────────────────────────────────────

/// A validated rule ready for downstream use.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Unique identifier (kebab-case).
    pub name: std::string::String,
    /// Human-readable explanation, shown in validation output.
    pub description: Option<std::string::String>,
    /// Whether a violation is an error or warning.
    pub severity: Severity,
    /// Conditions that select which work items this rule applies to.
    /// Keys are field references (possibly dot-notation). All must match (AND).
    pub match_conditions: IndexMap<std::string::String, Condition>,
    /// Assertions that must hold for each matching item.
    /// Keys are field references. All must hold (AND).
    pub require: IndexMap<std::string::String, Assertion>,
    /// Collection-wide count constraint on matching items.
    pub count: Option<CountConstraint>,
}

/// Raw deserialization target for a rule before validation.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRule {
    pub name: std::string::String,
    #[serde(default)]
    pub description: Option<std::string::String>,
    #[serde(default)]
    pub severity: Severity,
    #[serde(default, rename = "match")]
    pub match_conditions: IndexMap<std::string::String, Condition>,
    #[serde(default)]
    pub require: IndexMap<std::string::String, Assertion>,
    #[serde(default)]
    pub count: Option<CountConstraint>,
}

/// Rule severity level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[default]
    Error,
    Warning,
}

/// Collection-wide count constraint.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountConstraint {
    /// At least this many items must match.
    #[serde(default)]
    pub min: Option<u32>,
    /// At most this many items may match.
    #[serde(default)]
    pub max: Option<u32>,
}
