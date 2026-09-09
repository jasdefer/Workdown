//! Integration tests for `install_pre_commit`: the hook file the
//! operation writes, replaces, or refuses to touch.
//!
//! The script's wording and quoting are unit-tested beside `hook_script`;
//! these tests are about the file on disk.

use std::fs;

use workdown_core::operations::install_hooks::{
    hook_script, install_pre_commit, HookMode, HookTemplate, InstallHooksError, InstallOutcome,
};

fn template(mode: HookMode) -> HookTemplate {
    HookTemplate {
        mode,
        project_prefix: None,
        watched_paths: vec![
            "workdown-items".to_owned(),
            ".workdown/config.yaml".to_owned(),
            ".workdown/schema.yaml".to_owned(),
        ],
        output_dir: "views".to_owned(),
    }
}

#[test]
fn install_writes_fresh_hook() {
    let dir = tempfile::tempdir().unwrap();
    let hooks = dir.path().join("hooks");
    let script = hook_script(&template(HookMode::Stage));

    let outcome = install_pre_commit(&hooks, &script).unwrap();
    let expected = hooks.join("pre-commit");
    assert_eq!(
        outcome,
        InstallOutcome::Installed {
            path: expected.clone()
        }
    );
    assert_eq!(fs::read_to_string(expected).unwrap(), script);
}

#[test]
fn install_overwrites_own_hook() {
    let dir = tempfile::tempdir().unwrap();
    let hooks = dir.path().join("hooks");
    let first = hook_script(&template(HookMode::Stage));
    install_pre_commit(&hooks, &first).unwrap();

    let second = hook_script(&template(HookMode::Check));
    let outcome = install_pre_commit(&hooks, &second).unwrap();
    assert!(matches!(outcome, InstallOutcome::Replaced { .. }));
    assert_eq!(
        fs::read_to_string(hooks.join("pre-commit")).unwrap(),
        second
    );
}

#[test]
fn install_refuses_foreign_hook() {
    let dir = tempfile::tempdir().unwrap();
    let hooks = dir.path().join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    let foreign = "#!/bin/sh\nnpm run lint\n";
    fs::write(hooks.join("pre-commit"), foreign).unwrap();

    let script = hook_script(&template(HookMode::Stage));
    let error = install_pre_commit(&hooks, &script).unwrap_err();
    assert!(matches!(error, InstallHooksError::ForeignHook { .. }));
    // The foreign hook is untouched.
    assert_eq!(
        fs::read_to_string(hooks.join("pre-commit")).unwrap(),
        foreign
    );
}

#[cfg(unix)]
#[test]
fn installed_hook_is_executable() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let hooks = dir.path().join("hooks");
    let script = hook_script(&template(HookMode::Stage));
    install_pre_commit(&hooks, &script).unwrap();

    let mode = fs::metadata(hooks.join("pre-commit"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o111, 0o111);
}
