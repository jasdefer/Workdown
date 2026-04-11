//! Parsers for work item files (frontmatter + Markdown) and schema definitions.

pub mod schema;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::model::RawWorkItem;

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
}

/// Read a work item file from disk and parse it.
/// Convenience wrapper around [`parse_frontmatter`] that handles file I/O.
pub fn parse_work_item_file(path: &Path) -> Result<RawWorkItem, ParseError> {
    let content = std::fs::read_to_string(path).map_err(|source| ParseError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;

    parse_frontmatter(&content, path)
}

/// Parse work item content into its frontmatter and body.
///
/// `content` is the full text of the Markdown file. `path` is carried
/// through for error messages only — no I/O happens here.
///
/// The content must start with `---` on the first line, followed by YAML,
/// followed by a closing `---` on its own line. Everything after the
/// closing delimiter is returned as the body.
pub fn parse_frontmatter(
    content: &str,
    path: &Path,
) -> Result<RawWorkItem, ParseError> {
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
    let frontmatter = match value {
        serde_yaml::Value::Mapping(mapping) => mapping
            .into_iter()
            .filter_map(|(key, value)| {
                // YAML mapping keys can technically be anything, but we only
                // support string keys (field names).
                key.as_str().map(|key| (key.to_owned(), value))
            })
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

    Ok(RawWorkItem {
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
        let item = parse_frontmatter(content, test_path()).unwrap();

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
        let item = parse_frontmatter(content, test_path()).unwrap();

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
        let item = parse_frontmatter(content, test_path()).unwrap();

        assert_eq!(item.frontmatter.len(), 1);
        assert!(item.body.trim().is_empty());
    }

    #[test]
    fn missing_opening_delimiter() {
        let content = "title: oops\n---\nbody\n";
        let result = parse_frontmatter(content, test_path());

        assert!(matches!(result, Err(ParseError::MissingFrontmatter { .. })));
    }

    #[test]
    fn missing_closing_delimiter() {
        let content = "---\ntitle: oops\n";
        let result = parse_frontmatter(content, test_path());

        assert!(matches!(
            result,
            Err(ParseError::UnclosedFrontmatter { .. })
        ));
    }

    #[test]
    fn frontmatter_is_a_list() {
        let content = "---\n- one\n- two\n---\nbody\n";
        let result = parse_frontmatter(content, test_path());

        assert!(matches!(
            result,
            Err(ParseError::FrontmatterNotMapping { .. })
        ));
    }

    #[test]
    fn invalid_yaml() {
        let content = "---\n: :\n  bad:\n    - [\n---\nbody\n";
        let result = parse_frontmatter(content, test_path());

        assert!(matches!(result, Err(ParseError::InvalidYaml { .. })));
    }

    #[test]
    fn empty_file() {
        let content = "";
        let result = parse_frontmatter(content, test_path());

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
        let item = parse_frontmatter(content, test_path()).unwrap();

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
        let item = parse_frontmatter(content, test_path()).unwrap();

        assert!(item.frontmatter.get("name").unwrap().is_string());
        assert!(item.frontmatter.get("count").unwrap().is_number());
        assert!(item.frontmatter.get("ratio").unwrap().is_number());
        assert!(item.frontmatter.get("active").unwrap().is_bool());
        assert!(item.frontmatter.get("tags").unwrap().is_sequence());
    }
}

