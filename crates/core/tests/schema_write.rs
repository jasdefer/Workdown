//! Integration tests for the schema-write operations (`add_field`,
//! `update_field`, `remove_field`, `reorder_fields`, `field_usage`).
//!
//! Each test builds a throwaway project from the shipped defaults
//! (`schema.yaml`, `config.yaml`, `resources.yaml`), mutates the schema
//! through the public operations, and asserts on the outcome and the
//! file on disk — in particular that every field the operation did not
//! touch is content-identical afterwards.

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use workdown_core::model::config::Config;
use workdown_core::model::diagnostic::{DiagnosticBody, ItemDiagnosticKind};
use workdown_core::model::schema::{DefaultValue, FieldType, FieldTypeConfig};
use workdown_core::operations::schema_write::{
    add_field, field_usage, remove_field, reorder_fields, update_field, DefaultWrite,
    FieldDefinitionWrite, RemovedValues, SchemaWriteError,
};
use workdown_core::parser::config::load_config;
use workdown_core::parser::schema::load_schema;
use workdown_core::schema_definition_data::FieldShape;

const CONFIG: &str = include_str!("../defaults/config.yaml");
const SCHEMA: &str = include_str!("../defaults/schema.yaml");
const RESOURCES: &str = include_str!("../defaults/resources.yaml");
const CONFIG_PATH: &str = ".workdown/config.yaml";

fn setup() -> (TempDir, PathBuf, Config) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(CONFIG_PATH), CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), SCHEMA).unwrap();
    fs::write(root.join(".workdown/resources.yaml"), RESOURCES).unwrap();
    let config = load_config(&root.join(CONFIG_PATH)).unwrap();
    (directory, root, config)
}

fn config_path() -> &'static Path {
    Path::new(CONFIG_PATH)
}

/// An item that satisfies the default schema's required fields, plus
/// whatever extra frontmatter the test needs.
fn write_item(root: &Path, id: &str, extra_frontmatter: &str) {
    fs::write(
        root.join(format!("workdown-items/{id}.md")),
        format!(
            "---\ntitle: {id}\ntype: task\nstatus: open\ncreated: 2026-01-01\n{extra_frontmatter}---\n"
        ),
    )
    .unwrap();
}

fn write_views(root: &Path, content: &str) {
    fs::write(root.join(".workdown/views.yaml"), content).unwrap();
}

fn schema_text(root: &Path) -> String {
    fs::read_to_string(root.join(".workdown/schema.yaml")).unwrap()
}

/// The `fields:` mapping of the file on disk, as a generic tree — what
/// "content-identical" is measured on.
fn fields_tree(root: &Path) -> serde_yaml::Mapping {
    let document: serde_yaml::Value = serde_yaml::from_str(&schema_text(root)).unwrap();
    document["fields"].as_mapping().unwrap().clone()
}

fn field_names(tree: &serde_yaml::Mapping) -> Vec<String> {
    tree.keys()
        .map(|key| key.as_str().unwrap().to_owned())
        .collect()
}

fn property_keys(tree: &serde_yaml::Mapping, field: &str) -> Vec<String> {
    tree[field]
        .as_mapping()
        .unwrap()
        .keys()
        .map(|key| key.as_str().unwrap().to_owned())
        .collect()
}

/// Every field but the `touched` ones has the same content as before.
fn assert_untouched(before: &serde_yaml::Mapping, after: &serde_yaml::Mapping, touched: &[&str]) {
    for (key, value) in before {
        let name = key.as_str().unwrap();
        if touched.contains(&name) {
            continue;
        }
        assert_eq!(
            after.get(key),
            Some(value),
            "field '{name}' changed although the write did not touch it"
        );
    }
}

fn definition(field_type: FieldType, shape: FieldShape) -> FieldDefinitionWrite {
    FieldDefinitionWrite {
        field_type,
        required: false,
        description: None,
        default: None,
        resource: None,
        shape,
    }
}

// ── add_field ────────────────────────────────────────────────────

#[test]
fn add_field_appends_at_the_end_and_leaves_other_fields_untouched() {
    let (_directory, root, config) = setup();
    let before = fields_tree(&root);
    let mut estimate = definition(
        FieldType::Integer,
        FieldShape::Numeric {
            min: Some(0.0),
            max: Some(100.0),
        },
    );
    estimate.description = Some("Story points".to_owned());
    estimate.default = Some(DefaultWrite::Literal {
        value: serde_yaml::Value::Number(3.into()),
    });

    let outcome = add_field(&config, &root, config_path(), "estimate", &estimate).unwrap();

    assert_eq!(outcome.field_name.as_deref(), Some("estimate"));
    assert!(!outcome.mutation_caused_warning);
    let after = fields_tree(&root);
    assert_eq!(
        field_names(&after).last().map(String::as_str),
        Some("estimate")
    );
    assert_eq!(
        property_keys(&after, "estimate"),
        ["type", "min", "max", "required", "default", "description"]
    );
    assert_untouched(&before, &after, &["estimate"]);

    let reloaded = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    let added = &reloaded.fields["estimate"];
    assert!(matches!(
        added.type_config,
        FieldTypeConfig::Integer {
            min: Some(min),
            max: Some(max)
        } if min == 0.0 && max == 100.0
    ));
    assert_eq!(added.default, Some(DefaultValue::Integer(3)));
    assert_eq!(added.description.as_deref(), Some("Story points"));
}

#[test]
fn add_field_with_an_invalid_name_is_refused() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);

    let error = add_field(
        &config,
        &root,
        config_path(),
        "Bad Name",
        &definition(FieldType::Date, FieldShape::Scalar),
    )
    .unwrap_err();

    assert!(matches!(error, SchemaWriteError::InvalidName { name } if name == "Bad Name"));
    assert_eq!(schema_text(&root), original, "file must be untouched");
}

#[test]
fn add_field_with_an_existing_name_is_refused() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);

    let error = add_field(
        &config,
        &root,
        config_path(),
        "status",
        &definition(FieldType::Date, FieldShape::Scalar),
    )
    .unwrap_err();

    assert!(matches!(error, SchemaWriteError::DuplicateName { name } if name == "status"));
    assert_eq!(schema_text(&root), original);
}

#[test]
fn an_unloadable_candidate_is_refused_with_the_parse_error_and_nothing_is_written() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);
    // A choice without values parses as YAML but fails schema validation.
    let mood = definition(FieldType::Choice, FieldShape::Values { values: vec![] });

    let error = add_field(&config, &root, config_path(), "mood", &mood).unwrap_err();

    match &error {
        SchemaWriteError::Unloadable { detail } => {
            assert!(
                detail.contains("'values' must not be empty"),
                "detail: {detail}"
            );
        }
        other => panic!("expected Unloadable, got {other:?}"),
    }
    assert_eq!(schema_text(&root), original, "file must be untouched");
}

#[test]
fn bounds_the_parser_rejects_are_refused_with_its_message() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);

    // A duration bound is written as the editor spelled it and judged
    // by the parser, so a misspelling comes back as the parser's words.
    let misspelled = definition(
        FieldType::Duration,
        FieldShape::Duration {
            min: Some("2 days".to_owned()),
            max: None,
        },
    );
    let error = add_field(&config, &root, config_path(), "effort", &misspelled).unwrap_err();
    match &error {
        SchemaWriteError::Unloadable { detail } => {
            assert!(
                detail.contains("'min' is not a valid duration"),
                "detail: {detail}"
            );
        }
        other => panic!("expected Unloadable, got {other:?}"),
    }

    // An inverted numeric pair admits no value at all.
    let inverted = definition(
        FieldType::Integer,
        FieldShape::Numeric {
            min: Some(10.0),
            max: Some(5.0),
        },
    );
    let error = add_field(&config, &root, config_path(), "points", &inverted).unwrap_err();
    match &error {
        SchemaWriteError::Unloadable { detail } => {
            assert!(
                detail.contains("'min' must be less than or equal to 'max'"),
                "detail: {detail}"
            );
        }
        other => panic!("expected Unloadable, got {other:?}"),
    }

    assert_eq!(schema_text(&root), original, "file must be untouched");
}

// ── update_field ─────────────────────────────────────────────────

#[test]
fn update_field_replaces_plain_properties_and_keeps_the_recipe() {
    let (_directory, root, config) = setup();
    let before = fields_tree(&root);
    let position = field_names(&before)
        .iter()
        .position(|name| name == "status_color")
        .unwrap();
    let mut tint = definition(FieldType::Color, FieldShape::Scalar);
    tint.description = Some("Tint".to_owned());

    let outcome = update_field(&config, &root, config_path(), "status_color", &tint).unwrap();

    assert!(!outcome.mutation_caused_warning);
    let after = fields_tree(&root);
    assert_untouched(&before, &after, &["status_color"]);
    assert_eq!(
        field_names(&after)[position],
        "status_color",
        "the field keeps its position"
    );
    let entry = after["status_color"].as_mapping().unwrap();
    assert_eq!(
        entry.get("when"),
        before["status_color"].as_mapping().unwrap().get("when"),
        "the recipe survives untouched"
    );
    // Beside `when`, `default:` is the recipe's fallback, not an add-time
    // default: the definition's `default: None` must not clear it.
    assert_eq!(
        entry.get("default"),
        Some(&serde_yaml::Value::String("gray".to_owned()))
    );
    assert_eq!(
        entry.get("description"),
        Some(&serde_yaml::Value::String("Tint".to_owned()))
    );
}

#[test]
fn update_field_widens_the_type_and_drops_the_old_type_properties() {
    let (_directory, root, config) = setup();
    write_item(&root, "a", "priority: high\n");
    let before = fields_tree(&root);
    let text = definition(FieldType::String, FieldShape::Text { pattern: None });

    let outcome = update_field(&config, &root, config_path(), "priority", &text).unwrap();

    // Every value valid under `choice` is valid under `string`, so the
    // item holding `high` raises nothing new.
    assert!(!outcome.mutation_caused_warning);
    let after = fields_tree(&root);
    assert_untouched(&before, &after, &["priority"]);
    assert_eq!(property_keys(&after, "priority"), ["type", "required"]);
    let reloaded = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    assert_eq!(reloaded.fields["priority"].field_type(), FieldType::String);
}

#[test]
fn update_field_of_a_recipe_field_cannot_change_its_type() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);
    let text = definition(FieldType::String, FieldShape::Text { pattern: None });

    let error = update_field(&config, &root, config_path(), "status_color", &text).unwrap_err();

    assert!(matches!(
        error,
        SchemaWriteError::RecipeTypeLocked {
            current: FieldType::Color,
            requested: FieldType::String,
            ..
        }
    ));
    assert_eq!(schema_text(&root), original);
}

#[test]
fn update_field_refuses_a_type_change_outside_the_widening_table() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);
    let number = definition(
        FieldType::Integer,
        FieldShape::Numeric {
            min: None,
            max: None,
        },
    );

    let error = update_field(&config, &root, config_path(), "status", &number).unwrap_err();

    assert!(matches!(
        error,
        SchemaWriteError::TypeChangeNotAllowed {
            current: FieldType::Choice,
            requested: FieldType::Integer,
            ..
        }
    ));
    assert_eq!(schema_text(&root), original);
}

#[test]
fn an_unknown_field_name_is_refused_on_update_and_remove() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);

    let update_error = update_field(
        &config,
        &root,
        config_path(),
        "nope",
        &definition(FieldType::Date, FieldShape::Scalar),
    )
    .unwrap_err();
    let remove_error =
        remove_field(&config, &root, config_path(), "nope", RemovedValues::Keep).unwrap_err();

    assert!(matches!(update_error, SchemaWriteError::FieldNotFound { name } if name == "nope"));
    assert!(matches!(remove_error, SchemaWriteError::FieldNotFound { name } if name == "nope"));
    assert_eq!(schema_text(&root), original);
}

// ── remove_field ─────────────────────────────────────────────────

#[test]
fn remove_field_drops_the_entry_and_warns_about_items_holding_a_value() {
    let (_directory, root, config) = setup();
    write_item(&root, "a", "updated: 2026-02-01\n");
    let item_before = item_text(&root, "a");
    let before = fields_tree(&root);

    let outcome = remove_field(
        &config,
        &root,
        config_path(),
        "updated",
        RemovedValues::Keep,
    )
    .unwrap();

    assert!(outcome.mutation_caused_warning);
    assert!(warns_about_unknown_field(&outcome.warnings, "updated"));
    assert!(outcome.rewritten_items.is_empty());
    assert_eq!(item_text(&root, "a"), item_before);
    let after = fields_tree(&root);
    assert!(!after.contains_key("updated"));
    assert_untouched(&before, &after, &["updated"]);
}

#[test]
fn remove_field_dropping_values_rewrites_only_the_items_holding_the_key() {
    let (_directory, root, config) = setup();
    fs::write(
        root.join("workdown-items/a.md"),
        "---\ntitle: a\ntype: task\nstatus: open\ncreated: 2026-01-01\nupdated: 2026-02-01\n---\n\nNotes to keep.\n",
    )
    .unwrap();
    write_item(&root, "b", "");
    let other_item_before = item_text(&root, "b");

    let outcome = remove_field(
        &config,
        &root,
        config_path(),
        "updated",
        RemovedValues::Drop,
    )
    .unwrap();

    assert_eq!(outcome.rewritten_items, ["a"]);
    assert!(!outcome.mutation_caused_warning);
    assert!(!warns_about_unknown_field(&outcome.warnings, "updated"));
    let rewritten = item_text(&root, "a");
    assert!(!rewritten.contains("updated:"), "{rewritten}");
    assert!(
        rewritten.ends_with("---\n\nNotes to keep.\n"),
        "{rewritten}"
    );
    assert_eq!(item_text(&root, "b"), other_item_before);
    assert!(!fields_tree(&root).contains_key("updated"));
}

fn item_text(root: &Path, id: &str) -> String {
    fs::read_to_string(root.join(format!("workdown-items/{id}.md"))).unwrap()
}

fn warns_about_unknown_field(
    warnings: &[workdown_core::model::diagnostic::Diagnostic],
    field_name: &str,
) -> bool {
    warnings.iter().any(|diagnostic| matches!(
        &diagnostic.body,
        DiagnosticBody::Item(item)
            if matches!(&item.kind, ItemDiagnosticKind::UnknownField { field } if field == field_name)
    ))
}

#[test]
fn the_id_field_cannot_be_removed() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);

    let error = remove_field(&config, &root, config_path(), "id", RemovedValues::Keep).unwrap_err();

    assert!(matches!(error, SchemaWriteError::IdNotRemovable));
    assert_eq!(schema_text(&root), original);
}

#[test]
fn a_field_a_config_role_names_cannot_be_removed() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);

    // `status` is the board field; `title` is the display title default.
    let board_error =
        remove_field(&config, &root, config_path(), "status", RemovedValues::Keep).unwrap_err();
    let display_error =
        remove_field(&config, &root, config_path(), "title", RemovedValues::Keep).unwrap_err();

    assert!(matches!(
        &board_error,
        SchemaWriteError::NamedByConfig { name, slots }
            if name == "status" && slots == "defaults.board_field"
    ));
    assert!(matches!(
        &display_error,
        SchemaWriteError::NamedByConfig { slots, .. } if slots == "defaults.display.title"
    ));
    assert_eq!(schema_text(&root), original);
}

// ── reorder_fields ───────────────────────────────────────────────

#[test]
fn reorder_fields_writes_the_given_order_and_keeps_every_entry() {
    let (_directory, root, config) = setup();
    let before = fields_tree(&root);
    let mut reversed = field_names(&before);
    reversed.reverse();

    let outcome = reorder_fields(&config, &root, config_path(), &reversed).unwrap();

    assert_eq!(outcome.field_name, None);
    assert!(!outcome.mutation_caused_warning);
    let after = fields_tree(&root);
    assert_eq!(field_names(&after), reversed);
    assert_untouched(&before, &after, &[]);
}

#[test]
fn reorder_fields_refuses_a_list_that_is_not_a_permutation() {
    let (_directory, root, config) = setup();
    let original = schema_text(&root);
    let mut missing_one = field_names(&fields_tree(&root));
    missing_one.pop();

    let error = reorder_fields(&config, &root, config_path(), &missing_one).unwrap_err();

    assert!(matches!(error, SchemaWriteError::InvalidOrder { .. }));
    assert_eq!(schema_text(&root), original);
}

#[test]
fn an_existing_schema_that_does_not_load_is_refused() {
    let (_directory, root, config) = setup();
    let broken = "fields:\n  status:\n    type: choice\n";
    fs::write(root.join(".workdown/schema.yaml"), broken).unwrap();

    let error = add_field(
        &config,
        &root,
        config_path(),
        "estimate",
        &definition(FieldType::Date, FieldShape::Scalar),
    )
    .unwrap_err();

    assert!(matches!(error, SchemaWriteError::ExistingInvalid { .. }));
    assert_eq!(schema_text(&root), broken);
}

// ── field_usage ──────────────────────────────────────────────────

#[test]
fn field_usage_lists_the_view_and_the_items_naming_the_field() {
    let (_directory, root, config) = setup();
    write_views(
        &root,
        "views:\n  - id: duplicates-graph\n    type: graph\n    field: duplicates\n",
    );
    write_item(&root, "a", "duplicates: [b]\n");
    write_item(&root, "b", "duplicates: [a]\n");
    write_item(&root, "c", "");
    let original = schema_text(&root);

    let usage = field_usage(&config, &root, config_path(), "duplicates").unwrap();

    assert_eq!(usage.parse_error, None);
    let view_findings = usage
        .introduced
        .iter()
        .filter(|diagnostic| diagnostic.view_id() == Some("duplicates-graph"))
        .count();
    assert_eq!(view_findings, 1, "got: {:?}", usage.introduced);
    let item_findings = usage
        .introduced
        .iter()
        .filter(|diagnostic| matches!(diagnostic.body, DiagnosticBody::Item(_)))
        .count();
    assert_eq!(item_findings, 2, "got: {:?}", usage.introduced);
    assert_eq!(schema_text(&root), original, "usage never writes");
}

#[test]
fn field_usage_names_the_rule_naming_the_field_beside_the_parse_error() {
    let (_directory, root, config) = setup();

    // `in-progress-needs-assignee` requires `assignee`: removing the
    // field makes the schema unloadable, and the rule is the blocker
    // the client lists.
    let usage = field_usage(&config, &root, config_path(), "assignee").unwrap();

    assert_eq!(usage.rules, vec!["in-progress-needs-assignee"]);
    assert!(usage.recipes.is_empty());
    let parse_error = usage.parse_error.expect("a rule names the field");
    assert!(
        parse_error.contains("in-progress-needs-assignee"),
        "parse error: {parse_error}"
    );
    assert!(usage.introduced.is_empty());
}

#[test]
fn field_usage_reports_the_config_role_naming_the_field() {
    let (_directory, root, config) = setup();

    let usage = field_usage(&config, &root, config_path(), "title").unwrap();

    assert_eq!(usage.parse_error, None);
    assert!(
        usage
            .introduced
            .iter()
            .any(|diagnostic| diagnostic.config_slot() == Some("defaults.display.title")),
        "got: {:?}",
        usage.introduced
    );
}

#[test]
fn field_usage_of_an_unknown_field_is_refused() {
    let (_directory, root, config) = setup();

    let error = field_usage(&config, &root, config_path(), "nope").unwrap_err();

    assert!(matches!(error, SchemaWriteError::FieldNotFound { name } if name == "nope"));
}
