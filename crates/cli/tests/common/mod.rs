//! Shared fixture for the binary tests: a throwaway project on disk and
//! a way to run the compiled `workdown` in it.
//!
//! These tests assert only on what the CLI adds on top of the
//! operations — that a command reaches its operation, that each flag
//! arrives, and that the exit code follows the `0` / `1` / `2` contract
//! in `docs/architecture.md`. What an operation does to the files is
//! core's job and is tested in `crates/core/tests/`.
//!
//! Every test uses the same default project unless it needs a shape the
//! default cannot express (today only the `$today`-dependent variant),
//! so the tests read alike and a failure in one is easy to compare with
//! its neighbours.

// Each `tests/*.rs` file is its own crate and uses a different subset
// of these helpers, so an unused-function warning here would be noise.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// The config every test project starts from. Paths match what
/// `workdown init` scaffolds, so the tests read like a real project.
pub const CONFIG: &str = "\
project:
  name: Binary Test Project
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

/// One field of every type a `set` mode or an `add` flag needs to reach:
/// a choice for the board, a list for `--append` / `--remove`, an
/// integer for `--delta`, a boolean for `--toggle`, a link for the
/// rename rewrite, and a date for the duration arithmetic.
pub const SCHEMA: &str = "\
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
  tags:
    type: list
    required: false
  points:
    type: integer
    required: false
  due:
    type: date
    required: false
  effort:
    type: duration
    required: false
  urgent:
    type: boolean
    required: false
  parent:
    type: link
    required: false
    allow_cycles: false
    inverse: children
  depends_on:
    type: links
    required: false
    allow_cycles: false
";

/// A board and a table, enough for `render` to write two files and for
/// a single view id to pick one of them.
pub const VIEWS: &str = "\
views:
  - id: status-board
    type: board
    field: status
  - id: item-table
    type: table
    display:
      fields: [id, title, status]
";

/// The template `add --template` reaches for.
pub const TEMPLATE: &str = "\
---
tags:
  - templated
---

Body from the template.
";

pub const TASK_1: &str = "\
---
title: Task 1
status: open
tags:
  - alpha
points: 3
urgent: false
due: 2026-01-10
---

First task.
";

pub const TASK_2: &str = "\
---
title: Task 2
status: done
parent: task-1
---
";

/// A throwaway project directory. Deleted when dropped, so hold it for
/// the whole test.
pub struct Project {
    directory: TempDir,
}

impl Project {
    /// The default project: config, schema, views, one template and two
    /// items (`task-1`, `task-2` with `task-2`'s parent being `task-1`).
    pub fn new() -> Self {
        Self::with_schema(SCHEMA)
    }

    /// The default project with a different schema, for the few tests
    /// whose flag only shows through a field the default lacks.
    pub fn with_schema(schema: &str) -> Self {
        let project = Self::empty();
        let root = project.root();
        fs::create_dir_all(root.join(".workdown/templates")).unwrap();
        fs::create_dir_all(root.join("workdown-items")).unwrap();
        fs::write(root.join(".workdown/config.yaml"), CONFIG).unwrap();
        fs::write(root.join(".workdown/schema.yaml"), schema).unwrap();
        fs::write(root.join(".workdown/views.yaml"), VIEWS).unwrap();
        fs::write(root.join(".workdown/templates/bug.md"), TEMPLATE).unwrap();
        fs::write(root.join("workdown-items/task-1.md"), TASK_1).unwrap();
        fs::write(root.join("workdown-items/task-2.md"), TASK_2).unwrap();
        project
    }

    /// A bare directory with no project in it: the "no config" case.
    pub fn empty() -> Self {
        Self {
            directory: TempDir::new().expect("create temp directory"),
        }
    }

    pub fn root(&self) -> &Path {
        self.directory.path()
    }

    /// Run the compiled binary in the project root and wait for it.
    ///
    /// The two environment variables the CLI reads are cleared so a
    /// developer's shell cannot change what a test sees: `WORKDOWN_CONFIG`
    /// would redirect `--config`, `WORKDOWN_LOG` the log filter.
    pub fn run(&self, arguments: &[&str]) -> Output {
        let output = Command::new(env!("CARGO_BIN_EXE_workdown"))
            .args(arguments)
            .current_dir(self.root())
            .env_remove("WORKDOWN_CONFIG")
            .env_remove("WORKDOWN_LOG")
            .output()
            .expect("run the workdown binary");
        Output {
            code: output.status.code().expect("workdown exited with a code"),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    /// Run the binary but give up after `timeout`, killing it. For the
    /// one command that would otherwise run forever: `serve` is expected
    /// to exit before it binds a port when there is no project, and a
    /// regression there must fail this test rather than hang the suite.
    pub fn run_with_timeout(&self, arguments: &[&str], timeout: Duration) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_workdown"))
            .args(arguments)
            .current_dir(self.root())
            .env_remove("WORKDOWN_CONFIG")
            .env_remove("WORKDOWN_LOG")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the workdown binary");
        let started = Instant::now();
        loop {
            match child.try_wait().expect("poll the workdown binary") {
                Some(_) => break,
                None if started.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("workdown {arguments:?} did not exit within {timeout:?}");
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        let output = child.wait_with_output().expect("collect output");
        Output {
            code: output.status.code().expect("workdown exited with a code"),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    /// Turn the project directory into a git repository with one commit
    /// holding every file written so far. The repository lives inside the
    /// temp directory and disappears with it; the tests never touch any
    /// other repository.
    pub fn init_git_repository(&self) {
        self.git(&["init", "--quiet", "--initial-branch=main"]);
        self.git(&["add", "-A"]);
        self.git(&["commit", "--quiet", "-m", "Initial project"]);
    }

    /// Run git inside the project with a fixed identity, so a commit
    /// works on a machine (or CI runner) with no git config at all.
    pub fn git(&self, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.root())
            .args([
                "-c",
                "user.name=Binary Test",
                "-c",
                "user.email=binary-test@example.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(arguments)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {arguments:?} failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Read a file relative to the project root.
    pub fn read(&self, relative_path: &str) -> String {
        let path = self.root().join(relative_path);
        fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
    }

    /// Write a file relative to the project root, creating parents.
    pub fn write(&self, relative_path: &str, content: &str) {
        let path = self.root().join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }

    pub fn exists(&self, relative_path: &str) -> bool {
        self.root().join(relative_path).exists()
    }

    /// The raw file of a work item.
    pub fn item(&self, id: &str) -> String {
        self.read(&format!("workdown-items/{id}.md"))
    }

    pub fn item_path(&self, id: &str) -> PathBuf {
        self.root().join(format!("workdown-items/{id}.md"))
    }
}

/// What one run of the binary produced.
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// Assert the exit code, printing both streams on a mismatch so the
    /// failure explains itself.
    pub fn assert_code(&self, expected: i32) -> &Self {
        assert_eq!(
            self.code, expected,
            "expected exit {expected}, got {}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.code, self.stdout, self.stderr
        );
        self
    }

    pub fn assert_stdout_contains(&self, needle: &str) -> &Self {
        assert!(
            self.stdout.contains(needle),
            "stdout does not contain {needle:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.stdout,
            self.stderr
        );
        self
    }

    pub fn assert_stderr_contains(&self, needle: &str) -> &Self {
        assert!(
            self.stderr.contains(needle),
            "stderr does not contain {needle:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.stdout,
            self.stderr
        );
        self
    }

    /// Parse stdout as JSON, for the commands with a `--format json`.
    pub fn stdout_json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|error| {
            panic!(
                "stdout is not JSON: {error}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                self.stdout, self.stderr
            )
        })
    }
}
