//! `workdown serve`: the one slice that can be tested without a server
//! that never exits.
//!
//! The project config is loaded before any port is bound, so without a
//! project the command must fail fast. The timeout turns a regression
//! that makes it bind first into a test failure instead of a hung suite.
//! The HTTP contract itself is the server crate's, tested there.

mod common;

use std::time::Duration;

use common::Project;

#[test]
fn without_a_project_exits_one_without_blocking() {
    Project::empty()
        .run_with_timeout(&["serve"], Duration::from_secs(10))
        .assert_code(1)
        .assert_stderr_contains("failed to load config");
}

#[test]
fn unparseable_port_is_a_malformed_invocation() {
    Project::empty()
        .run_with_timeout(&["serve", "--port", "many"], Duration::from_secs(10))
        .assert_code(2);
}
