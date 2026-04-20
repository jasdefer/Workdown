use std::fs;

use tempfile::TempDir;
use workdown_core::operations::init::{run_init, InitOutcome};

#[test]
fn init_creates_project_structure() {
    let tmp = TempDir::new().unwrap();
    let result = run_init(tmp.path(), Some("Test App")).unwrap();
    assert_eq!(result, InitOutcome::Created);

    // Directories exist.
    assert!(tmp.path().join(".workdown").is_dir());
    assert!(tmp.path().join(".workdown/templates").is_dir());
    assert!(tmp.path().join("workdown-items").is_dir());

    // Files exist and are non-empty.
    let config = fs::read_to_string(tmp.path().join(".workdown/config.yaml")).unwrap();
    assert!(config.contains("name: Test App"));

    let schema = fs::read_to_string(tmp.path().join(".workdown/schema.yaml")).unwrap();
    assert!(schema.contains("fields:"));

    let resources = fs::read_to_string(tmp.path().join(".workdown/resources.yaml")).unwrap();
    assert!(resources.contains("people:"));

    // Sample bug-report template was dropped into .workdown/templates/.
    let bug_report =
        fs::read_to_string(tmp.path().join(".workdown/templates/bug-report.md")).unwrap();
    assert!(bug_report.contains("type: bug"));
    assert!(bug_report.contains("Steps to reproduce"));
}

#[test]
fn init_uses_directory_name_when_no_name_given() {
    let tmp = TempDir::new().unwrap();
    let result = run_init(tmp.path(), None).unwrap();
    assert_eq!(result, InitOutcome::Created);

    let config = fs::read_to_string(tmp.path().join(".workdown/config.yaml")).unwrap();
    // Should not contain the template placeholder — it was replaced with the dir name.
    assert!(!config.contains("name: My Project"));
}

#[test]
fn init_idempotent_skips_existing() {
    let tmp = TempDir::new().unwrap();

    // First init.
    let r1 = run_init(tmp.path(), Some("First")).unwrap();
    assert_eq!(r1, InitOutcome::Created);

    // Second init — should skip.
    let r2 = run_init(tmp.path(), Some("Second")).unwrap();
    assert_eq!(r2, InitOutcome::AlreadyExists);

    // Original config is untouched.
    let config = fs::read_to_string(tmp.path().join(".workdown/config.yaml")).unwrap();
    assert!(config.contains("name: First"));
}

#[test]
fn init_special_characters_produce_valid_yaml() {
    let tmp = TempDir::new().unwrap();
    run_init(tmp.path(), Some("My App: V2")).unwrap();

    let config = fs::read_to_string(tmp.path().join(".workdown/config.yaml")).unwrap();
    // Must be parseable YAML with the correct name value.
    let parsed: serde_yaml::Value = serde_yaml::from_str(&config).unwrap();
    let name = parsed["project"]["name"].as_str().unwrap();
    assert_eq!(name, "My App: V2");
}

#[test]
fn init_generated_config_is_loadable() {
    let tmp = TempDir::new().unwrap();
    run_init(tmp.path(), Some("Roundtrip Test")).unwrap();

    let config_path = tmp.path().join(".workdown/config.yaml");
    let config = workdown_core::parser::config::load_config(&config_path).unwrap();
    assert_eq!(config.project.name, "Roundtrip Test");
}
