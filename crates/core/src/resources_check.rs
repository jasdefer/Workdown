//! Load `resources.yaml` and validate the schema's `resource:` links
//! against it — the resources counterpart to [`crate::views_check`],
//! shaped like [`crate::compute_check`].
//!
//! Two kinds of finding live here, both pinned to `schema.yaml`:
//!
//! - the section a field points at is unusable — not declared (a typo,
//!   error) or empty (not filled in yet, warning),
//! - the field's `default:` could never be a valid entry — a literal
//!   outside the section, or a generator token that cannot produce one.
//!
//! Per-*item* values are checked elsewhere, in [`crate::store`], so that
//! every path which builds a store reports them — including the mutation
//! commands, which never call [`crate::project::load_project`]. Both
//! sides agree on which fields are checkable at all by asking
//! `validatable_fields`, the same way the store's derive pass asks
//! `compute_check::failed_fields` which computed fields to skip. A field
//! with an unusable option set is therefore reported once here and
//! stays silent on every item.

use std::collections::HashSet;
use std::path::Path;

use indexmap::IndexMap;

use crate::model::diagnostic::{ConfigDiagnosticKind, Diagnostic, FileDiagnosticKind};
use crate::model::resources::Resources;
use crate::model::schema::{DefaultValue, FieldType, Schema, Severity};
use crate::parser::resources::{load_resources, ResourcesLoadError};

/// Load `resources.yaml` from disk and return the parsed resources along
/// with any diagnostics produced.
///
/// Returns `(Resources::default(), [])` when the file is absent —
/// `resources.yaml` is optional. On an I/O or YAML-parse failure returns
/// `(Resources::default(), [diagnostic])` so the project still loads and
/// the failure surfaces through the same channel as every other finding.
pub fn load_and_check(resources_path: &Path) -> (Resources, Vec<Diagnostic>) {
    if !resources_path.exists() {
        return (Resources::default(), Vec::new());
    }
    match load_resources(resources_path) {
        Ok(resources) => (resources, Vec::new()),
        Err(error) => (
            Resources::default(),
            parse_errors_to_diagnostics(error, resources_path),
        ),
    }
}

/// Convert a [`ResourcesLoadError`] into a single file-scope diagnostic
/// pointed at `resources_path`. The detail carries the underlying I/O or
/// serde message.
pub fn parse_errors_to_diagnostics(
    error: ResourcesLoadError,
    resources_path: &Path,
) -> Vec<Diagnostic> {
    let detail = match error {
        ResourcesLoadError::ReadFailed(io) => io.to_string(),
        ResourcesLoadError::InvalidYaml(yaml) => yaml.to_string(),
        // The remaining variants (bad section, bad constant) carry their
        // context — section or constant name — in their Display form.
        other => other.to_string(),
    };
    vec![Diagnostic::file(
        Severity::Error,
        resources_path.to_path_buf(),
        FileDiagnosticKind::ReadError { detail },
    )]
}

// ── Schema-scope checks ───────────────────────────────────────────────

/// Check every `resource:`-backed field against the loaded resources.
/// Returns one diagnostic per finding, pinned to `schema.yaml`; an empty
/// vector means every such field points at a populated section and
/// carries a default that section could produce.
pub fn evaluate(schema: &Schema, resources: &Resources, schema_path: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (field_name, field_definition) in &schema.fields {
        let Some(section_name) = field_definition.resource.as_deref() else {
            continue;
        };

        // A generator can never name a person, whatever the section
        // holds — so this one does not wait for a usable option set.
        if let Some(DefaultValue::Generator(generator)) = &field_definition.default {
            diagnostics.push(Diagnostic::config(
                Severity::Error,
                schema_path.to_path_buf(),
                ConfigDiagnosticKind::ResourceDefaultGenerator {
                    field: field_name.clone(),
                    section: section_name.to_owned(),
                    generator: generator.token().to_owned(),
                },
            ));
        }

        match option_set(field_definition.resource.as_deref(), resources) {
            OptionSet::Usable(entry_ids) => {
                if let Some(DefaultValue::String(value)) = &field_definition.default {
                    if !entry_ids.contains(value.as_str()) {
                        diagnostics.push(Diagnostic::config(
                            Severity::Error,
                            schema_path.to_path_buf(),
                            ConfigDiagnosticKind::ResourceDefaultUnknown {
                                field: field_name.clone(),
                                section: section_name.to_owned(),
                                value: value.clone(),
                            },
                        ));
                    }
                }
            }
            OptionSet::SectionUnknown => diagnostics.push(Diagnostic::config(
                Severity::Error,
                schema_path.to_path_buf(),
                ConfigDiagnosticKind::ResourceSectionUnknown {
                    field: field_name.clone(),
                    section: section_name.to_owned(),
                },
            )),
            OptionSet::SectionEmpty => diagnostics.push(Diagnostic::config(
                Severity::Warning,
                schema_path.to_path_buf(),
                ConfigDiagnosticKind::ResourceSectionEmpty {
                    field: field_name.clone(),
                    section: section_name.to_owned(),
                },
            )),
        }
    }

    diagnostics
}

// ── Shared option-set resolution ──────────────────────────────────────

/// A resource-backed field whose values can actually be checked: the
/// section it names, and that section's entry ids.
pub(crate) struct ValidatableField<'a> {
    /// Section name, for the diagnostic's prose.
    pub section: &'a str,
    /// Every `id` the section declares.
    pub entry_ids: HashSet<&'a str>,
}

/// The fields whose values are worth checking against a resource, keyed
/// by field name in schema-declaration order.
///
/// Excludes any field whose option set is unusable — those are reported
/// once by [`evaluate`] against `schema.yaml`, and repeating the same
/// cause per item would only point at the wrong file N times.
pub(crate) fn validatable_fields<'a>(
    schema: &'a Schema,
    resources: &'a Resources,
) -> IndexMap<&'a str, ValidatableField<'a>> {
    let mut validatable = IndexMap::new();

    for (field_name, field_definition) in &schema.fields {
        let Some(section_name) = field_definition.resource.as_deref() else {
            continue;
        };
        // The schema parser allows `resource:` only on these two types;
        // the guard keeps the value walk below total regardless.
        if !matches!(
            field_definition.field_type(),
            FieldType::String | FieldType::List
        ) {
            continue;
        }
        if let OptionSet::Usable(entry_ids) = option_set(Some(section_name), resources) {
            validatable.insert(
                field_name.as_str(),
                ValidatableField {
                    section: section_name,
                    entry_ids,
                },
            );
        }
    }

    validatable
}

/// What a field's `resource:` resolves to, and why it might not be
/// checkable.
enum OptionSet<'a> {
    /// The section exists and declares at least one entry.
    Usable(HashSet<&'a str>),
    /// A `resources.yaml` was loaded but declares no such section — a
    /// typo in `schema.yaml`.
    SectionUnknown,
    /// The section exists but is empty, or no document was loaded at
    /// all. Either way there is nothing to validate against yet.
    SectionEmpty,
}

fn option_set<'a>(section_name: Option<&str>, resources: &'a Resources) -> OptionSet<'a> {
    let Some(section_name) = section_name else {
        return OptionSet::SectionEmpty;
    };
    match resources.section(section_name) {
        Some(entries) if !entries.is_empty() => {
            OptionSet::Usable(entries.iter().map(|entry| entry.id.as_str()).collect())
        }
        Some(_) => OptionSet::SectionEmpty,
        None if resources.document_loaded => OptionSet::SectionUnknown,
        None => OptionSet::SectionEmpty,
    }
}
