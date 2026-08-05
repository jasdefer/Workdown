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

use crate::expression::Expression;
use crate::model::FieldValue;

// Re-export rule-engine types so existing `use crate::model::schema::X` paths keep working.
pub use super::assertion::{Assertion, AssertionOperator};
pub use super::condition::{Condition, ConditionOperator, ConditionValue, NegationValue};
pub(crate) use super::rule::RawRule;
pub use super::rule::{CountConstraint, Rule, Severity};

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
    pub fn build_inverse_table(
        fields: &IndexMap<String, FieldDefinition>,
    ) -> HashMap<String, String> {
        let mut table = HashMap::new();
        for (field_name, field_def) in fields {
            if let Some(inverse) = field_def.inverse() {
                table.insert(inverse.to_owned(), field_name.clone());
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
    pub fields: IndexMap<String, RawFieldDefinition>,
    #[serde(default)]
    pub rules: Vec<RawRule>,
}

// ── Field definitions ─────────────────────────────────────────────────

/// A validated field definition with type-specific configuration
/// encoded in [`FieldTypeConfig`].
///
/// Produced by converting a `RawFieldDefinition` after schema validation.
/// Invalid states (e.g., a Boolean with `values`) are unrepresentable.
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    /// Type-specific configuration (replaces flat optional fields).
    pub type_config: FieldTypeConfig,

    /// Human-readable explanation.
    pub description: Option<String>,

    /// Whether this field must be present on every work item.
    pub required: bool,

    /// Default value applied by `workdown add`.
    pub default: Option<DefaultValue>,

    /// Resource section in `resources.yaml` that constrains this field's values.
    pub resource: Option<String>,

    /// Aggregation config for aggregated fields (cross-item, same field).
    pub aggregate: Option<AggregateConfig>,

    /// Compute config for computed fields (same item, cross-field).
    pub compute: Option<ComputeConfig>,

    /// Pull config for pull fields (cross-item, cross-field: a
    /// different field read through a forward link, reduced). Mutually
    /// exclusive with `compute` and `when`.
    pub pull: Option<PullConfig>,

    /// Conditional config: derive the value by first matching condition.
    /// Mutually exclusive with `compute`.
    pub when: Option<WhenConfig>,
}

impl FieldDefinition {
    /// Create a new field definition with only type-specific config.
    /// All shared fields default to `None`/`false`.
    pub fn new(type_config: FieldTypeConfig) -> Self {
        Self {
            type_config,
            description: None,
            required: false,
            default: None,
            resource: None,
            aggregate: None,
            compute: None,
            pull: None,
            when: None,
        }
    }

    /// Returns the [`FieldType`] discriminant for this field.
    pub fn field_type(&self) -> FieldType {
        match &self.type_config {
            FieldTypeConfig::String { .. } => FieldType::String,
            FieldTypeConfig::Choice { .. } => FieldType::Choice,
            FieldTypeConfig::Multichoice { .. } => FieldType::Multichoice,
            FieldTypeConfig::Integer { .. } => FieldType::Integer,
            FieldTypeConfig::Float { .. } => FieldType::Float,
            FieldTypeConfig::Date => FieldType::Date,
            FieldTypeConfig::Duration { .. } => FieldType::Duration,
            FieldTypeConfig::Color => FieldType::Color,
            FieldTypeConfig::Boolean => FieldType::Boolean,
            FieldTypeConfig::List => FieldType::List,
            FieldTypeConfig::Link { .. } => FieldType::Link,
            FieldTypeConfig::Links { .. } => FieldType::Links,
        }
    }

    /// Returns the inverse name if this is a Link/Links field with one set.
    pub fn inverse(&self) -> Option<&str> {
        match &self.type_config {
            FieldTypeConfig::Link { inverse, .. } | FieldTypeConfig::Links { inverse, .. } => {
                inverse.as_deref()
            }
            _ => None,
        }
    }

    /// Whether this field's value is derived same-item — by a `compute:`
    /// expression or a `when:` config. (`aggregate` is cross-item and
    /// deliberately not included.)
    pub fn is_derived(&self) -> bool {
        self.compute.is_some() || self.when.is_some()
    }

    /// Names of the fields this field's derivation reads — the compute
    /// expression's references, or every `when:` condition's references
    /// across all branches. In source order, duplicates included; empty
    /// for underived fields. These are the edges of the dependency graph
    /// behind evaluation order and cycle detection.
    pub fn derived_references(&self) -> Vec<&str> {
        if let Some(config) = &self.compute {
            return config.expression.field_references();
        }
        if let Some(when_config) = &self.when {
            return when_config
                .branches
                .iter()
                .flat_map(|branch| branch.condition.field_references())
                .collect();
        }
        Vec::new()
    }
}

/// Per-type configuration for a field definition.
///
/// Each variant carries only the fields that are valid for that type,
/// making invalid combinations unrepresentable.
#[derive(Debug, Clone)]
pub enum FieldTypeConfig {
    String {
        pattern: Option<String>,
    },
    Choice {
        values: Vec<String>,
    },
    Multichoice {
        values: Vec<String>,
    },
    Integer {
        min: Option<f64>,
        max: Option<f64>,
    },
    Float {
        min: Option<f64>,
        max: Option<f64>,
    },
    Date,
    /// A duration field. `min` / `max` are pre-parsed canonical i64
    /// seconds; the schema parser converts the suffix-shorthand string
    /// (`"0s"`, `"4w"`) at load time so coerce-time bounds checks are a
    /// plain integer comparison.
    Duration {
        min: Option<i64>,
        max: Option<i64>,
    },
    /// A color field: hex (`#rgb` / `#rrggbb`) or a built-in palette
    /// name. The palette is hardcoded in [`crate::model::color`] — no
    /// per-field configuration.
    Color,
    Boolean,
    List,
    Link {
        allow_cycles: Option<bool>,
        inverse: Option<String>,
    },
    Links {
        allow_cycles: Option<bool>,
        inverse: Option<String>,
    },
}

/// The raw deserialization target for a single field in `schema.yaml`.
///
/// This flat struct mirrors the YAML layout. After validation it is
/// converted into a [`FieldDefinition`] with a [`FieldTypeConfig`].
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawFieldDefinition {
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

    /// Minimum allowed value. Numeric for `integer`/`float`, suffix-shorthand
    /// duration string for `duration` (parsed type-aware in the validation pass).
    #[serde(default)]
    pub min: Option<serde_yaml::Value>,

    /// Maximum allowed value. Numeric for `integer`/`float`, suffix-shorthand
    /// duration string for `duration` (parsed type-aware in the validation pass).
    #[serde(default)]
    pub max: Option<serde_yaml::Value>,

    /// Whether circular references are allowed. Only valid for `link`/`links`.
    #[serde(default)]
    pub allow_cycles: Option<bool>,

    /// Inverse relationship name. Only valid for `link`/`links` types.
    #[serde(default)]
    pub inverse: Option<String>,

    /// Resource section in `resources.yaml` that constrains this field's values.
    #[serde(default)]
    pub resource: Option<String>,

    /// Aggregation config for aggregated fields.
    #[serde(default)]
    pub aggregate: Option<AggregateConfig>,

    /// Pull config for pull fields. Structured like `aggregate`, so it
    /// deserializes directly; reference resolution happens in
    /// `compute_check`.
    #[serde(default)]
    pub pull: Option<PullConfig>,

    /// Compute config for computed fields. Either an expression string
    /// (`compute: start_date + duration`) or a mapping with options —
    /// kept raw here and interpreted type-aware in the validation pass,
    /// like `min`/`max`.
    #[serde(default)]
    pub compute: Option<serde_yaml::Value>,

    /// Conditional config: a list of `if` / `then` branches. Kept raw
    /// here and attached type-aware after field conversion, because
    /// coercing each `then:` literal needs the typed field config.
    #[serde(default)]
    pub when: Option<serde_yaml::Value>,
}

/// The 12 built-in field types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Choice,
    Multichoice,
    Integer,
    Float,
    Date,
    Duration,
    Color,
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
            Self::Duration => "duration",
            Self::Color => "color",
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

impl Generator {
    /// The `$`-prefixed token this generator is written as in
    /// `schema.yaml` — the inverse of [`DefaultValue`]'s deserializer,
    /// and the form every diagnostic quotes it back in.
    pub fn token(&self) -> &'static str {
        match self {
            Generator::Filename => "$filename",
            Generator::FilenamePretty => "$filename_pretty",
            Generator::Uuid => "$uuid",
            Generator::Today => "$today",
            Generator::MaxPlusOne => "$max_plus_one",
        }
    }
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

/// Configuration for an aggregated field (cross-item, same field —
/// values roll up a link chain).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregateConfig {
    /// The aggregation function.
    pub function: AggregateFunction,

    /// Whether to report an error if a leaf item is missing this field.
    #[serde(default)]
    pub error_on_missing: bool,

    /// Name of the link field to walk upward for the rollup. Must reference
    /// a `link` (single-valued) field in the schema. `None` defaults to
    /// `"parent"` at use sites; the parser still requires that target field
    /// to exist.
    #[serde(default)]
    pub over: Option<String>,
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

/// The aggregate functions defined for values of `field_type`, or
/// `None` when the type cannot be reduced at all. Shared by the
/// aggregate and pull config checks.
pub(crate) fn allowed_aggregate_functions(
    field_type: FieldType,
) -> Option<&'static [AggregateFunction]> {
    match field_type {
        FieldType::Integer | FieldType::Float | FieldType::Duration => Some(&[
            AggregateFunction::Sum,
            AggregateFunction::Min,
            AggregateFunction::Max,
            AggregateFunction::Average,
            AggregateFunction::Median,
            AggregateFunction::Count,
        ]),
        FieldType::Date => Some(&[
            AggregateFunction::Min,
            AggregateFunction::Max,
            AggregateFunction::Average,
        ]),
        FieldType::Boolean => Some(&[
            AggregateFunction::All,
            AggregateFunction::Any,
            AggregateFunction::None,
            AggregateFunction::Count,
        ]),
        _ => None,
    }
}

/// The type `function` produces when reducing values of `input_type`,
/// or `None` when that combination is not defined. Mirrors the actual
/// reductions in `store::rollup::apply_aggregate`: `count` always
/// counts to integer, `average`/`median` of numbers are fractional.
pub(crate) fn aggregate_result_type(
    function: AggregateFunction,
    input_type: FieldType,
) -> Option<FieldType> {
    if !allowed_aggregate_functions(input_type)?.contains(&function) {
        return None;
    }
    match function {
        AggregateFunction::Count => Some(FieldType::Integer),
        AggregateFunction::Sum | AggregateFunction::Min | AggregateFunction::Max => {
            Some(input_type)
        }
        AggregateFunction::Average | AggregateFunction::Median => match input_type {
            FieldType::Integer | FieldType::Float => Some(FieldType::Float),
            other => Some(other),
        },
        AggregateFunction::All | AggregateFunction::Any | AggregateFunction::None => {
            Some(FieldType::Boolean)
        }
    }
}

// ── Pull config ───────────────────────────────────────────────────────

/// Configuration of a pull field (`pull:` in `schema.yaml`): read
/// `field` from the items this item's `over` link points at — forward
/// direction, one hop — and reduce the collected values with
/// `function`. Cross-item and cross-field, the forward counterpart to
/// [`AggregateConfig`]'s reverse-link rollup. Transitivity emerges
/// from recursion (b pulls from a, c pulls from b), never from
/// walking.
///
/// Reference resolution (does `over` name an acyclic link field, does
/// `field` exist, do the types line up) happens in `compute_check`,
/// to the same one-diagnostic-disables-the-field standard as compute
/// expressions.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PullConfig {
    /// The link/links field followed forward. Must declare
    /// `allow_cycles: false` — the pull needs an acyclic dependency
    /// graph to evaluate in.
    pub over: String,

    /// The field read on each linked item.
    pub field: String,

    /// The reduction applied to the collected values.
    pub function: AggregateFunction,

    /// Whether a linked item without the source value gets a
    /// diagnostic instead of the pull silently yielding nothing.
    /// All-or-nothing either way: one incomplete linked item means no
    /// value — a partial reduction would be a silent guess.
    #[serde(default)]
    pub error_on_missing: bool,
}

// ── Compute config ────────────────────────────────────────────────────

/// Configuration for a computed field (same item, cross-field — the
/// value derives from an expression over the item's other fields and
/// project constants).
///
/// Produced by the schema parser from the raw `compute:` value; the
/// expression is already parsed here, but *not* yet type-checked —
/// that needs the constants in `resources.yaml` and happens in
/// `compute_check`.
#[derive(Debug, Clone)]
pub struct ComputeConfig {
    /// The parsed expression tree.
    pub expression: Expression,
    /// The expression exactly as written in `schema.yaml`, kept so
    /// diagnostics can quote it.
    pub source: String,
    /// How a date-valued result with a sub-day remainder lands on a
    /// calendar day.
    pub round: RoundMode,
    /// Whether an item missing an expression input gets a diagnostic
    /// instead of silently lacking the computed value.
    pub error_on_missing: bool,
}

/// Rounding for date-valued compute results with a sub-day remainder.
/// `Floor` means "the last fully-used day", `Ceil` means "the day the
/// work spills into".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundMode {
    #[default]
    Nearest,
    Floor,
    Ceil,
}

/// Configuration of a conditional field (`when:` in `schema.yaml`): the
/// value is picked by the first branch whose condition holds, top to
/// bottom, with an optional evaluated fallback.
///
/// Produced by the schema parser; conditions are parsed but not yet
/// type-checked (that needs `resources.yaml` and happens in
/// `compute_check`, exactly like [`ComputeConfig`] expressions). Like
/// computed values, conditional values are derived at load, never
/// written to files, and lose to a hand-written frontmatter value.
#[derive(Debug, Clone)]
pub struct WhenConfig {
    /// The branches, in declaration order. First match wins.
    pub branches: Vec<WhenBranch>,
    /// The value when no branch matches. Shares the `default:` keyword
    /// with add-time defaults but is *evaluated*, never stamped into a
    /// file — a stamped default would permanently shadow every branch.
    /// `None` leaves the field unset when nothing matches.
    pub default: Option<FieldValue>,
}

/// One branch of a [`WhenConfig`]: a boolean condition and the literal
/// value the field takes when this is the first branch to match.
#[derive(Debug, Clone)]
pub struct WhenBranch {
    /// The parsed condition; must type-check as boolean.
    pub condition: Expression,
    /// The condition exactly as written, kept so diagnostics can quote it.
    pub condition_source: String,
    /// The `then:` literal, already coerced to the field's declared type.
    pub value: FieldValue,
}

// ── Field-map predicates ────────────────────────────────────────────

/// True iff `name` is a valid anchor for a relation traversal — either a
/// forward link/links field, or an inverse name declared by one.
///
/// Shared by schema rule-reference validation (dot-notation left-hand side)
/// and cross-file view validation (`views_check`). Operates on the field map
/// directly because the schema parser runs before `Schema::inverse_table` is
/// built.
pub(crate) fn is_relation_anchor(name: &str, fields: &IndexMap<String, FieldDefinition>) -> bool {
    let is_link_field = fields
        .get(name)
        .is_some_and(|f| matches!(f.field_type(), FieldType::Link | FieldType::Links));
    is_link_field || is_defined_inverse(name, fields)
}

/// True iff `name` is declared as an inverse on any link/links field.
pub(crate) fn is_defined_inverse(name: &str, fields: &IndexMap<String, FieldDefinition>) -> bool {
    fields.values().any(|f| f.inverse() == Some(name))
}
