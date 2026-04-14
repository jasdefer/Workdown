//! Schema types: field definitions and project configuration.
//!
//! These types are deserialized from `schema.yaml` and represent the
//! project's field configuration. They are data only —
//! the rule engine that *executes* them lives elsewhere (workdown validate).
//!
//! Rule, condition, and assertion types live in their own modules
//! (`model::rule`, `model::condition`, `model::assertion`) and are
//! re-exported here for backwards-compatible imports.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

// Re-export rule-engine types so existing `use crate::model::schema::X` paths keep working.
pub use super::assertion::{Assertion, AssertionOperator};
pub use super::condition::{Condition, ConditionOperator, ConditionValue, NegationValue};
pub use super::rule::{CountConstraint, Rule, Severity};
pub(crate) use super::rule::RawRule;

// ── Top-level schema ──────────────────────────────────────────────────

/// A parsed and validated project schema.
///
/// Produced by [`crate::parser::schema::parse_schema`]. Downstream code
/// can trust that all field definitions are internally consistent and
/// all rule references resolve.
#[derive(Debug, Clone)]
pub struct Schema {
    /// Field definitions, insertion-order preserved (matters for board columns).
    pub fields: IndexMap<String, FieldDefinition>,
    /// Validation rules (cross-field, cross-item, collection-wide).
    pub rules: Vec<Rule>,
    /// Maps inverse names to their original link field names.
    /// E.g., `"children" -> "parent"`. Computed once at schema load time.
    pub inverse_table: HashMap<String, String>,
}

impl Schema {
    /// Build the inverse name table from the schema's link/links field definitions.
    pub fn build_inverse_table(fields: &IndexMap<String, FieldDefinition>) -> HashMap<String, String> {
        let mut table = HashMap::new();
        for (field_name, field_def) in fields {
            if let Some(ref inverse) = field_def.inverse {
                table.insert(inverse.clone(), field_name.clone());
            }
        }
        table
    }
}

/// The raw deserialization target for `schema.yaml`.
/// After serde parsing this goes through semantic validation
/// before becoming a [`Schema`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSchema {
    pub fields: IndexMap<String, FieldDefinition>,
    #[serde(default)]
    pub rules: Vec<RawRule>,
}

// ── Field definitions ─────────────────────────────────────────────────

/// A single field definition from the `fields:` section of `schema.yaml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDefinition {
    /// The built-in type for this field.
    #[serde(rename = "type")]
    pub field_type: FieldType,

    /// Human-readable explanation.
    #[serde(default)]
    pub description: Option<String>,

    /// Whether this field must be present on every work item.
    #[serde(default)]
    pub required: bool,

    /// Default value applied by `workdown add`.
    #[serde(default)]
    pub default: Option<DefaultValue>,

    /// Allowed values. Required for `choice` and `multichoice` types.
    #[serde(default)]
    pub values: Option<Vec<String>>,

    /// Regex pattern the value must match. Only valid for `string` type.
    #[serde(default)]
    pub pattern: Option<String>,

    /// Minimum allowed value. Only valid for `integer` and `float` types.
    #[serde(default)]
    pub min: Option<f64>,

    /// Maximum allowed value. Only valid for `integer` and `float` types.
    #[serde(default)]
    pub max: Option<f64>,

    /// Whether circular references are allowed. Only valid for `link`/`links`.
    #[serde(default)]
    pub allow_cycles: Option<bool>,

    /// Inverse relationship name. Only valid for `link`/`links` types.
    /// Allows dot-notation traversal in rules (e.g. `parent` with `inverse: children`
    /// enables `children.type` references).
    #[serde(default)]
    pub inverse: Option<String>,

    /// Resource section in `resources.yaml` that constrains this field's values.
    #[serde(default)]
    pub resource: Option<String>,

    /// Aggregation config for computed fields.
    #[serde(default)]
    pub aggregate: Option<AggregateConfig>,
}

/// The 10 built-in field types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Choice,
    Multichoice,
    Integer,
    Float,
    Date,
    Boolean,
    List,
    Link,
    Links,
}

impl std::fmt::Display for FieldType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::String => "string",
            Self::Choice => "choice",
            Self::Multichoice => "multichoice",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::Date => "date",
            Self::Boolean => "boolean",
            Self::List => "list",
            Self::Link => "link",
            Self::Links => "links",
        };
        f.write_str(s)
    }
}

// ── Default values ────────────────────────────────────────────────────

/// A default value: either a literal or a generator token (e.g. `$today`).
#[derive(Debug, Clone, PartialEq)]
pub enum DefaultValue {
    /// A literal string value.
    String(std::string::String),
    /// A literal integer value.
    Integer(i64),
    /// A literal float value.
    Float(f64),
    /// A literal boolean value.
    Bool(bool),
    /// A generator applied at `workdown add` time.
    Generator(Generator),
}

/// Built-in generators that produce default values at `workdown add` time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generator {
    /// Filename without `.md` extension.
    Filename,
    /// Prettified filename (hyphens to spaces, title case).
    FilenamePretty,
    /// Random UUID.
    Uuid,
    /// Today's date in `YYYY-MM-DD` format.
    Today,
    /// One more than the current maximum value of this field across all items.
    MaxPlusOne,
}

impl<'de> Deserialize<'de> for DefaultValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        match value {
            serde_yaml::Value::String(s) => match s.as_str() {
                "$filename" => Ok(DefaultValue::Generator(Generator::Filename)),
                "$filename_pretty" => Ok(DefaultValue::Generator(Generator::FilenamePretty)),
                "$uuid" => Ok(DefaultValue::Generator(Generator::Uuid)),
                "$today" => Ok(DefaultValue::Generator(Generator::Today)),
                "$max_plus_one" => Ok(DefaultValue::Generator(Generator::MaxPlusOne)),
                _ => Ok(DefaultValue::String(s)),
            },
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(DefaultValue::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(DefaultValue::Float(f))
                } else {
                    Err(serde::de::Error::custom("unsupported numeric type"))
                }
            }
            serde_yaml::Value::Bool(b) => Ok(DefaultValue::Bool(b)),
            _ => Err(serde::de::Error::custom(
                "default must be a string, number, or boolean",
            )),
        }
    }
}

// ── Aggregate config ──────────────────────────────────────────────────

/// Configuration for a computed/aggregated field.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregateConfig {
    /// The aggregation function.
    pub function: AggregateFunction,

    /// Whether to report an error if a leaf item is missing this field.
    #[serde(default)]
    pub error_on_missing: bool,
}

/// Available aggregation functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AggregateFunction {
    Sum,
    Min,
    Max,
    Average,
    Median,
    Count,
    All,
    Any,
    None,
}

impl std::fmt::Display for AggregateFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Sum => "sum",
            Self::Min => "min",
            Self::Max => "max",
            Self::Average => "average",
            Self::Median => "median",
            Self::Count => "count",
            Self::All => "all",
            Self::Any => "any",
            Self::None => "none",
        };
        f.write_str(s)
    }
}
