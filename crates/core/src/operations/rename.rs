//! `workdown rename` — change a work item's id.
//!
//! Two coupled effects: move the file on disk, and rewrite every other
//! item's `link`/`links` fields that point at the old id. Carved out of
//! `set` because the operation crosses files and changes identity rather
//! than mutating one field.
//!
//! Same three-phase shape as `set.rs`: `preflight` → `compute_plan` →
//! `execute_plan` + `finalize`. Reuses `frontmatter_io::build_frontmatter_yaml`
//! and `write_file_atomically`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::model::config::Config;
use crate::model::diagnostic::Diagnostic;
use crate::model::schema::{FieldType, Schema};
use crate::model::work_item::is_valid_id;
use crate::model::WorkItemId;
use crate::operations::frontmatter_io::{build_frontmatter_yaml, write_file_atomically};
use crate::parser;
use crate::parser::schema::SchemaLoadError;
use crate::store::Store;

// ── Public types ─────────────────────────────────────────────────────

/// Knobs the caller can flip without growing the function signature.
/// Currently just `dry_run`; future flags (force, verbose) land here.
#[derive(Debug, Clone, Default)]
pub struct RenameOptions {
    /// When true, return the plan but write nothing to disk.
    pub dry_run: bool,
}

/// The result of a successful (or dry-run) `workdown rename`.
///
/// `mutation_caused_warning` is the post-write reload diff, same
/// convention as `SetOutcome`: `true` if a diagnostic exists after the
/// write that wasn't already present before. Pre-existing project-wide
/// problems remain visible in `warnings` but don't fail this rename.
#[derive(Debug)]
pub struct RenameOutcome {
    pub old_id: WorkItemId,
    pub new_id: WorkItemId,
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    /// Every file whose typed link fields were rewritten, including the
    /// renamed item itself (if it had self-links). Sorted by path.
    pub rewritten_files: Vec<RewrittenFile>,
    /// Occurrences of `old_id` outside typed link fields — bodies, YAML
    /// configs, templates, and the full contents of items that failed to
    /// parse. Warn-only; never rewritten.
    pub textual_matches: Vec<TextualMatch>,
    /// Full post-write diagnostic list (or empty under `dry_run`).
    pub warnings: Vec<Diagnostic>,
    pub mutation_caused_warning: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct RewrittenFile {
    pub path: PathBuf,
    pub id: WorkItemId,
    pub field_rewrites: Vec<FieldRewrite>,
}

#[derive(Debug, Clone)]
pub struct FieldRewrite {
    pub field: String,
    pub previous_value: serde_yaml::Value,
    pub new_value: serde_yaml::Value,
}

#[derive(Debug, Clone)]
pub struct TextualMatch {
    pub path: PathBuf,
    pub line: usize,
    pub kind: TextualMatchKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextualMatchKind {
    /// Mention in a parsed item's freeform Markdown body.
    ItemBody,
    /// Mention anywhere in a file whose frontmatter failed to parse —
    /// rename couldn't tell body from frontmatter, so the whole file is
    /// scanned.
    UnparseableItem,
    /// `.workdown/config.yaml`.
    Config,
    /// `schema.yaml` (resolved from `config.schema`).
    Schema,
    /// `views.yaml` (resolved from `config.paths.views`).
    Views,
    /// `resources.yaml` (resolved from `config.paths.resources`).
    Resources,
    /// A template file under `config.paths.templates`.
    Template,
}

/// Errors returned by [`run_rename`].
///
/// All variants except `PartialFailure` are pre-write hard fails — the
/// project is untouched. `PartialFailure` reports the in-between state
/// after an I/O error mid-execute, including which files were written
/// and which file is left over to delete.
#[derive(Debug, thiserror::Error)]
pub enum RenameError {
    #[error("failed to load schema: {0}")]
    SchemaLoad(#[from] SchemaLoadError),

    #[error("failed to load work items: {0}")]
    StoreLoad(#[from] std::io::Error),

    #[error("unknown work item '{id}'")]
    UnknownItem { id: String },

    #[error(
        "invalid new id '{id}': must be non-empty, lowercase alphanumeric with hyphens, \
         start with a letter or digit, and not end with a hyphen"
    )]
    InvalidNewId { id: String },

    #[error("an item with id '{id}' already exists")]
    IdAlreadyExists { id: String },

    #[error("a file already exists at '{}'", .path.display())]
    FileAlreadyExists { path: PathBuf },

    #[error("source and target ids are identical: '{id}'")]
    SameId { id: String },

    #[error("failed to read '{}': {source}", .path.display())]
    ReadTarget {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse '{}': {source}", .path.display())]
    ParseTarget {
        path: PathBuf,
        source: parser::ParseError,
    },

    /// Heavy payload (~128 bytes of paths + an io error) is boxed to
    /// keep `Result<_, RenameError>` cheap to pass around at every
    /// internal call site. See `clippy::result_large_err`.
    #[error(transparent)]
    PartialFailure(Box<PartialFailureContext>),
}

/// Detail for [`RenameError::PartialFailure`]. Boxed in the error
/// enum so the rest of the variants stay small.
#[derive(Debug, thiserror::Error)]
#[error(
    "partial failure renaming '{old_id}' → '{new_id}'. \
     Wrote {} file(s); failed writing '{}'. \
     Source file still at '{}'. \
     Re-run `workdown rename {old_id} {new_id}` to recover.",
    .written.len(),
    .failed.display(),
    .leftover_old_path.display(),
)]
pub struct PartialFailureContext {
    pub old_id: String,
    pub new_id: String,
    pub written: Vec<PathBuf>,
    pub failed: PathBuf,
    pub leftover_old_path: PathBuf,
    pub source: std::io::Error,
}

// ── Public API ───────────────────────────────────────────────────────

/// Rename a work item.
///
/// Three phases:
///
/// 1. **Preflight** — validate, load schema + store, snapshot
///    pre-write diagnostics, enumerate referrers across every
///    `Link`/`Links` schema field. Excludes the renamed item itself —
///    its own file (and any self-links) are handled in the move step.
/// 2. **Compute plan** — for each referrer, parse its frontmatter and
///    substitute `old_id → new_id` in typed link fields. For the
///    renamed item, do the same substitution *and* drop the `id:` key.
///    Run the textual scan over bodies, configs, templates, and any
///    unparseable items.
/// 3. **Execute** — referrer writes first (sorted by path), then the
///    renamed file: write new path, remove old. On any I/O failure
///    returns `PartialFailure` naming the leftover file; the operation
///    is recoverable by re-running.
pub fn run_rename(
    config: &Config,
    project_root: &Path,
    old_id: &WorkItemId,
    new_id: &WorkItemId,
    options: RenameOptions,
) -> Result<RenameOutcome, RenameError> {
    let context = preflight(config, project_root, old_id, new_id)?;
    let plan = compute_plan(&context)?;
    execute_plan(context, plan, options)
}

// ── Phase 1: preflight ───────────────────────────────────────────────

/// Data carried from preflight into compute/execute/finalize.
///
/// Owns what it needs (paths, schema, store snapshot) so downstream
/// phases don't re-borrow the config or re-load the store.
struct RenameContext {
    schema: Schema,
    items_path: PathBuf,
    schema_path: PathBuf,
    views_path: PathBuf,
    resources_path: PathBuf,
    templates_dir: PathBuf,
    config_yaml_path: PathBuf,
    old_id: WorkItemId,
    new_id: WorkItemId,
    old_path: PathBuf,
    new_path: PathBuf,
    /// Other items whose typed link fields contain `old_id`. Renamed
    /// item is intentionally excluded — handled separately.
    referrers: Vec<(WorkItemId, PathBuf)>,
    /// Pre-write `Store::load` + `rules::evaluate` snapshot. Diffed
    /// against the post-write snapshot to drive `mutation_caused_warning`.
    pre_diagnostics: Vec<Diagnostic>,
    store: Store,
}

fn preflight(
    config: &Config,
    project_root: &Path,
    old_id: &WorkItemId,
    new_id: &WorkItemId,
) -> Result<RenameContext, RenameError> {
    if old_id == new_id {
        return Err(RenameError::SameId {
            id: old_id.to_string(),
        });
    }

    if !is_valid_id(new_id.as_str()) {
        return Err(RenameError::InvalidNewId {
            id: new_id.to_string(),
        });
    }

    let schema_path = project_root.join(&config.schema);
    let schema = parser::schema::load_schema(&schema_path)?;

    let items_path = project_root.join(&config.paths.work_items);
    let store = Store::load(&items_path, &schema)?;

    let renamed_item = store
        .get(old_id.as_str())
        .ok_or_else(|| RenameError::UnknownItem {
            id: old_id.to_string(),
        })?;
    let old_path = renamed_item.source_path.clone();

    if store.get(new_id.as_str()).is_some() {
        return Err(RenameError::IdAlreadyExists {
            id: new_id.to_string(),
        });
    }

    let new_path = items_path.join(format!("{new_id}.md"));

    // Catch on-disk shadow files (e.g. an unparseable `.md` at the
    // target path that didn't make it into the store) and any
    // pre-existing parsed item whose filename happens to match
    // `<new_id>.md` even though its id differs.
    //
    // Skip this when new_path == old_path: that's the filename-≠-id
    // case where the user is reconciling an explicit `id:` key with the
    // filename. No collision, just a rewrite of the same file.
    if new_path != old_path && new_path.exists() {
        return Err(RenameError::FileAlreadyExists { path: new_path });
    }

    // Snapshot pre-write diagnostics for the post-write diff.
    let mut pre_diagnostics: Vec<Diagnostic> = store.diagnostics().to_vec();
    pre_diagnostics.extend(crate::rules::evaluate(&store, &schema));

    // Enumerate referrers: every other item that links to `old_id` via
    // any schema field of type `Link` or `Links`. Deduped — an item
    // referencing `old_id` from multiple fields appears once.
    let mut referrer_ids: HashSet<WorkItemId> = HashSet::new();
    for (field_name, field_def) in &schema.fields {
        if !matches!(field_def.field_type(), FieldType::Link | FieldType::Links) {
            continue;
        }
        for item in store.referring_items(old_id.as_str(), field_name) {
            if item.id != *old_id {
                referrer_ids.insert(item.id.clone());
            }
        }
    }

    let mut referrers: Vec<(WorkItemId, PathBuf)> = referrer_ids
        .into_iter()
        .map(|id| {
            let path = store
                .get(id.as_str())
                .expect("referrer id came from the store")
                .source_path
                .clone();
            (id, path)
        })
        .collect();
    referrers.sort_by(|a, b| a.1.cmp(&b.1));

    Ok(RenameContext {
        schema,
        items_path,
        schema_path,
        views_path: project_root.join(&config.paths.views),
        resources_path: project_root.join(&config.paths.resources),
        templates_dir: project_root.join(&config.paths.templates),
        // Convention — `.workdown/config.yaml` is the standard location.
        // A non-default `--config` flag would miss this scan, which is
        // acceptable for a warn-only feature.
        config_yaml_path: project_root.join(".workdown").join("config.yaml"),
        old_id: old_id.clone(),
        new_id: new_id.clone(),
        old_path,
        new_path,
        referrers,
        pre_diagnostics,
        store,
    })
}

// ── Phase 2: compute plan ────────────────────────────────────────────

/// In-memory description of every disk write the rename will perform.
/// Built once, consumed by `execute_plan` (or returned unchanged under
/// `--dry-run`).
struct RenamePlan {
    /// Referrer file writes — sorted by path for determinism. Each
    /// staged write replaces the existing file at its path.
    referrer_writes: Vec<StagedWrite>,
    /// The renamed file's write. Path is `new_path`; the original at
    /// `old_path` is removed after this write succeeds.
    renamed_file_write: StagedWrite,
    /// Warn-only textual mentions of `old_id` outside typed link fields.
    textual_matches: Vec<TextualMatch>,
}

struct StagedWrite {
    path: PathBuf,
    id: WorkItemId,
    new_content: String,
    field_rewrites: Vec<FieldRewrite>,
}

fn compute_plan(context: &RenameContext) -> Result<RenamePlan, RenameError> {
    let mut referrer_writes = Vec::with_capacity(context.referrers.len());
    for (id, path) in &context.referrers {
        referrer_writes.push(plan_file_rewrite(
            id,
            path,
            &context.schema,
            context.old_id.as_str(),
            context.new_id.as_str(),
            /* drop_id_key = */ false,
            /* output_path = */ path.clone(),
        )?);
    }

    let renamed_file_write = plan_file_rewrite(
        &context.old_id,
        &context.old_path,
        &context.schema,
        context.old_id.as_str(),
        context.new_id.as_str(),
        /* drop_id_key = */ true,
        /* output_path = */ context.new_path.clone(),
    )?;

    let textual_matches = scan_textual_matches(context);

    Ok(RenamePlan {
        referrer_writes,
        renamed_file_write,
        textual_matches,
    })
}

/// Read `read_path`, substitute `old_id → new_id` in every link-typed
/// frontmatter field, optionally drop the `id:` key, and return the
/// staged rewrite targeting `output_path`.
///
/// `output_path` lets the renamed file be staged to its new location
/// while every other file rewrites in place.
fn plan_file_rewrite(
    item_id: &WorkItemId,
    read_path: &Path,
    schema: &Schema,
    old_id: &str,
    new_id: &str,
    drop_id_key: bool,
    output_path: PathBuf,
) -> Result<StagedWrite, RenameError> {
    let file_content =
        std::fs::read_to_string(read_path).map_err(|source| RenameError::ReadTarget {
            path: read_path.to_path_buf(),
            source,
        })?;
    let (mut frontmatter, body) =
        parser::split_frontmatter(&file_content, read_path).map_err(|source| {
            RenameError::ParseTarget {
                path: read_path.to_path_buf(),
                source,
            }
        })?;

    let user_set_id_in_source = frontmatter.contains_key("id");
    let field_rewrites = substitute_old_id_in_link_fields(schema, &mut frontmatter, old_id, new_id);

    // Always remove `id:` for the renamed file — the new filename
    // carries it. The `user_set_id=false` flag below would suffice when
    // `id` is a schema field, but `remove` covers the case where it
    // isn't (the frontmatter helper would otherwise emit it as an
    // unknown extra key).
    if drop_id_key {
        frontmatter.remove("id");
    }

    let preserve_id_key = !drop_id_key && user_set_id_in_source;
    let yaml = build_frontmatter_yaml(&frontmatter, schema, preserve_id_key);
    let new_content = format!("---\n{yaml}---\n{body}");

    Ok(StagedWrite {
        path: output_path,
        id: item_id.clone(),
        new_content,
        field_rewrites,
    })
}

/// Walk every schema field of type `Link` or `Links` and substitute
/// `old_id → new_id` in its value. Returns one `FieldRewrite` per
/// field that actually changed.
///
/// Non-link fields are untouched even if they happen to contain
/// `old_id` as a string — only typed link fields are semantic
/// references.
fn substitute_old_id_in_link_fields(
    schema: &Schema,
    frontmatter: &mut HashMap<String, serde_yaml::Value>,
    old_id: &str,
    new_id: &str,
) -> Vec<FieldRewrite> {
    let mut rewrites = Vec::new();

    for (field_name, field_def) in &schema.fields {
        match field_def.field_type() {
            FieldType::Link => {
                let Some(value) = frontmatter.get(field_name) else {
                    continue;
                };
                if value.as_str() != Some(old_id) {
                    continue;
                }
                let previous_value = value.clone();
                let new_value = serde_yaml::Value::String(new_id.to_owned());
                frontmatter.insert(field_name.clone(), new_value.clone());
                rewrites.push(FieldRewrite {
                    field: field_name.clone(),
                    previous_value,
                    new_value,
                });
            }
            FieldType::Links => {
                let Some(value) = frontmatter.get(field_name) else {
                    continue;
                };
                let Some(sequence) = value.as_sequence() else {
                    continue;
                };
                let touches_old = sequence.iter().any(|v| v.as_str() == Some(old_id));
                if !touches_old {
                    continue;
                }
                let previous_value = value.clone();
                let new_sequence: Vec<serde_yaml::Value> = sequence
                    .iter()
                    .map(|element| {
                        if element.as_str() == Some(old_id) {
                            serde_yaml::Value::String(new_id.to_owned())
                        } else {
                            element.clone()
                        }
                    })
                    .collect();
                let new_value = serde_yaml::Value::Sequence(new_sequence);
                frontmatter.insert(field_name.clone(), new_value.clone());
                rewrites.push(FieldRewrite {
                    field: field_name.clone(),
                    previous_value,
                    new_value,
                });
            }
            _ => {}
        }
    }

    rewrites
}

// ── Phase 2b: textual scan (warn-only) ───────────────────────────────

fn scan_textual_matches(context: &RenameContext) -> Vec<TextualMatch> {
    let mut matches = Vec::new();
    let old_id = context.old_id.as_str();

    // 1. Bodies of every parsed item. Includes the renamed item — a
    //    self-mention in its own body is worth surfacing.
    for item in context.store.all_items() {
        scan_text_into(
            &item.body,
            old_id,
            &item.source_path,
            TextualMatchKind::ItemBody,
            &mut matches,
        );
    }

    // 2. Items that failed to parse — we can't tell body from
    //    frontmatter, so scan the whole file. The pre-snapshot
    //    diagnostics carry the paths.
    for diagnostic in context.store.diagnostics() {
        let Some(path) = path_for_parse_diagnostic(diagnostic) else {
            continue;
        };
        if let Ok(content) = std::fs::read_to_string(&path) {
            scan_text_into(
                &content,
                old_id,
                &path,
                TextualMatchKind::UnparseableItem,
                &mut matches,
            );
        }
    }

    // 3. Project YAML configs and templates.
    scan_file_into(
        &context.config_yaml_path,
        old_id,
        TextualMatchKind::Config,
        &mut matches,
    );
    scan_file_into(
        &context.schema_path,
        old_id,
        TextualMatchKind::Schema,
        &mut matches,
    );
    scan_file_into(
        &context.views_path,
        old_id,
        TextualMatchKind::Views,
        &mut matches,
    );
    scan_file_into(
        &context.resources_path,
        old_id,
        TextualMatchKind::Resources,
        &mut matches,
    );

    if let Ok(entries) = std::fs::read_dir(&context.templates_dir) {
        let mut template_paths: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("md"))
            .collect();
        template_paths.sort();
        for path in template_paths {
            scan_file_into(&path, old_id, TextualMatchKind::Template, &mut matches);
        }
    }

    matches
}

/// Read `path` and scan it for `id` mentions. Silent on I/O errors —
/// missing optional files (e.g. a project without a templates dir)
/// shouldn't fail the rename.
fn scan_file_into(path: &Path, id: &str, kind: TextualMatchKind, out: &mut Vec<TextualMatch>) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    scan_text_into(&content, id, path, kind, out);
}

/// One match per (path, line). A line containing the id twice still
/// produces one entry — line granularity is enough for the user to find
/// the mention by eye.
fn scan_text_into(
    text: &str,
    id: &str,
    path: &Path,
    kind: TextualMatchKind,
    out: &mut Vec<TextualMatch>,
) {
    for (zero_based_index, line) in text.lines().enumerate() {
        if line_contains_id(line, id) {
            out.push(TextualMatch {
                path: path.to_path_buf(),
                line: zero_based_index + 1,
                kind,
            });
        }
    }
}

/// Does `line` contain `id` as a standalone token?
///
/// Standalone means: not preceded or followed by another id-valid
/// character (`[a-z0-9-]`). Plain `\b` is wrong here because hyphens
/// are non-word characters under the standard regex word-boundary
/// definition — `\btask-1\b` would match `task-1` inside
/// `task-1-renamed`.
fn line_contains_id(line: &str, id: &str) -> bool {
    let mut search_start = 0;
    while let Some(position) = line[search_start..].find(id) {
        let match_start = search_start + position;
        let match_end = match_start + id.len();

        let preceding_char = line[..match_start].chars().next_back();
        let following_char = line[match_end..].chars().next();

        if is_id_boundary(preceding_char) && is_id_boundary(following_char) {
            return true;
        }

        // Advance past this position even on a near-miss so we don't
        // loop forever on overlapping candidates.
        search_start = match_start + id.chars().next().map_or(1, char::len_utf8);
    }
    false
}

/// `true` if `c` is absent (start/end of line) or not in the set of
/// id-valid characters `[a-z0-9-]`. Anything outside that set is a
/// boundary.
fn is_id_boundary(c: Option<char>) -> bool {
    match c {
        None => true,
        Some(character) => {
            !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-')
        }
    }
}

/// If this diagnostic represents a per-file parse failure, return the
/// path of that file. Used to find unparseable items for the
/// whole-file textual scan.
fn path_for_parse_diagnostic(diagnostic: &Diagnostic) -> Option<PathBuf> {
    use crate::model::diagnostic::{DiagnosticBody, FileDiagnosticKind};
    match &diagnostic.body {
        DiagnosticBody::File(file_diagnostic)
            if matches!(file_diagnostic.kind, FileDiagnosticKind::ReadError { .. }) =>
        {
            Some(file_diagnostic.source_path.clone())
        }
        _ => None,
    }
}

// ── Phase 3: execute + finalize ──────────────────────────────────────

fn execute_plan(
    context: RenameContext,
    plan: RenamePlan,
    options: RenameOptions,
) -> Result<RenameOutcome, RenameError> {
    if options.dry_run {
        let rewritten_files = collect_rewritten_files(&plan);
        return Ok(RenameOutcome {
            old_id: context.old_id,
            new_id: context.new_id,
            old_path: context.old_path,
            new_path: context.new_path,
            rewritten_files,
            textual_matches: plan.textual_matches,
            warnings: Vec::new(),
            mutation_caused_warning: false,
            dry_run: true,
        });
    }

    let mut written: Vec<PathBuf> = Vec::new();

    // Referrer writes first, sorted by path. Sorting is already done in
    // compute_plan (the referrers Vec is sorted in preflight) so we
    // walk it in order.
    for staged in &plan.referrer_writes {
        if let Err(source) = write_file_atomically(&staged.path, &staged.new_content) {
            return Err(partial_failure(
                &context,
                written,
                staged.path.clone(),
                source,
            ));
        }
        written.push(staged.path.clone());
    }

    // Renamed file last. Write the new path first, then remove the old
    // path. When new_path == old_path (filename-≠-id reconciliation),
    // skip the remove — we'd otherwise delete the file we just wrote.
    let renamed_write = &plan.renamed_file_write;
    if let Err(source) = write_file_atomically(&renamed_write.path, &renamed_write.new_content) {
        return Err(partial_failure(
            &context,
            written,
            renamed_write.path.clone(),
            source,
        ));
    }
    written.push(renamed_write.path.clone());

    if context.new_path != context.old_path {
        if let Err(source) = std::fs::remove_file(&context.old_path) {
            return Err(partial_failure(
                &context,
                written,
                context.old_path.clone(),
                source,
            ));
        }
    }

    let rewritten_files = collect_rewritten_files(&plan);

    // Reload and diff. Pre-existing warnings remain visible in
    // `warnings`; `mutation_caused_warning` only flags new ones.
    let reloaded = Store::load(&context.items_path, &context.schema)?;
    let mut post_diagnostics: Vec<Diagnostic> = reloaded.diagnostics().to_vec();
    post_diagnostics.extend(crate::rules::evaluate(&reloaded, &context.schema));

    let mutation_caused_warning = crate::operations::diagnostics::introduced_by_mutation(
        &context.pre_diagnostics,
        &post_diagnostics,
    );

    Ok(RenameOutcome {
        old_id: context.old_id,
        new_id: context.new_id,
        old_path: context.old_path,
        new_path: context.new_path,
        rewritten_files,
        textual_matches: plan.textual_matches,
        warnings: post_diagnostics,
        mutation_caused_warning,
        dry_run: false,
    })
}

fn collect_rewritten_files(plan: &RenamePlan) -> Vec<RewrittenFile> {
    plan.referrer_writes
        .iter()
        .chain(std::iter::once(&plan.renamed_file_write))
        .filter(|staged| !staged.field_rewrites.is_empty() || staged_is_renamed(plan, staged))
        .map(|staged| RewrittenFile {
            path: staged.path.clone(),
            id: staged.id.clone(),
            field_rewrites: staged.field_rewrites.clone(),
        })
        .collect()
}

/// Identify the renamed-file entry. We surface it in `rewritten_files`
/// even when its `field_rewrites` is empty (no self-links) because it
/// did change — the `id:` key was dropped and the file was moved.
fn staged_is_renamed(plan: &RenamePlan, staged: &StagedWrite) -> bool {
    std::ptr::eq(staged, &plan.renamed_file_write)
}

/// Build a `RenameError::PartialFailure` carrying the boxed context.
/// Centralized so the three execute-phase call sites stay terse.
fn partial_failure(
    context: &RenameContext,
    written: Vec<PathBuf>,
    failed: PathBuf,
    source: std::io::Error,
) -> RenameError {
    RenameError::PartialFailure(Box::new(PartialFailureContext {
        old_id: context.old_id.to_string(),
        new_id: context.new_id.to_string(),
        written,
        failed,
        leftover_old_path: context.old_path.clone(),
        source,
    }))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use crate::parser::config::load_config;

    const TEST_SCHEMA: &str = "\
fields:
  title:
    type: string
    required: false
    default: $filename_pretty
  status:
    type: choice
    values: [open, in_progress, done]
    required: true
    default: open
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
  depends_on:
    type: links
    required: false
    allow_cycles: false
    inverse: dependents
  related_to:
    type: links
    required: false
    allow_cycles: true
";

    const TEST_CONFIG: &str = "\
project:
  name: Test Project
  description: ''
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

    fn setup_project() -> (TempDir, PathBuf) {
        let directory = TempDir::new().unwrap();
        let root = directory.path().to_path_buf();
        fs::create_dir_all(root.join(".workdown/templates")).unwrap();
        fs::create_dir_all(root.join("workdown-items")).unwrap();
        fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
        fs::write(root.join(".workdown/schema.yaml"), TEST_SCHEMA).unwrap();
        (directory, root)
    }

    fn load_test_config(root: &Path) -> Config {
        load_config(&root.join(".workdown/config.yaml")).unwrap()
    }

    fn write_item(root: &Path, filename_stem: &str, content: &str) {
        fs::write(
            root.join(format!("workdown-items/{filename_stem}.md")),
            content,
        )
        .unwrap();
    }

    fn read_item(root: &Path, filename_stem: &str) -> String {
        fs::read_to_string(root.join(format!("workdown-items/{filename_stem}.md"))).unwrap()
    }

    fn item_exists(root: &Path, filename_stem: &str) -> bool {
        root.join(format!("workdown-items/{filename_stem}.md"))
            .exists()
    }

    fn run(
        config: &Config,
        root: &Path,
        old_id: &str,
        new_id: &str,
    ) -> Result<RenameOutcome, RenameError> {
        run_rename(
            config,
            root,
            &WorkItemId::from(old_id.to_owned()),
            &WorkItemId::from(new_id.to_owned()),
            RenameOptions::default(),
        )
    }

    fn run_dry(
        config: &Config,
        root: &Path,
        old_id: &str,
        new_id: &str,
    ) -> Result<RenameOutcome, RenameError> {
        run_rename(
            config,
            root,
            &WorkItemId::from(old_id.to_owned()),
            &WorkItemId::from(new_id.to_owned()),
            RenameOptions { dry_run: true },
        )
    }

    /// True iff any line in `text` contains `id` as a standalone token —
    /// reuses the same boundary logic the textual scan uses, so the
    /// "no stale references" assertions in these tests speak the same
    /// language as the production code they're checking.
    fn has_standalone_id(text: &str, id: &str) -> bool {
        text.lines().any(|line| line_contains_id(line, id))
    }

    // ── Validation (no disk changes) ─────────────────────────────────

    #[test]
    fn same_id_errors() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");

        let result = run(&config, &root, "task-1", "task-1");
        assert!(matches!(result, Err(RenameError::SameId { .. })));
        assert!(item_exists(&root, "task-1"));
    }

    #[test]
    fn invalid_new_id_errors() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");

        for bad in ["Foo", "-bad", "bad-", "has space", "snake_case"] {
            let result = run(&config, &root, "task-1", bad);
            assert!(
                matches!(result, Err(RenameError::InvalidNewId { .. })),
                "expected InvalidNewId for '{bad}', got {result:?}"
            );
        }
        assert!(item_exists(&root, "task-1"));
    }

    #[test]
    fn unknown_old_id_errors() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");

        let result = run(&config, &root, "ghost", "ghost-renamed");
        assert!(matches!(result, Err(RenameError::UnknownItem { .. })));
    }

    #[test]
    fn id_already_exists_in_store_errors() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        write_item(&root, "task-2", "---\ntitle: Two\nstatus: open\n---\n");

        let result = run(&config, &root, "task-1", "task-2");
        assert!(matches!(result, Err(RenameError::IdAlreadyExists { .. })));
        assert!(item_exists(&root, "task-1"));
        assert!(item_exists(&root, "task-2"));
    }

    #[test]
    fn file_already_exists_on_disk_shadow_errors() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        // Unparseable shadow file at the target name — won't enter the
        // store, but it occupies the path. Must error distinctly from
        // IdAlreadyExists.
        fs::write(
            root.join("workdown-items/task-1-renamed.md"),
            "not a work item\n",
        )
        .unwrap();

        let result = run(&config, &root, "task-1", "task-1-renamed");
        assert!(
            matches!(result, Err(RenameError::FileAlreadyExists { .. })),
            "got {result:?}"
        );
        assert!(item_exists(&root, "task-1"));
    }

    // ── Happy path ───────────────────────────────────────────────────

    #[test]
    fn rewrites_parent_referrer_and_moves_file() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: One\nstatus: open\n---\noriginal body\n",
        );
        write_item(
            &root,
            "task-2",
            "---\ntitle: Two\nstatus: open\nparent: task-1\n---\nchild body\n",
        );

        let outcome = run(&config, &root, "task-1", "task-1-renamed").unwrap();

        assert!(item_exists(&root, "task-1-renamed"));
        assert!(!item_exists(&root, "task-1"));

        let referrer = read_item(&root, "task-2");
        assert!(referrer.contains("parent: task-1-renamed"));
        assert!(!has_standalone_id(&referrer, "task-1"));
        assert!(referrer.contains("child body"));

        let renamed = read_item(&root, "task-1-renamed");
        assert!(renamed.contains("original body"));

        assert_eq!(outcome.old_id.as_str(), "task-1");
        assert_eq!(outcome.new_id.as_str(), "task-1-renamed");
        assert!(!outcome.mutation_caused_warning);
        assert!(!outcome.dry_run);
        // Referrer rewrite + renamed file move.
        assert_eq!(outcome.rewritten_files.len(), 2);
    }

    #[test]
    fn no_referrers_just_moves_file() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: One\nstatus: open\n---\nthe body\n",
        );

        let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
        assert!(item_exists(&root, "renamed-one"));
        assert!(!item_exists(&root, "task-1"));

        // Only the renamed file appears, with no field rewrites.
        assert_eq!(outcome.rewritten_files.len(), 1);
        assert!(outcome.rewritten_files[0].field_rewrites.is_empty());
    }

    #[test]
    fn rewrites_links_field_with_multiple_targets() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        write_item(&root, "task-x", "---\ntitle: X\nstatus: open\n---\n");
        write_item(
            &root,
            "task-2",
            "---\ntitle: Two\nstatus: open\ndepends_on: [task-1, task-x]\n---\n",
        );

        run(&config, &root, "task-1", "task-1-renamed").unwrap();

        let referrer = read_item(&root, "task-2");
        assert!(referrer.contains("task-1-renamed"));
        assert!(referrer.contains("task-x"));
        // No stray standalone "task-1" (the sibling target survives as
        // "task-x", which is a different token).
        assert!(!has_standalone_id(&referrer, "task-1"));
    }

    #[test]
    fn rewrites_user_defined_link_field() {
        // `related_to` is a Links field outside the default parent/depends_on set.
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        write_item(
            &root,
            "task-2",
            "---\ntitle: Two\nstatus: open\nrelated_to: [task-1]\n---\n",
        );

        run(&config, &root, "task-1", "renamed-one").unwrap();
        let referrer = read_item(&root, "task-2");
        assert!(referrer.contains("renamed-one"));
        assert!(!has_standalone_id(&referrer, "task-1"));
    }

    #[test]
    fn self_link_rewritten_inline_with_move() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        // related_to allows cycles, so self-link is legal.
        write_item(
            &root,
            "task-1",
            "---\ntitle: One\nstatus: open\nrelated_to: [task-1]\n---\nbody\n",
        );

        let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
        assert!(item_exists(&root, "renamed-one"));
        assert!(!item_exists(&root, "task-1"));

        let renamed = read_item(&root, "renamed-one");
        assert!(renamed.contains("renamed-one"));
        assert!(!has_standalone_id(&renamed, "task-1"));

        // Renamed file is in rewritten_files with its self-link rewrite.
        assert_eq!(outcome.rewritten_files.len(), 1);
        assert_eq!(outcome.rewritten_files[0].field_rewrites.len(), 1);
        assert_eq!(
            outcome.rewritten_files[0].field_rewrites[0].field,
            "related_to"
        );
    }

    #[test]
    fn explicit_id_key_dropped_after_rename() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\nid: task-1\ntitle: One\nstatus: open\n---\n",
        );

        run(&config, &root, "task-1", "renamed-one").unwrap();
        let renamed = read_item(&root, "renamed-one");
        assert!(
            !renamed.contains("id:"),
            "expected `id:` key to be removed, got:\n{renamed}"
        );
        assert!(renamed.contains("title: One"));
    }

    #[test]
    fn filename_not_equal_id_renames_to_new_filename() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        // File "whatever.md" carries `id: foo` — filename diverges from id.
        fs::write(
            root.join("workdown-items/whatever.md"),
            "---\nid: foo\ntitle: F\nstatus: open\n---\nbody\n",
        )
        .unwrap();

        run(&config, &root, "foo", "bar").unwrap();

        assert!(item_exists(&root, "bar"));
        assert!(!root.join("workdown-items/whatever.md").exists());
        let renamed = read_item(&root, "bar");
        assert!(!renamed.contains("id:"));
        assert!(renamed.contains("title: F"));
        assert!(renamed.contains("body"));
    }

    // ── Dry run ──────────────────────────────────────────────────────

    #[test]
    fn dry_run_returns_plan_without_writing() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        write_item(
            &root,
            "task-2",
            "---\ntitle: Two\nstatus: open\nparent: task-1\n---\n",
        );

        let outcome = run_dry(&config, &root, "task-1", "task-1-renamed").unwrap();
        assert!(outcome.dry_run);
        assert_eq!(outcome.rewritten_files.len(), 2);

        // Disk untouched.
        assert!(item_exists(&root, "task-1"));
        assert!(!item_exists(&root, "task-1-renamed"));
        let untouched = read_item(&root, "task-2");
        assert!(untouched.contains("parent: task-1"));
        assert!(!untouched.contains("task-1-renamed"));
    }

    // ── Textual scan ─────────────────────────────────────────────────

    #[test]
    fn body_prose_match_reported_but_not_rewritten() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        write_item(
            &root,
            "note-1",
            "---\ntitle: Note\nstatus: open\n---\nsee task-1 for context\n",
        );

        let outcome = run(&config, &root, "task-1", "task-1-renamed").unwrap();

        let body_matches: Vec<_> = outcome
            .textual_matches
            .iter()
            .filter(|m| m.kind == TextualMatchKind::ItemBody)
            .collect();
        assert_eq!(body_matches.len(), 1);

        // Body unchanged.
        let note = read_item(&root, "note-1");
        assert!(note.contains("see task-1 for context"));
    }

    #[test]
    fn body_match_respects_kebab_boundary() {
        // The body contains "task-1-renamed", which has "task-1" as a
        // prefix. The boundary check must reject this — otherwise the
        // user gets a false positive on every successor id.
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        write_item(
            &root,
            "note-1",
            "---\ntitle: Note\nstatus: open\n---\ndo NOT match task-1-renamed text\n",
        );

        let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
        let body_matches: Vec<_> = outcome
            .textual_matches
            .iter()
            .filter(|m| m.kind == TextualMatchKind::ItemBody)
            .collect();
        assert!(
            body_matches.is_empty(),
            "expected no body matches, got {body_matches:?}"
        );
    }

    #[test]
    fn standalone_match_at_various_positions() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        write_item(
            &root,
            "note-1",
            "---\ntitle: Note\nstatus: open\n---\n\
             task-1\n\
             blah task-1\n\
             foo task-1 bar\n\
             not-task-1-no\n",
        );

        let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
        let body_matches: Vec<_> = outcome
            .textual_matches
            .iter()
            .filter(|m| m.kind == TextualMatchKind::ItemBody)
            .collect();
        // First three lines match as standalone tokens. Fourth has the
        // id surrounded by hyphens, so the boundary check rejects it.
        assert_eq!(body_matches.len(), 3);
    }

    #[test]
    fn views_yaml_textual_match_reported() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        fs::write(
            root.join(".workdown/views.yaml"),
            "views:\n  - id: pin\n    note: task-1 is the anchor\n",
        )
        .unwrap();

        let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
        let view_matches: Vec<_> = outcome
            .textual_matches
            .iter()
            .filter(|m| m.kind == TextualMatchKind::Views)
            .collect();
        assert_eq!(view_matches.len(), 1);
    }

    #[test]
    fn unparseable_item_scanned_whole_file() {
        let (_dir, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: One\nstatus: open\n---\n");
        // Missing closing delimiter — parser fails on this file.
        fs::write(
            root.join("workdown-items/broken.md"),
            "---\nparent: task-1\nbroken file no closing\n",
        )
        .unwrap();

        let outcome = run(&config, &root, "task-1", "renamed-one").unwrap();
        let unparseable: Vec<_> = outcome
            .textual_matches
            .iter()
            .filter(|m| m.kind == TextualMatchKind::UnparseableItem)
            .collect();
        assert_eq!(unparseable.len(), 1);
    }

    // ── Boundary helper ──────────────────────────────────────────────

    #[test]
    fn line_contains_id_boundaries() {
        assert!(line_contains_id("task-1", "task-1"));
        assert!(line_contains_id("see task-1 here", "task-1"));
        assert!(line_contains_id("foo: task-1", "task-1"));
        assert!(line_contains_id("task-1 trailing", "task-1"));

        // Hyphen-adjacent: prefix or suffix of a longer id.
        assert!(!line_contains_id("task-1-renamed", "task-1"));
        assert!(!line_contains_id("pre-task-1", "task-1"));
        assert!(!line_contains_id("task-10", "task-1"));
        // Alphanumeric-adjacent.
        assert!(!line_contains_id("task-1a", "task-1"));
    }
}
