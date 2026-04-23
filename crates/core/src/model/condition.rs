//! Condition types for rule matching.
//!
//! These types represent the `match:` section of a rule in `schema.yaml`.
//! They are data only — evaluation lives in [`crate::rules`].

use serde::Deserialize;

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
    /// Set by the schema validator after parsing, when a condition targets a
    /// Date field. Deserialization never produces this variant — it stays a
    /// `String` until the rule/field cross-check can coerce it.
    Date(chrono::NaiveDate),
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

pub(crate) fn negation_from_value(value: &serde_yaml::Value) -> Result<NegationValue, String> {
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

pub(crate) fn condition_value_from_yaml(
    value: &serde_yaml::Value,
) -> Result<ConditionValue, String> {
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
