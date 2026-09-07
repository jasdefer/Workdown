//! Commit message generation for the web app's "Commit & push" button.
//!
//! Takes the changed files inside the workdown paths — each with its
//! text at `HEAD` and in the working tree — and describes the change the
//! way a person scanning history wants to read it: item titles instead
//! of filenames, prettified field names and values instead of raw slugs.
//! Pure: no git, no I/O. The server gathers the texts; a later
//! `workdown changes` command can reuse the same function.
//!
//! The generator knows field names, types and values from the schema and
//! nothing else. It may say "Status → In Progress"; it never says
//! "Started", because no field is privileged except `id`.
//!
//! Why not parse git's diff output instead: a unified diff is
//! line-oriented, so a reordered field or a multi-line list value would
//! produce misleading before/after pairs. Parsing both frontmatters with
//! the parser the tool already has and comparing field by field cannot
//! be fooled that way, and a value rewritten in an equivalent spelling
//! (`5d` versus `120h`) is correctly not reported as a change.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::coerce::coerce_fields;
use crate::generators::prettify_slug;
use crate::model::field_value::format_field_value;
use crate::model::schema::{FieldType, Schema};
use crate::model::FieldValue;
use crate::parser::parse_work_item;

/// How many per-entry lines the body carries before the rest is
/// summarized by count. A commit body that lists forty items is a diff
/// in disguise; anyone who wants that detail opens the diff.
pub const BODY_LINE_CAP: usize = 10;

/// Which kind of file a changed path is, decided by the caller from the
/// config's path list: anything under the work items directory is a
/// work item, everything else in scope (schema, views, resources,
/// templates, `config.yaml`) is a definition file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangedFileKind {
    WorkItem,
    Definition,
}

/// One file the commit will cover, with its text on both sides of the
/// change. `None` on the old side means the file is new; `None` on the
/// new side means it was deleted.
#[derive(Debug, Clone)]
pub struct ChangedFile {
    /// Project-relative path. For work items, the file stem is the id
    /// fallback the parser uses; for definition files, the path is the
    /// name the message shows (`schema.yaml`, `.workdown/templates/bug.md`).
    pub path: PathBuf,
    pub kind: ChangedFileKind,
    /// The file's text at `HEAD`, or `None` when it did not exist there.
    pub old_text: Option<String>,
    /// The file's text in the working tree, or `None` when it was deleted.
    pub new_text: Option<String>,
}

/// The generated message: a subject line, and body lines only when the
/// subject cannot carry the content on its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeSummary {
    pub subject: String,
    pub body: Vec<String>,
}

impl ChangeSummary {
    /// The full commit message: subject, then a blank line and the body
    /// when there is one.
    pub fn to_message(&self) -> String {
        if self.body.is_empty() {
            self.subject.clone()
        } else {
            format!("{}\n\n{}", self.subject, self.body.join("\n"))
        }
    }
}

/// Describe a set of changed files as a commit message.
///
/// `title_field` is the field the config's `title` display role points
/// to, if any; items without a value there — or projects without the
/// role — are named by their prettified id, as the board does.
///
/// Grouping rule:
/// - One entry: it is the subject (`Implement login: Status → In Progress`,
///   `Add Implement login`, `Edit schema.yaml`). No body.
/// - Several items that all moved the same field to the same value:
///   one line with the count (`Move 2 items to In Progress`). No body.
/// - Anything mixed: a count (`Update 3 work items, 1 added`, `Update 2
///   work items, edit schema.yaml`), then one body line per entry, capped
///   at [`BODY_LINE_CAP`] with the rest summarized by count.
pub fn summarize_changes(
    files: &[ChangedFile],
    schema: &Schema,
    title_field: Option<&str>,
) -> ChangeSummary {
    let mut items = Vec::new();
    let mut definitions = Vec::new();

    for file in files {
        // Line endings are not content: the `HEAD` side comes from the
        // blob as stored, the working-tree side from a file an editor
        // (or autocrlf) may have given CRLF endings.
        let file = ChangedFile {
            path: file.path.clone(),
            kind: file.kind,
            old_text: file.old_text.as_deref().map(normalize_newlines),
            new_text: file.new_text.as_deref().map(normalize_newlines),
        };
        let file = &file;
        if file.old_text == file.new_text {
            // Listed but identical — a mode change, or a caller being
            // generous. Nothing to say about it.
            continue;
        }
        match file.kind {
            ChangedFileKind::WorkItem => {
                items.push(describe_work_item(file, schema, title_field));
            }
            ChangedFileKind::Definition => definitions.push(DefinitionChange {
                name: file.path.to_string_lossy().replace('\\', "/"),
                operation: Operation::of(file),
            }),
        }
    }

    if items.is_empty() && definitions.is_empty() {
        return ChangeSummary {
            subject: "No changes".to_owned(),
            body: Vec::new(),
        };
    }

    // One entry: it is the subject.
    if items.len() + definitions.len() == 1 {
        let subject = match (items.first(), definitions.first()) {
            (Some(item), None) => item.as_subject(),
            (None, Some(definition)) => definition.as_line(true),
            _ => unreachable!("exactly one entry"),
        };
        return ChangeSummary {
            subject,
            body: Vec::new(),
        };
    }

    // Several items, same field, same new value: one line with the count.
    if definitions.is_empty() {
        if let Some(subject) = uniform_field_move(&items, schema) {
            return ChangeSummary {
                subject,
                body: Vec::new(),
            };
        }
    }

    // Mixed: a count, then the detail in the body.
    let mut parts = Vec::new();
    if let Some(item_part) = count_items(&items) {
        parts.push(item_part);
    }
    if let Some(definition_part) = count_definitions(&definitions, parts.is_empty()) {
        parts.push(definition_part);
    }
    let subject = parts.join(", ");

    let mut body: Vec<String> = items
        .iter()
        .map(ItemChange::as_body_line)
        .chain(
            definitions
                .iter()
                .map(|definition| definition.as_line(false)),
        )
        .collect();
    if body.len() > BODY_LINE_CAP {
        let rest = body.len() - BODY_LINE_CAP;
        body.truncate(BODY_LINE_CAP);
        body.push(format!("…and {rest} more"));
    }

    ChangeSummary { subject, body }
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

// ── Per-file description ────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Added,
    Modified,
    Deleted,
}

impl Operation {
    fn of(file: &ChangedFile) -> Self {
        match (&file.old_text, &file.new_text) {
            (None, _) => Operation::Added,
            (Some(_), None) => Operation::Deleted,
            (Some(_), Some(_)) => Operation::Modified,
        }
    }

    fn verb(self, capitalized: bool) -> &'static str {
        match (self, capitalized) {
            (Operation::Added, true) => "Add",
            (Operation::Added, false) => "add",
            (Operation::Modified, true) => "Edit",
            (Operation::Modified, false) => "edit",
            (Operation::Deleted, true) => "Delete",
            (Operation::Deleted, false) => "delete",
        }
    }
}

/// One field that differs between the two sides, already worded for
/// display: `field` is the prettified name, `new_value` the prettified
/// new value or `None` when the field was removed.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldChange {
    field_name: String,
    field: String,
    new_value: Option<String>,
}

impl FieldChange {
    fn as_text(&self) -> String {
        match &self.new_value {
            Some(value) => format!("{} → {}", self.field, value),
            None => format!("{} cleared", self.field),
        }
    }
}

#[derive(Debug, Clone)]
struct ItemChange {
    title: String,
    operation: Operation,
    field_changes: Vec<FieldChange>,
    body_edited: bool,
}

impl ItemChange {
    /// What changed, as a comma list: `Status → In Progress, description
    /// edited`. An item whose text changed without any difference the
    /// parser can name — unreadable frontmatter on one side, or a value
    /// rewritten in an equivalent spelling — is plainly "edited".
    fn changes_text(&self) -> String {
        let mut parts: Vec<String> = self
            .field_changes
            .iter()
            .map(FieldChange::as_text)
            .collect();
        if self.body_edited {
            parts.push("description edited".to_owned());
        }
        if parts.is_empty() {
            parts.push("edited".to_owned());
        }
        parts.join(", ")
    }

    fn as_subject(&self) -> String {
        match self.operation {
            Operation::Added => format!("Add {}", self.title),
            Operation::Deleted => format!("Delete {}", self.title),
            Operation::Modified => format!("{}: {}", self.title, self.changes_text()),
        }
    }

    fn as_body_line(&self) -> String {
        match self.operation {
            Operation::Added => format!("{}: added", self.title),
            Operation::Deleted => format!("{}: deleted", self.title),
            Operation::Modified => format!("{}: {}", self.title, self.changes_text()),
        }
    }
}

#[derive(Debug, Clone)]
struct DefinitionChange {
    name: String,
    operation: Operation,
}

impl DefinitionChange {
    fn as_line(&self, capitalized: bool) -> String {
        format!("{} {}", self.operation.verb(capitalized), self.name)
    }
}

/// Parse one side of a work item into the display strings the
/// comparison runs on: a map from field name to worded value, plus the
/// id and body. `None` when the text is not a readable work item.
struct ParsedSide {
    id: String,
    values: HashMap<String, String>,
    title: Option<String>,
    body: String,
}

fn parse_side(
    text: &str,
    path: &Path,
    schema: &Schema,
    title_field: Option<&str>,
) -> Option<ParsedSide> {
    let raw = parse_work_item(text, path).ok()?;
    let coerced = coerce_fields(&raw, schema);

    let mut values = HashMap::new();
    for (name, yaml) in &raw.frontmatter {
        // Coerced when the schema could type it; the raw scalar
        // otherwise (unknown field, or a value that failed coercion) —
        // a broken value still shows as what it is rather than as
        // "cleared".
        let worded = match coerced.fields.get(name) {
            Some(value) => word_value(name, value, schema),
            None => yaml_to_text(yaml),
        };
        values.insert(name.clone(), worded);
    }

    let title = match title_field {
        Some("id") => Some(raw.id.as_str().to_owned()),
        Some(field) => coerced.fields.get(field).map(format_field_value),
        None => None,
    };

    Some(ParsedSide {
        id: raw.id.as_str().to_owned(),
        values,
        title,
        body: raw.body,
    })
}

fn describe_work_item(
    file: &ChangedFile,
    schema: &Schema,
    title_field: Option<&str>,
) -> ItemChange {
    let old = file
        .old_text
        .as_deref()
        .and_then(|text| parse_side(text, &file.path, schema, title_field));
    let new = file
        .new_text
        .as_deref()
        .and_then(|text| parse_side(text, &file.path, schema, title_field));

    let fallback_id = file
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("")
        .to_owned();
    let side_for_title = new.as_ref().or(old.as_ref());
    let title = side_for_title
        .and_then(|side| side.title.clone())
        .unwrap_or_else(|| {
            let id = side_for_title.map_or(fallback_id.as_str(), |side| side.id.as_str());
            prettify_slug(id)
        });

    let operation = Operation::of(file);
    let (field_changes, body_edited) = match (&old, &new, operation) {
        (Some(old), Some(new), Operation::Modified) => (
            compare_fields(old, new, schema),
            old.body.trim_end() != new.body.trim_end(),
        ),
        // Added, deleted, or unreadable on a side: nothing field-wise to
        // say; the operation (or "edited") carries it.
        _ => (Vec::new(), false),
    };

    ItemChange {
        title,
        operation,
        field_changes,
        body_edited,
    }
}

/// Field-by-field comparison in schema declaration order, then any
/// out-of-schema fields alphabetically.
fn compare_fields(old: &ParsedSide, new: &ParsedSide, schema: &Schema) -> Vec<FieldChange> {
    let mut names: Vec<&str> = schema.fields.keys().map(String::as_str).collect();
    let mut extra: Vec<&str> = old
        .values
        .keys()
        .chain(new.values.keys())
        .map(String::as_str)
        .filter(|name| !schema.fields.contains_key(*name))
        .collect();
    extra.sort_unstable();
    extra.dedup();
    names.extend(extra);

    let mut changes = Vec::new();
    if old.id != new.id {
        changes.push(FieldChange {
            field_name: "id".to_owned(),
            field: "Id".to_owned(),
            new_value: Some(new.id.clone()),
        });
    }
    for name in names {
        let before = old.values.get(name);
        let after = new.values.get(name);
        if before != after {
            changes.push(FieldChange {
                field_name: name.to_owned(),
                field: prettify_slug(name),
                new_value: after.cloned(),
            });
        }
    }
    changes
}

/// Word a typed value for the message. Slug-shaped values — choices,
/// link targets, resource ids — read as labels; everything else is the
/// value's canonical text.
fn word_value(field_name: &str, value: &FieldValue, schema: &Schema) -> String {
    let definition = schema.fields.get(field_name);
    let slug_shaped = definition.is_some_and(|definition| {
        definition.resource.is_some()
            || matches!(
                definition.field_type(),
                FieldType::Choice | FieldType::Multichoice | FieldType::Link | FieldType::Links
            )
    });
    if !slug_shaped {
        return format_field_value(value);
    }
    match value {
        FieldValue::Multichoice(values) | FieldValue::List(values) => values
            .iter()
            .map(|entry| prettify_slug(entry))
            .collect::<Vec<_>>()
            .join(", "),
        FieldValue::Links(ids) => ids
            .iter()
            .map(|id| prettify_slug(id.as_str()))
            .collect::<Vec<_>>()
            .join(", "),
        other => prettify_slug(&format_field_value(other)),
    }
}

/// Plain text for a raw YAML value the schema could not type.
fn yaml_to_text(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::Null => String::new(),
        serde_yaml::Value::Bool(flag) => flag.to_string(),
        serde_yaml::Value::Number(number) => number.to_string(),
        serde_yaml::Value::String(text) => text.clone(),
        serde_yaml::Value::Sequence(entries) => entries
            .iter()
            .map(yaml_to_text)
            .collect::<Vec<_>>()
            .join(", "),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim_end()
            .to_owned(),
    }
}

// ── Grouping ────────────────────────────────────────────────────────

/// `Move 2 items to In Progress` when every item changed exactly one
/// field, the same one, to the same value, and nothing else. The verb
/// follows the field's *type*, never its name: a choice field is a
/// board column, so items move; anything else is set.
fn uniform_field_move(items: &[ItemChange], schema: &Schema) -> Option<String> {
    let first = items.first()?;
    let reference = single_change(first)?;
    let uniform = items
        .iter()
        .all(|item| single_change(item) == Some(reference));
    if !uniform {
        return None;
    }

    let count = items.len();
    let is_choice = schema
        .fields
        .get(&reference.field_name)
        .is_some_and(|definition| definition.field_type() == FieldType::Choice);
    Some(match (&reference.new_value, is_choice) {
        (Some(value), true) => format!("Move {count} items to {value}"),
        (Some(value), false) => format!("Set {} to {value} on {count} items", reference.field),
        (None, _) => format!("Clear {} on {count} items", reference.field),
    })
}

/// The one field change an item carries when that is all that happened to
/// it: modified, body untouched, exactly one field.
fn single_change(item: &ItemChange) -> Option<&FieldChange> {
    match (
        item.operation,
        item.body_edited,
        item.field_changes.as_slice(),
    ) {
        (Operation::Modified, false, [change]) => Some(change),
        _ => None,
    }
}

fn plural(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("{count} {singular}")
    } else {
        format!("{count} {plural}")
    }
}

/// `Update 3 work items, 1 added, 2 deleted` — the leading verb names
/// the most common thing that happened; the trailing counts name the
/// rest.
fn count_items(items: &[ItemChange]) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    let modified = items
        .iter()
        .filter(|item| item.operation == Operation::Modified)
        .count();
    let added = items
        .iter()
        .filter(|item| item.operation == Operation::Added)
        .count();
    let deleted = items
        .iter()
        .filter(|item| item.operation == Operation::Deleted)
        .count();

    let mut parts = Vec::new();
    if modified > 0 {
        parts.push(format!(
            "Update {}",
            plural(modified, "work item", "work items")
        ));
        if added > 0 {
            parts.push(format!("{added} added"));
        }
        if deleted > 0 {
            parts.push(format!("{deleted} deleted"));
        }
    } else if added > 0 {
        parts.push(format!("Add {}", plural(added, "work item", "work items")));
        if deleted > 0 {
            parts.push(format!("{deleted} deleted"));
        }
    } else {
        parts.push(format!(
            "Delete {}",
            plural(deleted, "work item", "work items")
        ));
    }
    Some(parts.join(", "))
}

/// `edit schema.yaml`, `edit schema.yaml and views.yaml`, or `update 3
/// project files` when the list is long or the operations differ.
fn count_definitions(definitions: &[DefinitionChange], leading: bool) -> Option<String> {
    let first = definitions.first()?;
    let same_operation = definitions
        .iter()
        .all(|definition| definition.operation == first.operation);
    Some(match definitions {
        [only] => only.as_line(leading),
        [first, second] if same_operation => format!(
            "{} {} and {}",
            first.operation.verb(leading),
            first.name,
            second.name
        ),
        _ => {
            let verb = if same_operation {
                first.operation.verb(leading)
            } else if leading {
                "Update"
            } else {
                "update"
            };
            format!("{verb} {} project files", definitions.len())
        }
    })
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::schema::FieldTypeConfig;
    use crate::view_data::test_support::make_schema;

    fn schema() -> Schema {
        let mut schema = make_schema(vec![
            ("title", FieldTypeConfig::String { pattern: None }),
            (
                "status",
                FieldTypeConfig::Choice {
                    values: vec!["to_do".into(), "in_progress".into(), "done".into()],
                },
            ),
            ("assignee", FieldTypeConfig::String { pattern: None }),
            (
                "estimate",
                FieldTypeConfig::Duration {
                    min: None,
                    max: None,
                },
            ),
            (
                "parent",
                FieldTypeConfig::Link {
                    allow_cycles: None,
                    inverse: None,
                },
            ),
        ]);
        schema
            .fields
            .get_mut("assignee")
            .expect("assignee field")
            .resource = Some("people".to_owned());
        schema
    }

    fn item(frontmatter: &str, body: &str) -> String {
        format!("---\n{frontmatter}\n---\n{body}")
    }

    fn modified(id: &str, old: &str, new: &str) -> ChangedFile {
        ChangedFile {
            path: PathBuf::from(format!("workdown-items/{id}.md")),
            kind: ChangedFileKind::WorkItem,
            old_text: Some(old.to_owned()),
            new_text: Some(new.to_owned()),
        }
    }

    fn added(id: &str, new: &str) -> ChangedFile {
        ChangedFile {
            path: PathBuf::from(format!("workdown-items/{id}.md")),
            kind: ChangedFileKind::WorkItem,
            old_text: None,
            new_text: Some(new.to_owned()),
        }
    }

    fn deleted(id: &str, old: &str) -> ChangedFile {
        ChangedFile {
            path: PathBuf::from(format!("workdown-items/{id}.md")),
            kind: ChangedFileKind::WorkItem,
            old_text: Some(old.to_owned()),
            new_text: None,
        }
    }

    fn definition(path: &str, old: Option<&str>, new: Option<&str>) -> ChangedFile {
        ChangedFile {
            path: PathBuf::from(path),
            kind: ChangedFileKind::Definition,
            old_text: old.map(str::to_owned),
            new_text: new.map(str::to_owned),
        }
    }

    fn status_move(id: &str, title: &str, from: &str, to: &str) -> ChangedFile {
        modified(
            id,
            &item(&format!("title: {title}\nstatus: {from}"), "Body.\n"),
            &item(&format!("title: {title}\nstatus: {to}"), "Body.\n"),
        )
    }

    fn summarize(files: &[ChangedFile]) -> ChangeSummary {
        summarize_changes(files, &schema(), Some("title"))
    }

    // ── One entry ────────────────────────────────────────────────────

    #[test]
    fn one_item_one_field_is_the_subject() {
        let summary = summarize(&[status_move(
            "implement-login",
            "Implement login",
            "to_do",
            "in_progress",
        )]);
        assert_eq!(summary.subject, "Implement login: Status → In Progress");
        assert!(summary.body.is_empty());
    }

    #[test]
    fn one_item_several_fields_lists_them_in_schema_order() {
        let file = modified(
            "implement-login",
            &item("title: Implement login\nstatus: to_do\nassignee: bob", ""),
            &item("title: Implement login\nassignee: alice\nstatus: done", ""),
        );
        let summary = summarize(&[file]);
        assert_eq!(
            summary.subject,
            "Implement login: Status → Done, Assignee → Alice"
        );
    }

    #[test]
    fn cleared_field_is_named_as_cleared() {
        let file = modified(
            "implement-login",
            &item("title: Implement login\nassignee: bob", ""),
            &item("title: Implement login", ""),
        );
        assert_eq!(
            summarize(&[file]).subject,
            "Implement login: Assignee cleared"
        );
    }

    #[test]
    fn body_only_change_is_description_edited() {
        let file = modified(
            "implement-login",
            &item("title: Implement login\nstatus: to_do", "Old body.\n"),
            &item("title: Implement login\nstatus: to_do", "New body.\n"),
        );
        assert_eq!(
            summarize(&[file]).subject,
            "Implement login: description edited"
        );
    }

    #[test]
    fn trailing_newline_on_body_is_not_a_change() {
        let file = modified(
            "implement-login",
            &item("title: Implement login\nstatus: to_do", "Body."),
            &item("title: Implement login\nstatus: in_progress", "Body.\n\n"),
        );
        assert_eq!(
            summarize(&[file]).subject,
            "Implement login: Status → In Progress"
        );
    }

    #[test]
    fn line_endings_are_not_a_change() {
        // HEAD stores LF; the working tree came back CRLF from an editor.
        let file = modified(
            "implement-login",
            "---\ntitle: Implement login\nstatus: to_do\n---\nBody.\n",
            "---\r\ntitle: Implement login\r\nstatus: done\r\n---\r\nBody.\r\n",
        );
        assert_eq!(summarize(&[file]).subject, "Implement login: Status → Done");
        let only_endings = modified(
            "implement-login",
            "---\ntitle: Implement login\n---\nBody.\n",
            "---\r\ntitle: Implement login\r\n---\r\nBody.\r\n",
        );
        assert_eq!(summarize(&[only_endings]).subject, "No changes");
    }

    #[test]
    fn field_and_body_together() {
        let file = modified(
            "implement-login",
            &item("title: Implement login\nstatus: to_do", "Old.\n"),
            &item("title: Implement login\nstatus: done", "New.\n"),
        );
        assert_eq!(
            summarize(&[file]).subject,
            "Implement login: Status → Done, description edited"
        );
    }

    #[test]
    fn equivalent_spelling_is_edited_not_a_field_change() {
        // 5d and 120h are the same duration; the text changed, the value
        // did not. The file is still in the commit, so it is named —
        // plainly, without inventing a field change.
        let file = modified(
            "implement-login",
            &item("title: Implement login\nestimate: 5d", ""),
            &item("title: Implement login\nestimate: 120h", ""),
        );
        assert_eq!(summarize(&[file]).subject, "Implement login: edited");
    }

    #[test]
    fn added_item_uses_its_new_title() {
        let file = added("add-logout", &item("title: Add logout\nstatus: to_do", ""));
        assert_eq!(summarize(&[file]).subject, "Add Add logout");
    }

    #[test]
    fn deleted_item_uses_its_old_title() {
        let file = deleted("add-logout", &item("title: Add logout\nstatus: done", ""));
        assert_eq!(summarize(&[file]).subject, "Delete Add logout");
    }

    #[test]
    fn definition_file_is_one_line_with_a_verb() {
        assert_eq!(
            summarize(&[definition(".workdown/schema.yaml", Some("a"), Some("b"))]).subject,
            "Edit .workdown/schema.yaml"
        );
        assert_eq!(
            summarize(&[definition(".workdown/templates/bug.md", None, Some("b"))]).subject,
            "Add .workdown/templates/bug.md"
        );
        assert_eq!(
            summarize(&[definition("views.yaml", Some("a"), None)]).subject,
            "Delete views.yaml"
        );
    }

    // ── Naming ───────────────────────────────────────────────────────

    #[test]
    fn title_falls_back_to_prettified_id() {
        let file = modified(
            "implement-login",
            &item("status: to_do", ""),
            &item("status: done", ""),
        );
        assert_eq!(summarize(&[file]).subject, "Implement Login: Status → Done");
    }

    #[test]
    fn no_title_role_names_items_by_id() {
        let file = status_move("implement-login", "Implement login", "to_do", "done");
        let summary = summarize_changes(&[file], &schema(), None);
        assert_eq!(summary.subject, "Implement Login: Status → Done");
    }

    #[test]
    fn title_role_id_uses_the_raw_id() {
        let file = status_move("implement-login", "Implement login", "to_do", "done");
        let summary = summarize_changes(&[file], &schema(), Some("id"));
        assert_eq!(summary.subject, "implement-login: Status → Done");
    }

    #[test]
    fn frontmatter_id_takes_precedence_over_filename() {
        let file = modified(
            "some-file",
            &item("id: real-id\nstatus: to_do", ""),
            &item("id: real-id\nstatus: done", ""),
        );
        assert_eq!(summarize(&[file]).subject, "Real Id: Status → Done");
    }

    #[test]
    fn link_and_resource_values_read_as_labels() {
        let file = modified(
            "implement-login",
            &item(
                "title: Implement login\nparent: auth-epic\nassignee: bob",
                "",
            ),
            &item(
                "title: Implement login\nparent: user-accounts\nassignee: alice-b",
                "",
            ),
        );
        assert_eq!(
            summarize(&[file]).subject,
            "Implement login: Assignee → Alice B, Parent → User Accounts"
        );
    }

    #[test]
    fn unknown_field_shows_its_raw_text() {
        let file = modified(
            "implement-login",
            &item("title: Implement login\nextra: 1", ""),
            &item("title: Implement login\nextra: 2", ""),
        );
        assert_eq!(summarize(&[file]).subject, "Implement login: Extra → 2");
    }

    #[test]
    fn invalid_value_shows_as_written_rather_than_cleared() {
        let file = modified(
            "implement-login",
            &item("title: Implement login\nstatus: to_do", ""),
            &item("title: Implement login\nstatus: nonsense", ""),
        );
        assert_eq!(
            summarize(&[file]).subject,
            "Implement login: Status → nonsense"
        );
    }

    #[test]
    fn unreadable_side_is_plainly_edited() {
        let file = modified(
            "implement-login",
            &item("title: Implement login\nstatus: to_do", ""),
            "no frontmatter at all",
        );
        assert_eq!(summarize(&[file]).subject, "Implement login: edited");
    }

    // ── Several items, uniform ───────────────────────────────────────

    #[test]
    fn same_choice_move_collapses_to_move() {
        let summary = summarize(&[
            status_move("a", "A", "to_do", "in_progress"),
            status_move("b", "B", "done", "in_progress"),
        ]);
        assert_eq!(summary.subject, "Move 2 items to In Progress");
        assert!(summary.body.is_empty());
    }

    #[test]
    fn same_non_choice_value_collapses_to_set() {
        let assign = |id: &str| {
            modified(
                id,
                &item("title: T\nassignee: bob", ""),
                &item("title: T\nassignee: alice", ""),
            )
        };
        let summary = summarize(&[assign("a"), assign("b"), assign("c")]);
        assert_eq!(summary.subject, "Set Assignee to Alice on 3 items");
    }

    #[test]
    fn same_field_cleared_collapses_to_clear() {
        let clear = |id: &str| {
            modified(
                id,
                &item("title: T\nassignee: bob", ""),
                &item("title: T", ""),
            )
        };
        let summary = summarize(&[clear("a"), clear("b")]);
        assert_eq!(summary.subject, "Clear Assignee on 2 items");
    }

    #[test]
    fn different_targets_do_not_collapse() {
        let summary = summarize(&[
            status_move("a", "A", "to_do", "in_progress"),
            status_move("b", "B", "to_do", "done"),
        ]);
        assert_eq!(summary.subject, "Update 2 work items");
        assert_eq!(
            summary.body,
            vec!["A: Status → In Progress", "B: Status → Done"]
        );
    }

    // ── Mixed ────────────────────────────────────────────────────────

    #[test]
    fn mixed_operations_count_with_body() {
        let summary = summarize(&[
            status_move("a", "A", "to_do", "in_progress"),
            status_move("b", "B", "to_do", "in_progress"),
            added("c", &item("title: C\nstatus: to_do", "")),
            deleted("d", &item("title: D\nstatus: done", "")),
        ]);
        assert_eq!(summary.subject, "Update 2 work items, 1 added, 1 deleted");
        assert_eq!(
            summary.body,
            vec![
                "A: Status → In Progress",
                "B: Status → In Progress",
                "C: added",
                "D: deleted",
            ]
        );
    }

    #[test]
    fn only_additions_lead_with_add() {
        let summary = summarize(&[
            added("a", &item("title: A", "")),
            added("b", &item("title: B", "")),
        ]);
        assert_eq!(summary.subject, "Add 2 work items");
    }

    #[test]
    fn only_deletions_lead_with_delete() {
        let summary = summarize(&[
            deleted("a", &item("title: A", "")),
            deleted("b", &item("title: B", "")),
        ]);
        assert_eq!(summary.subject, "Delete 2 work items");
    }

    #[test]
    fn items_and_a_definition_file() {
        let summary = summarize(&[
            status_move("a", "A", "to_do", "in_progress"),
            status_move("b", "B", "to_do", "in_progress"),
            definition("schema.yaml", Some("x"), Some("y")),
        ]);
        assert_eq!(summary.subject, "Update 2 work items, edit schema.yaml");
        assert_eq!(
            summary.body,
            vec![
                "A: Status → In Progress",
                "B: Status → In Progress",
                "edit schema.yaml",
            ]
        );
    }

    #[test]
    fn one_item_and_one_definition_is_mixed() {
        let summary = summarize(&[
            status_move("a", "A", "to_do", "in_progress"),
            definition("config.yaml", Some("x"), Some("y")),
        ]);
        assert_eq!(summary.subject, "Update 1 work item, edit config.yaml");
    }

    #[test]
    fn two_definition_files_are_named_together() {
        let summary = summarize(&[
            definition("schema.yaml", Some("x"), Some("y")),
            definition("views.yaml", Some("x"), Some("y")),
        ]);
        assert_eq!(summary.subject, "Edit schema.yaml and views.yaml");
        assert_eq!(summary.body, vec!["edit schema.yaml", "edit views.yaml"]);
    }

    #[test]
    fn many_or_differing_definition_files_are_counted() {
        let summary = summarize(&[
            definition("schema.yaml", Some("x"), Some("y")),
            definition("templates/bug.md", None, Some("y")),
        ]);
        assert_eq!(summary.subject, "Update 2 project files");
        let summary = summarize(&[
            definition("schema.yaml", Some("x"), Some("y")),
            definition("views.yaml", Some("x"), Some("y")),
            definition("resources.yaml", Some("x"), Some("y")),
        ]);
        assert_eq!(summary.subject, "Edit 3 project files");
    }

    #[test]
    fn body_is_capped_with_a_count_of_the_rest() {
        let files: Vec<ChangedFile> = (0..13)
            .map(|index| {
                let to = if index % 2 == 0 {
                    "done"
                } else {
                    "in_progress"
                };
                status_move(
                    &format!("item-{index}"),
                    &format!("Item {index}"),
                    "to_do",
                    to,
                )
            })
            .collect();
        let summary = summarize(&files);
        assert_eq!(summary.subject, "Update 13 work items");
        assert_eq!(summary.body.len(), BODY_LINE_CAP + 1);
        assert_eq!(summary.body[0], "Item 0: Status → Done");
        assert_eq!(summary.body[BODY_LINE_CAP], "…and 3 more");
    }

    // ── Edge cases ───────────────────────────────────────────────────

    #[test]
    fn identical_texts_are_skipped() {
        let same = item("title: A\nstatus: to_do", "");
        let summary = summarize(&[
            modified("a", &same, &same),
            status_move("b", "B", "to_do", "done"),
        ]);
        assert_eq!(summary.subject, "B: Status → Done");
    }

    #[test]
    fn nothing_to_say() {
        assert_eq!(summarize(&[]).subject, "No changes");
    }

    #[test]
    fn message_joins_subject_and_body() {
        let summary = ChangeSummary {
            subject: "Update 2 work items".to_owned(),
            body: vec!["A: x".to_owned(), "B: y".to_owned()],
        };
        assert_eq!(summary.to_message(), "Update 2 work items\n\nA: x\nB: y");
        let subject_only = ChangeSummary {
            subject: "Edit schema.yaml".to_owned(),
            body: Vec::new(),
        };
        assert_eq!(subject_only.to_message(), "Edit schema.yaml");
    }
}
