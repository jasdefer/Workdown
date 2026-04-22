//! Schema loader: parse `schema.yaml`, validate, and produce a [`Schema`].
//!
//! The public API is [`parse_schema`] (from a string) and
//! [`load_schema`] / [`load_schema_or_default`] (from disk).

use std::path::Path;

use indexmap::IndexMap;

use crate::model::schema::{
    AggregateFunction, Assertion, Condition, CountConstraint, DefaultValue, FieldDefinition,
    FieldType, FieldTypeConfig, Generator, RawFieldDefinition, RawRule, RawSchema, Rule, Schema,
};

// ── Public API ────────────────────────────────────────────────────────

/// Parse a schema from a YAML string.
///
/// Performs serde deserialization followed by semantic validation.
/// Returns all validation errors at once (does not stop at the first).
pub fn parse_schema(yaml: &str) -> Result<Schema, SchemaLoadError> {
    let raw: RawSchema = serde_yaml::from_str(yaml).map_err(SchemaLoadError::InvalidYaml)?;

    let mut errors = Vec::new();

    // Validate raw field definitions (type-specific properties, defaults, aggregates, inverses).
    validate_fields(&raw.fields, &mut errors);

    // Convert raw fields → typed FieldDefinition with FieldTypeConfig.
    let fields: IndexMap<String, FieldDefinition> = raw
        .fields
        .into_iter()
        .map(|(name, raw_field)| (name, convert_field(raw_field)))
        .collect();

    // Validate rules against the converted fields.
    validate_raw_rules(&raw.rules, &fields, &mut errors);

    if !errors.is_empty() {
        return Err(SchemaLoadError::Validation(errors));
    }

    let rules = raw
        .rules
        .into_iter()
        .map(|r| Rule {
            name: r.name,
            description: r.description,
            severity: r.severity,
            match_conditions: r.match_conditions,
            require: r.require,
            count: r.count,
        })
        .collect();

    let inverse_table = Schema::build_inverse_table(&fields);

    Ok(Schema {
        fields,
        rules,
        inverse_table,
    })
}

/// Load a schema from a file on disk.
pub fn load_schema(path: &Path) -> Result<Schema, SchemaLoadError> {
    let content = std::fs::read_to_string(path).map_err(SchemaLoadError::ReadFailed)?;
    parse_schema(&content)
}

/// Load a schema from a file, falling back to built-in defaults if the
/// file does not exist. Other I/O errors are propagated.
pub fn load_schema_or_default(path: &Path) -> Result<Schema, SchemaLoadError> {
    match std::fs::read_to_string(path) {
        Ok(content) => parse_schema(&content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            parse_schema(include_str!("../../defaults/schema.yaml"))
        }
        Err(e) => Err(SchemaLoadError::ReadFailed(e)),
    }
}

// ── Errors ────────────────────────────────────────────────────────────

/// Errors from loading or validating a schema.
#[derive(Debug, thiserror::Error)]
pub enum SchemaLoadError {
    #[error("failed to read schema file: {0}")]
    ReadFailed(std::io::Error),

    #[error("invalid YAML in schema: {0}")]
    InvalidYaml(serde_yaml::Error),

    #[error("schema validation failed:\n{}", format_validation_errors(.0))]
    Validation(Vec<SchemaValidationError>),
}

/// A single semantic validation error.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{context}: {message}")]
pub struct SchemaValidationError {
    /// Where the error occurred, e.g. `"field 'priority'"` or `"rule 'wip-limit'"`.
    pub context: String,
    /// What went wrong.
    pub message: String,
}

fn format_validation_errors(errors: &[SchemaValidationError]) -> String {
    errors
        .iter()
        .map(|e| format!("  - {e}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn field_error(name: &str, message: impl Into<String>) -> SchemaValidationError {
    SchemaValidationError {
        context: format!("field '{name}'"),
        message: message.into(),
    }
}

fn rule_error(name: &str, message: impl Into<String>) -> SchemaValidationError {
    SchemaValidationError {
        context: format!("rule '{name}'"),
        message: message.into(),
    }
}

// ── Field validation ──────────────────────────────────────────────────

/// Regex for valid field names: lowercase letters/digits/underscores,
/// starting with a letter or underscore.
fn is_valid_field_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Regex for valid rule names: kebab-case, starting with a lowercase letter.
fn is_valid_rule_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Regex for valid field references: `field_name` or `field_name.field_name`.
fn is_valid_field_reference(reference: &str) -> bool {
    let parts: Vec<&str> = reference.split('.').collect();
    match parts.len() {
        1 => is_valid_field_name(parts[0]),
        2 => is_valid_field_name(parts[0]) && is_valid_field_name(parts[1]),
        _ => false,
    }
}

fn validate_fields(
    fields: &IndexMap<String, RawFieldDefinition>,
    errors: &mut Vec<SchemaValidationError>,
) {
    let mut seen_inverses = std::collections::HashMap::new();

    for (name, field) in fields {
        if !is_valid_field_name(name) {
            errors.push(field_error(
                name,
                "field name must be lowercase letters, digits, and underscores, starting with a letter or underscore",
            ));
        }

        validate_type_specific_properties(name, field, errors);
        validate_aggregate_compatibility(name, field, errors);
        validate_default_compatibility(name, field, errors);
        validate_inverse_property(name, field, fields, &mut seen_inverses, errors);
    }
}

/// Validate the `inverse` property on a raw field definition.
fn validate_inverse_property(
    name: &str,
    field: &RawFieldDefinition,
    fields: &IndexMap<String, RawFieldDefinition>,
    seen_inverses: &mut std::collections::HashMap<String, String>,
    errors: &mut Vec<SchemaValidationError>,
) {
    let inverse = match &field.inverse {
        Some(inv) => inv,
        None => return,
    };

    // inverse only valid on link/links
    if field.field_type != FieldType::Link && field.field_type != FieldType::Links {
        errors.push(field_error(
            name,
            "'inverse' is only valid for link and links types",
        ));
        return;
    }

    // inverse name must be a valid identifier
    if !is_valid_field_name(inverse) {
        errors.push(field_error(
            name,
            format!("inverse name '{inverse}' must be lowercase letters, digits, and underscores"),
        ));
    }

    // inverse must not collide with a field name
    if fields.contains_key(inverse.as_str()) {
        errors.push(field_error(
            name,
            format!("inverse name '{inverse}' conflicts with a defined field name"),
        ));
    }

    // inverse names must be unique across all fields
    if let Some(other_field) = seen_inverses.get(inverse.as_str()) {
        errors.push(field_error(
            name,
            format!("inverse name '{inverse}' is already used by field '{other_field}'"),
        ));
    } else {
        seen_inverses.insert(inverse.clone(), name.to_owned());
    }
}

/// Check that only properties valid for the field's type are set.
fn validate_type_specific_properties(
    name: &str,
    field: &RawFieldDefinition,
    errors: &mut Vec<SchemaValidationError>,
) {
    match field.field_type {
        FieldType::Choice | FieldType::Multichoice => {
            // Must have values
            match &field.values {
                None => errors.push(field_error(
                    name,
                    format!("'values' is required for type '{}'", field.field_type),
                )),
                Some(v) if v.is_empty() => {
                    errors.push(field_error(name, "'values' must not be empty"))
                }
                _ => {}
            }
            // Reject invalid properties
            reject_prop(name, "pattern", &field.pattern, field.field_type, errors);
            reject_prop(name, "min", &field.min, field.field_type, errors);
            reject_prop(name, "max", &field.max, field.field_type, errors);
            reject_prop(
                name,
                "allow_cycles",
                &field.allow_cycles,
                field.field_type,
                errors,
            );
            reject_prop(name, "resource", &field.resource, field.field_type, errors);
            reject_prop(
                name,
                "aggregate",
                &field.aggregate,
                field.field_type,
                errors,
            );
            reject_prop(name, "inverse", &field.inverse, field.field_type, errors);
        }
        FieldType::String => {
            reject_prop(name, "values", &field.values, field.field_type, errors);
            reject_prop(name, "min", &field.min, field.field_type, errors);
            reject_prop(name, "max", &field.max, field.field_type, errors);
            reject_prop(
                name,
                "allow_cycles",
                &field.allow_cycles,
                field.field_type,
                errors,
            );
            reject_prop(
                name,
                "aggregate",
                &field.aggregate,
                field.field_type,
                errors,
            );
            reject_prop(name, "inverse", &field.inverse, field.field_type, errors);
        }
        FieldType::Integer | FieldType::Float => {
            reject_prop(name, "values", &field.values, field.field_type, errors);
            reject_prop(name, "pattern", &field.pattern, field.field_type, errors);
            reject_prop(
                name,
                "allow_cycles",
                &field.allow_cycles,
                field.field_type,
                errors,
            );
            reject_prop(name, "resource", &field.resource, field.field_type, errors);
            reject_prop(name, "inverse", &field.inverse, field.field_type, errors);
        }
        FieldType::Date => {
            reject_prop(name, "values", &field.values, field.field_type, errors);
            reject_prop(name, "pattern", &field.pattern, field.field_type, errors);
            reject_prop(name, "min", &field.min, field.field_type, errors);
            reject_prop(name, "max", &field.max, field.field_type, errors);
            reject_prop(
                name,
                "allow_cycles",
                &field.allow_cycles,
                field.field_type,
                errors,
            );
            reject_prop(name, "resource", &field.resource, field.field_type, errors);
            reject_prop(name, "inverse", &field.inverse, field.field_type, errors);
        }
        FieldType::Boolean => {
            reject_prop(name, "values", &field.values, field.field_type, errors);
            reject_prop(name, "pattern", &field.pattern, field.field_type, errors);
            reject_prop(name, "min", &field.min, field.field_type, errors);
            reject_prop(name, "max", &field.max, field.field_type, errors);
            reject_prop(
                name,
                "allow_cycles",
                &field.allow_cycles,
                field.field_type,
                errors,
            );
            reject_prop(name, "resource", &field.resource, field.field_type, errors);
            reject_prop(name, "inverse", &field.inverse, field.field_type, errors);
        }
        FieldType::List => {
            reject_prop(name, "values", &field.values, field.field_type, errors);
            reject_prop(name, "pattern", &field.pattern, field.field_type, errors);
            reject_prop(name, "min", &field.min, field.field_type, errors);
            reject_prop(name, "max", &field.max, field.field_type, errors);
            reject_prop(
                name,
                "allow_cycles",
                &field.allow_cycles,
                field.field_type,
                errors,
            );
            reject_prop(
                name,
                "aggregate",
                &field.aggregate,
                field.field_type,
                errors,
            );
            reject_prop(name, "inverse", &field.inverse, field.field_type, errors);
        }
        FieldType::Link | FieldType::Links => {
            reject_prop(name, "values", &field.values, field.field_type, errors);
            reject_prop(name, "pattern", &field.pattern, field.field_type, errors);
            reject_prop(name, "min", &field.min, field.field_type, errors);
            reject_prop(name, "max", &field.max, field.field_type, errors);
            reject_prop(name, "resource", &field.resource, field.field_type, errors);
            reject_prop(
                name,
                "aggregate",
                &field.aggregate,
                field.field_type,
                errors,
            );
            // Note: inverse IS valid for link/links — no rejection here.
        }
    }
}

/// Helper: push an error if the property is `Some` (i.e. set when it shouldn't be).
fn reject_prop<T: std::fmt::Debug>(
    field_name: &str,
    prop_name: &str,
    value: &Option<T>,
    field_type: FieldType,
    errors: &mut Vec<SchemaValidationError>,
) {
    if value.is_some() {
        errors.push(field_error(
            field_name,
            format!("'{prop_name}' is not valid for type '{field_type}'"),
        ));
    }
}

/// Convert a raw (flat) field definition into a typed [`FieldDefinition`]
/// with a [`FieldTypeConfig`] variant. Called after validation passes,
/// so type-specific fields are guaranteed to be present where required.
fn convert_field(raw: RawFieldDefinition) -> FieldDefinition {
    let type_config = match raw.field_type {
        FieldType::String => FieldTypeConfig::String {
            pattern: raw.pattern,
        },
        FieldType::Choice => FieldTypeConfig::Choice {
            values: raw.values.unwrap_or_default(),
        },
        FieldType::Multichoice => FieldTypeConfig::Multichoice {
            values: raw.values.unwrap_or_default(),
        },
        FieldType::Integer => FieldTypeConfig::Integer {
            min: raw.min,
            max: raw.max,
        },
        FieldType::Float => FieldTypeConfig::Float {
            min: raw.min,
            max: raw.max,
        },
        FieldType::Date => FieldTypeConfig::Date,
        FieldType::Boolean => FieldTypeConfig::Boolean,
        FieldType::List => FieldTypeConfig::List,
        FieldType::Link => FieldTypeConfig::Link {
            allow_cycles: raw.allow_cycles,
            inverse: raw.inverse,
        },
        FieldType::Links => FieldTypeConfig::Links {
            allow_cycles: raw.allow_cycles,
            inverse: raw.inverse,
        },
    };
    FieldDefinition {
        type_config,
        description: raw.description,
        required: raw.required,
        default: raw.default,
        resource: raw.resource,
        aggregate: raw.aggregate,
    }
}

/// Check that the aggregate function is compatible with the field type.
fn validate_aggregate_compatibility(
    name: &str,
    field: &RawFieldDefinition,
    errors: &mut Vec<SchemaValidationError>,
) {
    let agg = match &field.aggregate {
        Some(a) => a,
        None => return,
    };

    let allowed: &[AggregateFunction] = match field.field_type {
        FieldType::Integer | FieldType::Float => &[
            AggregateFunction::Sum,
            AggregateFunction::Min,
            AggregateFunction::Max,
            AggregateFunction::Average,
            AggregateFunction::Median,
            AggregateFunction::Count,
        ],
        FieldType::Date => &[AggregateFunction::Min, AggregateFunction::Max],
        FieldType::Boolean => &[
            AggregateFunction::All,
            AggregateFunction::Any,
            AggregateFunction::None,
        ],
        // Other types can't have aggregate (caught by reject_prop), but
        // guard against it here too.
        _ => {
            // Already reported by type-specific validation; skip to avoid duplicate.
            return;
        }
    };

    if !allowed.contains(&agg.function) {
        let allowed_str: Vec<_> = allowed.iter().map(|f| f.to_string()).collect();
        errors.push(field_error(
            name,
            format!(
                "aggregate function '{}' is not valid for type '{}' (allowed: {})",
                agg.function,
                field.field_type,
                allowed_str.join(", ")
            ),
        ));
    }
}

/// Check that the default value is compatible with the field type.
fn validate_default_compatibility(
    name: &str,
    field: &RawFieldDefinition,
    errors: &mut Vec<SchemaValidationError>,
) {
    let default = match &field.default {
        Some(d) => d,
        None => return,
    };

    match default {
        DefaultValue::Generator(gen) => {
            let compatible = match gen {
                Generator::Filename | Generator::FilenamePretty => {
                    field.field_type == FieldType::String
                }
                Generator::Uuid => field.field_type == FieldType::String,
                Generator::Today => field.field_type == FieldType::Date,
                Generator::MaxPlusOne => {
                    field.field_type == FieldType::Integer || field.field_type == FieldType::Float
                }
            };
            if !compatible {
                let gen_name = match gen {
                    Generator::Filename => "$filename",
                    Generator::FilenamePretty => "$filename_pretty",
                    Generator::Uuid => "$uuid",
                    Generator::Today => "$today",
                    Generator::MaxPlusOne => "$max_plus_one",
                };
                errors.push(field_error(
                    name,
                    format!(
                        "generator '{gen_name}' is not compatible with type '{}'",
                        field.field_type
                    ),
                ));
            }
        }
        DefaultValue::String(s) => match field.field_type {
            FieldType::String | FieldType::Date => {}
            FieldType::Choice | FieldType::Multichoice => {
                if let Some(ref values) = field.values {
                    if !values.contains(s) {
                        errors.push(field_error(
                            name,
                            format!("default '{s}' is not in the allowed values"),
                        ));
                    }
                }
            }
            _ => {
                errors.push(field_error(
                    name,
                    format!(
                        "string default is not compatible with type '{}'",
                        field.field_type
                    ),
                ));
            }
        },
        DefaultValue::Integer(_) => {
            if field.field_type != FieldType::Integer {
                errors.push(field_error(
                    name,
                    format!(
                        "integer default is not compatible with type '{}'",
                        field.field_type
                    ),
                ));
            }
        }
        DefaultValue::Float(_) => {
            if field.field_type != FieldType::Integer && field.field_type != FieldType::Float {
                errors.push(field_error(
                    name,
                    format!(
                        "float default is not compatible with type '{}'",
                        field.field_type
                    ),
                ));
            }
        }
        DefaultValue::Bool(_) => {
            if field.field_type != FieldType::Boolean {
                errors.push(field_error(
                    name,
                    format!(
                        "boolean default is not compatible with type '{}'",
                        field.field_type
                    ),
                ));
            }
        }
    }
}

// ── Rule validation ───────────────────────────────────────────────────

fn validate_raw_rules(
    rules: &[RawRule],
    fields: &IndexMap<String, FieldDefinition>,
    errors: &mut Vec<SchemaValidationError>,
) {
    let mut seen_names = std::collections::HashSet::new();

    for rule in rules {
        // Name format
        if !is_valid_rule_name(&rule.name) {
            errors.push(rule_error(
                &rule.name,
                "rule name must be kebab-case (lowercase letters, digits, hyphens), starting with a letter",
            ));
        }

        // Unique name
        if !seen_names.insert(&rule.name) {
            errors.push(rule_error(&rule.name, "duplicate rule name"));
        }

        // Must have require or count
        if rule.require.is_empty() && rule.count.is_none() {
            errors.push(rule_error(
                &rule.name,
                "rule must have at least 'require' or 'count'",
            ));
        }

        // Count must have min or max
        if let Some(ref count) = rule.count {
            validate_count_constraint(&rule.name, count, errors);
        }

        // Validate field references in match
        for ref_key in rule.match_conditions.keys() {
            validate_field_reference(&rule.name, ref_key, "match", fields, errors);
        }

        // Validate field references in require
        for ref_key in rule.require.keys() {
            validate_field_reference(&rule.name, ref_key, "require", fields, errors);
        }

        // Validate quantifiers are only on one-to-many references
        for (ref_key, condition) in &rule.match_conditions {
            validate_condition_quantifiers(&rule.name, ref_key, condition, fields, errors);
        }

        // Validate assertion quantifiers
        for (ref_key, assertion) in &rule.require {
            validate_assertion_quantifiers(&rule.name, ref_key, assertion, fields, errors);
        }
    }
}

fn validate_count_constraint(
    rule_name: &str,
    count: &CountConstraint,
    errors: &mut Vec<SchemaValidationError>,
) {
    if count.min.is_none() && count.max.is_none() {
        errors.push(rule_error(
            rule_name,
            "'count' must have at least 'min' or 'max'",
        ));
    }
    if let (Some(min), Some(max)) = (count.min, count.max) {
        if min > max {
            errors.push(rule_error(
                rule_name,
                format!("count 'min' ({min}) must not exceed 'max' ({max})"),
            ));
        }
    }
}

/// Check that a field reference resolves against the schema.
fn validate_field_reference(
    rule_name: &str,
    reference: &str,
    section: &str,
    fields: &IndexMap<String, FieldDefinition>,
    errors: &mut Vec<SchemaValidationError>,
) {
    if !is_valid_field_reference(reference) {
        errors.push(rule_error(
            rule_name,
            format!("'{reference}' in '{section}' is not a valid field reference"),
        ));
        return;
    }

    let parts: Vec<&str> = reference.split('.').collect();

    if parts.len() == 1 {
        // Simple field reference — must be a defined field or a defined inverse
        // (bare inverse references are used for min_count/max_count assertions).
        if !fields.contains_key(parts[0]) && !is_defined_inverse(parts[0], fields) {
            errors.push(rule_error(
                rule_name,
                format!("'{reference}' in '{section}' does not match any defined field or inverse"),
            ));
        }
    } else {
        // Dot-notation: first segment must be a link/links field or a known
        // inverse name (e.g. "children" as the inverse of "parent").
        let first = parts[0];
        let second = parts[1];

        if !is_relation_anchor(first, fields) {
            errors.push(rule_error(
                rule_name,
                format!(
                    "'{first}' in '{reference}' ({section}) must be a link/links field or a defined inverse"
                ),
            ));
        }

        // Second segment must be a defined field
        if !fields.contains_key(second) {
            errors.push(rule_error(
                rule_name,
                format!("'{second}' in '{reference}' ({section}) does not match any defined field"),
            ));
        }
    }
}

/// Check if a name is a defined inverse relation in the schema.
///
/// Returns `true` when any link/links field has `inverse: <name>`.
fn is_defined_inverse(name: &str, fields: &IndexMap<String, FieldDefinition>) -> bool {
    fields.values().any(|f| f.inverse() == Some(name))
}

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

/// Check that quantifiers (all/any/none) in conditions are only used
/// on one-to-many traversals.
fn validate_condition_quantifiers(
    rule_name: &str,
    reference: &str,
    condition: &Condition,
    fields: &IndexMap<String, FieldDefinition>,
    errors: &mut Vec<SchemaValidationError>,
) {
    let has_quantifier = match condition {
        Condition::Operator(op) => op.all.is_some() || op.any.is_some() || op.none.is_some(),
        _ => false,
    };

    if has_quantifier && !is_one_to_many_reference(reference, fields) {
        errors.push(rule_error(
            rule_name,
            format!(
                "quantifiers (all/any/none) on '{reference}' require a one-to-many relationship (links field or inverse)"
            ),
        ));
    }
}

/// Check that count-based assertions (min_count/max_count) are only used
/// on one-to-many traversals.
fn validate_assertion_quantifiers(
    rule_name: &str,
    reference: &str,
    assertion: &Assertion,
    fields: &IndexMap<String, FieldDefinition>,
    errors: &mut Vec<SchemaValidationError>,
) {
    let has_count = match assertion {
        Assertion::Operator(op) => op.min_count.is_some() || op.max_count.is_some(),
        _ => false,
    };

    if has_count && !is_one_to_many_reference(reference, fields) {
        errors.push(rule_error(
            rule_name,
            format!("min_count/max_count on '{reference}' require a one-to-many relationship"),
        ));
    }
}

/// Does a field reference point to a one-to-many relationship?
fn is_one_to_many_reference(reference: &str, fields: &IndexMap<String, FieldDefinition>) -> bool {
    let parts: Vec<&str> = reference.split('.').collect();
    let first = parts[0];

    // Bare inverse name (e.g., "children") or dot-notation with inverse prefix
    if is_defined_inverse(first, fields) {
        return true;
    }

    // links field is one-to-many (forward traversal)
    if let Some(field) = fields.get(first) {
        return field.field_type() == FieldType::Links;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::Severity;

    // ── Happy path ────────────────────────────────────────────────

    #[test]
    fn parse_default_schema() {
        let yaml = include_str!("../../defaults/schema.yaml");
        let schema = parse_schema(yaml).expect("default schema should be valid");

        assert!(schema.fields.contains_key("id"));
        assert!(schema.fields.contains_key("status"));
        assert!(schema.fields.contains_key("parent"));

        let status = &schema.fields["status"];
        assert_eq!(status.field_type(), FieldType::Choice);
        assert!(status.required);
        match &status.type_config {
            FieldTypeConfig::Choice { values } => assert_eq!(
                values,
                &["backlog", "open", "in_progress", "review", "done", "closed"]
            ),
            other => panic!("expected Choice, got: {other:?}"),
        }

        let parent = &schema.fields["parent"];
        assert_eq!(parent.field_type(), FieldType::Link);
        match &parent.type_config {
            FieldTypeConfig::Link { allow_cycles, .. } => assert_eq!(*allow_cycles, Some(false)),
            other => panic!("expected Link, got: {other:?}"),
        }
    }

    #[test]
    fn parse_minimal_schema() {
        let yaml = "fields:\n  title:\n    type: string\n";
        let schema = parse_schema(yaml).unwrap();
        assert_eq!(schema.fields.len(), 1);
        assert!(schema.rules.is_empty());
    }

    #[test]
    fn field_order_preserved() {
        let yaml = "\
fields:
  zebra:
    type: string
  alpha:
    type: string
  middle:
    type: string
";
        let schema = parse_schema(yaml).unwrap();
        let names: Vec<&str> = schema.fields.keys().map(|s| s.as_str()).collect();
        assert_eq!(names, vec!["zebra", "alpha", "middle"]);
    }

    #[test]
    fn default_values_parsed() {
        let yaml = "\
fields:
  id:
    type: string
    default: $filename
  created:
    type: date
    default: $today
  count:
    type: integer
    default: 42
  status:
    type: choice
    values: [open, closed]
    default: open
";
        let schema = parse_schema(yaml).unwrap();
        assert_eq!(
            schema.fields["id"].default,
            Some(DefaultValue::Generator(Generator::Filename))
        );
        assert_eq!(
            schema.fields["created"].default,
            Some(DefaultValue::Generator(Generator::Today))
        );
        assert_eq!(
            schema.fields["count"].default,
            Some(DefaultValue::Integer(42))
        );
        assert_eq!(
            schema.fields["status"].default,
            Some(DefaultValue::String("open".into()))
        );
    }

    #[test]
    fn rules_parsed() {
        let yaml = "\
fields:
  status:
    type: choice
    values: [open, in_progress, done]
  assignee:
    type: string
rules:
  - name: in-progress-needs-assignee
    description: Items in progress must have an assignee
    match:
      status: in_progress
    require:
      assignee: required
";
        let schema = parse_schema(yaml).unwrap();
        assert_eq!(schema.rules.len(), 1);
        assert_eq!(schema.rules[0].name, "in-progress-needs-assignee");
        assert_eq!(schema.rules[0].severity, Severity::Error);
    }

    #[test]
    fn count_rule_parsed() {
        let yaml = "\
fields:
  status:
    type: choice
    values: [open, in_progress]
rules:
  - name: wip-limit
    match:
      status: in_progress
    count:
      max: 5
";
        let schema = parse_schema(yaml).unwrap();
        let rule = &schema.rules[0];
        assert!(rule.count.is_some());
        assert_eq!(rule.count.as_ref().unwrap().max, Some(5));
    }

    // ── Field validation errors ───────────────────────────────────

    #[test]
    fn choice_without_values() {
        let yaml = "fields:\n  status:\n    type: choice\n";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.message.contains("'values' is required")));
    }

    #[test]
    fn min_on_string_rejected() {
        let yaml = "\
fields:
  name:
    type: string
    min: 5
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.message.contains("'min' is not valid")));
    }

    #[test]
    fn aggregate_on_link_rejected() {
        let yaml = "\
fields:
  parent:
    type: link
    aggregate:
      function: sum
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.message.contains("'aggregate' is not valid")));
    }

    #[test]
    fn invalid_aggregate_function_for_type() {
        let yaml = "\
fields:
  done:
    type: boolean
    aggregate:
      function: sum
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.message.contains("aggregate function 'sum' is not valid")));
    }

    #[test]
    fn invalid_field_name() {
        let yaml = "fields:\n  Status:\n    type: string\n";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.context.contains("Status") && e.message.contains("field name")));
    }

    #[test]
    fn multiple_errors_collected() {
        let yaml = "\
fields:
  BadName:
    type: choice
  good:
    type: string
    min: 10
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        // Should have at least 2 errors: bad name + choice without values, and min on string
        assert!(
            errors.len() >= 2,
            "expected multiple errors, got: {errors:?}"
        );
    }

    // ── Default compatibility ─────────────────────────────────────

    #[test]
    fn incompatible_default_rejected() {
        let yaml = "\
fields:
  count:
    type: integer
    default: hello
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors.iter().any(|e| e.message.contains("not compatible")));
    }

    #[test]
    fn generator_on_wrong_type_rejected() {
        let yaml = "\
fields:
  count:
    type: integer
    default: $today
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.message.contains("$today") && e.message.contains("not compatible")));
    }

    #[test]
    fn choice_default_not_in_values() {
        let yaml = "\
fields:
  status:
    type: choice
    values: [open, closed]
    default: pending
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.message.contains("not in the allowed values")));
    }

    // ── Rule validation errors ────────────────────────────────────

    #[test]
    fn rule_missing_require_and_count() {
        let yaml = "\
fields:
  status:
    type: choice
    values: [open, closed]
rules:
  - name: empty-rule
    match:
      status: open
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.message.contains("must have at least")));
    }

    #[test]
    fn rule_references_nonexistent_field() {
        let yaml = "\
fields:
  status:
    type: choice
    values: [open, closed]
rules:
  - name: bad-ref
    match:
      nonexistent: open
    require:
      status: required
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors.iter().any(|e| {
            e.message.contains("nonexistent")
                && e.message.contains("does not match any defined field")
        }));
    }

    #[test]
    fn rule_invalid_name() {
        let yaml = "\
fields:
  status:
    type: choice
    values: [open]
rules:
  - name: Bad_Name
    require:
      status: required
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors.iter().any(|e| e.message.contains("kebab-case")));
    }

    #[test]
    fn duplicate_rule_names() {
        let yaml = "\
fields:
  status:
    type: choice
    values: [open]
rules:
  - name: my-rule
    require:
      status: required
  - name: my-rule
    require:
      status: required
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors.iter().any(|e| e.message.contains("duplicate")));
    }

    #[test]
    fn dot_notation_with_non_link_field() {
        let yaml = "\
fields:
  status:
    type: choice
    values: [open]
  title:
    type: string
rules:
  - name: bad-traversal
    match:
      title.status: open
    require:
      status: required
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.message.contains("link/links field or a defined inverse")));
    }

    #[test]
    fn count_without_min_or_max() {
        let yaml = "\
fields:
  status:
    type: choice
    values: [open]
rules:
  - name: bad-count
    match:
      status: open
    count: {}
";
        let err = parse_schema(yaml).unwrap_err();
        let errors = match err {
            SchemaLoadError::Validation(e) => e,
            other => panic!("expected Validation error, got: {other}"),
        };
        assert!(errors
            .iter()
            .any(|e| e.message.contains("'count' must have at least")));
    }

    // ── Serde rejection ───────────────────────────────────────────

    #[test]
    fn unknown_field_type_rejected() {
        let yaml = "fields:\n  x:\n    type: banana\n";
        let err = parse_schema(yaml).unwrap_err();
        assert!(matches!(err, SchemaLoadError::InvalidYaml(_)));
    }

    #[test]
    fn unknown_property_rejected() {
        let yaml = "\
fields:
  status:
    type: string
    unknown_prop: true
";
        let err = parse_schema(yaml).unwrap_err();
        assert!(matches!(err, SchemaLoadError::InvalidYaml(_)));
    }

    #[test]
    fn missing_fields_section() {
        let yaml = "rules: []\n";
        let err = parse_schema(yaml).unwrap_err();
        assert!(matches!(err, SchemaLoadError::InvalidYaml(_)));
    }

    // ── Fallback ──────────────────────────────────────────────────

    #[test]
    fn load_from_nonexistent_path_returns_default() {
        let schema = load_schema_or_default(std::path::Path::new("/nonexistent/schema.yaml"))
            .expect("should fall back to defaults");
        assert!(schema.fields.contains_key("status"));
    }
}
