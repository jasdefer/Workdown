//! Shared test fixtures and helpers for the `set` operation's family modules.
#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::model::config::Config;
use crate::parser::config::load_config;

pub(super) const TEST_SCHEMA: &str = "\
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
  priority:
    type: choice
    values: [low, medium, high]
    required: false
  points:
    type: integer
    required: false
  tags:
    type: list
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
  assignees:
    type: links
    required: false
    allow_cycles: false
  labels:
    type: multichoice
    values: [bug, feature, chore]
    required: false
  velocity:
    type: float
    required: false
  estimate:
    type: duration
    required: false
  due_date:
    type: date
    required: false
  archived:
    type: boolean
    required: false
";

pub(super) const TEST_CONFIG: &str = "\
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

pub(super) const AGGREGATE_SCHEMA: &str = "\
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
  effort:
    type: integer
    required: false
    aggregate:
      function: sum
      error_on_missing: true
";

pub(super) fn setup_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), TEST_SCHEMA).unwrap();
    (directory, root)
}

pub(super) fn setup_aggregate_project() -> (TempDir, PathBuf) {
    let directory = TempDir::new().unwrap();
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join(".workdown/templates")).unwrap();
    fs::create_dir_all(root.join("workdown-items")).unwrap();
    fs::write(root.join(".workdown/config.yaml"), TEST_CONFIG).unwrap();
    fs::write(root.join(".workdown/schema.yaml"), AGGREGATE_SCHEMA).unwrap();
    (directory, root)
}

pub(super) fn load_test_config(root: &Path) -> Config {
    load_config(&root.join(".workdown/config.yaml")).unwrap()
}

pub(super) fn write_item(root: &Path, id: &str, content: &str) {
    fs::write(root.join(format!("workdown-items/{id}.md")), content).unwrap();
}

pub(super) fn read_item(root: &Path, id: &str) -> String {
    fs::read_to_string(root.join(format!("workdown-items/{id}.md"))).unwrap()
}
