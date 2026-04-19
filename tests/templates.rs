//! Integration tests for the templates module: loading and listing.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;
use workdown::commands::templates::{list_template_names, load_template_by_name};
use workdown::model::template::TemplateError;

fn make_templates_dir() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let templates_dir = directory.path().join(".workdown/templates");
    fs::create_dir_all(&templates_dir).unwrap();
    (directory, templates_dir)
}

#[test]
fn list_templates_empty_directory() {
    let (_directory, templates_dir) = make_templates_dir();
    let names = list_template_names(&templates_dir);
    assert!(names.is_empty());
}

#[test]
fn list_templates_returns_alphabetical_names() {
    let (_directory, templates_dir) = make_templates_dir();
    fs::write(templates_dir.join("zebra.md"), "---\ntype: a\n---\n").unwrap();
    fs::write(templates_dir.join("alpha.md"), "---\ntype: b\n---\n").unwrap();
    fs::write(templates_dir.join("middle.md"), "---\ntype: c\n---\n").unwrap();

    let names = list_template_names(&templates_dir);
    assert_eq!(
        names,
        vec!["alpha".to_owned(), "middle".to_owned(), "zebra".to_owned()]
    );
}

#[test]
fn list_templates_missing_directory_returns_empty() {
    let directory = TempDir::new().unwrap();
    let missing = directory.path().join("nope");
    assert!(list_template_names(&missing).is_empty());
}

#[test]
fn load_template_success() {
    let (_directory, templates_dir) = make_templates_dir();
    fs::write(
        templates_dir.join("foo.md"),
        "---\ntype: bug\npriority: medium\n---\n\nBody here.\n",
    )
    .unwrap();

    let template = load_template_by_name(&templates_dir, "foo").unwrap();
    assert_eq!(
        template.frontmatter.get("type").unwrap(),
        &serde_yaml::Value::String("bug".into())
    );
    assert!(template.body.contains("Body here."));
}

#[test]
fn load_missing_template_returns_not_found_with_available() {
    let (_directory, templates_dir) = make_templates_dir();
    fs::write(templates_dir.join("alpha.md"), "---\ntype: a\n---\n").unwrap();
    fs::write(templates_dir.join("beta.md"), "---\ntype: b\n---\n").unwrap();

    let result = load_template_by_name(&templates_dir, "gamma");
    match result {
        Err(TemplateError::NotFound { name, available }) => {
            assert_eq!(name, "gamma");
            assert_eq!(available, vec!["alpha".to_owned(), "beta".to_owned()]);
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn load_from_empty_dir_not_found_suggests_nothing() {
    let (_directory, templates_dir) = make_templates_dir();

    let result = load_template_by_name(&templates_dir, "ghost");
    match result {
        Err(TemplateError::NotFound { available, .. }) => {
            assert!(available.is_empty());
        }
        other => panic!("expected NotFound, got {other:?}"),
    }
}

#[test]
fn load_from_missing_dir_returns_directory_missing() {
    let directory = TempDir::new().unwrap();
    let missing = directory.path().join("no-templates");

    let result = load_template_by_name(&missing, "anything");
    assert!(matches!(result, Err(TemplateError::DirectoryMissing { .. })));
}
