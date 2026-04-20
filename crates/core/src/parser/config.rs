//! Config loader: parse `config.yaml` into a [`Config`].
//!
//! Follows the same pattern as [`super::schema`]: read from disk,
//! deserialize, and fall back to built-in defaults when the file is missing.

use std::path::Path;

use crate::model::config::Config;

// ── Public API ────────────────────────────────────────────────────────

/// Load a config from a file on disk.
pub fn load_config(path: &Path) -> Result<Config, ConfigLoadError> {
    let content = std::fs::read_to_string(path).map_err(ConfigLoadError::ReadFailed)?;
    parse_config(&content)
}

/// Load a config from a file, falling back to built-in defaults if the
/// file does not exist. Other I/O errors are propagated.
pub fn load_config_or_default(path: &Path) -> Result<Config, ConfigLoadError> {
    match std::fs::read_to_string(path) {
        Ok(content) => parse_config(&content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            parse_config(include_str!("../../defaults/config.yaml"))
        }
        Err(e) => Err(ConfigLoadError::ReadFailed(e)),
    }
}

/// Parse a config from a YAML string.
pub fn parse_config(yaml: &str) -> Result<Config, ConfigLoadError> {
    serde_yaml::from_str(yaml).map_err(ConfigLoadError::InvalidYaml)
}

// ── Errors ────────────────────────────────────────────────────────────

/// Errors from loading a config file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigLoadError {
    #[error("failed to read config file: {0}")]
    ReadFailed(std::io::Error),

    #[error("invalid YAML in config: {0}")]
    InvalidYaml(serde_yaml::Error),
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_config() {
        let config = parse_config(include_str!("../../defaults/config.yaml")).unwrap();
        assert_eq!(config.project.name, "My Project");
        assert_eq!(
            config.paths.work_items,
            std::path::PathBuf::from("workdown-items")
        );
        assert_eq!(
            config.schema,
            std::path::PathBuf::from(".workdown/schema.yaml")
        );
        assert_eq!(config.defaults.board_field, "status");
    }

    #[test]
    fn parse_minimal_config() {
        let yaml = r#"
project:
  name: Test
paths:
  work_items: items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
"#;
        let config = parse_config(yaml).unwrap();
        assert_eq!(config.project.name, "Test");
        assert!(config.project.description.is_empty());
    }

    #[test]
    fn parse_rejects_unknown_fields() {
        let yaml = r#"
project:
  name: Test
  bogus: oops
paths:
  work_items: items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
"#;
        assert!(parse_config(yaml).is_err());
    }

    #[test]
    fn parse_rejects_missing_required_sections() {
        let yaml = "project:\n  name: Test\n";
        assert!(parse_config(yaml).is_err());
    }
}
