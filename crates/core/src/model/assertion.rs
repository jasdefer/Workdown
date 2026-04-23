//! Assertion types for rule requirements.
//!
//! These types represent the `require:` section of a rule in `schema.yaml`.
//! They are data only — evaluation lives in [`crate::rules`].

use serde::Deserialize;

use super::condition::{
    condition_value_from_yaml, negation_from_value, ConditionValue, NegationValue,
};

/// Build an assertion with only `values` set — used for scalar shorthand.
fn scalar_values_assertion(value: ConditionValue) -> Assertion {
    Assertion::Operator(AssertionOperator {
        required: None,
        forbidden: None,
        values: Some(vec![value]),
        not: None,
        eq_field: None,
        lt_field: None,
        lte_field: None,
        gt_field: None,
        gte_field: None,
        min_count: None,
        max_count: None,
    })
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
            other => Ok(scalar_values_assertion(ConditionValue::String(
                other.to_owned(),
            ))),
        },
        serde_yaml::Value::Number(n) => {
            let f = n.as_f64().ok_or("unsupported numeric type in assertion")?;
            Ok(scalar_values_assertion(ConditionValue::Number(f)))
        }
        serde_yaml::Value::Bool(b) => Ok(scalar_values_assertion(ConditionValue::Bool(*b))),
        serde_yaml::Value::Mapping(map) => {
            let op = assertion_operator_from_map(map)?;
            Ok(Assertion::Operator(op))
        }
        _ => Err("assertion must be a scalar, object, or a 'required'/'forbidden' keyword".into()),
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
