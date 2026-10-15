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
use workdown_core::model::rule::{CountConstraint, Severity};
use workdown_core::model::schema::{
    AggregateConfig, AggregateFunction, DefaultValue, FieldType, FieldTypeConfig, PullConfig,
    RoundMode,
};
use workdown_core::operations::schema_write::{
    add_field, add_rule, field_usage, preview_field, preview_rule, remove_field, remove_rule,
    reorder_fields, update_field, update_rule, DefaultWrite, FieldDefinitionWrite, RemovedValues,
    RuleDefinitionWrite, SchemaWriteError,
};
use workdown_core::parser::config::load_config;
use workdown_core::parser::schema::load_schema;
use workdown_core::rule_shape::{
    MatchRow, RequireRow, RequireTest, RuleShape, ScalarTest, ScalarValue,
};
use workdown_core::schema_definition_data::{FieldRecipe, FieldShape};

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
        aggregate: None,
        recipe: None,
    }
}

/// An integer field rolled up along `over`.
fn rollup(over: &str) -> FieldDefinitionWrite {
    let mut points = definition(
        FieldType::Integer,
        FieldShape::Numeric {
            min: None,
            max: None,
        },
    );
    points.aggregate = Some(AggregateConfig {
        function: AggregateFunction::Sum,
        error_on_missing: false,
        over: over.to_owned(),
    });
    points
}

/// A date field computed from `expression`, with no options set.
fn computed(expression: &str) -> FieldDefinitionWrite {
    let mut when_done = definition(FieldType::Date, FieldShape::Scalar);
    when_done.recipe = Some(FieldRecipe::Compute {
        expression: expression.to_owned(),
        round: RoundMode::Nearest,
        error_on_missing: false,
    });
    when_done
}

/// A date field pulling the earliest `field` of its dependencies.
fn pulled(field: &str) -> FieldDefinitionWrite {
    let mut earliest = definition(FieldType::Date, FieldShape::Scalar);
    earliest.recipe = Some(FieldRecipe::Pull(PullConfig {
        over: "depends_on".to_owned(),
        field: field.to_owned(),
        function: AggregateFunction::Min,
        error_on_missing: false,
    }));
    earliest
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
fn add_field_with_a_rollup_writes_the_aggregate_block_and_clearing_it_removes_the_block() {
    let (_directory, root, config) = setup();
    write_item(&root, "epic", "points: 2\n");
    write_item(&root, "task", "parent: epic\npoints: 3\n");

    let added = add_field(&config, &root, config_path(), "points", &rollup("parent")).unwrap();

    // The epic and its child both set the value by hand: the rollup
    // reports the chain, so the write goes through with that warning.
    assert!(added.mutation_caused_warning);
    assert_eq!(
        property_keys(&fields_tree(&root), "points"),
        ["type", "required", "aggregate"]
    );
    let reloaded = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    assert_eq!(
        reloaded.fields["points"].aggregate,
        Some(AggregateConfig {
            function: AggregateFunction::Sum,
            error_on_missing: false,
            over: "parent".to_owned(),
        })
    );

    let mut plain = rollup("parent");
    plain.aggregate = None;
    update_field(&config, &root, config_path(), "points", &plain).unwrap();

    assert_eq!(
        property_keys(&fields_tree(&root), "points"),
        ["type", "required"]
    );
    let reloaded = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    assert_eq!(reloaded.fields["points"].aggregate, None);
}

#[test]
fn add_field_with_a_compute_writes_the_bare_expression_and_its_options_when_set() {
    let (_directory, root, config) = setup();
    write_item(&root, "a", "");

    add_field(&config, &root, config_path(), "copy", &computed("created")).unwrap();

    assert_eq!(
        property_keys(&fields_tree(&root), "copy"),
        ["type", "required", "compute"]
    );
    assert!(schema_text(&root).contains("compute: created\n"));
    let reloaded = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    let compute = reloaded.fields["copy"].compute.as_ref().unwrap();
    assert_eq!(
        (compute.source.as_str(), compute.round),
        ("created", RoundMode::Nearest)
    );

    let mut strict = computed("created");
    strict.recipe = Some(FieldRecipe::Compute {
        expression: "created".to_owned(),
        round: RoundMode::Ceil,
        error_on_missing: true,
    });
    update_field(&config, &root, config_path(), "copy", &strict).unwrap();
    let reloaded = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    let compute = reloaded.fields["copy"].compute.as_ref().unwrap();
    assert_eq!(
        (compute.round, compute.error_on_missing),
        (RoundMode::Ceil, true)
    );

    let mut by_hand = computed("created");
    by_hand.recipe = None;
    update_field(&config, &root, config_path(), "copy", &by_hand).unwrap();
    assert_eq!(
        property_keys(&fields_tree(&root), "copy"),
        ["type", "required"]
    );
}

#[test]
fn add_field_with_a_pull_writes_the_block_and_clearing_the_recipe_removes_it() {
    let (_directory, root, config) = setup();
    write_item(&root, "a", "");
    write_item(&root, "b", "depends_on: [a]\n");

    add_field(
        &config,
        &root,
        config_path(),
        "earliest",
        &pulled("created"),
    )
    .unwrap();

    assert_eq!(
        property_keys(&fields_tree(&root), "earliest"),
        ["type", "required", "pull"]
    );
    let reloaded = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    assert_eq!(
        reloaded.fields["earliest"].pull,
        Some(PullConfig {
            over: "depends_on".to_owned(),
            field: "created".to_owned(),
            function: AggregateFunction::Min,
            error_on_missing: false,
        })
    );

    let mut by_hand = pulled("created");
    by_hand.recipe = None;
    update_field(&config, &root, config_path(), "earliest", &by_hand).unwrap();

    assert_eq!(
        property_keys(&fields_tree(&root), "earliest"),
        ["type", "required"]
    );
    let reloaded = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    assert_eq!(reloaded.fields["earliest"].pull, None);
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
fn update_field_writes_the_relation_settings_and_clearing_them_removes_the_keys() {
    let (_directory, root, config) = setup();
    let before = fields_tree(&root);
    let forbidding = definition(
        FieldType::Links,
        FieldShape::Relation {
            allow_cycles: Some(false),
            inverse: Some("linked_from".to_owned()),
        },
    );

    update_field(&config, &root, config_path(), "related_to", &forbidding).unwrap();

    let after = fields_tree(&root);
    assert_untouched(&before, &after, &["related_to"]);
    let entry = after["related_to"].as_mapping().unwrap();
    assert_eq!(
        entry.get("allow_cycles"),
        Some(&serde_yaml::Value::Bool(false))
    );
    assert_eq!(
        entry.get("inverse"),
        Some(&serde_yaml::Value::String("linked_from".to_owned()))
    );

    let cleared = definition(
        FieldType::Links,
        FieldShape::Relation {
            allow_cycles: None,
            inverse: None,
        },
    );

    update_field(&config, &root, config_path(), "related_to", &cleared).unwrap();

    assert_eq!(
        property_keys(&fields_tree(&root), "related_to"),
        ["type", "required"]
    );
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

// ── Rules ────────────────────────────────────────────────────────

/// The `rules:` sequence of the file on disk, as a generic tree.
fn rules_tree(root: &Path) -> Vec<serde_yaml::Value> {
    let document: serde_yaml::Value = serde_yaml::from_str(&schema_text(root)).unwrap();
    document["rules"].as_sequence().unwrap().clone()
}

fn rule(name: &str, shape: RuleShape) -> RuleDefinitionWrite {
    RuleDefinitionWrite {
        name: name.to_owned(),
        description: None,
        severity: Severity::default(),
        shape,
    }
}

/// `match: {status: open}`, `require: {priority: required}`.
fn open_needs_priority() -> RuleShape {
    RuleShape {
        match_rows: vec![MatchRow {
            reference: "status".to_owned(),
            quantifier: None,
            test: ScalarTest::Equals {
                value: ScalarValue::Text("open".to_owned()),
            },
        }],
        require_rows: vec![RequireRow {
            reference: "priority".to_owned(),
            test: RequireTest::Required,
        }],
        count: None,
    }
}

fn require_only(reference: &str) -> RuleShape {
    RuleShape {
        match_rows: Vec::new(),
        require_rows: vec![RequireRow {
            reference: reference.to_owned(),
            test: RequireTest::Required,
        }],
        count: None,
    }
}

/// An equality beside another test on one reference: rows the writer
/// cannot express as one condition.
fn unwritable_rows() -> RuleShape {
    RuleShape {
        match_rows: vec![
            MatchRow {
                reference: "status".to_owned(),
                quantifier: None,
                test: ScalarTest::Equals {
                    value: ScalarValue::Text("open".to_owned()),
                },
            },
            MatchRow {
                reference: "status".to_owned(),
                quantifier: None,
                test: ScalarTest::IsSet { value: true },
            },
        ],
        require_rows: vec![RequireRow {
            reference: "priority".to_owned(),
            test: RequireTest::Required,
        }],
        count: None,
    }
}

#[test]
fn add_rule_appends_and_leaves_fields_and_other_rules_untouched() {
    let (_directory, root, config) = setup();
    let before_fields = fields_tree(&root);
    let before_rules = rules_tree(&root);
    let mut definition = rule("open-needs-priority", open_needs_priority());
    definition.description = Some("Open items carry a priority".to_owned());
    definition.severity = Severity::Warning;

    let outcome = add_rule(&config, &root, config_path(), &definition).unwrap();

    assert_eq!(outcome.rule_name.as_deref(), Some("open-needs-priority"));
    assert_eq!(outcome.field_name, None);
    let after_rules = rules_tree(&root);
    assert_eq!(after_rules.len(), before_rules.len() + 1);
    assert_eq!(&after_rules[..before_rules.len()], &before_rules[..]);
    let added = after_rules.last().unwrap();
    assert_eq!(added["name"], "open-needs-priority");
    assert_eq!(added["description"], "Open items carry a priority");
    assert_eq!(added["severity"], "warning");
    assert_eq!(added["match"]["status"], "open");
    assert_eq!(added["require"]["priority"], "required");
    assert_eq!(fields_tree(&root), before_fields);
    let schema = load_schema(&root.join(".workdown/schema.yaml")).unwrap();
    assert!(schema
        .rules
        .iter()
        .any(|rule| rule.name == "open-needs-priority"));
}

#[test]
fn add_rule_reports_the_items_it_now_flags() {
    let (_directory, root, config) = setup();
    write_item(&root, "alpha", "");

    let outcome = add_rule(
        &config,
        &root,
        config_path(),
        &rule("open-needs-priority", open_needs_priority()),
    )
    .unwrap();

    assert!(outcome.mutation_caused_warning);
    assert!(!outcome.warnings.is_empty());
}

#[test]
fn add_rule_refuses_a_taken_or_invalid_name() {
    let (_directory, root, config) = setup();
    let before = schema_text(&root);

    let taken = add_rule(
        &config,
        &root,
        config_path(),
        &rule("in-progress-needs-assignee", require_only("assignee")),
    )
    .unwrap_err();
    assert!(
        matches!(taken, SchemaWriteError::DuplicateRuleName { .. }),
        "{taken}"
    );

    let invalid = add_rule(
        &config,
        &root,
        config_path(),
        &rule("Needs_Assignee", require_only("assignee")),
    )
    .unwrap_err();
    assert!(
        matches!(invalid, SchemaWriteError::InvalidRuleName { .. }),
        "{invalid}"
    );
    assert_eq!(schema_text(&root), before);
}

#[test]
fn add_rule_refuses_rows_that_cannot_be_written() {
    let (_directory, root, config) = setup();
    let before = schema_text(&root);

    let error = add_rule(
        &config,
        &root,
        config_path(),
        &rule("open-is-set", unwritable_rows()),
    )
    .unwrap_err();

    assert!(
        matches!(error, SchemaWriteError::InvalidRule { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("'status'"), "{error}");
    assert_eq!(schema_text(&root), before);
}

#[test]
fn add_rule_refuses_a_candidate_the_parser_rejects() {
    let (_directory, root, config) = setup();
    let before = schema_text(&root);

    let error = add_rule(
        &config,
        &root,
        config_path(),
        &rule("needs-nothing", require_only("no_such_field")),
    )
    .unwrap_err();

    assert!(
        matches!(error, SchemaWriteError::Unloadable { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("no_such_field"), "{error}");
    assert_eq!(schema_text(&root), before);
}

#[test]
fn update_rule_replaces_in_place_and_may_rename() {
    let (_directory, root, config) = setup();
    let before_rules = rules_tree(&root);
    let mut shape = require_only("assignee");
    shape.count = Some(CountConstraint {
        min: None,
        max: Some(5),
    });

    let outcome = update_rule(
        &config,
        &root,
        config_path(),
        "in-progress-needs-assignee",
        &rule("in-progress-capped", shape),
    )
    .unwrap();

    assert_eq!(outcome.rule_name.as_deref(), Some("in-progress-capped"));
    let after_rules = rules_tree(&root);
    assert_eq!(after_rules.len(), before_rules.len());
    assert_eq!(after_rules[0]["name"], "in-progress-capped");
    assert_eq!(after_rules[0]["require"]["assignee"], "required");
    assert_eq!(after_rules[0]["count"]["max"], 5);
    assert!(after_rules[0].get("match").is_none());
    assert!(after_rules[0].get("severity").is_none());
    assert_eq!(after_rules[1..], before_rules[1..]);
}

#[test]
fn update_rule_refuses_renaming_onto_another_rule() {
    let (_directory, root, config) = setup();
    let before = schema_text(&root);

    let error = update_rule(
        &config,
        &root,
        config_path(),
        "in-progress-needs-assignee",
        &rule("bugs-need-priority", require_only("assignee")),
    )
    .unwrap_err();

    assert!(
        matches!(error, SchemaWriteError::DuplicateRuleName { .. }),
        "{error}"
    );
    assert_eq!(schema_text(&root), before);
}

#[test]
fn remove_rule_cuts_the_entry_and_keeps_the_rest() {
    let (_directory, root, config) = setup();
    let before_fields = fields_tree(&root);
    let before_rules = rules_tree(&root);

    let outcome = remove_rule(&config, &root, config_path(), "in-progress-needs-assignee").unwrap();

    assert_eq!(
        outcome.rule_name.as_deref(),
        Some("in-progress-needs-assignee")
    );
    let after_rules = rules_tree(&root);
    assert_eq!(after_rules, before_rules[1..]);
    assert_eq!(fields_tree(&root), before_fields);
}

#[test]
fn an_unknown_rule_is_not_found() {
    let (_directory, root, config) = setup();

    let removed = remove_rule(&config, &root, config_path(), "no-such-rule").unwrap_err();
    assert!(
        matches!(removed, SchemaWriteError::RuleNotFound { .. }),
        "{removed}"
    );

    let updated = update_rule(
        &config,
        &root,
        config_path(),
        "no-such-rule",
        &rule("no-such-rule", require_only("assignee")),
    )
    .unwrap_err();
    assert!(
        matches!(updated, SchemaWriteError::RuleNotFound { .. }),
        "{updated}"
    );
}

#[test]
fn preview_field_shows_the_entry_and_the_verdict_without_writing() {
    let (_directory, root, config) = setup();
    let before = schema_text(&root);

    let addition = preview_field(
        &config,
        &root,
        config_path(),
        "points",
        false,
        &rollup("parent"),
    )
    .unwrap();
    assert_eq!(
        addition.yaml,
        "type: integer\nrequired: false\naggregate:\n  function: sum\n  over: parent"
    );
    assert_eq!(addition.problem, None);

    let taken_name = preview_field(
        &config,
        &root,
        config_path(),
        "priority",
        false,
        &rollup("parent"),
    )
    .unwrap();
    let problem = taken_name.problem.expect("the add would refuse the name");
    assert!(problem.contains("already exists"), "{problem}");

    let bad_name =
        preview_field(&config, &root, config_path(), "", false, &rollup("parent")).unwrap();
    assert!(bad_name.problem.unwrap().contains("invalid field name"));

    let over_links = preview_field(
        &config,
        &root,
        config_path(),
        "points",
        false,
        &rollup("depends_on"),
    )
    .unwrap();
    let problem = over_links
        .problem
        .expect("a rollup over a links field is a parse error");
    assert!(problem.contains("must be 'link'"), "{problem}");

    // Replacing keeps what the write keeps: the `when` block and, beside
    // it, the `default`, both of which the draft does not carry.
    let mut tint = definition(FieldType::Color, FieldShape::Scalar);
    tint.description = Some("Tint".to_owned());
    let replacement =
        preview_field(&config, &root, config_path(), "status_color", true, &tint).unwrap();
    assert_eq!(replacement.problem, None);
    assert!(replacement.yaml.contains("when:"), "{}", replacement.yaml);
    assert!(
        replacement.yaml.contains("default: gray"),
        "{}",
        replacement.yaml
    );
    assert!(
        replacement.yaml.contains("description: Tint"),
        "{}",
        replacement.yaml
    );

    let unknown = preview_field(&config, &root, config_path(), "no_such", true, &tint).unwrap();
    assert!(unknown.problem.unwrap().contains("no field named"));

    assert_eq!(schema_text(&root), before);
}

#[test]
fn preview_field_reports_a_formulas_problem_with_the_column_the_text_names() {
    let (_directory, root, config) = setup();
    let before = schema_text(&root);
    let preview = |definition: &FieldDefinitionWrite| {
        preview_field(&config, &root, config_path(), "copy", false, definition).unwrap()
    };

    let sound = preview(&computed("created"));
    assert_eq!(sound.yaml, "type: date\nrequired: false\ncompute: created");
    assert_eq!((sound.problem, sound.problem_column), (None, None));

    // A syntax error is the schema parser's refusal; the column comes
    // from the expression parser and matches the text.
    let unparsable = preview(&computed("created + )"));
    let problem = unparsable
        .problem
        .expect("a formula with a stray ')' is a problem");
    let column = unparsable
        .problem_column
        .expect("a stray token has a column");
    assert!(
        problem.contains(&format!("at column {column}")),
        "{problem}"
    );

    // A type error passes the parser and comes from the load; the column
    // is the type check's.
    let mistyped = preview(&computed("created + created"));
    let problem = mistyped.problem.expect("adding two dates is a problem");
    let column = mistyped
        .problem_column
        .expect("an invalid operation has a column");
    assert!(problem.contains("cannot apply '+'"), "{problem}");
    assert!(
        problem.contains(&format!("at column {column}")),
        "{problem}"
    );

    // A formula that ends early has no spot to point at.
    let unfinished = preview(&computed("created +"));
    assert!(unfinished.problem.is_some());
    assert_eq!(unfinished.problem_column, None);

    assert_eq!(schema_text(&root), before);
}

#[test]
fn preview_field_reports_a_pull_the_load_would_disable() {
    let (_directory, root, config) = setup();
    let before = schema_text(&root);

    let sound = preview_field(
        &config,
        &root,
        config_path(),
        "earliest",
        false,
        &pulled("created"),
    )
    .unwrap();
    assert_eq!(
        sound.yaml,
        "type: date\nrequired: false\npull:\n  over: depends_on\n  field: created\n  function: min"
    );
    assert_eq!(sound.problem, None);

    // The parser takes the block as written; it is the load's config
    // check that disables the field. Decision 10's rejected recipe.
    let mut mismatched = pulled("created");
    mismatched.field_type = FieldType::Integer;
    mismatched.shape = FieldShape::Numeric {
        min: None,
        max: None,
    };
    let problem = preview_field(
        &config,
        &root,
        config_path(),
        "earliest",
        false,
        &mismatched,
    )
    .unwrap()
    .problem
    .expect("a date reduced into an integer field is a problem");
    assert!(
        problem.contains("produces date, but the field is declared integer"),
        "{problem}"
    );

    // A default beside a pull is the parser's refusal.
    let mut defaulted = pulled("created");
    defaulted.default = Some(DefaultWrite::Literal {
        value: serde_yaml::Value::String("2026-01-01".to_owned()),
    });
    let problem = preview_field(&config, &root, config_path(), "earliest", false, &defaulted)
        .unwrap()
        .problem
        .expect("a default beside a pull is a problem");
    assert!(
        problem.contains("'pull' and 'default' cannot be combined"),
        "{problem}"
    );

    assert_eq!(schema_text(&root), before);
}

#[test]
fn preview_rule_shows_the_entry_and_the_parsers_verdict_without_writing() {
    let (_directory, root, config) = setup();
    let before = schema_text(&root);

    let addition = preview_rule(
        &config,
        &root,
        None,
        &rule("open-needs-priority", open_needs_priority()),
    )
    .unwrap();
    assert_eq!(
        addition.yaml,
        "name: open-needs-priority\nmatch:\n  status: open\nrequire:\n  priority: required"
    );
    assert_eq!(addition.problem, None);

    let taken_name = preview_rule(
        &config,
        &root,
        None,
        &rule("in-progress-needs-assignee", require_only("assignee")),
    )
    .unwrap();
    let problem = taken_name
        .problem
        .expect("a second rule with that name is a parse error");
    assert!(problem.contains("duplicate rule name"), "{problem}");

    let replacing_itself = preview_rule(
        &config,
        &root,
        Some("in-progress-needs-assignee"),
        &rule("in-progress-needs-assignee", require_only("assignee")),
    )
    .unwrap();
    assert_eq!(replacing_itself.problem, None);

    let unknown_field = preview_rule(
        &config,
        &root,
        None,
        &rule("needs-nothing", require_only("no_such_field")),
    )
    .unwrap();
    let problem = unknown_field
        .problem
        .expect("a reference to no field is a parse error");
    assert!(problem.contains("no_such_field"), "{problem}");

    let unwritable = preview_rule(
        &config,
        &root,
        None,
        &rule("open-is-set", unwritable_rows()),
    )
    .unwrap();
    assert_eq!(unwritable.yaml, "");
    assert!(unwritable.problem.unwrap().contains("cannot share"));

    assert_eq!(schema_text(&root), before);
}
