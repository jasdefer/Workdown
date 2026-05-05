//! Parsers for work item files (frontmatter + Markdown), schema, and config.

pub mod config;
pub mod schema;
pub mod template;
pub mod views;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::work_item::is_valid_id;
use crate::model::WorkItemId;

/// A work item as parsed from a Markdown file, before type coercion.
/// Has a resolved ID but raw YAML field values. Internal intermediate — the
/// store converts this into a [`WorkItem`] with typed fields.
#[derive(Debug)]
pub(crate) struct RawWorkItem {
    /// Resolved ID: from frontmatter `id` field if present, otherwise filename without `.md`.
    pub id: WorkItemId,
    /// Field names to their raw YAML values, as written in the frontmatter.
    /// The `id` field (if present in frontmatter) is excluded — use `id` above.
    pub frontmatter: HashMap<String, serde_yaml::Value>,
    /// Everything below the closing `---` delimiter — freeform Markdown.
    pub body: String,
    /// The file this was parsed from, kept for error messages downstream.
    pub source_path: PathBuf,
}

/// Errors that can occur when parsing a work item file.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("{path}: could not read file: {source}")]
    ReadFailed {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("{path}: missing opening frontmatter delimiter (expected `---` on the first line)")]
    MissingFrontmatter { path: PathBuf },

    #[error("{path}: missing closing frontmatter delimiter (expected `---`)")]
    UnclosedFrontmatter { path: PathBuf },

    #[error("{path}: invalid YAML in frontmatter: {source}")]
    InvalidYaml {
        path: PathBuf,
        source: serde_yaml::Error,
    },

    #[error("{path}: frontmatter must be a YAML mapping, not a scalar or list")]
    FrontmatterNotMapping { path: PathBuf },

    #[error("{path}: frontmatter `id` field must be a string")]
    IdNotString { path: PathBuf },

    #[error("{path}: invalid ID '{id}': must be non-empty, lowercase alphanumeric with hyphens, starting with a letter or digit")]
    InvalidId { path: PathBuf, id: String },
}

/// Read a work item file from disk and parse it.
/// Convenience wrapper around [`parse_work_item`] that handles file I/O.
pub(crate) fn parse_work_item_file(path: &Path) -> Result<RawWorkItem, ParseError> {
    let content = std::fs::read_to_string(path).map_err(|source| ParseError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;

    parse_work_item(&content, path)
}

/// Split a file's content into a frontmatter map and body.
///
/// Handles the `---` delimiters, YAML parsing, and body extraction — but
/// does not touch the `id` field. The caller decides whether `id` is
/// special (for work items) or just another field (for templates).
///
/// `content` is the full text. `path` is used for error messages only;
/// no I/O happens here.
pub(crate) fn split_frontmatter(
    content: &str,
    path: &Path,
) -> Result<(HashMap<String, serde_yaml::Value>, String), ParseError> {
    let mut lines = content.lines();

    // First line must be `---`
    match lines.next() {
        Some(line) if line.trim() == "---" => {}
        _ => {
            return Err(ParseError::MissingFrontmatter {
                path: path.to_path_buf(),
            })
        }
    }

    // Collect lines until the closing `---`
    let mut yaml_lines = Vec::new();
    let mut found_closing = false;
    let mut bytes_consumed = 0;

    for line in &mut lines {
        // Track how many bytes we've consumed (line + its newline).
        // `lines()` strips the newline, so we account for it.
        bytes_consumed += line.len() + 1;

        if line.trim() == "---" {
            found_closing = true;
            break;
        }
        yaml_lines.push(line);
    }

    if !found_closing {
        return Err(ParseError::UnclosedFrontmatter {
            path: path.to_path_buf(),
        });
    }

    // Parse the YAML
    let yaml_text = yaml_lines.join("\n");
    let value: serde_yaml::Value =
        serde_yaml::from_str(&yaml_text).map_err(|source| ParseError::InvalidYaml {
            path: path.to_path_buf(),
            source,
        })?;

    // Must be a mapping (key-value pairs), not a scalar or list
    let frontmatter: HashMap<String, serde_yaml::Value> = match value {
        serde_yaml::Value::Mapping(mapping) => mapping
            .into_iter()
            .filter_map(|(key, value)| key.as_str().map(|key| (key.to_owned(), value)))
            .collect(),
        serde_yaml::Value::Null => {
            // Empty frontmatter (just `---\n---`) is valid — no fields set.
            HashMap::new()
        }
        _ => {
            return Err(ParseError::FrontmatterNotMapping {
                path: path.to_path_buf(),
            })
        }
    };

    // The opening `---\n` plus everything we consumed gives us the body offset.
    let opening_delimiter_len = content.lines().next().unwrap().len() + 1;
    let body_offset = opening_delimiter_len + bytes_consumed;
    let body = content.get(body_offset..).unwrap_or("").to_owned();

    Ok((frontmatter, body))
}

/// Parse work item content into a [`RawWorkItem`] with a resolved ID.
///
/// Thin wrapper over [`split_frontmatter`] that resolves the work item ID:
/// - If the frontmatter contains an `id` field, that value is used (must be a string).
/// - Otherwise, the filename without `.md` is used.
///
/// The resolved ID is validated for format (kebab-case).
pub(crate) fn parse_work_item(content: &str, path: &Path) -> Result<RawWorkItem, ParseError> {
    let (mut frontmatter, body) = split_frontmatter(content, path)?;

    // Resolve ID: frontmatter `id` field takes precedence over filename.
    let id_str = if let Some(id_value) = frontmatter.remove("id") {
        id_value
            .as_str()
            .ok_or_else(|| ParseError::IdNotString {
                path: path.to_path_buf(),
            })?
            .to_owned()
    } else {
        // Derive from filename: strip `.md` extension.
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_owned()
    };

    // Validate ID format: non-empty, lowercase alphanumeric + hyphens, starts with a letter or digit.
    if !is_valid_id(&id_str) {
        return Err(ParseError::InvalidId {
            path: path.to_path_buf(),
            id: id_str,
        });
    }

    let id = WorkItemId::from(id_str);

    Ok(RawWorkItem {
        id,
        frontmatter,
        body,
        source_path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_path() -> &'static Path {
        Path::new("test-item.md")
    }

    // ── split_frontmatter ────────────────────────────────────────────

    #[test]
    fn split_retains_id_in_map() {
        let content = "---\nid: custom-id\ntitle: Test\n---\nbody\n";
        let (frontmatter, body) = split_frontmatter(content, test_path()).unwrap();
        assert_eq!(
            frontmatter.get("id").unwrap(),
            &serde_yaml::Value::String("custom-id".into())
        );
        assert_eq!(
            frontmatter.get("title").unwrap(),
            &serde_yaml::Value::String("Test".into())
        );
        assert!(body.contains("body"));
    }

    #[test]
    fn split_empty_frontmatter_returns_empty_map() {
        let content = "---\n---\nSome body.\n";
        let (frontmatter, body) = split_frontmatter(content, test_path()).unwrap();
        assert!(frontmatter.is_empty());
        assert!(body.contains("Some body."));
    }

    #[test]
    fn split_missing_opening_errors() {
        let content = "title: x\n---\nbody\n";
        let result = split_frontmatter(content, test_path());
        assert!(matches!(result, Err(ParseError::MissingFrontmatter { .. })));
    }

    #[test]
    fn split_missing_closing_errors() {
        let content = "---\ntitle: x\n";
        let result = split_frontmatter(content, test_path());
        assert!(matches!(
            result,
            Err(ParseError::UnclosedFrontmatter { .. })
        ));
    }

    #[test]
    fn split_list_frontmatter_errors() {
        let content = "---\n- one\n- two\n---\nbody\n";
        let result = split_frontmatter(content, test_path());
        assert!(matches!(
            result,
            Err(ParseError::FrontmatterNotMapping { .. })
        ));
    }

    #[test]
    fn split_invalid_yaml_errors() {
        let content = "---\n: :\n  bad:\n    - [\n---\nbody\n";
        let result = split_frontmatter(content, test_path());
        assert!(matches!(result, Err(ParseError::InvalidYaml { .. })));
    }

    #[test]
    fn split_preserves_body_with_markdown() {
        let content = "---\ntitle: X\n---\n\n# Heading\n\n- a\n- b\n";
        let (_, body) = split_frontmatter(content, test_path()).unwrap();
        assert!(body.contains("# Heading"));
        assert!(body.contains("- a"));
    }

    // ── Frontmatter parsing ──────────────────────────────────────────

    #[test]
    fn parse_typical_work_item() {
        let content = "\
---
title: Implement login
status: open
priority: high
tags: [auth, backend]
---

## Description

This is the body.
";
        let item = parse_work_item(content, test_path()).unwrap();

        assert_eq!(item.id, "test-item");
        assert_eq!(
            item.frontmatter.get("title").unwrap(),
            &serde_yaml::Value::String("Implement login".into())
        );
        assert_eq!(
            item.frontmatter.get("status").unwrap(),
            &serde_yaml::Value::String("open".into())
        );
        assert_eq!(
            item.frontmatter.get("priority").unwrap(),
            &serde_yaml::Value::String("high".into())
        );

        // tags is a sequence
        let tags = item.frontmatter.get("tags").unwrap();
        assert!(tags.is_sequence());

        assert!(item.body.contains("## Description"));
        assert!(item.body.contains("This is the body."));
    }

    #[test]
    fn parse_empty_frontmatter() {
        let content = "\
---
---
Some body text.
";
        let item = parse_work_item(content, test_path()).unwrap();

        assert_eq!(item.id, "test-item");
        assert!(item.frontmatter.is_empty());
        assert!(item.body.contains("Some body text."));
    }

    #[test]
    fn parse_empty_body() {
        let content = "\
---
title: No body
---
";
        let item = parse_work_item(content, test_path()).unwrap();

        assert_eq!(item.frontmatter.len(), 1);
        assert!(item.body.trim().is_empty());
    }

    #[test]
    fn missing_opening_delimiter() {
        let content = "title: oops\n---\nbody\n";
        let result = parse_work_item(content, test_path());

        assert!(matches!(result, Err(ParseError::MissingFrontmatter { .. })));
    }

    #[test]
    fn missing_closing_delimiter() {
        let content = "---\ntitle: oops\n";
        let result = parse_work_item(content, test_path());

        assert!(matches!(
            result,
            Err(ParseError::UnclosedFrontmatter { .. })
        ));
    }

    #[test]
    fn frontmatter_is_a_list() {
        let content = "---\n- one\n- two\n---\nbody\n";
        let result = parse_work_item(content, test_path());

        assert!(matches!(
            result,
            Err(ParseError::FrontmatterNotMapping { .. })
        ));
    }

    #[test]
    fn invalid_yaml() {
        let content = "---\n: :\n  bad:\n    - [\n---\nbody\n";
        let result = parse_work_item(content, test_path());

        assert!(matches!(result, Err(ParseError::InvalidYaml { .. })));
    }

    #[test]
    fn empty_file() {
        let content = "";
        let result = parse_work_item(content, test_path());

        assert!(matches!(result, Err(ParseError::MissingFrontmatter { .. })));
    }

    #[test]
    fn body_preserves_markdown_structure() {
        let content = "\
---
title: Rich body
---

# Heading

- bullet 1
- bullet 2

```rust
fn main() {}
```
";
        let item = parse_work_item(content, test_path()).unwrap();

        assert!(item.body.contains("# Heading"));
        assert!(item.body.contains("- bullet 1"));
        assert!(item.body.contains("fn main() {}"));
    }

    #[test]
    fn frontmatter_with_all_yaml_types() {
        let content = "\
---
name: test
count: 42
ratio: 3.14
active: true
tags: [a, b, c]
---
";
        let item = parse_work_item(content, test_path()).unwrap();

        assert!(item.frontmatter.get("name").unwrap().is_string());
        assert!(item.frontmatter.get("count").unwrap().is_number());
        assert!(item.frontmatter.get("ratio").unwrap().is_number());
        assert!(item.frontmatter.get("active").unwrap().is_bool());
        assert!(item.frontmatter.get("tags").unwrap().is_sequence());
    }

    // ── ID resolution ────────────────────────────────────────────────

    #[test]
    fn id_from_filename() {
        let content = "---\ntitle: Test\n---\n";
        let item = parse_work_item(content, Path::new("fix-login.md")).unwrap();
        assert_eq!(item.id, "fix-login");
    }

    #[test]
    fn id_from_frontmatter_override() {
        let content = "---\nid: custom-id\ntitle: Test\n---\n";
        let item = parse_work_item(content, Path::new("fix-login.md")).unwrap();
        assert_eq!(item.id, "custom-id");
        // `id` should be stripped from frontmatter
        assert!(item.frontmatter.get("id").is_none());
    }

    #[test]
    fn id_not_string_rejected() {
        let content = "---\nid: 42\ntitle: Test\n---\n";
        let result = parse_work_item(content, Path::new("fix-login.md"));
        assert!(matches!(result, Err(ParseError::IdNotString { .. })));
    }

    #[test]
    fn id_invalid_format_uppercase() {
        let content = "---\nid: Fix-Login\ntitle: Test\n---\n";
        let result = parse_work_item(content, Path::new("whatever.md"));
        assert!(matches!(result, Err(ParseError::InvalidId { .. })));
    }

    #[test]
    fn id_starts_with_digit_accepted() {
        let content = "---\ntitle: Test\n---\n";
        let item = parse_work_item(content, Path::new("123-task.md")).unwrap();
        assert_eq!(item.id, "123-task");
    }

    #[test]
    fn id_invalid_format_trailing_hyphen() {
        let content = "---\ntitle: Test\n---\n";
        let result = parse_work_item(content, Path::new("fix-login-.md"));
        assert!(matches!(result, Err(ParseError::InvalidId { .. })));
    }

    #[test]
    fn id_invalid_format_underscores() {
        let content = "---\ntitle: Test\n---\n";
        let result = parse_work_item(content, Path::new("fix_login.md"));
        assert!(matches!(result, Err(ParseError::InvalidId { .. })));
    }

    #[test]
    fn id_with_digits() {
        let content = "---\ntitle: Test\n---\n";
        let item = parse_work_item(content, Path::new("task-42.md")).unwrap();
        assert_eq!(item.id, "task-42");
    }

    #[test]
    fn id_single_letter() {
        let content = "---\ntitle: Test\n---\n";
        let item = parse_work_item(content, Path::new("x.md")).unwrap();
        assert_eq!(item.id, "x");
    }
}
