//! Shared harness for the editor-JSON-schema drift guards.
//!
//! `schema.schema.json`, `resources.schema.json` and `views.schema.json`
//! are editor-only (ADR-005) — the CLI never loads them, so each one is an
//! independent restatement of what the Rust parser accepts, and each has a
//! test file proving the two still agree. All three do the same three
//! things: compile the schema, assert a YAML fixture validates, assert a
//! malformed one does not. This is that, written once.
//!
//! Every failure message names the schema file, so a red test points at the
//! file to fix rather than at this module.

// Each of the three test binaries includes this module and uses a subset of
// it — `accepts_yaml` is only wanted where a test probes many small
// documents rather than asserting one outcome.
#![allow(dead_code)]

use jsonschema::{Draft, JSONSchema};

/// A compiled editor schema, paired with the file name to blame.
pub struct SchemaGuard {
    file_name: &'static str,
    compiled: JSONSchema,
}

impl SchemaGuard {
    /// Compile one of the shipped `defaults/*.schema.json` files.
    pub fn compile(file_name: &'static str, schema_json: &str) -> Self {
        let schema_value: serde_json::Value = serde_json::from_str(schema_json)
            .unwrap_or_else(|error| panic!("{file_name} must be valid JSON: {error}"));
        let compiled = JSONSchema::options()
            .with_draft(Draft::Draft202012)
            .compile(&schema_value)
            .unwrap_or_else(|error| panic!("{file_name} must be a valid JSON Schema: {error}"));
        Self {
            file_name,
            compiled,
        }
    }

    /// Whether `yaml` validates. For tests that probe a matrix of small
    /// documents and report the disagreements themselves.
    pub fn accepts_yaml(&self, yaml: &str) -> bool {
        self.compiled.is_valid(&yaml_to_json(yaml))
    }

    /// Assert `yaml` validates, listing every violation if it does not.
    pub fn assert_valid(&self, yaml: &str) {
        let value = yaml_to_json(yaml);
        let messages: Vec<String> = match self.compiled.validate(&value) {
            Ok(()) => return,
            Err(errors) => errors
                .map(|error| format!("  at {}: {}", error.instance_path, error))
                .collect(),
        };
        panic!(
            "expected YAML to validate against {}, but got errors:\n{}\nYAML:\n{}",
            self.file_name,
            messages.join("\n"),
            yaml
        );
    }

    /// Assert `yaml` is rejected.
    pub fn assert_invalid(&self, yaml: &str) {
        assert!(
            !self.accepts_yaml(yaml),
            "expected YAML to be rejected by {}, but it validated:\n{yaml}",
            self.file_name,
        );
    }
}

/// Parse a YAML fixture into the JSON value the schema validates against.
pub fn yaml_to_json(yaml: &str) -> serde_json::Value {
    serde_yaml::from_str(yaml).expect("YAML fixture must parse")
}
