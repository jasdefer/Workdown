//! Drift guard for `crates/core/defaults/schema.schema.json`.
//!
//! ADR-005 keeps the JSON Schema editor-only — the CLI never loads it.
//! That means the schema and the Rust parser
//! (`crates/core/src/parser/schema.rs`) are two independent
//! representations of the same shape. This test compiles the schema and
//! runs it against the default `schema.yaml` plus the constraints the
//! parser enforces that editors must agree on — most importantly the
//! reserved field name `none` (the display roles' no-tint sentinel).

mod json_schema;

use std::collections::BTreeSet;

use strum::VariantArray;

use json_schema::SchemaGuard;
use workdown_core::model::schema::{field_property_allowed, FieldProperty, FieldType};

const SCHEMA_JSON: &str = include_str!("../defaults/schema.schema.json");
const DEFAULT_SCHEMA_YAML: &str = include_str!("../defaults/schema.yaml");

/// The compiled schema under test.
fn guard() -> SchemaGuard {
    SchemaGuard::compile("schema.schema.json", SCHEMA_JSON)
}

#[test]
fn default_schema_yaml_validates() {
    let schema = guard();
    schema.assert_valid(DEFAULT_SCHEMA_YAML);
}

#[test]
fn reserved_field_name_none_rejected() {
    // The parser rejects a field named `none` (display roles use it as
    // their no-tint sentinel); editor validation must agree.
    let schema = guard();
    schema.assert_invalid(
        "\
fields:
  none:
    type: string
",
    );
}

#[test]
fn uppercase_field_name_rejected() {
    let schema = guard();
    schema.assert_invalid(
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
    let schema = guard();
    schema.assert_valid(
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

// ── Compute config ────────────────────────────────────────────────────

#[test]
fn compute_shorthand_and_mapping_accepted() {
    let schema = guard();
    schema.assert_valid(
        "\
fields:
  start_date:
    type: date
  duration:
    type: duration
  end_date:
    type: date
    compute: start_date + duration
  finish:
    type: date
    compute:
      expression: start_date + duration
      round: ceil
      error_on_missing: true
",
    );
}

#[test]
fn compute_on_type_without_arithmetic_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
fields:
  status:
    type: choice
    values: [open, done]
    compute: other + 1
",
    );
}

#[test]
fn compute_with_default_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
fields:
  end_date:
    type: date
    default: $today
    compute: start_date + duration
",
    );
}

#[test]
fn compute_round_on_non_date_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
fields:
  weight:
    type: float
    compute:
      expression: 1.5 * 2
      round: floor
",
    );
}

#[test]
fn compute_unknown_option_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
fields:
  end_date:
    type: date
    compute:
      expression: start_date + duration
      rounding: ceil
",
    );
}

#[test]
fn compute_mapping_without_expression_rejected() {
    let schema = guard();
    schema.assert_invalid(
        "\
fields:
  end_date:
    type: date
    compute:
      round: ceil
",
    );
}

// ── Field-type property matrix ───────────────────────────────────────────
//
// `schema.schema.json` hand-encodes which properties each field type
// accepts, as `if`/`then` blocks. Rust answers the same question in
// `model::schema::field_property_allowed`, and that is the answer the CLI
// enforces. The tests below probe the two against each other rather than
// reading the schema's blocks: what the schema *accepts* is what an editor
// will let through, however the schema chooses to say it.
//
// `compute:` and `pull:` are deliberately not probed — the two sides
// genuinely disagree there, and deciding which is right is a rule change
// tracked in `compute-type-support-mismatch`.

/// A representative value for `property`, well-formed for `field_type`, so
/// the only thing a rejection can be about is whether the property belongs
/// on that type at all.
fn representative_value(field_type: FieldType, property: FieldProperty) -> &'static str {
    match property {
        FieldProperty::Values => "[a, b]",
        FieldProperty::Pattern => r#""^x$""#,
        // The bounds are typed: numbers on the numeric types, the suffix
        // shorthand on durations. A number on a duration field would be
        // rejected for its shape rather than for the property.
        FieldProperty::Min | FieldProperty::Max => match field_type {
            FieldType::Duration => r#""1d""#,
            _ => "1",
        },
        FieldProperty::AllowCycles => "false",
        FieldProperty::Resource => "people",
        // Which functions a type supports is a separate rule; any name from
        // the enum is enough to ask whether `aggregate:` is allowed here.
        FieldProperty::Aggregate => "{ function: count, over: parent }",
        FieldProperty::Inverse => "children",
    }
}

/// A one-field `schema.yaml` setting exactly `property` on a field of
/// `field_type`, plus whatever that type requires to be well-formed.
fn probe_document(field_type: FieldType, property: FieldProperty) -> String {
    // `choice` and `multichoice` do not merely permit `values`, they
    // require a non-empty list — unless `values` is the property under
    // probe, in which case it is already there.
    let companion = if matches!(field_type, FieldType::Choice | FieldType::Multichoice)
        && property != FieldProperty::Values
    {
        "    values: [a, b]\n"
    } else {
        ""
    };
    format!(
        "fields:\n  probe:\n    type: {field_type}\n{companion}    {property}: {}\n",
        representative_value(field_type, property)
    )
}

#[test]
fn json_schema_allows_exactly_the_properties_rust_allows() {
    let schema = guard();
    let mut disagreements = Vec::new();

    for &field_type in FieldType::VARIANTS {
        for &property in FieldProperty::VARIANTS {
            let document = probe_document(field_type, property);
            let editor_accepts = schema.accepts_yaml(&document);
            let cli_accepts = field_property_allowed(field_type, property);
            if editor_accepts != cli_accepts {
                let (accepts, rejects) = if editor_accepts {
                    ("schema.schema.json", "the CLI")
                } else {
                    ("the CLI", "schema.schema.json")
                };
                disagreements.push(format!(
                    "  `{property}` on a `{field_type}` field: {accepts} accepts it, {rejects} rejects it"
                ));
            }
        }
    }

    assert!(
        disagreements.is_empty(),
        "schema.schema.json and `field_property_allowed` disagree on {} of {} type/property \
         pairs:\n{}",
        disagreements.len(),
        FieldType::VARIANTS.len() * FieldProperty::VARIANTS.len(),
        disagreements.join("\n")
    );
}

#[test]
fn json_schema_lists_exactly_the_enum_field_types() {
    // The `type:` enum is the other half: a thirteenth field type absent
    // from it is flagged as invalid by every editor, and every probe above
    // would silently agree with the schema that the type does not exist.
    let document: serde_json::Value =
        serde_json::from_str(SCHEMA_JSON).expect("schema.schema.json must be valid JSON");
    let in_schema: BTreeSet<String> = document["$defs"]["field"]["properties"]["type"]["enum"]
        .as_array()
        .expect("the `field` definition pins `type` with an enum")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("every field type is a string")
                .to_owned()
        })
        .collect();
    let in_enum: BTreeSet<String> = FieldType::VARIANTS
        .iter()
        .map(ToString::to_string)
        .collect();

    assert_eq!(
        in_schema, in_enum,
        "schema.schema.json and the FieldType enum disagree about which field types exist"
    );
}
