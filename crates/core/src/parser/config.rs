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
    use crate::model::weekday::Weekday;

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
        // working_days commented out → calendar falls back to Mon–Fri.
        assert!(config.working_days.is_none());
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
  views: .workdown/views.yaml
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
  views: .workdown/views.yaml
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

    #[test]
    fn parse_working_days_explicit() {
        let yaml = r#"
project:
  name: Test
paths:
  work_items: items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
working_days: [monday, tuesday, friday]
"#;
        let config = parse_config(yaml).unwrap();
        assert_eq!(
            config.working_days,
            Some(vec![Weekday::Monday, Weekday::Tuesday, Weekday::Friday])
        );
    }

    #[test]
    fn working_calendar_falls_back_to_business_week_when_unset() {
        let yaml = r#"
project:
  name: Test
paths:
  work_items: items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
"#;
        let config = parse_config(yaml).unwrap();
        let calendar = config.working_calendar();
        // Mon Jan 5 2026 is a working day; Sat Jan 10 is not.
        let monday = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        let saturday = chrono::NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        assert!(calendar.is_working(monday));
        assert!(!calendar.is_working(saturday));
    }

    #[test]
    fn working_calendar_uses_explicit_days() {
        let yaml = r#"
project:
  name: Test
paths:
  work_items: items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
working_days: [saturday, sunday]
"#;
        let config = parse_config(yaml).unwrap();
        let calendar = config.working_calendar();
        let saturday = chrono::NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        let sunday = chrono::NaiveDate::from_ymd_opt(2026, 1, 11).unwrap();
        let monday = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
        assert!(calendar.is_working(saturday));
        assert!(calendar.is_working(sunday));
        assert!(!calendar.is_working(monday));
    }

    #[test]
    fn parse_rejects_abbreviated_working_day() {
        // Memory rule: full day names only.
        let yaml = r#"
project:
  name: Test
paths:
  work_items: items
  templates: .workdown/templates
  resources: .workdown/resources.yaml
  views: .workdown/views.yaml
schema: .workdown/schema.yaml
defaults:
  board_field: status
  tree_field: parent
  graph_field: depends_on
working_days: [mon, tue]
"#;
        assert!(parse_config(yaml).is_err());
    }
}
