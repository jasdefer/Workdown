//! Persist single field definitions to `schema.yaml`.
//!
//! The read side of the schema is [`crate::parser::schema`] and, for the
//! web app, [`crate::schema_definition_data`]; this module is the write
//! side. It supports what the schema editor needs: adding a field,
//! replacing a field's plain properties, removing a field, reordering
//! the fields, and saying what a field is used by
//! ([`field_usage`]). Renaming a field and editing a recipe (`compute`,
//! `when`, `pull`, `aggregate`) are not offered; the design item
//! `schema-editor-web-design` explains why.
//!
//! Like every other mutation in the tool, the repo stays the source of
//! truth: writes update the working tree only, nothing is staged or
//! committed.
//!
//! ## Generic-tree edit
//!
//! A write reads `schema.yaml` fresh, parses it into a generic
//! `serde_yaml` tree, changes the one entry under `fields:` and writes
//! the whole tree back. The typed [`Schema`] model has no writer, and
//! one would have to turn compiled expressions back into text; editing
//! the tree cannot damage what it does not touch, so a field's recipe
//! keys survive a replace untouched and every other field keeps its
//! content and its position. What is given up is formatting: comments
//! are not preserved, and compact `[a, b]` lists come back one entry per
//! line. The file is the source of truth for *content*, not for
//! formatting. Because only one entry is ever replaced, a stale editor
//! tab can overwrite that field and nothing else: last write wins, per
//! field, and there is no conflict detection.
//!
//! ## What blocks a write vs. what only warns
//!
//! A write is rejected, leaving `schema.yaml` untouched, when the
//! candidate would not *load*: the serialized tree is run through
//! [`parse_schema`] first, and a parse failure comes back as
//! [`SchemaWriteError::Unloadable`] carrying the parser's message. That
//! covers everything the parser checks — a `choice` without values, a
//! default the type cannot hold, a rule or an `aggregate.over` naming a
//! field being removed. Problems that still load but fail the cross-file
//! checks — items holding a value for a removed field, a view naming it,
//! a compute expression reading it — are written and surfaced through
//! [`SchemaWriteOutcome::warnings`], the save-with-warning convention
//! from ADR-001.
//!
//! On top of the parser's verdict, a handful of refusals are this
//! module's own, so that a CLI wrapper later inherits them: the `id`
//! field cannot be removed; a field a `config.yaml` role names cannot
//! be removed, because the config is read once at startup and the break
//! would surface only on restart; a field with a recipe keeps its type,
//! because the recipe was type-checked against it; any other type
//! change must be a pair in [`widening_targets`].
//!
//! ## Field usage
//!
//! [`field_usage`] answers "what depends on this field" by simulation
//! rather than enumeration: it removes the field from the schema in
//! memory and loads the project against the candidate. If the candidate
//! does not parse, the parse error is the blocker (a rule or a rollup
//! names the field). If it parses, the diagnostics the removal would
//! introduce are returned, each with its scope (ADR-007), so a client can
//! name the view or the config role and count the items holding a value.
//! Asking the checks that would complain after a real removal cannot
//! drift from them when a view kind or a check is added.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config_check;
use crate::model::config::Config;
use crate::model::diagnostic::{ConfigDiagnosticKind, Diagnostic};
use crate::model::duration::format_duration_seconds;
use crate::model::schema::{widening_targets, FieldType, Generator, Schema, Severity};
use crate::operations::diagnostics::{introduced_by_mutation, introduced_diagnostics};
use crate::operations::frontmatter_io::write_file_atomically;
use crate::parser::schema::{is_valid_field_name, parse_schema, SchemaLoadError};
use crate::project::{load_project_with_schema, LoadError};
use crate::schema_definition_data::FieldShape;

// ── Public types ─────────────────────────────────────────────────────

/// One field definition as the editor writes it: the header every type
/// shares plus the type-specific shape. The read payload's
/// [`FieldDefinitionData`](crate::schema_definition_data::FieldDefinitionData)
/// minus its `name` (the operation takes it separately) and minus the
/// derived blocks, which are displayed but not edited. What the form
/// GETs is what it PUTs back.
///
/// The `shape` is not checked against `field_type` here: the pair is
/// written to the tree as the properties it spells out, and the schema
/// parser is the judge of whether they fit the type, exactly as for a
/// hand-edited file.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
pub struct FieldDefinitionWrite {
    /// The built-in type.
    pub field_type: FieldType,
    /// Whether every item must carry the field.
    #[serde(default)]
    pub required: bool,
    /// Human-readable explanation; `None` clears it.
    #[serde(default)]
    pub description: Option<String>,
    /// The `default:` applied at `workdown add` time; `None` clears it.
    /// Left untouched on a field with a `when:` recipe, where `default:`
    /// is the recipe's evaluated fallback rather than an add-time value.
    #[serde(default)]
    pub default: Option<DefaultWrite>,
    /// Resource section in `resources.yaml` that constrains the values;
    /// `None` clears it.
    #[serde(default)]
    pub resource: Option<String>,
    /// The type-specific properties.
    pub shape: FieldShape,
}

/// A field's `default:` as the editor sends it.
#[derive(Debug, Clone, Deserialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DefaultWrite {
    /// A `$`-generator, written as its token.
    Generator { generator: Generator },
    /// A literal, in the form the item editor's set endpoint sends a
    /// value (a JSON scalar; a duration as its shorthand text). Written
    /// to the file as it arrives and judged by the schema parser, which
    /// runs every literal default through the field's own coercion.
    Literal {
        #[ts(type = "unknown")]
        value: serde_yaml::Value,
    },
}

/// What depends on a field: the answer [`field_usage`] gives.
///
/// The schema's own references come by name — the rules and the
/// recipes naming the field — because a client shows them as a list to
/// edit first. Everything outside the schema file comes as the
/// diagnostics the removal would introduce, with their scope, which is
/// how a client tells a view from a config role from an item.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct FieldUsage {
    /// The rules whose `match`, `require` or `count` names the field,
    /// directly or through its inverse (`children.status` names
    /// `status`, and `parent` through `children`). In file order.
    pub rules: Vec<String>,
    /// The other fields whose `aggregate` or `pull` climbs or reads
    /// the field (`aggregate.over`, `pull.over`, `pull.field`). In
    /// declaration order. A `compute` expression reading the field is
    /// reported through `introduced` instead, as the warning it would
    /// become.
    pub recipes: Vec<String>,
    /// Set when removing the field would leave `schema.yaml` unable to
    /// load, with the parser's message. `rules` and `recipes` are the
    /// explanation when either is non-empty; the message is for a
    /// client to show when neither names anything. `introduced` is
    /// empty then: nothing further can be asked of a schema that does
    /// not parse.
    pub parse_error: Option<String>,
    /// The diagnostics removing the field would introduce, each with
    /// its scope: config findings for the views and the `config.yaml`
    /// roles naming the field and for recipes reading it, item findings
    /// for every item holding a value.
    pub introduced: Vec<Diagnostic>,
}

/// The outcome of a successful schema write.
#[derive(Debug)]
pub struct SchemaWriteOutcome {
    /// Path to the `schema.yaml` that was written.
    pub path: PathBuf,
    /// The field that was added, changed or removed; `None` for a
    /// reorder, which touches every field and none.
    pub field_name: Option<String>,
    /// Every diagnostic from loading the project against the written
    /// schema. Includes any problem this write introduced as well as
    /// pre-existing ones (surfaced, per the "always show all"
    /// convention, but not blocking).
    pub warnings: Vec<Diagnostic>,
    /// `true` if this write introduced a diagnostic that wasn't present
    /// before. Drives the caller's exit code / response, distinct from
    /// pre-existing problems elsewhere in the project.
    pub mutation_caused_warning: bool,
}

/// Errors returned by the schema-write operations.
///
/// Every variant here is a hard fail: `schema.yaml` is left untouched.
/// Soft problems ride through [`SchemaWriteOutcome::warnings`] instead —
/// the file still gets written.
#[derive(Debug, thiserror::Error)]
pub enum SchemaWriteError {
    /// The current `schema.yaml` cannot be read or does not load. The
    /// whole file is re-serialized from its tree, so a file that cannot
    /// be read as a schema cannot be safely written back.
    #[error("existing schema file at '{path}' does not load; fix it in a text editor before writing from the UI: {detail}")]
    ExistingInvalid { path: PathBuf, detail: String },

    /// The work items could not be read, so the effect of the change on
    /// them cannot be judged. A hard fail for the same reason `add` and
    /// `set` treat it as one: a mutation decided against an unknown
    /// project state is worse than a mutation refused.
    #[error("{0}")]
    ProjectLoad(#[from] LoadError),

    #[error("invalid field name '{name}': must be lowercase letters, digits and underscores, starting with a letter or underscore")]
    InvalidName { name: String },

    #[error("a field named '{name}' already exists")]
    DuplicateName { name: String },

    #[error("no field named '{name}'")]
    FieldNotFound { name: String },

    #[error("the 'id' field cannot be removed")]
    IdNotRemovable,

    /// `config.yaml` is read once at startup, so a role naming a removed
    /// field would break only on the next restart; the config has to
    /// change first.
    #[error("field '{name}' is named by {slots} in config.yaml; change the config first")]
    NamedByConfig { name: String, slots: String },

    #[error("field '{name}' has a recipe (compute, when, pull or aggregate) type-checked against '{current}'; changing its type to '{requested}' means editing schema.yaml")]
    RecipeTypeLocked {
        name: String,
        current: FieldType,
        requested: FieldType,
    },

    #[error("field '{name}' cannot change type from '{current}' to '{requested}': existing values could become invalid")]
    TypeChangeNotAllowed {
        name: String,
        current: FieldType,
        requested: FieldType,
    },

    #[error("field order must list every current field exactly once: {detail}")]
    InvalidOrder { detail: String },

    /// The candidate schema does not parse. Carries the parser's message
    /// so the editor can show what the file would have said.
    #[error("the change would leave schema.yaml unable to load; nothing was written: {detail}")]
    Unloadable { detail: String },

    #[error("failed to serialize schema: {0}")]
    Serialize(serde_yaml::Error),

    #[error("failed to write '{path}': {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
}

// ── Public API ───────────────────────────────────────────────────────

/// Add a field, appended at the end of `fields:`.
///
/// The properties are written in a fixed order — type, the
/// type-specific properties, required, default, description, resource —
/// so a created entry reads like a hand-written one.
pub fn add_field(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    name: &str,
    definition: &FieldDefinitionWrite,
) -> Result<SchemaWriteOutcome, SchemaWriteError> {
    if !is_valid_field_name(name) {
        return Err(SchemaWriteError::InvalidName {
            name: name.to_owned(),
        });
    }
    let mut context = load_for_write(config, project_root, config_path)?;

    let fields = fields_mut(&mut context.document);
    if fields.contains_key(name) {
        return Err(SchemaWriteError::DuplicateName {
            name: name.to_owned(),
        });
    }
    let mut entry = serde_yaml::Mapping::new();
    for (key, value) in plain_properties(definition) {
        entry.insert(yaml_key(key), value);
    }
    fields.insert(yaml_key(name), serde_yaml::Value::Mapping(entry));

    finalize(context, Some(name.to_owned()))
}

/// Replace a field's plain properties, keeping its position and its
/// recipe keys.
///
/// The field's existing mapping is the starting point: the plain
/// properties the definition sets are overwritten in place, those it
/// no longer sets (a `values:` list after a change to `string`, a
/// cleared description) are removed, and everything else — `compute`,
/// `when`, `pull`, `aggregate`, and `default` beside a `when` — is left
/// exactly as the file has it.
///
/// A type change is allowed only for a field without a recipe, and only
/// to a type in [`widening_targets`].
pub fn update_field(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    name: &str,
    definition: &FieldDefinitionWrite,
) -> Result<SchemaWriteOutcome, SchemaWriteError> {
    let mut context = load_for_write(config, project_root, config_path)?;

    let current =
        context
            .current
            .fields
            .get(name)
            .ok_or_else(|| SchemaWriteError::FieldNotFound {
                name: name.to_owned(),
            })?;
    let current_type = current.field_type();
    if definition.field_type != current_type {
        if current.has_fill_mechanism() {
            return Err(SchemaWriteError::RecipeTypeLocked {
                name: name.to_owned(),
                current: current_type,
                requested: definition.field_type,
            });
        }
        if !widening_targets(current_type).contains(&definition.field_type) {
            return Err(SchemaWriteError::TypeChangeNotAllowed {
                name: name.to_owned(),
                current: current_type,
                requested: definition.field_type,
            });
        }
    }
    // Beside `when`, `default:` is the recipe's evaluated fallback (see
    // `parser::schema::attach_when_configs`), not an add-time default:
    // it belongs to the recipe and stays as written.
    let default_is_recipe = current.when.is_some();

    let entry = fields_mut(&mut context.document)
        .get_mut(name)
        .and_then(serde_yaml::Value::as_mapping_mut)
        .expect("a field the typed schema knows is a mapping in the tree");
    let properties = plain_properties(definition);
    let is_editable = |key: &str| !(default_is_recipe && key == "default");
    for key in PLAIN_PROPERTY_KEYS {
        let still_set = properties.iter().any(|(set_key, _)| *set_key == key);
        if is_editable(key) && !still_set {
            entry.shift_remove(key);
        }
    }
    for (key, value) in properties {
        if is_editable(key) {
            entry.insert(yaml_key(key), value);
        }
    }

    finalize(context, Some(name.to_owned()))
}

/// Remove a field. Items keep the key and get the unknown-field warning
/// on the next load; nothing rewrites them.
pub fn remove_field(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    name: &str,
) -> Result<SchemaWriteOutcome, SchemaWriteError> {
    if name == "id" {
        return Err(SchemaWriteError::IdNotRemovable);
    }
    let mut context = load_for_write(config, project_root, config_path)?;

    if !context.current.fields.contains_key(name) {
        return Err(SchemaWriteError::FieldNotFound {
            name: name.to_owned(),
        });
    }
    let slots = config_check::roles_naming_field(config, &context.current, name);
    if !slots.is_empty() {
        return Err(SchemaWriteError::NamedByConfig {
            name: name.to_owned(),
            slots: slots.join(", "),
        });
    }
    fields_mut(&mut context.document).shift_remove(name);

    finalize(context, Some(name.to_owned()))
}

/// Rewrite the order of `fields:` from a full ordered list of the
/// current names. Declaration order drives form layout and board
/// columns, which is why it is editable at all.
pub fn reorder_fields(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    order: &[String],
) -> Result<SchemaWriteOutcome, SchemaWriteError> {
    let mut context = load_for_write(config, project_root, config_path)?;

    let fields = fields_mut(&mut context.document);
    let current_names: Vec<String> = fields
        .keys()
        .filter_map(serde_yaml::Value::as_str)
        .map(str::to_owned)
        .collect();
    check_permutation(&current_names, order)?;
    let mut reordered = serde_yaml::Mapping::with_capacity(order.len());
    for name in order {
        let entry = fields
            .shift_remove(name.as_str())
            .expect("the permutation check found every name");
        reordered.insert(yaml_key(name), entry);
    }
    *fields = reordered;

    finalize(context, None)
}

/// What depends on `name`: the parse error removing it would cause, or
/// the diagnostics it would introduce (see the module docs).
///
/// The `config.yaml` roles naming the field are reported by the
/// project's own config check; should that check ever stay silent
/// about one, the role is added from the config directly, so the
/// answer never omits a reason [`remove_field`] would refuse for.
pub fn field_usage(
    config: &Config,
    project_root: &Path,
    config_path: &Path,
    name: &str,
) -> Result<FieldUsage, SchemaWriteError> {
    let mut context = load_for_write(config, project_root, config_path)?;
    if !context.current.fields.contains_key(name) {
        return Err(SchemaWriteError::FieldNotFound {
            name: name.to_owned(),
        });
    }
    let named_by = config_check::roles_naming_field(config, &context.current, name);
    let rules = rules_naming_field(&context.current, name);
    let recipes = recipes_naming_field(&context.current, name);

    fields_mut(&mut context.document).shift_remove(name);
    let candidate_text =
        serde_yaml::to_string(&context.document).map_err(SchemaWriteError::Serialize)?;
    let candidate = match parse_schema(&candidate_text) {
        Ok(candidate) => candidate,
        Err(error) => {
            return Ok(FieldUsage {
                rules,
                recipes,
                parse_error: Some(error.to_string()),
                introduced: Vec::new(),
            })
        }
    };
    let project = load_project_with_schema(config, project_root, config_path, candidate, None)?;

    let mut introduced = introduced_diagnostics(&context.pre_diagnostics, &project.diagnostics);
    for slot in named_by {
        let already_reported = introduced
            .iter()
            .any(|diagnostic| diagnostic.config_slot() == Some(slot));
        if !already_reported {
            introduced.push(Diagnostic::config(
                Severity::Warning,
                project_root.join(config_path),
                ConfigDiagnosticKind::ConfigUnknownField {
                    slot,
                    field_name: name.to_owned(),
                },
            ));
        }
    }
    Ok(FieldUsage {
        rules,
        recipes,
        parse_error: None,
        introduced,
    })
}

/// The rules naming `name` in a `match` or `require` key, in file
/// order. A key is a field reference, bare or dotted (`children.status`);
/// it names the field when any segment is the field's name or its
/// inverse — the same segments the parser resolves against the fields.
fn rules_naming_field(schema: &Schema, name: &str) -> Vec<String> {
    let inverse = schema
        .fields
        .get(name)
        .and_then(|definition| definition.inverse());
    let names_field = |reference: &str| {
        reference
            .split('.')
            .any(|segment| segment == name || Some(segment) == inverse)
    };
    schema
        .rules
        .iter()
        .filter(|rule| {
            rule.match_conditions
                .keys()
                .chain(rule.require.keys())
                .any(|reference| names_field(reference))
        })
        .map(|rule| rule.name.clone())
        .collect()
}

/// The other fields whose rollup or pull names `name`: as the relation
/// climbed (`aggregate.over`, `pull.over`) or as the field read
/// (`pull.field`). In declaration order.
fn recipes_naming_field(schema: &Schema, name: &str) -> Vec<String> {
    schema
        .fields
        .iter()
        .filter(|(other, _)| other.as_str() != name)
        .filter(|(_, definition)| {
            let climbs = definition
                .aggregate
                .as_ref()
                .is_some_and(|aggregate| aggregate.over == name);
            let pulls = definition
                .pull
                .as_ref()
                .is_some_and(|pull| pull.over == name || pull.field == name);
            climbs || pulls
        })
        .map(|(other, _)| other.clone())
        .collect()
}

// ── Internals ────────────────────────────────────────────────────────

/// Every key the editor may set or clear on a field. The complement —
/// `compute`, `when`, `pull`, `aggregate` — is the recipe, which a
/// replace never touches.
const PLAIN_PROPERTY_KEYS: [&str; 11] = [
    "type",
    "values",
    "pattern",
    "min",
    "max",
    "allow_cycles",
    "inverse",
    "required",
    "default",
    "description",
    "resource",
];

/// Everything a write starts from, loaded once: the current file as a
/// generic tree (what gets edited), as a typed schema (what the
/// refusals consult), and the diagnostics of the project against it
/// (the baseline the post-write diagnostics are diffed against).
struct WriteContext<'a> {
    config: &'a Config,
    project_root: &'a Path,
    config_path: &'a Path,
    schema_path: PathBuf,
    document: serde_yaml::Value,
    current: Schema,
    pre_diagnostics: Vec<Diagnostic>,
}

fn load_for_write<'a>(
    config: &'a Config,
    project_root: &'a Path,
    config_path: &'a Path,
) -> Result<WriteContext<'a>, SchemaWriteError> {
    let schema_path = project_root.join(&config.schema);
    let existing_invalid = |error: SchemaLoadError| SchemaWriteError::ExistingInvalid {
        path: schema_path.clone(),
        detail: error.to_string(),
    };
    let text = std::fs::read_to_string(&schema_path)
        .map_err(|error| existing_invalid(SchemaLoadError::ReadFailed(error)))?;
    let current = parse_schema(&text).map_err(existing_invalid)?;
    // The validating parse accepted the same text, so a generic parse
    // cannot fail on syntax; a failure here would be a serde_yaml bug.
    let document: serde_yaml::Value = serde_yaml::from_str(&text)
        .map_err(|error| existing_invalid(SchemaLoadError::InvalidYaml(error)))?;

    let project = load_project_with_schema(config, project_root, config_path, current, None)?;

    Ok(WriteContext {
        config,
        project_root,
        config_path,
        schema_path,
        document,
        current: project.schema,
        pre_diagnostics: project.diagnostics,
    })
}

/// The `fields:` mapping of a loaded schema's tree. The typed parse
/// already required it to be a mapping.
fn fields_mut(document: &mut serde_yaml::Value) -> &mut serde_yaml::Mapping {
    document
        .get_mut("fields")
        .and_then(serde_yaml::Value::as_mapping_mut)
        .expect("a loaded schema has a fields mapping")
}

fn yaml_key(key: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(key.to_owned())
}

/// The plain properties a definition spells out, as `(key, value)` pairs
/// in the order they are written: type, the type-specific properties,
/// required, default, description, resource. Only set properties appear.
fn plain_properties(definition: &FieldDefinitionWrite) -> Vec<(&'static str, serde_yaml::Value)> {
    use serde_yaml::Value;

    let mut properties = vec![("type", Value::String(definition.field_type.to_string()))];

    match &definition.shape {
        FieldShape::Scalar => {}
        FieldShape::Numeric { min, max } => {
            if let Some(min) = min {
                properties.push(("min", numeric_bound(*min, definition.field_type)));
            }
            if let Some(max) = max {
                properties.push(("max", numeric_bound(*max, definition.field_type)));
            }
        }
        FieldShape::Duration {
            min_seconds,
            max_seconds,
        } => {
            if let Some(min) = min_seconds {
                properties.push(("min", Value::String(format_duration_seconds(*min))));
            }
            if let Some(max) = max_seconds {
                properties.push(("max", Value::String(format_duration_seconds(*max))));
            }
        }
        FieldShape::Text { pattern } => {
            if let Some(pattern) = pattern {
                properties.push(("pattern", Value::String(pattern.clone())));
            }
        }
        FieldShape::Values { values } => {
            properties.push((
                "values",
                Value::Sequence(values.iter().cloned().map(Value::String).collect()),
            ));
        }
        FieldShape::Relation {
            allow_cycles,
            inverse,
        } => {
            if let Some(allow_cycles) = allow_cycles {
                properties.push(("allow_cycles", Value::Bool(*allow_cycles)));
            }
            if let Some(inverse) = inverse {
                properties.push(("inverse", Value::String(inverse.clone())));
            }
        }
    }

    properties.push(("required", Value::Bool(definition.required)));
    match &definition.default {
        None => {}
        Some(DefaultWrite::Generator { generator }) => {
            properties.push(("default", Value::String(generator.token().to_owned())));
        }
        Some(DefaultWrite::Literal { value }) => properties.push(("default", value.clone())),
    }
    if let Some(description) = &definition.description {
        properties.push(("description", Value::String(description.clone())));
    }
    if let Some(resource) = &definition.resource {
        properties.push(("resource", Value::String(resource.clone())));
    }
    properties
}

/// A numeric bound as the file spells it: an integer field's whole-number
/// bound is written without a fraction (`min: 0`, not `min: 0.0`), a
/// float field's bound keeps the shape the editor sent.
fn numeric_bound(value: f64, field_type: FieldType) -> serde_yaml::Value {
    let is_whole = value.fract() == 0.0 && value.abs() < (i64::MAX as f64);
    if field_type == FieldType::Integer && is_whole {
        serde_yaml::Value::Number((value as i64).into())
    } else {
        serde_yaml::Value::Number(value.into())
    }
}

/// `order` names every entry of `current` exactly once.
fn check_permutation(current: &[String], order: &[String]) -> Result<(), SchemaWriteError> {
    let invalid = |detail: String| SchemaWriteError::InvalidOrder { detail };
    let mut seen: HashSet<&str> = HashSet::with_capacity(order.len());
    for name in order {
        if !current.contains(name) {
            return Err(invalid(format!("'{name}' is not a field")));
        }
        if !seen.insert(name.as_str()) {
            return Err(invalid(format!("'{name}' is listed twice")));
        }
    }
    if let Some(missing) = current.iter().find(|name| !seen.contains(name.as_str())) {
        return Err(invalid(format!("'{missing}' is missing")));
    }
    Ok(())
}

/// Serialize the edited tree, refuse it if it would not load, load the
/// project against it, write atomically, and diff diagnostics to flag
/// whether this write introduced a new problem.
fn finalize(
    context: WriteContext<'_>,
    field_name: Option<String>,
) -> Result<SchemaWriteOutcome, SchemaWriteError> {
    let WriteContext {
        config,
        project_root,
        config_path,
        schema_path,
        document,
        pre_diagnostics,
        current: _,
    } = context;
    let candidate_text = serde_yaml::to_string(&document).map_err(SchemaWriteError::Serialize)?;

    // The parser is the judge of the candidate: a schema that does not
    // load would take every read of the project down with it, so it
    // never reaches disk.
    let candidate =
        parse_schema(&candidate_text).map_err(|error| SchemaWriteError::Unloadable {
            detail: error.to_string(),
        })?;
    let project = load_project_with_schema(config, project_root, config_path, candidate, None)?;

    write_file_atomically(&schema_path, &candidate_text).map_err(|source| {
        SchemaWriteError::WriteFile {
            path: schema_path.clone(),
            source,
        }
    })?;

    let mutation_caused_warning = introduced_by_mutation(&pre_diagnostics, &project.diagnostics);
    Ok(SchemaWriteOutcome {
        path: schema_path,
        field_name,
        warnings: project.diagnostics,
        mutation_caused_warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn keys(properties: &[(&'static str, serde_yaml::Value)]) -> Vec<&'static str> {
        properties.iter().map(|(key, _)| *key).collect()
    }

    /// Every way a rule or a recipe can name a field: bare, as the
    /// second segment of a dotted reference, as the relation walked
    /// (directly or by its inverse), and as the field a pull reads.
    const REFERENCING_SCHEMA: &str = "\
fields:
  id:
    type: string
  status:
    type: choice
    values: [open, done]
  parent:
    type: link
    inverse: children
    allow_cycles: false
  depends_on:
    type: links
    allow_cycles: false
  points:
    type: integer
  total:
    type: integer
    aggregate:
      function: sum
      over: parent
  blocked_points:
    type: integer
    pull:
      over: depends_on
      field: points
      function: sum
rules:
  - name: bare
    require:
      points: required
  - name: through-children
    match:
      children.status: done
    count:
      min: 0
  - name: counts-children
    require:
      children:
        min_count: 1
  - name: via-dependency
    match:
      depends_on.status: open
    count:
      min: 0
  - name: unrelated
    match:
      id: x
    count:
      min: 0
";

    #[test]
    fn rules_naming_field_resolves_bare_dotted_and_inverse_references() {
        let schema = parse_schema(REFERENCING_SCHEMA).unwrap();
        assert_eq!(rules_naming_field(&schema, "points"), vec!["bare"]);
        assert_eq!(
            rules_naming_field(&schema, "status"),
            vec!["through-children", "via-dependency"]
        );
        assert_eq!(
            rules_naming_field(&schema, "parent"),
            vec!["through-children", "counts-children"]
        );
        assert_eq!(
            rules_naming_field(&schema, "depends_on"),
            vec!["via-dependency"]
        );
        assert!(rules_naming_field(&schema, "total").is_empty());
    }

    #[test]
    fn recipes_naming_field_finds_the_relation_climbed_and_the_field_read() {
        let schema = parse_schema(REFERENCING_SCHEMA).unwrap();
        assert_eq!(recipes_naming_field(&schema, "parent"), vec!["total"]);
        assert_eq!(
            recipes_naming_field(&schema, "depends_on"),
            vec!["blocked_points"]
        );
        assert_eq!(
            recipes_naming_field(&schema, "points"),
            vec!["blocked_points"]
        );
        assert!(recipes_naming_field(&schema, "status").is_empty());
        assert!(
            recipes_naming_field(&schema, "total").is_empty(),
            "a field never names itself"
        );
    }

    #[test]
    fn properties_are_written_in_the_fixed_order() {
        let mut full = definition(
            FieldType::Choice,
            FieldShape::Values {
                values: vec!["a".to_owned()],
            },
        );
        full.required = true;
        full.default = Some(DefaultWrite::Literal {
            value: serde_yaml::Value::String("a".to_owned()),
        });
        full.description = Some("d".to_owned());
        full.resource = Some("r".to_owned());
        assert_eq!(
            keys(&plain_properties(&full)),
            [
                "type",
                "values",
                "required",
                "default",
                "description",
                "resource"
            ]
        );
    }

    #[test]
    fn every_shape_spells_its_properties() {
        let numeric = definition(
            FieldType::Float,
            FieldShape::Numeric {
                min: Some(0.5),
                max: None,
            },
        );
        assert_eq!(
            keys(&plain_properties(&numeric)),
            ["type", "min", "required"]
        );

        let duration = definition(
            FieldType::Duration,
            FieldShape::Duration {
                min_seconds: Some(3600),
                max_seconds: Some(14 * 24 * 3600),
            },
        );
        let properties = plain_properties(&duration);
        assert_eq!(properties[1].1, serde_yaml::Value::String("1h".to_owned()));
        assert_eq!(properties[2].1, serde_yaml::Value::String("2w".to_owned()));

        let text = definition(
            FieldType::String,
            FieldShape::Text {
                pattern: Some("^x".to_owned()),
            },
        );
        assert_eq!(
            keys(&plain_properties(&text)),
            ["type", "pattern", "required"]
        );

        let relation = definition(
            FieldType::Link,
            FieldShape::Relation {
                allow_cycles: Some(false),
                inverse: Some("children".to_owned()),
            },
        );
        assert_eq!(
            keys(&plain_properties(&relation)),
            ["type", "allow_cycles", "inverse", "required"]
        );

        let scalar = definition(FieldType::Date, FieldShape::Scalar);
        assert_eq!(keys(&plain_properties(&scalar)), ["type", "required"]);
    }

    #[test]
    fn a_generator_default_is_written_as_its_token() {
        let mut with_generator = definition(FieldType::Date, FieldShape::Scalar);
        with_generator.default = Some(DefaultWrite::Generator {
            generator: Generator::Today,
        });
        let properties = plain_properties(&with_generator);
        assert_eq!(
            properties[2],
            ("default", serde_yaml::Value::String("$today".to_owned()))
        );
    }

    #[test]
    fn integer_bounds_lose_the_fraction_float_bounds_keep_it() {
        assert_eq!(
            numeric_bound(3.0, FieldType::Integer),
            serde_yaml::Value::Number(3.into())
        );
        assert_eq!(
            numeric_bound(3.0, FieldType::Float),
            serde_yaml::Value::Number(3.0.into())
        );
        assert_eq!(
            numeric_bound(2.5, FieldType::Integer),
            serde_yaml::Value::Number(2.5.into())
        );
    }

    #[test]
    fn permutation_check_names_the_first_problem() {
        let current = ["a".to_owned(), "b".to_owned()];
        assert!(check_permutation(&current, &["b".to_owned(), "a".to_owned()]).is_ok());
        for (order, expected) in [
            (vec!["a".to_owned(), "c".to_owned()], "'c' is not a field"),
            (vec!["a".to_owned(), "a".to_owned()], "'a' is listed twice"),
            (vec!["a".to_owned()], "'b' is missing"),
        ] {
            let error = check_permutation(&current, &order).unwrap_err();
            assert!(
                matches!(&error, SchemaWriteError::InvalidOrder { detail } if detail == expected),
                "{order:?}: {error}"
            );
        }
    }
}
