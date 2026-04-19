//! Build a dynamic `clap::Command` from a project schema.
//!
//! The `add` command's flags are not known at compile time — they come
//! from the user's runtime `schema.yaml`. This module turns a [`Schema`]
//! into a [`clap::Command`] with one `--<field>` flag per schema field,
//! and converts the resulting [`clap::ArgMatches`] back into a field
//! map (`HashMap<String, serde_yaml::Value>`) that
//! [`crate::commands::add`] consumes.

use std::collections::HashMap;

use clap::builder::{BoolValueParser, PossibleValuesParser, StringValueParser};
use clap::{Arg, ArgAction, Command};

use crate::model::schema::{FieldTypeConfig, Schema};

// ── Public API ───────────────────────────────────────────────────────

/// Build the dynamic `clap::Command` for `workdown add`, with one flag
/// per schema field.
///
/// The returned command has `no_binary_name(true)` set so it can be
/// parsed against a `Vec<String>` that contains only the post-`add`
/// arguments.
pub fn build_add_command(schema: &Schema) -> Command {
    let mut command = Command::new("workdown add")
        .bin_name("workdown add")
        .about("Create a new work item. Fields are derived from schema.yaml.")
        .no_binary_name(true);

    // If the schema defines a field named `help`, clap's auto-help would
    // collide. Let the schema field win.
    if schema.fields.contains_key("help") {
        command = command.disable_help_flag(true);
    }

    for (field_name, field_definition) in &schema.fields {
        let arg = build_field_arg(
            field_name,
            &field_definition.type_config,
            field_definition.description.as_deref(),
        );
        command = command.arg(arg);
    }

    command
}

/// Convert parsed clap matches into a field map consumable by
/// [`crate::commands::add::run_add`].
///
/// Only fields the user actually supplied are inserted. Defaults are
/// applied downstream by the add command itself.
pub fn matches_to_field_map(
    matches: &clap::ArgMatches,
    schema: &Schema,
) -> HashMap<String, serde_yaml::Value> {
    let mut field_map = HashMap::new();

    for (field_name, field_definition) in &schema.fields {
        if let Some(value) = extract_field_value(matches, field_name, &field_definition.type_config)
        {
            field_map.insert(field_name.clone(), value);
        }
    }

    field_map
}

// ── Argument construction ────────────────────────────────────────────

fn build_field_arg(name: &str, type_config: &FieldTypeConfig, description: Option<&str>) -> Arg {
    // clap 4.6's `Str` only implements `From<&'static str>`, not `From<String>`.
    // Leak the string so clap can store a static reference. Schema fields are
    // loaded once per process invocation, so the leak is bounded.
    let leaked_name: &'static str = Box::leak(name.to_owned().into_boxed_str());

    let help = description
        .map(str::to_owned)
        .unwrap_or_else(|| default_help_for_type(type_config));

    let mut arg = Arg::new(leaked_name).long(leaked_name).help(help);

    match type_config {
        FieldTypeConfig::String { .. }
        | FieldTypeConfig::Date
        | FieldTypeConfig::Link { .. } => {
            arg = arg
                .action(ArgAction::Set)
                .value_parser(StringValueParser::new());
        }
        FieldTypeConfig::Integer { .. } => {
            arg = arg
                .action(ArgAction::Set)
                .value_parser(clap::value_parser!(i64));
        }
        FieldTypeConfig::Float { .. } => {
            arg = arg
                .action(ArgAction::Set)
                .value_parser(clap::value_parser!(f64));
        }
        FieldTypeConfig::Boolean => {
            arg = arg
                .action(ArgAction::Set)
                .num_args(0..=1)
                .default_missing_value("true")
                .value_parser(BoolValueParser::new());
        }
        FieldTypeConfig::Choice { values } => {
            arg = arg
                .action(ArgAction::Set)
                .value_parser(PossibleValuesParser::new(leak_values(values)));
        }
        FieldTypeConfig::Multichoice { values } => {
            arg = arg
                .action(ArgAction::Append)
                .value_delimiter(',')
                .value_parser(PossibleValuesParser::new(leak_values(values)));
        }
        FieldTypeConfig::List | FieldTypeConfig::Links { .. } => {
            arg = arg
                .action(ArgAction::Append)
                .value_delimiter(',')
                .value_parser(StringValueParser::new());
        }
    }

    arg
}

/// Leak each string in `values` to satisfy clap's `Str: From<&'static str>` bound.
fn leak_values(values: &[String]) -> Vec<&'static str> {
    values
        .iter()
        .map(|value| {
            let leaked: &'static str = Box::leak(value.clone().into_boxed_str());
            leaked
        })
        .collect()
}

fn default_help_for_type(type_config: &FieldTypeConfig) -> String {
    match type_config {
        FieldTypeConfig::String { .. } => "string".to_owned(),
        FieldTypeConfig::Date => "date (YYYY-MM-DD)".to_owned(),
        FieldTypeConfig::Integer { .. } => "integer".to_owned(),
        FieldTypeConfig::Float { .. } => "float".to_owned(),
        FieldTypeConfig::Boolean => "boolean".to_owned(),
        FieldTypeConfig::Choice { .. } => "one of the allowed choices".to_owned(),
        FieldTypeConfig::Multichoice { .. } => {
            "one or more allowed choices (repeatable, comma-split)".to_owned()
        }
        FieldTypeConfig::List => "list of values (repeatable, comma-split)".to_owned(),
        FieldTypeConfig::Link { .. } => "reference to another work item id".to_owned(),
        FieldTypeConfig::Links { .. } => {
            "references to other work item ids (repeatable, comma-split)".to_owned()
        }
    }
}

// ── Matches extraction ───────────────────────────────────────────────

fn extract_field_value(
    matches: &clap::ArgMatches,
    name: &str,
    type_config: &FieldTypeConfig,
) -> Option<serde_yaml::Value> {
    match type_config {
        FieldTypeConfig::String { .. }
        | FieldTypeConfig::Date
        | FieldTypeConfig::Link { .. }
        | FieldTypeConfig::Choice { .. } => matches
            .get_one::<String>(name)
            .map(|value| serde_yaml::Value::String(value.clone())),

        FieldTypeConfig::Integer { .. } => matches
            .get_one::<i64>(name)
            .map(|value| serde_yaml::Value::Number(serde_yaml::Number::from(*value))),

        FieldTypeConfig::Float { .. } => matches
            .get_one::<f64>(name)
            .and_then(|value| serde_yaml::to_value(*value).ok()),

        FieldTypeConfig::Boolean => matches
            .get_one::<bool>(name)
            .map(|value| serde_yaml::Value::Bool(*value)),

        FieldTypeConfig::Multichoice { .. }
        | FieldTypeConfig::List
        | FieldTypeConfig::Links { .. } => {
            let values: Vec<serde_yaml::Value> = matches
                .get_many::<String>(name)?
                .map(|value| serde_yaml::Value::String(value.trim().to_owned()))
                .collect();
            if values.is_empty() {
                None
            } else {
                Some(serde_yaml::Value::Sequence(values))
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use indexmap::IndexMap;

    fn schema_with(fields: Vec<(&str, FieldDefinition)>) -> Schema {
        let fields: IndexMap<String, FieldDefinition> = fields
            .into_iter()
            .map(|(name, definition)| (name.to_owned(), definition))
            .collect();
        let inverse_table = Schema::build_inverse_table(&fields);
        Schema {
            fields,
            rules: vec![],
            inverse_table,
        }
    }

    fn parse(schema: &Schema, args: &[&str]) -> clap::ArgMatches {
        let command = build_add_command(schema);
        let owned: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
        command.try_get_matches_from(owned).unwrap()
    }

    // ── String ───────────────────────────────────────────────────────

    #[test]
    fn string_field_parsed_and_extracted() {
        let schema = schema_with(vec![(
            "title",
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        )]);
        let matches = parse(&schema, &["--title", "My Task"]);
        let field_map = matches_to_field_map(&matches, &schema);

        assert_eq!(
            field_map.get("title").unwrap(),
            &serde_yaml::Value::String("My Task".to_owned())
        );
    }

    #[test]
    fn missing_string_field_absent_from_map() {
        let schema = schema_with(vec![(
            "title",
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        )]);
        let matches = parse(&schema, &[]);
        let field_map = matches_to_field_map(&matches, &schema);

        assert!(field_map.get("title").is_none());
    }

    // ── Integer ──────────────────────────────────────────────────────

    #[test]
    fn integer_field_parsed_as_number() {
        let schema = schema_with(vec![(
            "priority",
            FieldDefinition::new(FieldTypeConfig::Integer {
                min: None,
                max: None,
            }),
        )]);
        let matches = parse(&schema, &["--priority", "42"]);
        let field_map = matches_to_field_map(&matches, &schema);

        let value = field_map.get("priority").unwrap();
        assert_eq!(value.as_i64().unwrap(), 42);
    }

    #[test]
    fn integer_rejects_non_numeric() {
        let schema = schema_with(vec![(
            "priority",
            FieldDefinition::new(FieldTypeConfig::Integer {
                min: None,
                max: None,
            }),
        )]);
        let command = build_add_command(&schema);
        let result = command.try_get_matches_from(vec!["--priority".to_owned(), "high".to_owned()]);
        assert!(result.is_err());
    }

    // ── Float ────────────────────────────────────────────────────────

    #[test]
    fn float_field_parsed_as_number() {
        let schema = schema_with(vec![(
            "ratio",
            FieldDefinition::new(FieldTypeConfig::Float {
                min: None,
                max: None,
            }),
        )]);
        let matches = parse(&schema, &["--ratio", "3.14"]);
        let field_map = matches_to_field_map(&matches, &schema);

        let value = field_map.get("ratio").unwrap();
        assert!((value.as_f64().unwrap() - 3.14).abs() < 1e-9);
    }

    // ── Boolean ──────────────────────────────────────────────────────

    #[test]
    fn boolean_bare_flag_sets_true() {
        let schema = schema_with(vec![("done", FieldDefinition::new(FieldTypeConfig::Boolean))]);
        let matches = parse(&schema, &["--done"]);
        let field_map = matches_to_field_map(&matches, &schema);

        assert_eq!(field_map.get("done").unwrap(), &serde_yaml::Value::Bool(true));
    }

    #[test]
    fn boolean_explicit_true() {
        let schema = schema_with(vec![("done", FieldDefinition::new(FieldTypeConfig::Boolean))]);
        let matches = parse(&schema, &["--done", "true"]);
        let field_map = matches_to_field_map(&matches, &schema);

        assert_eq!(field_map.get("done").unwrap(), &serde_yaml::Value::Bool(true));
    }

    #[test]
    fn boolean_explicit_false() {
        let schema = schema_with(vec![("done", FieldDefinition::new(FieldTypeConfig::Boolean))]);
        let matches = parse(&schema, &["--done", "false"]);
        let field_map = matches_to_field_map(&matches, &schema);

        assert_eq!(
            field_map.get("done").unwrap(),
            &serde_yaml::Value::Bool(false)
        );
    }

    // ── Choice ───────────────────────────────────────────────────────

    #[test]
    fn choice_field_accepts_allowed_value() {
        let schema = schema_with(vec![(
            "status",
            FieldDefinition::new(FieldTypeConfig::Choice {
                values: vec!["open".into(), "closed".into()],
            }),
        )]);
        let matches = parse(&schema, &["--status", "open"]);
        let field_map = matches_to_field_map(&matches, &schema);

        assert_eq!(
            field_map.get("status").unwrap(),
            &serde_yaml::Value::String("open".into())
        );
    }

    #[test]
    fn choice_field_rejects_invalid_value() {
        let schema = schema_with(vec![(
            "status",
            FieldDefinition::new(FieldTypeConfig::Choice {
                values: vec!["open".into()],
            }),
        )]);
        let command = build_add_command(&schema);
        let result = command.try_get_matches_from(vec!["--status".to_owned(), "bogus".to_owned()]);
        assert!(result.is_err());
    }

    // ── List (repeat and comma) ──────────────────────────────────────

    #[test]
    fn list_field_accepts_repeated_flags() {
        let schema = schema_with(vec![("tags", FieldDefinition::new(FieldTypeConfig::List))]);
        let matches = parse(&schema, &["--tags", "auth", "--tags", "backend"]);
        let field_map = matches_to_field_map(&matches, &schema);

        let sequence = field_map.get("tags").unwrap().as_sequence().unwrap();
        assert_eq!(sequence.len(), 2);
        assert_eq!(sequence[0].as_str().unwrap(), "auth");
        assert_eq!(sequence[1].as_str().unwrap(), "backend");
    }

    #[test]
    fn list_field_accepts_comma_separated() {
        let schema = schema_with(vec![("tags", FieldDefinition::new(FieldTypeConfig::List))]);
        let matches = parse(&schema, &["--tags", "auth,backend"]);
        let field_map = matches_to_field_map(&matches, &schema);

        let sequence = field_map.get("tags").unwrap().as_sequence().unwrap();
        assert_eq!(sequence.len(), 2);
        assert_eq!(sequence[0].as_str().unwrap(), "auth");
        assert_eq!(sequence[1].as_str().unwrap(), "backend");
    }

    #[test]
    fn list_field_trims_whitespace() {
        let schema = schema_with(vec![("tags", FieldDefinition::new(FieldTypeConfig::List))]);
        let matches = parse(&schema, &["--tags", "auth, backend"]);
        let field_map = matches_to_field_map(&matches, &schema);

        let sequence = field_map.get("tags").unwrap().as_sequence().unwrap();
        assert_eq!(sequence[0].as_str().unwrap(), "auth");
        assert_eq!(sequence[1].as_str().unwrap(), "backend");
    }

    #[test]
    fn list_field_combines_repeat_and_comma() {
        let schema = schema_with(vec![("tags", FieldDefinition::new(FieldTypeConfig::List))]);
        let matches = parse(&schema, &["--tags", "a,b", "--tags", "c"]);
        let field_map = matches_to_field_map(&matches, &schema);

        let sequence = field_map.get("tags").unwrap().as_sequence().unwrap();
        assert_eq!(sequence.len(), 3);
        assert_eq!(sequence[0].as_str().unwrap(), "a");
        assert_eq!(sequence[1].as_str().unwrap(), "b");
        assert_eq!(sequence[2].as_str().unwrap(), "c");
    }

    // ── Scalars reject repeat ────────────────────────────────────────

    #[test]
    fn scalar_field_rejects_repeat() {
        let schema = schema_with(vec![(
            "title",
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        )]);
        let command = build_add_command(&schema);
        let result = command.try_get_matches_from(vec![
            "--title".to_owned(),
            "one".to_owned(),
            "--title".to_owned(),
            "two".to_owned(),
        ]);
        assert!(result.is_err());
    }

    // ── Links (list of ids) ──────────────────────────────────────────

    #[test]
    fn links_field_accepts_repeat_and_comma() {
        let schema = schema_with(vec![(
            "depends_on",
            FieldDefinition::new(FieldTypeConfig::Links {
                allow_cycles: None,
                inverse: None,
            }),
        )]);
        let matches = parse(&schema, &["--depends_on", "a,b", "--depends_on", "c"]);
        let field_map = matches_to_field_map(&matches, &schema);

        let sequence = field_map.get("depends_on").unwrap().as_sequence().unwrap();
        assert_eq!(sequence.len(), 3);
    }

    // ── Underscore in field name preserved ───────────────────────────

    #[test]
    fn underscore_field_name_preserved() {
        let schema = schema_with(vec![(
            "depends_on",
            FieldDefinition::new(FieldTypeConfig::Link {
                allow_cycles: None,
                inverse: None,
            }),
        )]);
        // The flag should be --depends_on (not --depends-on).
        let matches = parse(&schema, &["--depends_on", "foo"]);
        let field_map = matches_to_field_map(&matches, &schema);

        assert_eq!(
            field_map.get("depends_on").unwrap(),
            &serde_yaml::Value::String("foo".into())
        );
    }

    // ── Unknown flag rejected ────────────────────────────────────────

    #[test]
    fn unknown_flag_rejected() {
        let schema = schema_with(vec![(
            "title",
            FieldDefinition::new(FieldTypeConfig::String { pattern: None }),
        )]);
        let command = build_add_command(&schema);
        let result = command.try_get_matches_from(vec!["--bogus".to_owned(), "x".to_owned()]);
        assert!(result.is_err());
    }
}
