//! Validate `config.yaml` against the schema — the config-file
//! counterpart to [`crate::views_check`].
//!
//! `views_check` validates the display roles a view sets in
//! `views.yaml`, but the project-wide role defaults in `config.yaml`
//! (`defaults.display`) are inherited by every view and validated
//! nowhere else. A typo'd field there is silently skipped at render
//! time (the extractors filter unresolvable names defensively), so the
//! user gets no signal that their default is dead. This module closes
//! that gap.
//!
//! The checks mirror the per-view role checks exactly: the text roles
//! (`title`, `subtitle`, `fields`) are existence-only — any field's
//! value renders as text — while `color` must name a `color`-typed
//! field, or be the `none` sentinel (always valid). The virtual `id`
//! is accepted everywhere, as it is for the per-view roles.
//!
//! Every diagnostic here is project-wide, not pinned to a view, so it
//! never marks a single view unrenderable: a bad default degrades every
//! view to its fallback rather than blanking it, and this is the signal
//! that the fallback is in effect. Validating the structural defaults
//! (`board_field`, `tree_field`, `graph_field`) is a separate concern —
//! those fail loudly at use time (e.g. `workdown move`) — and would grow
//! its own checks here if it lands.

use std::path::Path;

use crate::model::config::Config;
use crate::model::diagnostic::{ConfigDiagnosticKind, Diagnostic};
use crate::model::schema::{FieldType, Schema, Severity};
use crate::model::views::ColorRole;

/// Run all cross-file checks on `config.yaml` against a schema.
///
/// Returns one [`Diagnostic`] per problem found; does not stop at the
/// first. All diagnostics produced here have [`Severity::Error`] and are
/// pinned to `config_path`.
pub fn evaluate(config: &Config, schema: &Schema, config_path: &Path) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    check_display_defaults(config, schema, config_path, &mut out);
    out
}

/// Check the `defaults.display` role references. Same rules as the
/// per-view `check_display` in [`crate::views_check`], kept as a small
/// parallel implementation rather than shared machinery: the two emit
/// different diagnostic variants (config-scope here, view-scope there),
/// and the overlap is a few lines.
fn check_display_defaults(
    config: &Config,
    schema: &Schema,
    config_path: &Path,
    out: &mut Vec<Diagnostic>,
) {
    let display = &config.defaults.display;

    if let Some(field_name) = display.title.as_deref() {
        check_slot(
            schema,
            config_path,
            "defaults.display.title",
            field_name,
            &[],
            "",
            out,
        );
    }
    if let Some(field_name) = display.subtitle.as_deref() {
        check_slot(
            schema,
            config_path,
            "defaults.display.subtitle",
            field_name,
            &[],
            "",
            out,
        );
    }
    for field_name in &display.fields {
        check_slot(
            schema,
            config_path,
            "defaults.display.fields",
            field_name,
            &[],
            "",
            out,
        );
    }
    if let Some(ColorRole::Field(field_name)) = &display.color {
        check_slot(
            schema,
            config_path,
            "defaults.display.color",
            field_name,
            &[FieldType::Color],
            "color",
            out,
        );
    }
}

/// Check one role's field reference against the schema. Existence-only
/// when `allowed` is empty; otherwise the field's type must be in
/// `allowed`. The virtual `id` is always accepted (it isn't a schema
/// field) — matching `views_check::check_slot`, including its quirk that
/// `id` skips the type check.
fn check_slot(
    schema: &Schema,
    config_path: &Path,
    slot: &'static str,
    field_name: &str,
    allowed: &[FieldType],
    expected_label: &'static str,
    out: &mut Vec<Diagnostic>,
) {
    if field_name == "id" {
        return;
    }

    let Some(definition) = schema.fields.get(field_name) else {
        out.push(Diagnostic::config(
            Severity::Error,
            config_path.to_path_buf(),
            ConfigDiagnosticKind::ConfigDisplayUnknownField {
                slot,
                field_name: field_name.to_owned(),
            },
        ));
        return;
    };

    if allowed.is_empty() {
        return;
    }

    let actual = definition.field_type();
    if !allowed.contains(&actual) {
        out.push(Diagnostic::config(
            Severity::Error,
            config_path.to_path_buf(),
            ConfigDiagnosticKind::ConfigDisplayFieldTypeMismatch {
                slot,
                field_name: field_name.to_owned(),
                actual_type: actual,
                expected: expected_label.to_owned(),
            },
        ));
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::{Config, Paths, ProjectMeta, ViewDefaults};
    use crate::model::diagnostic::DiagnosticBody;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use crate::model::views::DisplayConfig;
    use indexmap::IndexMap;
    use std::path::{Path, PathBuf};

    fn config_path() -> &'static Path {
        Path::new(".workdown/config.yaml")
    }

    fn schema_with(fields: Vec<(&str, FieldTypeConfig)>) -> Schema {
        let mut map = IndexMap::new();
        for (name, cfg) in fields {
            map.insert(name.to_owned(), FieldDefinition::new(cfg));
        }
        let inverse_table = Schema::build_inverse_table(&map);
        Schema {
            fields: map,
            rules: vec![],
            inverse_table,
        }
    }

    fn config_with_display(display: DisplayConfig) -> Config {
        Config {
            project: ProjectMeta {
                name: "test".into(),
                description: String::new(),
            },
            paths: Paths {
                work_items: PathBuf::from("workdown-items"),
                templates: PathBuf::from(".workdown/templates"),
                resources: PathBuf::from(".workdown/resources.yaml"),
                views: PathBuf::from(".workdown/views.yaml"),
            },
            schema: PathBuf::from(".workdown/schema.yaml"),
            defaults: ViewDefaults {
                board_field: "status".into(),
                tree_field: "parent".into(),
                graph_field: "depends_on".into(),
                display,
            },
            working_days: None,
            serve: None,
        }
    }

    fn config_kind(diagnostic: &Diagnostic) -> &ConfigDiagnosticKind {
        match &diagnostic.body {
            DiagnosticBody::Config(config) => &config.kind,
            other => panic!("expected Config body, got {other:?}"),
        }
    }

    fn simple_schema() -> Schema {
        schema_with(vec![
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["open".into()],
                },
            ),
            ("title", FieldTypeConfig::String { pattern: None }),
            ("team_color", FieldTypeConfig::Color),
        ])
    }

    #[test]
    fn valid_display_defaults_produce_no_diagnostics() {
        let config = config_with_display(DisplayConfig {
            title: Some("title".into()),
            fields: vec!["id".into(), "status".into()],
            color: Some(ColorRole::Field("team_color".into())),
            ..DisplayConfig::default()
        });
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn no_display_defaults_produce_no_diagnostics() {
        let config = config_with_display(DisplayConfig::default());
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn unknown_text_role_field_errors_without_view_id() {
        let config = config_with_display(DisplayConfig {
            title: Some("titel".into()), // typo
            ..DisplayConfig::default()
        });
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1);
        // The diagnostic is project-wide — no view_id — so it never
        // trips the server's per-view unrenderable tier.
        assert_eq!(diagnostics[0].view_id(), None);
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigDisplayUnknownField { slot, field_name }
                if *slot == "defaults.display.title" && field_name == "titel"
        ));
    }

    #[test]
    fn color_default_none_sentinel_is_valid() {
        let config = config_with_display(DisplayConfig {
            color: Some(ColorRole::None),
            ..DisplayConfig::default()
        });
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn color_default_must_be_color_typed() {
        let config = config_with_display(DisplayConfig {
            color: Some(ColorRole::Field("status".into())), // choice, not color
            ..DisplayConfig::default()
        });
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigDisplayFieldTypeMismatch { slot, field_name, expected, .. }
                if *slot == "defaults.display.color" && field_name == "status" && expected == "color"
        ));
    }

    #[test]
    fn unknown_color_default_field_reports_unknown_not_mismatch() {
        let config = config_with_display(DisplayConfig {
            color: Some(ColorRole::Field("gone".into())),
            ..DisplayConfig::default()
        });
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1);
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigDisplayUnknownField { slot, field_name }
                if *slot == "defaults.display.color" && field_name == "gone"
        ));
    }

    #[test]
    fn id_accepted_in_text_roles() {
        let config = config_with_display(DisplayConfig {
            title: Some("id".into()),
            fields: vec!["id".into()],
            ..DisplayConfig::default()
        });
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }
}
