//! Integration tests for `workdown_core::schema_definition_data::load`,
//! the entry point behind `GET /api/schema/definition`.
//!
//! The projection's input shapes (every field shape, default kind,
//! derived block) are pinned by the module's unit tests. This file
//! covers what only a real file on disk shows: the happy path through
//! the read, and the two ways the read fails — no file, and a file the
//! schema parser rejects — each worded as the loader words it.

use std::path::PathBuf;

use tempfile::TempDir;
use workdown_core::project::LoadError;
use workdown_core::schema_definition_data::{content_hash, load, FieldShape};

const SCHEMA_YAML: &str = "\
fields:
  id:
    type: string
    default: $filename
  status:
    type: choice
    values: [open, done]
    default: open
  parent:
    type: link
    allow_cycles: false
    inverse: children
";

fn write_schema(directory: &TempDir, yaml: &str) -> PathBuf {
    let path = directory.path().join("schema.yaml");
    std::fs::write(&path, yaml).expect("write schema.yaml");
    path
}

#[test]
fn load_reads_the_file_once_and_hashes_what_it_parsed() {
    let directory = TempDir::new().unwrap();
    let path = write_schema(&directory, SCHEMA_YAML);

    let data = load(&path).expect("the schema loads");

    let names: Vec<&str> = data
        .fields
        .iter()
        .map(|field| field.name.as_str())
        .collect();
    assert_eq!(names, ["id", "status", "parent"]);
    assert_eq!(
        data.fields[1].shape,
        FieldShape::Values {
            values: vec!["open".to_owned(), "done".to_owned()]
        }
    );
    assert!(data.rules.is_empty());
    assert_eq!(data.hash, content_hash(SCHEMA_YAML));
}

#[test]
fn load_reports_a_missing_file_as_a_schema_load_error() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("schema.yaml");

    let error = load(&path).unwrap_err();

    match &error {
        LoadError::Schema {
            path: reported,
            detail,
        } => {
            assert_eq!(reported, &path);
            assert!(
                detail.starts_with("failed to read schema file"),
                "detail: {detail}"
            );
        }
        other => panic!("expected LoadError::Schema, got {other:?}"),
    }
}

#[test]
fn load_reports_an_invalid_schema_with_the_parser_detail() {
    let directory = TempDir::new().unwrap();
    let path = write_schema(&directory, "fields:\n  status:\n    type: choice\n");

    let error = load(&path).unwrap_err();

    match &error {
        LoadError::Schema {
            path: reported,
            detail,
        } => {
            assert_eq!(reported, &path);
            assert!(
                detail.contains("'values' is required for type 'choice'"),
                "detail: {detail}"
            );
        }
        other => panic!("expected LoadError::Schema, got {other:?}"),
    }
}

#[test]
fn rules_carry_their_rows_or_the_reason_they_cannot() {
    use workdown_core::rule_shape::{
        MatchRow, RequireRow, RequireTest, RuleShape, ScalarTest, ScalarValue,
    };
    use workdown_core::schema_definition_data::RuleShapeData;

    let data = workdown_core::schema_definition_data::build(
        "fields:\n  id:\n    type: string\n  status:\n    type: choice\n    values: [open, done]\n  parent:\n    type: link\n    inverse: children\nrules:\n  - name: plain\n    match:\n      status: open\n    require:\n      parent: required\n  - name: nested\n    match:\n      children.status:\n        any: { all: done }\n    require:\n      parent: required\n",
    )
    .unwrap();

    assert_eq!(
        data.rules[0].shape,
        RuleShapeData::Rows {
            shape: RuleShape {
                match_rows: vec![MatchRow {
                    reference: "status".to_owned(),
                    quantifier: None,
                    test: ScalarTest::Equals {
                        value: ScalarValue::Text("open".to_owned())
                    },
                }],
                require_rows: vec![RequireRow {
                    reference: "parent".to_owned(),
                    test: RequireTest::Required,
                }],
                count: None,
            }
        }
    );
    assert!(matches!(
        &data.rules[1].shape,
        RuleShapeData::Unrepresentable { reason } if reason.contains("nests a quantifier")
    ));
}
