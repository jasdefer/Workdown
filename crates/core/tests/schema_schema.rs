//! Drift guard for `crates/core/defaults/schema.schema.json`.
//!
//! ADR-005 keeps the JSON Schema editor-only — the CLI never loads it.
//! That means the schema and the Rust parser
//! (`crates/core/src/parser/schema.rs`) are two independent
//! representations of the same shape. This test compiles the schema and
//! runs it against the default `schema.yaml` plus the constraints the
//! parser enforces that editors must agree on — most importantly the
//! reserved field name `none` (the display roles' no-tint sentinel).

use jsonschema::{Draft, JSONSchema};

const SCHEMA_JSON: &str = include_str!("../defaults/schema.schema.json");
const DEFAULT_SCHEMA_YAML: &str = include_str!("../defaults/schema.yaml");

fn compile_schema() -> JSONSchema {
    let schema_value: serde_json::Value =
        serde_json::from_str(SCHEMA_JSON).expect("schema.schema.json must be valid JSON");
    JSONSchema::options()
        .with_draft(Draft::Draft202012)
        .compile(&schema_value)
        .expect("schema.schema.json must be a valid JSON Schema")
}

fn yaml_to_json(yaml: &str) -> serde_json::Value {
    serde_yaml::from_str(yaml).expect("YAML fixture must parse")
}

fn assert_valid(schema: &JSONSchema, yaml: &str) {
    let value = yaml_to_json(yaml);
    let messages: Vec<String> = match schema.validate(&value) {
        Ok(()) => return,
        Err(errors) => errors
            .map(|error| format!("  at {}: {}", error.instance_path, error))
            .collect(),
    };
    panic!(
        "expected YAML to validate against schema.schema.json, but got errors:\n{}\nYAML:\n{}",
        messages.join("\n"),
        yaml
    );
}

fn assert_invalid(schema: &JSONSchema, yaml: &str) {
    let value = yaml_to_json(yaml);
    assert!(
        schema.validate(&value).is_err(),
        "expected YAML to be rejected by schema.schema.json, but it validated:\n{yaml}"
    );
}

#[test]
fn default_schema_yaml_validates() {
    let schema = compile_schema();
    assert_valid(&schema, DEFAULT_SCHEMA_YAML);
}

#[test]
fn reserved_field_name_none_rejected() {
    // The parser rejects a field named `none` (display roles use it as
    // their no-tint sentinel); editor validation must agree.
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
fields:
  none:
    type: string
",
    );
}

#[test]
fn uppercase_field_name_rejected() {
    let schema = compile_schema();
    assert_invalid(
        &schema,
        "\
fields:
  BadName:
    type: string
",
    );
}

#[test]
fn plain_field_names_still_accepted() {
    // The `none` exclusion must not over-reject: ordinary names — and
    // names merely containing "none" — stay valid.
    let schema = compile_schema();
    assert_valid(
        &schema,
        "\
fields:
  status:
    type: choice
    values: [open, done]
  none_field:
    type: string
",
    );
}
