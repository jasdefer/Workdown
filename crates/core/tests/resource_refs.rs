//! Integration tests for `resource:`-backed value validation.
//!
//! Two halves meet here. The per-item warning lives in the store so that
//! every path building one reports it; the section- and default-level
//! findings live in `resources_check` and are pinned to `schema.yaml`.
//! `validate` is where both surface together, which is what these tests
//! exercise — including the rule that an unusable option set is reported
//! once against the schema and never per item.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use workdown_core::model::config::Config;
use workdown_core::model::diagnostic::{
    ConfigDiagnosticKind, Diagnostic, DiagnosticBody, ItemDiagnosticKind,
};
use workdown_core::model::schema::Severity;
use workdown_core::operations::validate::validate;
use workdown_core::parser::config::load_config;

// ── Fixture helper ──────────────────────────────────────────────────────

/// Stage a project with a schema, an optional `resources.yaml`, and work
/// items. `None` for the resources argument leaves the file absent.
fn setup_project(
    schema_yaml: &str,
    resources_yaml: Option<&str>,
    items: &[(&str, &str)],
) -> (TempDir, Config, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().to_path_buf();

    fs::create_dir_all(root.join(".workdown")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();

    let config_yaml = "\
project:
  name: Test
  description: \"\"
paths:
  work_items: workdown-items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
";
    fs::write(root.join(".workdown/config.yaml"), config_yaml).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), schema_yaml).unwrap();
    if let Some(yaml) = resources_yaml {
        fs::write(root.join(".workdown/resources.yaml"), yaml).unwrap();
    }
    for (name, content) in items {
        fs::write(root.join("workdown-items").join(name), content).unwrap();
    }

    let config = load_config(&root.join(".workdown/config.yaml")).unwrap();
    (tmp, config, root)
}

fn run_validate(project: &(TempDir, Config, PathBuf)) -> Vec<Diagnostic> {
    let (_tmp, config, root) = project;
    validate(
        config,
        root.as_path(),
        std::path::Path::new(".workdown/config.yaml"),
        None,
    )
    .unwrap()
    .diagnostics
}

/// Schema with a single-valued and a multi-valued resource-backed field.
const SCHEMA_WITH_ASSIGNEE: &str = "\
fields:
  assignee:
    type: string
    resource: people
  reviewers:
    type: list
    resource: people
";

const POPULATED_PEOPLE: &str = "\
people:
  - id: alice
    name: Alice Smith
  - id: bob
";

fn unknown_resource_refs(diagnostics: &[Diagnostic]) -> Vec<(&str, &str, &str)> {
    diagnostics
        .iter()
        .filter_map(|diagnostic| match &diagnostic.body {
            DiagnosticBody::Item(item) => match &item.kind {
                ItemDiagnosticKind::UnknownResourceRef {
                    field,
                    section,
                    value,
                } => Some((field.as_str(), section.as_str(), value.as_str())),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn config_kinds(diagnostics: &[Diagnostic]) -> Vec<&ConfigDiagnosticKind> {
    diagnostics
        .iter()
        .filter_map(|diagnostic| match &diagnostic.body {
            DiagnosticBody::Config(config) => Some(&config.kind),
            _ => None,
        })
        .collect()
}

// ── Per-item values ─────────────────────────────────────────────────────

#[test]
fn known_entry_validates_without_warning() {
    let project = setup_project(
        SCHEMA_WITH_ASSIGNEE,
        Some(POPULATED_PEOPLE),
        &[("login.md", "---\nassignee: alice\n---\n")],
    );
    let diagnostics = run_validate(&project);

    assert!(
        diagnostics.is_empty(),
        "a known assignee should produce nothing: {diagnostics:?}"
    );
}

#[test]
fn unknown_entry_warns_but_keeps_the_item() {
    let project = setup_project(
        SCHEMA_WITH_ASSIGNEE,
        Some(POPULATED_PEOPLE),
        &[("login.md", "---\nassignee: carol\n---\n")],
    );
    let diagnostics = run_validate(&project);

    assert_eq!(
        unknown_resource_refs(&diagnostics),
        vec![("assignee", "people", "carol")]
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.severity == Severity::Warning),
        "an unknown resource reference must not block: {diagnostics:?}"
    );
}

#[test]
fn every_unknown_element_of_a_list_field_warns() {
    let project = setup_project(
        SCHEMA_WITH_ASSIGNEE,
        Some(POPULATED_PEOPLE),
        &[("login.md", "---\nreviewers: [alice, carol, dave]\n---\n")],
    );
    let diagnostics = run_validate(&project);

    assert_eq!(
        unknown_resource_refs(&diagnostics),
        vec![
            ("reviewers", "people", "carol"),
            ("reviewers", "people", "dave"),
        ]
    );
}

#[test]
fn absent_value_never_warns() {
    let project = setup_project(
        SCHEMA_WITH_ASSIGNEE,
        Some(POPULATED_PEOPLE),
        &[("login.md", "---\ntitle: Login\n---\n")],
    );
    let diagnostics = run_validate(&project);

    assert!(
        unknown_resource_refs(&diagnostics).is_empty(),
        "an unset field is not an unknown reference: {diagnostics:?}"
    );
}

#[test]
fn stamped_default_is_held_to_the_same_standard() {
    // The default is a valid entry, so the schema itself is clean; the
    // item's hand-written value is the only thing that warns. Proves the
    // check reads final values rather than only what the parser saw.
    let schema = "\
fields:
  assignee:
    type: string
    resource: people
    default: alice
";
    let project = setup_project(
        schema,
        Some(POPULATED_PEOPLE),
        &[("login.md", "---\nassignee: carol\n---\n")],
    );
    let diagnostics = run_validate(&project);

    assert_eq!(
        unknown_resource_refs(&diagnostics),
        vec![("assignee", "people", "carol")]
    );
}

#[test]
fn conditional_value_is_checked_like_a_written_one() {
    // `when:` derives the value at load; it never touches the file, and
    // the check runs after the derive passes, so it still sees it.
    let schema = "\
fields:
  urgent:
    type: boolean
  assignee:
    type: string
    resource: people
    when:
      - if: urgent == true
        then: carol
";
    let project = setup_project(
        schema,
        Some(POPULATED_PEOPLE),
        &[("login.md", "---\nurgent: true\n---\n")],
    );
    let diagnostics = run_validate(&project);

    assert_eq!(
        unknown_resource_refs(&diagnostics),
        vec![("assignee", "people", "carol")]
    );
}

// ── Unusable option sets ────────────────────────────────────────────────

#[test]
fn empty_section_warns_once_and_silences_the_items() {
    let project = setup_project(
        SCHEMA_WITH_ASSIGNEE,
        Some("people:\n"),
        &[
            ("login.md", "---\nassignee: carol\n---\n"),
            ("logout.md", "---\nassignee: dave\n---\n"),
        ],
    );
    let diagnostics = run_validate(&project);

    assert!(
        unknown_resource_refs(&diagnostics).is_empty(),
        "an empty section has nothing to validate against: {diagnostics:?}"
    );
    let kinds = config_kinds(&diagnostics);
    assert_eq!(kinds.len(), 2, "one per referencing field: {kinds:?}");
    assert!(kinds.iter().all(|kind| matches!(
        kind,
        ConfigDiagnosticKind::ResourceSectionEmpty { section, .. } if section == "people"
    )));
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic.severity == Severity::Warning));
}

#[test]
fn absent_resources_file_warns_rather_than_erroring() {
    let project = setup_project(
        SCHEMA_WITH_ASSIGNEE,
        None,
        &[("login.md", "---\nassignee: carol\n---\n")],
    );
    let diagnostics = run_validate(&project);

    assert!(unknown_resource_refs(&diagnostics).is_empty());
    assert!(config_kinds(&diagnostics)
        .iter()
        .all(|kind| matches!(kind, ConfigDiagnosticKind::ResourceSectionEmpty { .. })));
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic.severity == Severity::Warning));
}

#[test]
fn malformed_resources_file_is_one_read_error_with_downgraded_sections() {
    // A resources.yaml that fails to parse must not cascade: one Error
    // against the file itself, and the schema's sections downgrade to
    // the same "nothing to validate against" warnings an absent file
    // gets — never `ResourceSectionUnknown` errors (the section may
    // well exist in the broken file), never per-item noise.
    let project = setup_project(
        SCHEMA_WITH_ASSIGNEE,
        Some("people: [\n"),
        &[("login.md", "---\nassignee: carol\n---\n")],
    );
    let diagnostics = run_validate(&project);

    assert!(unknown_resource_refs(&diagnostics).is_empty());

    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect();
    let (_tmp, _config, root) = &project;
    assert_eq!(errors.len(), 1, "got: {diagnostics:?}");
    assert_eq!(
        errors[0].source_path(),
        Some(root.join(".workdown/resources.yaml").as_path())
    );

    let kinds = config_kinds(&diagnostics);
    assert_eq!(kinds.len(), 2, "one per referencing field: {kinds:?}");
    assert!(kinds
        .iter()
        .all(|kind| matches!(kind, ConfigDiagnosticKind::ResourceSectionEmpty { .. })));
}

#[test]
fn misspelled_section_errors_against_the_schema() {
    let schema = "\
fields:
  assignee:
    type: string
    resource: peple
";
    let project = setup_project(
        schema,
        Some(POPULATED_PEOPLE),
        &[("login.md", "---\nassignee: alice\n---\n")],
    );
    let diagnostics = run_validate(&project);

    assert!(
        unknown_resource_refs(&diagnostics).is_empty(),
        "the typo is reported against schema.yaml, not against every item"
    );
    let kinds = config_kinds(&diagnostics);
    assert!(
        matches!(
            kinds.as_slice(),
            [ConfigDiagnosticKind::ResourceSectionUnknown { field, section }]
                if field == "assignee" && section == "peple"
        ),
        "{kinds:?}"
    );
    let (_tmp, _config, root) = &project;
    assert_eq!(
        diagnostics[0].source_path(),
        Some(root.join(".workdown/schema.yaml").as_path())
    );
    assert_eq!(diagnostics[0].severity, Severity::Error);
}

// ── Defaults ────────────────────────────────────────────────────────────

#[test]
fn default_outside_the_section_errors() {
    let schema = "\
fields:
  assignee:
    type: string
    resource: people
    default: carol
";
    let project = setup_project(schema, Some(POPULATED_PEOPLE), &[]);
    let diagnostics = run_validate(&project);

    let kinds = config_kinds(&diagnostics);
    assert!(
        matches!(
            kinds.as_slice(),
            [ConfigDiagnosticKind::ResourceDefaultUnknown { field, section, value }]
                if field == "assignee" && section == "people" && value == "carol"
        ),
        "{kinds:?}"
    );
    assert_eq!(diagnostics[0].severity, Severity::Error);
}

#[test]
fn generator_default_errors_even_against_an_empty_section() {
    // `$uuid` type-checks against `string` and so passes the parser, but
    // no generator can produce an entry — true whatever the list holds.
    let schema = "\
fields:
  assignee:
    type: string
    resource: people
    default: $uuid
";
    let project = setup_project(schema, Some("people:\n"), &[]);
    let diagnostics = run_validate(&project);

    let kinds = config_kinds(&diagnostics);
    assert!(
        kinds.iter().any(|kind| matches!(
            kind,
            ConfigDiagnosticKind::ResourceDefaultGenerator { field, generator, .. }
                if field == "assignee" && generator == "$uuid"
        )),
        "{kinds:?}"
    );
}
