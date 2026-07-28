//! Drift guard for `crates/core/defaults/resources.schema.json`.
//!
//! ADR-005 keeps the JSON Schema editor-only — the CLI never loads it. That
//! means the schema and the Rust parser (`crates/core/src/parser/resources.rs`)
//! are two independent representations of the same shape. This test compiles
//! the schema and runs it against the default `resources.yaml`, a full
//! example with sections and constants, and a battery of bad shapes to
//! confirm the schema agrees with the parser on what is and is not legal.
//!
//! One deliberate asymmetry: the schema constrains section and constant
//! names to a lowercase pattern, while the parser accepts any string key.
//! The naming convention is editor guidance, not a load-time rule.

use jsonschema::{Draft, JSONSchema};

const SCHEMA_JSON: &str = include_str!("../defaults/resources.schema.json");
const DEFAULT_RESOURCES_YAML: &str = include_str!("../defaults/resources.yaml");

const FULL_EXAMPLE_YAML: &str = r#"
people:
  - id: alice
    name: Alice Smith
    email: alice@example.com
  - id: bob
teams:
  - id: backend
    name: Backend Team
constants:
  daily_rate:
    type: float
    value: 800
  team_size:
    type: integer
    value: 4
  work_hours_per_day:
    type: duration
    value: "8h"
  kickoff:
    type: date
    value: "2026-04-20"
  project_code:
    type: string
    value: WD
  billable:
    type: boolean
    value: true
"#;

// ── Helpers ──────────────────────────────────────────────────────────────

fn compile_schema() -> JSONSchema {
    let schema_value: serde_json::Value =
        serde_json::from_str(SCHEMA_JSON).expect("resources.schema.json must be valid JSON");
    JSONSchema::options()
        .with_draft(Draft::Draft202012)
        .compile(&schema_value)
        .expect("resources.schema.json must be a valid JSON Schema")
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
        "expected YAML to validate against resources.schema.json, but got errors:\n{}\nYAML:\n{}",
        messages.join("\n"),
        yaml
    );
}

fn assert_invalid(schema: &JSONSchema, yaml: &str) {
    let value = yaml_to_json(yaml);
    assert!(
        schema.validate(&value).is_err(),
        "expected YAML to be rejected by resources.schema.json, but it validated:\n{yaml}"
    );
}

// ── Positive cases ───────────────────────────────────────────────────────

#[test]
fn default_resources_yaml_validates() {
    // The shipped default has every entry commented out, so its sections
    // deserialize to null — the schema must accept that shape.
    assert_valid(&compile_schema(), DEFAULT_RESOURCES_YAML);
}

#[test]
fn full_example_validates() {
    assert_valid(&compile_schema(), FULL_EXAMPLE_YAML);
}

#[test]
fn empty_document_validates() {
    assert_valid(&compile_schema(), "# comments only\n");
}

#[test]
fn entry_with_freeform_attributes_validates() {
    let yaml = "\
sprints:
  - id: sprint-1
    name: Sprint 1
    start: 2026-04-01
    velocity: 12
";
    assert_valid(&compile_schema(), yaml);
}

#[test]
fn null_constants_section_validates() {
    assert_valid(&compile_schema(), "constants:\n  # commented out\n");
}

// ── Negative cases: sections ─────────────────────────────────────────────

#[test]
fn entry_missing_id_is_rejected() {
    assert_invalid(&compile_schema(), "people:\n  - name: No Id\n");
}

#[test]
fn section_with_scalar_value_is_rejected() {
    assert_invalid(&compile_schema(), "people: just a string\n");
}

#[test]
fn uppercase_section_name_is_rejected() {
    assert_invalid(&compile_schema(), "People:\n  - id: alice\n");
}

// ── Negative cases: constants ────────────────────────────────────────────

#[test]
fn constant_missing_value_is_rejected() {
    assert_invalid(&compile_schema(), "constants:\n  rate:\n    type: float\n");
}

#[test]
fn constant_missing_type_is_rejected() {
    assert_invalid(&compile_schema(), "constants:\n  rate:\n    value: 800\n");
}

#[test]
fn constant_with_unknown_key_is_rejected() {
    let yaml = "\
constants:
  rate:
    type: float
    value: 800
    description: typo territory
";
    assert_invalid(&compile_schema(), yaml);
}

#[test]
fn constant_with_unsupported_type_is_rejected() {
    let yaml = "constants:\n  status:\n    type: choice\n    value: open\n";
    assert_invalid(&compile_schema(), yaml);
}

#[test]
fn integer_constant_with_string_value_is_rejected() {
    let yaml = "constants:\n  size:\n    type: integer\n    value: four\n";
    assert_invalid(&compile_schema(), yaml);
}

#[test]
fn integer_constant_with_fractional_value_is_rejected() {
    let yaml = "constants:\n  size:\n    type: integer\n    value: 4.5\n";
    assert_invalid(&compile_schema(), yaml);
}

#[test]
fn boolean_constant_with_string_value_is_rejected() {
    let yaml = "constants:\n  billable:\n    type: boolean\n    value: yes please\n";
    assert_invalid(&compile_schema(), yaml);
}

#[test]
fn date_constant_without_date_shape_is_rejected() {
    let yaml = "constants:\n  kickoff:\n    type: date\n    value: April 20th\n";
    assert_invalid(&compile_schema(), yaml);
}

#[test]
fn duration_constant_with_number_value_is_rejected() {
    let yaml = "constants:\n  pace:\n    type: duration\n    value: 8\n";
    assert_invalid(&compile_schema(), yaml);
}

#[test]
fn constants_as_sequence_is_rejected() {
    assert_invalid(&compile_schema(), "constants:\n  - id: rate\n");
}
