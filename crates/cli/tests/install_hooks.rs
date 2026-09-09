//! `workdown install-hooks`: writes the pre-commit hook into the
//! repository the project lives in.
//!
//! Each test creates its own git repository inside the temp directory,
//! so nothing here can see or touch any other repository.

mod common;

use common::Project;

#[test]
fn installs_the_hook_in_stage_mode() {
    let project = Project::new();
    project.init_git_repository();
    project
        .run(&["install-hooks"])
        .assert_code(0)
        .assert_stderr_contains("Installed pre-commit hook");
    let hook = project.read(".git/hooks/pre-commit");
    assert!(hook.contains("workdown pre-commit hook"));
    assert!(hook.contains("Mode: stage"));
}

#[test]
fn check_flag_installs_the_check_mode_hook() {
    let project = Project::new();
    project.init_git_repository();
    project.run(&["install-hooks", "--check"]).assert_code(0);
    assert!(project
        .read(".git/hooks/pre-commit")
        .contains("Mode: check"));
}

#[test]
fn outside_a_repository_exits_one() {
    Project::new()
        .run(&["install-hooks"])
        .assert_code(1)
        .assert_stderr_contains("not inside a git repository");
}

#[test]
fn unknown_flag_is_a_malformed_invocation() {
    Project::new()
        .run(&["install-hooks", "--bogus"])
        .assert_code(2);
}
