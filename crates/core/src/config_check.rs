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
//! The rules are identical to the per-view role checks by construction:
//! both delegate to the crate-private `display_check` module, which
//! owns the role vocabulary's constraints in one place; this module
//! only wraps each violation into a config-scoped diagnostic.
//!
//! The second half of this module checks the *field-role* keys —
//! `board_field`, `tree_field`, `graph_field`, `effort_field` — the
//! project's answer to "which field plays this role". Nothing checked
//! the first three before: only `board_field` has a consumer at all
//! (`workdown move`, which reports its own miss when you run it), and
//! a typo in the other two sat in the file unreported forever.
//! `effort_field` needs the eager check most: its consumer is a timer
//! in the web app with no command to run and no error to print, and
//! unset legitimately means "no timer" — so without this check a typo
//! and a deliberate blank would be indistinguishable.
//!
//! Every diagnostic here is project-wide, not pinned to a view, so it
//! never marks a single view unrenderable: a bad default degrades every
//! view to its fallback rather than blanking it, and this is the signal
//! that the fallback is in effect.
//!
//! The two halves differ in severity, and deliberately. A bad
//! `defaults.display` value silently makes every view render wrong, so
//! it is an error. A bad field-role key makes nothing render wrong —
//! `workdown move` prints its own message, the tree and graph keys are
//! read by no code at all — so it is a warning, and `workdown validate`
//! keeps exiting zero.

use std::path::Path;

use crate::display_check::{check_display_roles, RoleViolation};
use crate::model::config::{Config, ViewDefaults};
use crate::model::diagnostic::{ConfigDiagnosticKind, Diagnostic};
use crate::model::schema::{FieldType, Schema, Severity};
use crate::model::view_slots;

/// Run all cross-file checks on `config.yaml` against a schema.
///
/// Returns one [`Diagnostic`] per problem found; does not stop at the
/// first. All diagnostics are pinned to `config_path`: display-role
/// defaults at [`Severity::Error`], field-role keys at
/// [`Severity::Warning`] (see the module docs).
pub fn evaluate(config: &Config, schema: &Schema, config_path: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for violation in check_display_roles(&config.defaults.display, schema) {
        let kind = match violation {
            RoleViolation::UnknownField { role, field_name } => {
                ConfigDiagnosticKind::ConfigUnknownField {
                    slot: role.config_slot(),
                    field_name,
                }
            }
            RoleViolation::TypeMismatch {
                role,
                field_name,
                actual_type,
                expected,
            } => ConfigDiagnosticKind::ConfigFieldTypeMismatch {
                slot: role.config_slot(),
                field_name,
                actual_type,
                expected: expected.to_owned(),
            },
        };
        diagnostics.push(Diagnostic::config(
            Severity::Error,
            config_path.to_path_buf(),
            kind,
        ));
    }

    for role in field_roles(&config.defaults) {
        check_field_role(&role, schema, config_path, &mut diagnostics);
    }

    diagnostics
}

// ── Field roles ──────────────────────────────────────────────────────

/// One field-role key as configured, paired with the rule its field has
/// to satisfy.
struct FieldRoleReference<'a> {
    /// The key's path, as the diagnostic reports it.
    slot: &'static str,
    /// The field name the project wrote there.
    field_name: &'a str,
    /// Types that can play this role. Taken from the matching slot in
    /// `model::view_slots` wherever the role stands in for a view slot,
    /// so the two cannot come apart; the mismatch message is worded from
    /// this list rather than written beside it.
    allowed: &'static [FieldType],
    /// Whether an inverse relation name (declared via `inverse:` on a
    /// link field) may stand in for a field of its own.
    inverse_names_allowed: bool,
}

/// Pair every field-role key in `config.yaml` with its rule.
///
/// Each rule reads the matching slot's accepted types from
/// [`crate::model::view_slots`], so a field the tool already accepts for
/// a board, a tree or a graph is never reported here — deliberately not
/// stricter: `workdown move` type-checks nothing at all today, so a
/// `string` board field works and must not be flagged as if it were
/// broken.
///
/// The defaults are destructured so a field role added to
/// [`ViewDefaults`] fails to compile here rather than silently going
/// unvalidated.
fn field_roles(defaults: &ViewDefaults) -> Vec<FieldRoleReference<'_>> {
    let ViewDefaults {
        board_field,
        tree_field,
        graph_field,
        effort_field,
        display: _, // checked by `check_display_roles`, not as a field role
    } = defaults;

    let mut roles = vec![
        FieldRoleReference {
            slot: "defaults.board_field",
            field_name: board_field,
            allowed: view_slots::BOARD_FIELD.accepts,
            inverse_names_allowed: false,
        },
        FieldRoleReference {
            slot: "defaults.tree_field",
            field_name: tree_field,
            allowed: view_slots::TREE_FIELD.accepts,
            inverse_names_allowed: false,
        },
        FieldRoleReference {
            slot: "defaults.graph_field",
            field_name: graph_field,
            // Inverse names resolve to their original field at
            // extraction time, exactly as the graph view's own slot
            // accepts them.
            allowed: view_slots::GRAPH_FIELD.accepts,
            inverse_names_allowed: true,
        },
    ];

    // Unlike its three siblings the key is optional, and unset is a
    // normal state — it means "no effort field", and the surfaces that
    // would need one are simply absent. An empty string is not that
    // state's second spelling: it falls through to the unknown-field
    // warning below, because a deleted value with the key left behind
    // is a typo, not a decision.
    if let Some(effort_field) = effort_field {
        roles.push(FieldRoleReference {
            slot: "defaults.effort_field",
            field_name: effort_field,
            // No view slot to borrow from: the timer writes a duration,
            // and this role exists only in config.yaml.
            allowed: &[FieldType::Duration],
            inverse_names_allowed: false,
        });
    }

    roles
}

/// Check that one field-role key names a field that exists and can play
/// the role. Existence and type only — whether the field is computed,
/// aggregated or pulled is not this check's business.
fn check_field_role(
    role: &FieldRoleReference<'_>,
    schema: &Schema,
    config_path: &Path,
    out: &mut Vec<Diagnostic>,
) {
    let warn = |kind| Diagnostic::config(Severity::Warning, config_path.to_path_buf(), kind);

    // Rejected by name, before the schema is consulted, so the verdict
    // does not depend on whether the project declares `id` itself —
    // this is how the structural view slots treat it too.
    if role.field_name == "id" {
        out.push(warn(ConfigDiagnosticKind::ConfigVirtualIdNotAllowed {
            slot: role.slot,
        }));
        return;
    }

    if let Some(definition) = schema.fields.get(role.field_name) {
        let actual_type = definition.field_type();
        if !role.allowed.contains(&actual_type) {
            out.push(warn(ConfigDiagnosticKind::ConfigFieldTypeMismatch {
                slot: role.slot,
                field_name: role.field_name.to_owned(),
                actual_type,
                expected: view_slots::describe(role.allowed),
            }));
        }
        return;
    }

    if role.inverse_names_allowed && schema.inverse_table.contains_key(role.field_name) {
        return;
    }

    out.push(warn(ConfigDiagnosticKind::ConfigUnknownField {
        slot: role.slot,
        field_name: role.field_name.to_owned(),
    }));
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::config::{Config, Paths, ProjectMeta, ViewDefaults};
    use crate::model::diagnostic::DiagnosticBody;
    use crate::model::schema::{FieldDefinition, FieldTypeConfig};
    use crate::model::views::{ColorRole, DisplayConfig};
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
        Schema::new(map, vec![])
    }

    fn config_with_display(display: DisplayConfig) -> Config {
        config_with_roles("status", "parent", "depends_on", display)
    }

    fn config_with_roles(
        board_field: &str,
        tree_field: &str,
        graph_field: &str,
        display: DisplayConfig,
    ) -> Config {
        config_with_all_roles(board_field, tree_field, graph_field, None, display)
    }

    fn config_with_all_roles(
        board_field: &str,
        tree_field: &str,
        graph_field: &str,
        effort_field: Option<&str>,
        display: DisplayConfig,
    ) -> Config {
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
                board_field: board_field.into(),
                tree_field: tree_field.into(),
                graph_field: graph_field.into(),
                effort_field: effort_field.map(str::to_owned),
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

    /// A schema that satisfies every field-role key the test config
    /// sets, so the display-role tests below see only their own
    /// diagnostics.
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
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: None,
                    inverse: Some("children".into()),
                },
            ),
            (
                "depends_on",
                FieldTypeConfig::Links {
                    allow_cycles: None,
                    inverse: None,
                },
            ),
            (
                "effort",
                FieldTypeConfig::Duration {
                    min: None,
                    max: None,
                },
            ),
        ])
    }

    #[test]
    fn valid_display_defaults_produce_no_diagnostics() {
        let config = config_with_display(DisplayConfig {
            title: Some("title".into()),
            fields: Some(vec!["id".into(), "status".into()]),
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
            ConfigDiagnosticKind::ConfigUnknownField { slot, field_name }
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
            ConfigDiagnosticKind::ConfigFieldTypeMismatch { slot, field_name, expected, .. }
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
            ConfigDiagnosticKind::ConfigUnknownField { slot, field_name }
                if *slot == "defaults.display.color" && field_name == "gone"
        ));
    }

    #[test]
    fn id_accepted_in_text_roles() {
        let config = config_with_display(DisplayConfig {
            title: Some("id".into()),
            fields: Some(vec!["id".into()]),
            ..DisplayConfig::default()
        });
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn id_rejected_as_color_default() {
        // The virtual `id` renders as text everywhere, but it can never
        // feed a tint — accepting it here would just be a dead config.
        let config = config_with_display(DisplayConfig {
            color: Some(ColorRole::Field("id".into())),
            ..DisplayConfig::default()
        });
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigFieldTypeMismatch { slot, field_name, expected, .. }
                if *slot == "defaults.display.color" && field_name == "id" && expected == "color"
        ));
    }

    #[test]
    fn empty_fields_default_is_valid() {
        // `fields: []` is the explicit "show no fields" — nothing to
        // resolve, nothing to report.
        let config = config_with_display(DisplayConfig {
            fields: Some(vec![]),
            ..DisplayConfig::default()
        });
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    // ── Field roles ──────────────────────────────────────────────────

    fn roles(board_field: &str, tree_field: &str, graph_field: &str) -> Config {
        config_with_roles(
            board_field,
            tree_field,
            graph_field,
            DisplayConfig::default(),
        )
    }

    #[test]
    fn resolvable_field_roles_produce_no_diagnostics() {
        let config = roles("status", "parent", "depends_on");
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn unknown_field_role_is_reported_as_a_warning() {
        // The typo nothing caught before: no code reads `tree_field`,
        // so this check is its only enforcement — and it is a warning
        // because nothing renders wrong because of it.
        let config = roles("status", "parnet", "depends_on");
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert_eq!(diagnostics[0].view_id(), None);
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigUnknownField { slot, field_name }
                if *slot == "defaults.tree_field" && field_name == "parnet"
        ));
    }

    #[test]
    fn every_bad_field_role_is_reported_not_just_the_first() {
        let config = roles("gone", "also_gone", "still_gone");
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 3, "got: {diagnostics:?}");
    }

    #[test]
    fn tree_field_must_be_a_link() {
        let config = roles("status", "depends_on", "depends_on");
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigFieldTypeMismatch { slot, field_name, actual_type, expected }
                if *slot == "defaults.tree_field"
                    && field_name == "depends_on"
                    && *actual_type == FieldType::Links
                    && expected == "link"
        ));
    }

    #[test]
    fn board_field_accepts_every_type_a_board_view_accepts() {
        // Deliberately not narrowed to `choice`: a board view takes all
        // three, and `workdown move` type-checks nothing at all, so a
        // string board field works today.
        for board_field in ["status", "title"] {
            let config = roles(board_field, "parent", "depends_on");
            let diagnostics = evaluate(&config, &simple_schema(), config_path());
            assert!(diagnostics.is_empty(), "{board_field}: {diagnostics:?}");
        }
    }

    #[test]
    fn board_field_rejects_a_type_no_board_can_show() {
        let config = roles("team_color", "parent", "depends_on");
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigFieldTypeMismatch { slot, actual_type, expected, .. }
                if *slot == "defaults.board_field"
                    && *actual_type == FieldType::Color
                    && expected == "choice, multichoice, or string"
        ));
    }

    #[test]
    fn graph_field_accepts_a_single_link_and_an_inverse_name() {
        // Both are what the graph view's own slot accepts: an inverse
        // name resolves to its original field at extraction time.
        for graph_field in ["parent", "children"] {
            let config = roles("status", "parent", graph_field);
            let diagnostics = evaluate(&config, &simple_schema(), config_path());
            assert!(diagnostics.is_empty(), "{graph_field}: {diagnostics:?}");
        }
    }

    #[test]
    fn tree_field_rejects_an_inverse_name() {
        // The tree view's slot resolves schema fields only, so accepting
        // one here would promise something no surface delivers.
        let config = roles("status", "children", "depends_on");
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigUnknownField { slot, field_name }
                if *slot == "defaults.tree_field" && field_name == "children"
        ));
    }

    fn roles_with_effort(effort_field: Option<&str>) -> Config {
        config_with_all_roles(
            "status",
            "parent",
            "depends_on",
            effort_field,
            DisplayConfig::default(),
        )
    }

    #[test]
    fn unset_effort_field_is_a_normal_state() {
        // Unlike its three mandatory siblings the key is optional, and
        // no key means "no effort field, deliberately" — nothing to
        // check, nothing to report.
        let config = roles_with_effort(None);
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn effort_field_naming_a_duration_produces_no_diagnostics() {
        let config = roles_with_effort(Some("effort"));
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert!(diagnostics.is_empty(), "got: {diagnostics:?}");
    }

    #[test]
    fn unknown_effort_field_is_reported_as_a_warning() {
        // The check this key exists for: its consumer is a timer in
        // the web app, which has no command to run and no error to
        // print — without this warning a typo would just mean "no
        // timer" and say nothing.
        let config = roles_with_effort(Some("efort"));
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigUnknownField { slot, field_name }
                if *slot == "defaults.effort_field" && field_name == "efort"
        ));
    }

    #[test]
    fn effort_field_must_be_a_duration() {
        let config = roles_with_effort(Some("status"));
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigFieldTypeMismatch { slot, field_name, actual_type, expected }
                if *slot == "defaults.effort_field"
                    && field_name == "status"
                    && *actual_type == FieldType::Choice
                    && expected == "duration"
        ));
    }

    #[test]
    fn empty_effort_field_is_a_typo_not_a_second_unset() {
        // `effort_field: ""` is a deleted value with the key left
        // behind. Unset already means "no effort field", and a second
        // spelling of that state would only be something to learn —
        // so the empty string warns like any other name the schema
        // does not know.
        let config = roles_with_effort(Some(""));
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigUnknownField { slot, field_name }
                if *slot == "defaults.effort_field" && field_name.is_empty()
        ));
    }

    #[test]
    fn virtual_id_is_rejected_as_effort_field() {
        let config = roles_with_effort(Some("id"));
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigVirtualIdNotAllowed { slot }
                if *slot == "defaults.effort_field"
        ));
    }

    #[test]
    fn virtual_id_is_rejected_by_name_in_a_field_role() {
        // Reported as its own thing: "unknown field" would be a lie
        // (the id resolves everywhere), and "wrong type" would be one
        // too for the board role, which accepts strings.
        let config = roles("id", "parent", "depends_on");
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigVirtualIdNotAllowed { slot }
                if *slot == "defaults.board_field"
        ));
    }

    #[test]
    fn virtual_id_is_rejected_even_when_the_schema_declares_it() {
        // Projects may declare `id` as a string field; the verdict must
        // not depend on that, exactly as the view slots reject it by
        // name before consulting the schema.
        let schema = schema_with(vec![
            ("id", FieldTypeConfig::String { pattern: None }),
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: None,
                    inverse: None,
                },
            ),
            (
                "depends_on",
                FieldTypeConfig::Links {
                    allow_cycles: None,
                    inverse: None,
                },
            ),
        ]);
        let config = roles("id", "parent", "depends_on");
        let diagnostics = evaluate(&config, &schema, config_path());
        assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
        assert!(matches!(
            config_kind(&diagnostics[0]),
            ConfigDiagnosticKind::ConfigVirtualIdNotAllowed { .. }
        ));
    }

    #[test]
    fn display_defaults_stay_errors_alongside_field_role_warnings() {
        // The two halves of this module carry different severities on
        // purpose — a dead display default breaks rendering, a dead
        // field-role key breaks nothing.
        let config = config_with_roles(
            "status",
            "parnet",
            "depends_on",
            DisplayConfig {
                title: Some("titel".into()),
                ..DisplayConfig::default()
            },
        );
        let diagnostics = evaluate(&config, &simple_schema(), config_path());
        assert_eq!(diagnostics.len(), 2, "got: {diagnostics:?}");
        let severities: Vec<Severity> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.severity)
            .collect();
        assert!(severities.contains(&Severity::Error), "got: {severities:?}");
        assert!(
            severities.contains(&Severity::Warning),
            "got: {severities:?}"
        );
    }
}
