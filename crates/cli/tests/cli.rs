//! The top-level parse and the startup every command but `init` shares.
//!
//! Clap owns the `2` for a malformed invocation and the `0` for `--help`
//! and `--version`; `main` owns the `1` for a config that will not load.
//! Each is pinned once here rather than once per command.

mod common;

use common::Project;

#[test]
fn help_exits_zero() {
    Project::empty()
        .run(&["--help"])
        .assert_code(0)
        .assert_stdout_contains("Usage:");
}

#[test]
fn version_exits_zero_and_names_the_crate_version() {
    Project::empty()
        .run(&["--version"])
        .assert_code(0)
        .assert_stdout_contains(env!("CARGO_PKG_VERSION"));
}

#[test]
fn unknown_subcommand_is_a_malformed_invocation() {
    Project::empty().run(&["frobnicate"]).assert_code(2);
}

#[test]
fn no_subcommand_is_a_malformed_invocation() {
    Project::empty().run(&[]).assert_code(2);
}

#[test]
fn missing_config_fails_the_work_not_the_invocation() {
    // Well-formed command line, no project to run it in: the operation
    // side of the contract, so `1` and not `2`.
    Project::empty()
        .run(&["validate"])
        .assert_code(1)
        .assert_stderr_contains("failed to load config");
}

#[test]
fn config_flag_points_at_another_config_file() {
    let project = Project::new();
    project.write("elsewhere/config.yaml", common::CONFIG);
    // The relocated config still names `.workdown/schema.yaml` and
    // `workdown-items` relative to the working directory, so the run
    // succeeds only if `--config` was honoured.
    project
        .run(&["--config", "elsewhere/config.yaml", "validate"])
        .assert_code(0);
    project
        .run(&["--config", "nowhere/config.yaml", "validate"])
        .assert_code(1)
        .assert_stderr_contains("failed to load config");
}

#[test]
fn quiet_and_verbose_conflict() {
    Project::new().run(&["-q", "-v", "validate"]).assert_code(2);
}
