//! The two spellings every writer of a generic `serde_yaml` tree needs
//! and would otherwise repeat: a mapping key, and a number written the
//! way a person writes it. Shared by the schema write and the rule
//! rows (`crate::operations::schema_write`, `crate::rule_shape`).

/// A string as a mapping key or scalar value.
pub(crate) fn key(key: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(key.to_owned())
}

/// A number as the file spells it: a whole number without a fraction
/// (`3`, not `3.0`), since the parsed model holds every number as a
/// float and `3.0` in the file would not read as the `3` the author
/// wrote; anything else as the float it is. Whole numbers beyond 2^53
/// are not exactly representable as floats and are kept as floats.
pub(crate) fn number(value: f64) -> serde_yaml::Value {
    const EXACT_INTEGER_LIMIT: f64 = 9_007_199_254_740_992.0;
    if value.fract() == 0.0 && value.abs() < EXACT_INTEGER_LIMIT {
        serde_yaml::Value::Number((value as i64).into())
    } else {
        serde_yaml::Value::Number(value.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_numbers_lose_the_fraction_and_others_keep_it() {
        assert_eq!(number(3.0), serde_yaml::Value::Number(3.into()));
        assert_eq!(number(-2.0), serde_yaml::Value::Number((-2).into()));
        assert_eq!(number(2.5), serde_yaml::Value::Number(2.5.into()));
        assert_eq!(number(1e17), serde_yaml::Value::Number(1e17.into()));
    }
}
