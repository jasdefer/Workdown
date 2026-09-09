//! `workdown init`: the one command that runs without a project.

mod common;

use common::Project;

#[test]
fn init_scaffolds_a_project() {
    let project = Project::empty();
    project
        .run(&["init"])
        .assert_code(0)
        .assert_stderr_contains("Initialized workdown project");
    assert!(project.exists(".workdown/config.yaml"));
    assert!(project.exists(".workdown/schema.yaml"));
    assert!(project.exists("workdown-items"));
}

#[test]
fn name_flag_lands_in_the_config() {
    let project = Project::empty();
    project
        .run(&["init", "--name", "Named Project"])
        .assert_code(0);
    assert!(project
        .read(".workdown/config.yaml")
        .contains("Named Project"));
}

#[test]
fn a_second_init_is_a_no_op_that_still_exits_zero() {
    let project = Project::empty();
    project.run(&["init"]).assert_code(0);
    project
        .run(&["init"])
        .assert_code(0)
        .assert_stderr_contains("Already initialized");
}

#[test]
fn install_hooks_flag_installs_the_hook_after_scaffolding() {
    let project = Project::empty();
    project.git(&["init", "--quiet"]);
    project.run(&["init", "--install-hooks"]).assert_code(0);
    assert!(project
        .read(".git/hooks/pre-commit")
        .contains("workdown pre-commit hook"));
}

#[test]
fn install_hooks_flag_outside_a_repository_exits_one() {
    let project = Project::empty();
    project
        .run(&["init", "--install-hooks"])
        .assert_code(1)
        .assert_stderr_contains("not inside a git repository");
    // The scaffold itself still happened; only the hook step failed.
    assert!(project.exists(".workdown/config.yaml"));
}

#[test]
fn unknown_flag_is_a_malformed_invocation() {
    Project::empty().run(&["init", "--bogus"]).assert_code(2);
}
