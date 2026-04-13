//! Schema types: field definitions, rules, conditions, and assertions.
//!
//! These types are deserialized from `schema.yaml` and represent the
//! project's field configuration and validation rules. They are data only —
//! the rule engine that *executes* them lives elsewhere (workdown validate).

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

// ── Top-level schema ──────────────────────────────────────────────────

/// A parsed and validated project schema.
///
/// Produced by [`crate::parser::schema::parse_schema`]. Downstream code
/// can trust that all field definitions are internally consistent and
/// all rule references resolve.
#[derive(Debug, Clone)]
pub struct Schema {
    /// Field definitions, insertion-order preserved (matters for board columns).
    pub fields: IndexMap<String, FieldDef>,
    /// Validation rules (cross-field, cross-item, collection-wide).
    pub rules: Vec<Rule>,
}

/// The raw deserialization target for `schema.yaml`.
/// After serde parsing this goes through semantic validation
/// before becoming a [`Schema`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawSchema {
    pub fields: IndexMap<String, FieldDef>,
    #[serde(default)]
    pub rules: Vec<RawRule>,
}

// ── Field definitions ─────────────────────────────────────────────────

/// A single field definition from the `fields:` section of `schema.yaml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
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

// ── Conditions ────────────────────────────────────────────────────────

/// A condition that a field must satisfy in the `match:` section.
///
/// Deserialization determines the variant from the YAML value type:
/// - scalar → equality
/// - array → membership (one of)
/// - object → explicit operators
#[derive(Debug, Clone)]
pub enum Condition {
    /// Field must equal this value.
    Equals(ConditionValue),
    /// Field must be one of these values.
    OneOf(Vec<ConditionValue>),
    /// Explicit operators (all must be satisfied).
    Operator(ConditionOperator),
}

/// A primitive value used in conditions and assertions.
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionValue {
    String(std::string::String),
    Number(f64),
    Bool(bool),
}

/// Explicit condition operators.
#[derive(Debug, Clone)]
pub struct ConditionOperator {
    /// Field must not equal this value (or any value in the list).
    pub not: Option<NegationValue>,
    /// `true` = field must be set; `false` = field must be absent/null.
    pub is_set: Option<bool>,
    /// Every related item must satisfy this condition (one-to-many only).
    pub all: Option<Box<Condition>>,
    /// At least one related item must satisfy this condition.
    pub any: Option<Box<Condition>>,
    /// No related item may satisfy this condition.
    pub none: Option<Box<Condition>>,
}

/// Value for a `not` operator — single value or list.
#[derive(Debug, Clone)]
pub enum NegationValue {
    Single(ConditionValue),
    Multiple(Vec<ConditionValue>),
}

impl<'de> Deserialize<'de> for Condition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        condition_from_value(&value).map_err(serde::de::Error::custom)
    }
}

/// Parse a [`Condition`] from a raw YAML value.
fn condition_from_value(value: &serde_yaml::Value) -> Result<Condition, String> {
    match value {
        serde_yaml::Value::String(s) => Ok(Condition::Equals(ConditionValue::String(s.clone()))),
        serde_yaml::Value::Number(n) => {
            let f = n.as_f64().ok_or("unsupported numeric type in condition")?;
            Ok(Condition::Equals(ConditionValue::Number(f)))
        }
        serde_yaml::Value::Bool(b) => Ok(Condition::Equals(ConditionValue::Bool(*b))),
        serde_yaml::Value::Sequence(seq) => {
            let values = seq
                .iter()
                .map(condition_value_from_yaml)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Condition::OneOf(values))
        }
        serde_yaml::Value::Mapping(map) => {
            let op = condition_operator_from_map(map)?;
            Ok(Condition::Operator(op))
        }
        _ => Err("condition must be a scalar, array, or object".into()),
    }
}

fn condition_operator_from_map(map: &serde_yaml::Mapping) -> Result<ConditionOperator, String> {
    let mut op = ConditionOperator {
        not: None,
        is_set: None,
        all: None,
        any: None,
        none: None,
    };

    for (key, value) in map {
        let key_str = key
            .as_str()
            .ok_or("condition operator key must be a string")?;
        match key_str {
            "not" => op.not = Some(negation_from_value(value)?),
            "is_set" => {
                op.is_set = Some(value.as_bool().ok_or("is_set must be a boolean")?);
            }
            "all" => op.all = Some(Box::new(condition_from_value(value)?)),
            "any" => op.any = Some(Box::new(condition_from_value(value)?)),
            "none" => op.none = Some(Box::new(condition_from_value(value)?)),
            other => return Err(format!("unknown condition operator: {other}")),
        }
    }

    Ok(op)
}

fn negation_from_value(value: &serde_yaml::Value) -> Result<NegationValue, String> {
    match value {
        serde_yaml::Value::Sequence(seq) => {
            let values = seq
                .iter()
                .map(condition_value_from_yaml)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(NegationValue::Multiple(values))
        }
        other => {
            let v = condition_value_from_yaml(other)?;
            Ok(NegationValue::Single(v))
        }
    }
}

fn condition_value_from_yaml(value: &serde_yaml::Value) -> Result<ConditionValue, String> {
    match value {
        serde_yaml::Value::String(s) => Ok(ConditionValue::String(s.clone())),
        serde_yaml::Value::Number(n) => {
            let f = n.as_f64().ok_or("unsupported numeric type")?;
            Ok(ConditionValue::Number(f))
        }
        serde_yaml::Value::Bool(b) => Ok(ConditionValue::Bool(*b)),
        _ => Err("value must be a string, number, or boolean".into()),
    }
}

// ── Assertions ────────────────────────────────────────────────────────

/// An assertion that must hold in the `require:` section.
///
/// Deserialization: `"required"` / `"forbidden"` strings are shortcuts;
/// objects use explicit operators.
#[derive(Debug, Clone)]
pub enum Assertion {
    /// Shorthand: field must be set.
    Required,
    /// Shorthand: field must not be set.
    Forbidden,
    /// Explicit operators (all must hold).
    Operator(AssertionOperator),
}

/// Explicit assertion operators.
#[derive(Debug, Clone)]
pub struct AssertionOperator {
    /// Field must be set.
    pub required: Option<bool>,
    /// Field must not be set.
    pub forbidden: Option<bool>,
    /// Field must be one of these values.
    pub values: Option<Vec<ConditionValue>>,
    /// Field must not equal this value (or any in the list).
    pub not: Option<NegationValue>,
    /// Field must equal the referenced field's value.
    pub eq_field: Option<std::string::String>,
    /// Field must be less than the referenced field's value.
    pub lt_field: Option<std::string::String>,
    /// Field must be less than or equal to the referenced field's value.
    pub lte_field: Option<std::string::String>,
    /// Field must be greater than the referenced field's value.
    pub gt_field: Option<std::string::String>,
    /// Field must be greater than or equal to the referenced field's value.
    pub gte_field: Option<std::string::String>,
    /// Related items must number at least this many.
    pub min_count: Option<u32>,
    /// Related items must number at most this many.
    pub max_count: Option<u32>,
}

impl<'de> Deserialize<'de> for Assertion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        assertion_from_value(&value).map_err(serde::de::Error::custom)
    }
}

fn assertion_from_value(value: &serde_yaml::Value) -> Result<Assertion, String> {
    match value {
        serde_yaml::Value::String(s) => match s.as_str() {
            "required" => Ok(Assertion::Required),
            "forbidden" => Ok(Assertion::Forbidden),
            other => Err(format!(
                "unknown assertion keyword: '{other}' (expected 'required' or 'forbidden')"
            )),
        },
        serde_yaml::Value::Mapping(map) => {
            let op = assertion_operator_from_map(map)?;
            Ok(Assertion::Operator(op))
        }
        _ => Err("assertion must be a string keyword or an object".into()),
    }
}

fn assertion_operator_from_map(map: &serde_yaml::Mapping) -> Result<AssertionOperator, String> {
    let mut op = AssertionOperator {
        required: None,
        forbidden: None,
        values: None,
        not: None,
        eq_field: None,
        lt_field: None,
        lte_field: None,
        gt_field: None,
        gte_field: None,
        min_count: None,
        max_count: None,
    };

    for (key, value) in map {
        let key_str = key
            .as_str()
            .ok_or("assertion operator key must be a string")?;
        match key_str {
            "required" => {
                op.required = Some(value.as_bool().ok_or("required must be a boolean")?);
            }
            "forbidden" => {
                op.forbidden = Some(value.as_bool().ok_or("forbidden must be a boolean")?);
            }
            "values" => {
                let seq = value.as_sequence().ok_or("values must be an array")?;
                let values = seq
                    .iter()
                    .map(condition_value_from_yaml)
                    .collect::<Result<Vec<_>, _>>()?;
                op.values = Some(values);
            }
            "not" => op.not = Some(negation_from_value(value)?),
            "eq_field" => {
                op.eq_field = Some(
                    value
                        .as_str()
                        .ok_or("eq_field must be a string")?
                        .to_owned(),
                );
            }
            "lt_field" => {
                op.lt_field = Some(
                    value
                        .as_str()
                        .ok_or("lt_field must be a string")?
                        .to_owned(),
                );
            }
            "lte_field" => {
                op.lte_field = Some(
                    value
                        .as_str()
                        .ok_or("lte_field must be a string")?
                        .to_owned(),
                );
            }
            "gt_field" => {
                op.gt_field = Some(
                    value
                        .as_str()
                        .ok_or("gt_field must be a string")?
                        .to_owned(),
                );
            }
            "gte_field" => {
                op.gte_field = Some(
                    value
                        .as_str()
                        .ok_or("gte_field must be a string")?
                        .to_owned(),
                );
            }
            "min_count" => {
                op.min_count = Some(
                    value
                        .as_u64()
                        .ok_or("min_count must be a non-negative integer")?
                        as u32,
                );
            }
            "max_count" => {
                op.max_count = Some(
                    value
                        .as_u64()
                        .ok_or("max_count must be a non-negative integer")?
                        as u32,
                );
            }
            other => return Err(format!("unknown assertion operator: {other}")),
        }
    }

    Ok(op)
}
