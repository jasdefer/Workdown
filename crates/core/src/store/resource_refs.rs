//! Check every item's `resource:`-backed values against the entries
//! `resources.yaml` declares.
//!
//! The counterpart to the broken-link walk in [`super`]: both answer
//! "does this name refer to something that exists?", one against the
//! item set, one against a resource section. It runs here rather than in
//! a project-level pass for two reasons — the mutation commands build
//! their own store and never call
//! [`load_project`](crate::project::load_project), so `workdown set
//! my-task assignee carol` would otherwise write the file in silence;
//! and running after the derive passes means a value gets the same
//! treatment however it arrived, hand-written, stamped from a `default:`
//! or produced by `compute:`/`when:`.
//!
//! Fields whose section is missing or empty are excluded upstream by
//! [`crate::resources_check::validatable_fields`], which reports that
//! cause once against `schema.yaml` instead.

use std::collections::HashMap;

use crate::model::diagnostic::{Diagnostic, ItemDiagnosticKind};
use crate::model::resources::Resources;
use crate::model::schema::{Schema, Severity};
use crate::model::{FieldValue, WorkItem, WorkItemId};
use crate::resources_check::validatable_fields;

/// One warning per item value that is not an entry of its section.
pub(super) fn check(
    items: &HashMap<WorkItemId, WorkItem>,
    schema: &Schema,
    resources: &Resources,
) -> Vec<Diagnostic> {
    let validatable = validatable_fields(schema, resources);
    if validatable.is_empty() {
        return Vec::new();
    }

    let mut diagnostics = Vec::new();
    for item in items.values() {
        for (field_name, field) in &validatable {
            let Some(value) = item.fields.get(*field_name) else {
                continue;
            };
            for candidate in referenced_values(value) {
                if field.entry_ids.contains(candidate) {
                    continue;
                }
                diagnostics.push(Diagnostic::item(
                    Severity::Warning,
                    item.source_path.clone(),
                    item.id.clone(),
                    ItemDiagnosticKind::UnknownResourceRef {
                        field: (*field_name).to_owned(),
                        section: field.section.to_owned(),
                        value: candidate.to_owned(),
                    },
                ));
            }
        }
    }

    diagnostics
}

/// The entry ids a value claims: one for a string field, each element
/// for a list field. Any other variant yields none — the schema parser
/// allows `resource:` only on `string` and `list`.
fn referenced_values(value: &FieldValue) -> Vec<&str> {
    match value {
        FieldValue::String(text) => vec![text.as_str()],
        FieldValue::List(entries) => entries.iter().map(String::as_str).collect(),
        _ => Vec::new(),
    }
}
