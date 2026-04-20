//! Parse a template file's content into a [`Template`].
//!
//! Templates share the frontmatter + body file shape with work items, so
//! parsing reuses [`crate::parser::split_frontmatter`]. Unlike work items,
//! the `id` field is preserved in the map — generator tokens such as
//! `$uuid` get resolved at add-time instead of being rejected here.

use std::path::Path;

use crate::model::template::Template;
use crate::parser::{split_frontmatter, ParseError};

/// Parse template content into a [`Template`].
///
/// `content` is the full file text. `path` is retained on the result for
/// error messages and is used in parser error variants; no I/O happens here.
pub(crate) fn parse_template_content(content: &str, path: &Path) -> Result<Template, ParseError> {
    let (frontmatter, body) = split_frontmatter(content, path)?;
    Ok(Template {
        frontmatter,
        body,
        path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn test_path() -> &'static Path {
        Path::new("bug-report.md")
    }

    #[test]
    fn template_without_id_parses() {
        let content = "\
---
type: bug
priority: medium
---

## Steps

1. ...
";
        let template = parse_template_content(content, test_path()).unwrap();
        assert_eq!(
            template.frontmatter.get("type").unwrap(),
            &serde_yaml::Value::String("bug".into())
        );
        assert!(template.frontmatter.get("id").is_none());
        assert!(template.body.contains("## Steps"));
    }

    #[test]
    fn template_with_literal_id_preserves_it() {
        let content = "---\nid: fixed-id\ntype: bug\n---\nbody\n";
        let template = parse_template_content(content, test_path()).unwrap();
        assert_eq!(
            template.frontmatter.get("id").unwrap(),
            &serde_yaml::Value::String("fixed-id".into())
        );
    }

    #[test]
    fn template_with_uuid_token_preserves_raw_token() {
        let content = "---\nid: $uuid\ntype: bug\n---\nbody\n";
        let template = parse_template_content(content, test_path()).unwrap();
        assert_eq!(
            template.frontmatter.get("id").unwrap(),
            &serde_yaml::Value::String("$uuid".into())
        );
    }

    #[test]
    fn template_missing_opening_delimiter_errors() {
        let content = "type: bug\n---\nbody\n";
        let result = parse_template_content(content, test_path());
        assert!(matches!(result, Err(ParseError::MissingFrontmatter { .. })));
    }

    #[test]
    fn template_body_preserved() {
        let content = "---\ntype: bug\n---\n\n## Steps\n1. reproduce\n";
        let template = parse_template_content(content, test_path()).unwrap();
        assert!(template.body.contains("## Steps"));
        assert!(template.body.contains("1. reproduce"));
    }
}
