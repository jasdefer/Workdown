//! The schema as the schema editor sees it — every field's full
//! persisted definition, every rule, and the type system's own tables:
//! which properties a type accepts, which generators may default it,
//! which aggregate functions apply, which type changes are safe.
//!
//! This is the second projection of a [`crate::model::schema::Schema`]
//! for the web client.
//! [`crate::schema_data`] is the *editing vocabulary* for item editors
//! and stays small because every page fetches it; this one is the
//! *schema editor's* payload, fetched only by the `/schema` page.
//! Served by `GET /api/schema/definition`.
//!
//! Two properties of the build are deliberate (settled in
//! `workdown-items/schema-definition-api.md`):
//!
//! - **It reads `schema.yaml` and nothing else.** No items, resources
//!   or views are loaded: the payload needs none of them, and the
//!   schema page then works while the items directory is broken —
//!   exactly when someone wants to see the schema. The content hash,
//!   the typed schema and the text blocks all come from the same bytes,
//!   read once, so the hash can never describe a different file than
//!   the fields do.
//! - **Derived blocks and rule bodies are YAML text from a second,
//!   generic parse of those bytes.** The typed model has already
//!   discarded how a `when:` or `pull:` block was written; re-parsing
//!   the file as a plain value tree and re-serializing each sub-block
//!   gives text that is exact in content and normalized in form
//!   (comments inside the block do not survive). That is enough for
//!   read-only display, and it needs no span-aware parser.
//!
//! Every type here carries a `ts_rs` derive; `cargo xtask gen-types`
//! emits the TypeScript. A new type needs its `exports.add` line in
//! `crates/core/examples/gen_types.rs`.

use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};
use strum::VariantArray;

use crate::coerce::coerce_value;
use crate::model::schema::{
    allowed_aggregate_functions, allowed_generators, field_property_allowed, widening_targets,
    AggregateFunction, DefaultValue, FieldDefinition, FieldProperty, FieldType, FieldTypeConfig,
    Generator, Severity,
};
use crate::model::FieldValue;
use crate::parser::schema::{parse_schema, SchemaLoadError};
use crate::project::LoadError;

// ── Wire types ────────────────────────────────────────────────────────

/// Everything the schema page needs: the definitions as persisted, and
/// the type system's tables so the client never hardcodes a fact about
/// a type.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct SchemaDefinitionData {
    /// Every field's full definition, in `schema.yaml` declaration
    /// order — the order forms and boards use.
    pub fields: Vec<FieldDefinitionData>,
    /// Every rule, in file order.
    pub rules: Vec<RuleData>,
    /// Which type-restricted properties each type accepts. One entry
    /// per [`FieldType`], in declaration order. Header properties
    /// (`description`, `required`, `default`) are valid everywhere and
    /// absent here; `resource` is listed because only some types take it.
    pub properties_by_type: Vec<FieldTypeProperties>,
    /// Which aggregate functions reduce values of each type; an empty
    /// list means the type cannot be aggregated or pulled at all.
    pub aggregate_functions_by_type: Vec<FieldTypeAggregateFunctions>,
    /// Which `$`-generators may be the `default:` of each type.
    pub generators_by_type: Vec<FieldTypeGenerators>,
    /// Which types each type may be changed to without invalidating an
    /// existing value. Empty means the type is locked to itself.
    pub widening_by_type: Vec<FieldTypeWidening>,
    /// Content hash of `schema.yaml` as read for this payload. Opaque to
    /// the client: the write endpoints demand it back and compare it
    /// with the bytes they read at write time, refusing with `409` when
    /// the file changed underneath an open editor.
    pub hash: String,
}

/// One field as `schema.yaml` defines it.
///
/// The header is common to every type; `shape` is the type-specific
/// part, a tagged union so a combination the type system forbids (a
/// choice with `min`) cannot be expressed on the wire in either
/// direction.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct FieldDefinitionData {
    /// The frontmatter key.
    pub name: String,
    /// The built-in type.
    pub field_type: FieldType,
    /// Whether every item must carry the field.
    pub required: bool,
    /// Human-readable explanation, as written.
    pub description: Option<String>,
    /// The `default:` applied at `workdown add` time, if any.
    pub default: Option<DefaultData>,
    /// Resource section in `resources.yaml` that constrains the values,
    /// if any. A header property because it is not tied to one shape
    /// (`string` and `list` both take it); `properties_by_type` says
    /// which types accept it.
    pub resource: Option<String>,
    /// The type-specific properties.
    pub shape: FieldShape,
    /// The fill mechanisms declared on the field, each as the YAML
    /// text of its block. All `None` for a plain field.
    pub derived: DerivedBlocks,
}

/// The type-specific part of a field definition. One variant per
/// distinct property set, mirroring [`FieldTypeConfig`]; the `kind`
/// tag names the editor block the web app renders.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldShape {
    /// No type-specific properties: `date`, `color`, `boolean`, `list`.
    Scalar,
    /// `integer` and `float`: inclusive bounds.
    Numeric { min: Option<f64>, max: Option<f64> },
    /// `duration`: inclusive bounds in canonical seconds. The file
    /// spells them as suffix shorthand (`"2d"`); the client's duration
    /// editor works in seconds like the item panel's does.
    Duration {
        min_seconds: Option<i64>,
        max_seconds: Option<i64>,
    },
    /// `string`: an optional regex the value must match.
    Text { pattern: Option<String> },
    /// `choice` and `multichoice`: the allowed values, in order.
    Values { values: Vec<String> },
    /// `link` and `links`: cycle policy and the derived inverse name.
    Relation {
        allow_cycles: Option<bool>,
        inverse: Option<String>,
    },
}

/// A field's `default:`.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DefaultData {
    /// A `$`-generator, applied when an item is created.
    Generator { generator: Generator },
    /// A literal, coerced through the field's own definition so the
    /// client can render it in the type's editor.
    Literal { value: FieldValue },
    /// A literal the field's coercion rejects. The parser checks a
    /// default's YAML kind against the type, not its full validity —
    /// `default: 1.5` on an integer, an out-of-range number, a
    /// malformed date all load. Kept as text so the page can show what
    /// the file says and why the editor cannot.
    Invalid { text: String, reason: String },
}

/// The fill-mechanism blocks of a field, each as YAML text for
/// read-only display. Editing them is a later item; their type is
/// locked meanwhile because the expression was type-checked against it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, ts_rs::TS)]
pub struct DerivedBlocks {
    /// The `compute:` block — an expression string or a mapping.
    pub compute: Option<String>,
    /// The `when:` block — the branch list.
    pub when: Option<String>,
    /// The `pull:` block.
    pub pull: Option<String>,
    /// The `aggregate:` block.
    pub aggregate: Option<String>,
}

/// One rule: its header as data, its body as YAML text.
///
/// The header is what the rules list shows in columns. The body —
/// `match`, `require`, `count` — is displayed as the file has it; the
/// structured shape a rule *form* needs is defined with the rules
/// editor and added beside this text then.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct RuleData {
    /// Unique kebab-case identifier.
    pub name: String,
    /// Human-readable explanation, as written.
    pub description: Option<String>,
    /// Whether a violation is an error or a warning.
    pub severity: Severity,
    /// The rule mapping minus its header keys, serialized as YAML.
    /// Empty when the rule has no body.
    pub body: String,
}

/// The type-restricted properties one type accepts.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct FieldTypeProperties {
    pub field_type: FieldType,
    pub properties: Vec<FieldProperty>,
}

/// The aggregate functions defined for one type.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct FieldTypeAggregateFunctions {
    pub field_type: FieldType,
    pub functions: Vec<AggregateFunction>,
}

/// The generators that may default one type.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct FieldTypeGenerators {
    pub field_type: FieldType,
    pub generators: Vec<Generator>,
}

/// The types one type may be changed to.
#[derive(Debug, Clone, PartialEq, Serialize, ts_rs::TS)]
pub struct FieldTypeWidening {
    pub field_type: FieldType,
    pub widens_to: Vec<FieldType>,
}

// ── Entry points ──────────────────────────────────────────────────────

/// Read `schema_path` once and build the payload from those bytes.
///
/// A missing or unparseable file is the same hard failure
/// [`crate::project::load_project`] reports for it, worded identically,
/// so the web layer maps it to the `422` tier with the diagnostic the
/// rest of the app shows.
pub fn load(schema_path: &Path) -> Result<SchemaDefinitionData, LoadError> {
    let schema_error = |detail: String| LoadError::Schema {
        path: schema_path.to_path_buf(),
        detail,
    };
    let yaml = std::fs::read_to_string(schema_path)
        .map_err(|error| schema_error(SchemaLoadError::ReadFailed(error).to_string()))?;
    build(&yaml).map_err(|error| schema_error(error.to_string()))
}

/// Build the payload from the text of a `schema.yaml`.
///
/// Parses it as a [`Schema`](crate::model::schema::Schema) (the validating parse) and again as a
/// generic YAML tree (for the text blocks), and hashes the text.
pub fn build(yaml: &str) -> Result<SchemaDefinitionData, SchemaLoadError> {
    let schema = parse_schema(yaml)?;
    // The validating parse accepted the same text, so a generic parse
    // cannot fail on syntax; a failure here would be a serde_yaml bug.
    let document: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(SchemaLoadError::InvalidYaml)?;

    let fields = schema
        .fields
        .iter()
        .map(|(name, definition)| {
            let raw_field = document
                .get("fields")
                .and_then(|fields| fields.get(name.as_str()));
            FieldDefinitionData::from_definition(name, definition, raw_field)
        })
        .collect();

    let raw_rules = document
        .get("rules")
        .and_then(serde_yaml::Value::as_sequence);
    let rules = schema
        .rules
        .iter()
        .enumerate()
        .map(|(index, rule)| RuleData {
            name: rule.name.clone(),
            description: rule.description.clone(),
            severity: rule.severity,
            body: raw_rules
                .and_then(|rules| rules.get(index))
                .map(rule_body_yaml)
                .unwrap_or_default(),
        })
        .collect();

    Ok(SchemaDefinitionData {
        fields,
        rules,
        properties_by_type: FieldType::VARIANTS
            .iter()
            .map(|&field_type| FieldTypeProperties {
                field_type,
                properties: FieldProperty::VARIANTS
                    .iter()
                    .copied()
                    .filter(|&property| field_property_allowed(field_type, property))
                    .collect(),
            })
            .collect(),
        aggregate_functions_by_type: FieldType::VARIANTS
            .iter()
            .map(|&field_type| FieldTypeAggregateFunctions {
                field_type,
                functions: allowed_aggregate_functions(field_type)
                    .map(<[AggregateFunction]>::to_vec)
                    .unwrap_or_default(),
            })
            .collect(),
        generators_by_type: FieldType::VARIANTS
            .iter()
            .map(|&field_type| FieldTypeGenerators {
                field_type,
                generators: allowed_generators(field_type).to_vec(),
            })
            .collect(),
        widening_by_type: FieldType::VARIANTS
            .iter()
            .map(|&field_type| FieldTypeWidening {
                field_type,
                widens_to: widening_targets(field_type).to_vec(),
            })
            .collect(),
        hash: content_hash(yaml),
    })
}

/// The hex SHA-256 of the file text. Opaque to every consumer but the
/// write path, which recomputes it with this same function.
pub fn content_hash(yaml: &str) -> String {
    format!("{:x}", Sha256::digest(yaml.as_bytes()))
}

// ── Projection ────────────────────────────────────────────────────────

impl FieldDefinitionData {
    /// Project one typed definition, taking the derived blocks' text
    /// from `raw_field`, the field's mapping in the generic parse.
    fn from_definition(
        name: &str,
        definition: &FieldDefinition,
        raw_field: Option<&serde_yaml::Value>,
    ) -> Self {
        let block_text = |key: &str| raw_field.and_then(|field| field.get(key)).map(yaml_text);
        Self {
            name: name.to_owned(),
            field_type: definition.field_type(),
            required: definition.required,
            description: definition.description.clone(),
            default: default_data(definition),
            resource: definition.resource.clone(),
            shape: FieldShape::from_config(&definition.type_config),
            derived: DerivedBlocks {
                compute: block_text("compute"),
                when: block_text("when"),
                pull: block_text("pull"),
                aggregate: block_text("aggregate"),
            },
        }
    }
}

impl FieldShape {
    fn from_config(config: &FieldTypeConfig) -> Self {
        match config {
            FieldTypeConfig::String { pattern } => FieldShape::Text {
                pattern: pattern.as_ref().map(|pattern| pattern.source().to_owned()),
            },
            FieldTypeConfig::Choice { values } | FieldTypeConfig::Multichoice { values } => {
                FieldShape::Values {
                    values: values.clone(),
                }
            }
            FieldTypeConfig::Integer { min, max } | FieldTypeConfig::Float { min, max } => {
                FieldShape::Numeric {
                    min: *min,
                    max: *max,
                }
            }
            FieldTypeConfig::Duration { min, max } => FieldShape::Duration {
                min_seconds: *min,
                max_seconds: *max,
            },
            FieldTypeConfig::Date
            | FieldTypeConfig::Color
            | FieldTypeConfig::Boolean
            | FieldTypeConfig::List => FieldShape::Scalar,
            FieldTypeConfig::Link {
                allow_cycles,
                inverse,
            }
            | FieldTypeConfig::Links {
                allow_cycles,
                inverse,
            } => FieldShape::Relation {
                allow_cycles: *allow_cycles,
                inverse: inverse.clone(),
            },
        }
    }
}

/// The field's `default:` on the wire: a literal is run through the
/// field's own coercion so the client gets a typed value.
fn default_data(definition: &FieldDefinition) -> Option<DefaultData> {
    let literal = match definition.default.as_ref()? {
        DefaultValue::Generator(generator) => {
            return Some(DefaultData::Generator {
                generator: *generator,
            })
        }
        DefaultValue::String(text) => serde_yaml::Value::String(text.clone()),
        DefaultValue::Integer(integer) => serde_yaml::Value::Number((*integer).into()),
        DefaultValue::Float(float) => serde_yaml::Value::Number((*float).into()),
        DefaultValue::Bool(boolean) => serde_yaml::Value::Bool(*boolean),
    };
    Some(match coerce_value(&literal, definition) {
        Ok(value) => DefaultData::Literal { value },
        Err(error) => DefaultData::Invalid {
            text: yaml_text(&literal),
            reason: error.to_string(),
        },
    })
}

/// A YAML value as text, without the trailing newline the serializer
/// adds. A scalar comes back as its bare spelling.
fn yaml_text(value: &serde_yaml::Value) -> String {
    serde_yaml::to_string(value)
        .map(|text| text.trim_end().to_owned())
        .unwrap_or_default()
}

/// A rule mapping minus the header keys the [`RuleData`] carries as
/// data, as YAML text; empty when nothing else is set.
fn rule_body_yaml(raw_rule: &serde_yaml::Value) -> String {
    let Some(mapping) = raw_rule.as_mapping() else {
        return String::new();
    };
    let mut body = mapping.clone();
    for header_key in ["name", "description", "severity"] {
        body.shift_remove(serde_yaml::Value::String(header_key.to_owned()));
    }
    if body.is_empty() {
        return String::new();
    }
    yaml_text(&serde_yaml::Value::Mapping(body))
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// One field per shape, every default kind, every derived block,
    /// and two rules — the input space `build` distinguishes.
    const SCHEMA_YAML: &str = "\
fields:
  id:
    type: string
    default: $filename
    # a comment inside the entry
  title:
    type: string
    pattern: '^[A-Z]'
    resource: titles
  status:
    type: choice
    values: [open, done]
    required: true
    default: open
  tags:
    type: multichoice
    values: [a, b]
  points:
    type: integer
    min: 0
    max: 100
    default: 3
  ratio:
    type: float
    default: 1.5
  effort:
    type: duration
    min: 1h
    max: 2w
  when_done:
    type: date
    compute:
      expression: start + effort
      round: ceil
  start:
    type: date
    default: not-a-date
  flag:
    type: boolean
    default: true
  color:
    type: color
    when:
      - if: status == \"done\"
        then: green
  labels:
    type: list
  parent:
    type: link
    allow_cycles: false
    inverse: children
  depends_on:
    type: links
    allow_cycles: false
  total:
    type: integer
    aggregate:
      function: sum
      over: parent
  earliest:
    type: date
    pull:
      over: depends_on
      field: start
      function: min
rules:
  - name: done-needs-points
    description: Finished work is sized
    match:
      status: done
    require:
      points: required
  - name: at-least-one
    severity: warning
    count:
      min: 1
";

    fn built() -> SchemaDefinitionData {
        build(SCHEMA_YAML).expect("the test schema loads")
    }

    fn field<'a>(data: &'a SchemaDefinitionData, name: &str) -> &'a FieldDefinitionData {
        data.fields
            .iter()
            .find(|field| field.name == name)
            .unwrap_or_else(|| panic!("field {name} is in the payload"))
    }

    #[test]
    fn fields_come_in_declaration_order() {
        let data = built();
        let names: Vec<&str> = data.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "id",
                "title",
                "status",
                "tags",
                "points",
                "ratio",
                "effort",
                "when_done",
                "start",
                "flag",
                "color",
                "labels",
                "parent",
                "depends_on",
                "total",
                "earliest",
            ]
        );
    }

    #[test]
    fn every_shape_is_projected_from_its_type_config() {
        let data = built();
        assert_eq!(
            field(&data, "title").shape,
            FieldShape::Text {
                pattern: Some("^[A-Z]".to_owned())
            }
        );
        assert_eq!(field(&data, "title").resource.as_deref(), Some("titles"));
        assert_eq!(
            field(&data, "status").shape,
            FieldShape::Values {
                values: vec!["open".to_owned(), "done".to_owned()]
            }
        );
        assert!(field(&data, "status").required);
        assert_eq!(
            field(&data, "tags").shape,
            FieldShape::Values {
                values: vec!["a".to_owned(), "b".to_owned()]
            }
        );
        assert_eq!(
            field(&data, "points").shape,
            FieldShape::Numeric {
                min: Some(0.0),
                max: Some(100.0)
            }
        );
        assert_eq!(
            field(&data, "effort").shape,
            FieldShape::Duration {
                min_seconds: Some(3600),
                max_seconds: Some(14 * 24 * 3600),
            }
        );
        for scalar in ["start", "flag", "color", "labels"] {
            assert_eq!(field(&data, scalar).shape, FieldShape::Scalar, "{scalar}");
        }
        assert_eq!(
            field(&data, "parent").shape,
            FieldShape::Relation {
                allow_cycles: Some(false),
                inverse: Some("children".to_owned()),
            }
        );
        assert_eq!(
            field(&data, "depends_on").shape,
            FieldShape::Relation {
                allow_cycles: Some(false),
                inverse: None,
            }
        );
    }

    #[test]
    fn defaults_are_generator_literal_or_invalid() {
        let data = built();
        assert_eq!(
            field(&data, "id").default,
            Some(DefaultData::Generator {
                generator: Generator::Filename
            })
        );
        assert_eq!(
            field(&data, "status").default,
            Some(DefaultData::Literal {
                value: FieldValue::Choice("open".to_owned())
            })
        );
        assert_eq!(
            field(&data, "points").default,
            Some(DefaultData::Literal {
                value: FieldValue::Integer(3)
            })
        );
        assert_eq!(
            field(&data, "ratio").default,
            Some(DefaultData::Literal {
                value: FieldValue::Float(1.5)
            })
        );
        assert_eq!(
            field(&data, "flag").default,
            Some(DefaultData::Literal {
                value: FieldValue::Boolean(true)
            })
        );
        // The parser accepts any string as a date default; coercion
        // does not, and the page is told why.
        match &field(&data, "start").default {
            Some(DefaultData::Invalid { text, reason }) => {
                assert_eq!(text, "not-a-date");
                assert!(!reason.is_empty());
            }
            other => panic!("expected an invalid default, got {other:?}"),
        }
        assert_eq!(field(&data, "title").default, None);
    }

    #[test]
    fn derived_blocks_are_the_yaml_of_the_block() {
        let data = built();
        assert_eq!(field(&data, "title").derived, DerivedBlocks::default());
        assert_eq!(
            field(&data, "when_done").derived.compute.as_deref(),
            Some("expression: start + effort\nround: ceil")
        );
        assert_eq!(
            field(&data, "color").derived.when.as_deref(),
            Some("- if: status == \"done\"\n  then: green")
        );
        assert_eq!(
            field(&data, "total").derived.aggregate.as_deref(),
            Some("function: sum\nover: parent")
        );
        assert_eq!(
            field(&data, "earliest").derived.pull.as_deref(),
            Some("over: depends_on\nfield: start\nfunction: min")
        );
    }

    #[test]
    fn rules_carry_header_as_data_and_body_as_yaml() {
        let data = built();
        assert_eq!(data.rules.len(), 2);
        assert_eq!(
            data.rules[0],
            RuleData {
                name: "done-needs-points".to_owned(),
                description: Some("Finished work is sized".to_owned()),
                severity: Severity::Error,
                body: "match:\n  status: done\nrequire:\n  points: required".to_owned(),
            }
        );
        assert_eq!(data.rules[1].severity, Severity::Warning);
        assert_eq!(data.rules[1].body, "count:\n  min: 1");
    }

    #[test]
    fn a_rule_with_only_a_header_has_an_empty_body() {
        let mapping: serde_yaml::Value =
            serde_yaml::from_str("name: bare\nseverity: warning\n").unwrap();
        assert_eq!(rule_body_yaml(&mapping), "");
    }

    #[test]
    fn tables_cover_every_field_type_from_the_model() {
        let data = built();
        let types: Vec<FieldType> = FieldType::VARIANTS.to_vec();
        assert_eq!(
            data.properties_by_type
                .iter()
                .map(|row| row.field_type)
                .collect::<Vec<_>>(),
            types
        );
        assert_eq!(data.aggregate_functions_by_type.len(), types.len());
        assert_eq!(data.generators_by_type.len(), types.len());
        assert_eq!(data.widening_by_type.len(), types.len());

        let row = |rows: &[FieldTypeProperties], field_type| {
            rows.iter()
                .find(|row| row.field_type == field_type)
                .unwrap()
                .properties
                .clone()
        };
        assert_eq!(
            row(&data.properties_by_type, FieldType::Choice),
            vec![FieldProperty::Values]
        );
        assert!(
            row(&data.properties_by_type, FieldType::Integer).contains(&FieldProperty::Aggregate)
        );

        let generators = data
            .generators_by_type
            .iter()
            .find(|row| row.field_type == FieldType::Date)
            .unwrap();
        assert_eq!(generators.generators, vec![Generator::Today]);

        let functions = data
            .aggregate_functions_by_type
            .iter()
            .find(|row| row.field_type == FieldType::Choice)
            .unwrap();
        assert!(functions.functions.is_empty());

        let widening = data
            .widening_by_type
            .iter()
            .find(|row| row.field_type == FieldType::Integer)
            .unwrap();
        assert_eq!(widening.widens_to, vec![FieldType::Float]);
    }

    #[test]
    fn hash_follows_the_bytes() {
        let data = built();
        assert_eq!(data.hash, content_hash(SCHEMA_YAML));
        assert_eq!(data.hash.len(), 64, "hex sha-256");
        let edited = SCHEMA_YAML.replace("required: true", "required: false");
        assert_ne!(build(&edited).unwrap().hash, data.hash);
    }

    #[test]
    fn an_invalid_schema_is_the_parser_error() {
        let error = build("fields:\n  x:\n    type: choice\n").unwrap_err();
        assert!(matches!(error, SchemaLoadError::Validation(_)), "{error}");
    }
}
