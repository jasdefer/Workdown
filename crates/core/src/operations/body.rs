//! `workdown body` — replace the freeform Markdown body of a work item.
//!
//! Body content is freeform by design — no schema validation applies, so
//! this command's only job is to splice a new body onto the existing
//! frontmatter bytes. The frontmatter is left byte-identical: we slice
//! the file at the body offset rather than re-emitting the YAML.
//!
//! Interactive editing is out of scope — the user opens the `.md` file
//! directly. This command exists for non-interactive callers (UI,
//! scripts, the future server).

use std::path::{Path, PathBuf};

use crate::model::config::Config;
use crate::model::diagnostic::Diagnostic;
use crate::model::WorkItemId;
use crate::operations::frontmatter_io::write_file_atomically;
use crate::parser;
use crate::parser::schema::SchemaLoadError;

// ── Public types ─────────────────────────────────────────────────────

/// The outcome of a successful `workdown body`.
#[derive(Debug)]
pub struct BodyOutcome {
    /// Path to the file that was written.
    pub path: PathBuf,
    /// The body string before the write — exactly as it appeared on disk.
    pub previous_body: String,
    /// The body string after the write — normalised (trailing whitespace-only
    /// lines collapsed to a single `\n`, or empty if the body is empty).
    pub new_body: String,
    /// All non-blocking diagnostics from the post-write store reload plus
    /// rule evaluation. Body content cannot itself produce warnings, but
    /// pre-existing warnings elsewhere in the project are surfaced here per
    /// the milestone's "always show all" convention.
    pub warnings: Vec<Diagnostic>,
}

/// Errors returned by [`run_body_replace`].
///
/// All variants are hard-fails: nothing is written to disk.
#[derive(Debug, thiserror::Error)]
pub enum BodyError {
    #[error("failed to load schema: {0}")]
    SchemaLoad(#[from] SchemaLoadError),

    #[error("failed to load work items: {0}")]
    StoreLoad(#[from] std::io::Error),

    #[error("unknown work item '{id}'")]
    UnknownItem { id: String },

    #[error("failed to read '{path}': {source}")]
    ReadTarget {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to parse '{path}': {source}")]
    ParseTarget {
        path: PathBuf,
        source: parser::ParseError,
    },

    #[error("failed to write '{path}': {source}")]
    WriteFile {
        path: PathBuf,
        source: std::io::Error,
    },
}

// ── Public API ───────────────────────────────────────────────────────

/// Replace the freeform Markdown body of `id`.
///
/// The frontmatter bytes are preserved verbatim — we slice the on-disk
/// file at the body offset and write `frontmatter_bytes + normalised_body`.
/// This is the only mutation in the CLI that does not round-trip the
/// frontmatter through serde_yaml.
///
/// Trailing newline rule: exactly one `\n` for a non-empty body, none for
/// an empty body. Trailing `\r` is stripped alongside `\n` so CRLF input
/// doesn't leave a dangling carriage return.
pub fn run_body_replace(
    config: &Config,
    project_root: &Path,
    id: &WorkItemId,
    new_body: String,
) -> Result<BodyOutcome, BodyError> {
    let schema_path = project_root.join(&config.schema);
    let schema = parser::schema::load_schema(&schema_path)?;

    let items_path = project_root.join(&config.paths.work_items);
    let store = crate::store::Store::load(&items_path, &schema)?;

    let work_item = store
        .get(id.as_str())
        .ok_or_else(|| BodyError::UnknownItem { id: id.to_string() })?;
    let file_path = work_item.source_path.clone();

    let file_content =
        std::fs::read_to_string(&file_path).map_err(|source| BodyError::ReadTarget {
            path: file_path.clone(),
            source,
        })?;

    let (_frontmatter, body_offset) =
        parser::split_frontmatter_with_body_offset(&file_content, &file_path).map_err(
            |source| BodyError::ParseTarget {
                path: file_path.clone(),
                source,
            },
        )?;

    let previous_body = file_content.get(body_offset..).unwrap_or("").to_owned();
    let normalised_new_body = normalise_body(&new_body);

    let frontmatter_bytes = file_content.get(..body_offset).unwrap_or("");
    let new_file_content = format!("{frontmatter_bytes}{normalised_new_body}");

    write_file_atomically(&file_path, &new_file_content).map_err(|source| {
        BodyError::WriteFile {
            path: file_path.clone(),
            source,
        }
    })?;

    // Reload and surface every diagnostic. Body content cannot itself
    // introduce a new warning (rules and schema only look at frontmatter),
    // so there is no pre/post diff — the post-write snapshot is what the
    // user sees.
    let reloaded = crate::store::Store::load(&items_path, &schema)?;
    let mut post_diagnostics: Vec<Diagnostic> = reloaded.diagnostics().to_vec();
    post_diagnostics.extend(crate::rules::evaluate(&reloaded, &schema));

    Ok(BodyOutcome {
        path: file_path,
        previous_body,
        new_body: normalised_new_body,
        warnings: post_diagnostics,
    })
}

// ── Internals ────────────────────────────────────────────────────────

/// Normalise the body's trailing whitespace into the canonical form:
/// exactly one `\n` for a non-empty body, none for an empty one.
///
/// Strips both `\n` and `\r` from the tail so CRLF input from Windows
/// editors doesn't leave a dangling carriage return.
fn normalise_body(body: &str) -> String {
    let trimmed = body.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

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

    fn write_item(root: &Path, id: &str, content: &str) {
        fs::write(root.join(format!("workdown-items/{id}.md")), content).unwrap();
    }

    fn read_item(root: &Path, id: &str) -> String {
        fs::read_to_string(root.join(format!("workdown-items/{id}.md"))).unwrap()
    }

    // ── Happy path ───────────────────────────────────────────────────

    #[test]
    fn replaces_body_text() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
        );

        let outcome = run_body_replace(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "New body content.".to_owned(),
        )
        .unwrap();

        assert_eq!(outcome.previous_body, "Old body.\n");
        assert_eq!(outcome.new_body, "New body content.\n");
        assert_eq!(
            read_item(&root, "task-1"),
            "---\ntitle: Task one\nstatus: open\n---\nNew body content.\n"
        );
    }

    #[test]
    fn empty_body_clears_body() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
        );

        let outcome = run_body_replace(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            String::new(),
        )
        .unwrap();

        assert_eq!(outcome.new_body, "");
        // File ends right after the closing `---\n` — no extra trailing newline.
        assert_eq!(
            read_item(&root, "task-1"),
            "---\ntitle: Task one\nstatus: open\n---\n"
        );
    }

    #[test]
    fn trims_multiple_trailing_newlines_to_one() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
        );

        let outcome = run_body_replace(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "hello\n\n\n".to_owned(),
        )
        .unwrap();

        assert_eq!(outcome.new_body, "hello\n");
    }

    #[test]
    fn adds_trailing_newline_when_missing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
        );

        let outcome = run_body_replace(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "hello".to_owned(),
        )
        .unwrap();

        assert_eq!(outcome.new_body, "hello\n");
    }

    #[test]
    fn strips_crlf_trailing_whitespace() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
        );

        let outcome = run_body_replace(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "hello\r\n\r\n".to_owned(),
        )
        .unwrap();

        assert_eq!(outcome.new_body, "hello\n");
    }

    #[test]
    fn whitespace_only_body_clears_body() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
        );

        let outcome = run_body_replace(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "\n\n\r\n".to_owned(),
        )
        .unwrap();

        assert_eq!(outcome.new_body, "");
        assert_eq!(
            read_item(&root, "task-1"),
            "---\ntitle: Task one\nstatus: open\n---\n"
        );
    }

    #[test]
    fn preserves_multiline_body() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(&root, "task-1", "---\ntitle: Task one\nstatus: open\n---\n");

        let new_body = "Line one.\n\nLine three after a blank line.\n\n## Heading\n\nMore text.";

        let outcome = run_body_replace(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            new_body.to_owned(),
        )
        .unwrap();

        assert_eq!(
            outcome.new_body,
            "Line one.\n\nLine three after a blank line.\n\n## Heading\n\nMore text.\n"
        );
        let on_disk = read_item(&root, "task-1");
        assert!(on_disk.contains("\n\nLine three after a blank line.\n\n"));
    }

    // ── Frontmatter preservation ─────────────────────────────────────

    #[test]
    fn frontmatter_bytes_are_preserved_verbatim() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        // Hand-edited frontmatter with quirky-but-valid formatting:
        // out-of-schema-order fields, an inline comment, extra blank
        // lines inside the frontmatter, and an explicit `id:` field.
        let original =
            "---\nstatus: open\nid: task-1\ntitle: Task one  # important\n\n\n---\nBody.\n";
        write_item(&root, "task-1", original);

        run_body_replace(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "Replaced.".to_owned(),
        )
        .unwrap();

        let after = read_item(&root, "task-1");
        // The bytes from start through the closing `---\n` must be
        // byte-identical to the original — only the body changes.
        let original_through_close =
            "---\nstatus: open\nid: task-1\ntitle: Task one  # important\n\n\n---\n";
        assert!(
            after.starts_with(original_through_close),
            "frontmatter bytes diverged.\nexpected prefix: {original_through_close:?}\nafter:           {after:?}"
        );
        assert_eq!(after, format!("{original_through_close}Replaced.\n"));
    }

    // ── Errors ───────────────────────────────────────────────────────

    #[test]
    fn unknown_id_errors_and_writes_nothing() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
        );
        let before = read_item(&root, "task-1");

        let result = run_body_replace(
            &config,
            &root,
            &WorkItemId::from("does-not-exist".to_owned()),
            "Whatever.".to_owned(),
        );

        assert!(matches!(result, Err(BodyError::UnknownItem { .. })));
        // The existing item is untouched.
        assert_eq!(read_item(&root, "task-1"), before);
    }

    // ── Diagnostics ──────────────────────────────────────────────────

    #[test]
    fn surfaces_preexisting_warnings_from_other_items() {
        let (_directory, root) = setup_project();
        let config = load_test_config(&root);
        write_item(
            &root,
            "task-1",
            "---\ntitle: Task one\nstatus: open\n---\nOld body.\n",
        );
        // Second item with a broken `parent` link — a pre-existing warning
        // that has nothing to do with our mutation.
        write_item(
            &root,
            "task-2",
            "---\ntitle: Task two\nstatus: open\nparent: ghost-item\n---\n",
        );

        let outcome = run_body_replace(
            &config,
            &root,
            &WorkItemId::from("task-1".to_owned()),
            "Replaced.".to_owned(),
        )
        .unwrap();

        assert!(
            !outcome.warnings.is_empty(),
            "expected the pre-existing broken-link warning on task-2 to surface"
        );
    }
}
