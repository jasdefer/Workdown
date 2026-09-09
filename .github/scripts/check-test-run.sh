#!/usr/bin/env bash
# Fail when a test target exists in the tree but did not run.
#
# CI was once green for weeks while `cargo test` was scoped to a single
# crate: every test that ran passed, and nothing said that most of the
# suite had not run. This script derives the list of test targets from
# the repository — one unit-test binary per crate under `crates/`, plus
# every `crates/*/tests/*.rs` — and compares it against the `Running ...`
# lines a captured `cargo test --workspace` run printed. It also checks
# that the Vitest run inside `cargo xtask build-ui` happened and passed
# at least one test file.
#
# Usage: check-test-run.sh <cargo-test-log> <build-ui-log>
#
# Both logs are what the corresponding command printed; capture them with
# `tee` under `set -o pipefail` so the command's own exit code still fails
# the step. No tooling beyond bash, grep and sed.
set -euo pipefail

cargo_log=${1:?path to the captured cargo test output}
ui_log=${2:?path to the captured cargo xtask build-ui output}

for log in "$cargo_log" "$ui_log"; do
    if [[ ! -s "$log" ]]; then
        echo "check-test-run: $log is missing or empty" >&2
        exit 1
    fi
done

failures=0

# ── Rust: one unit binary per crate ──────────────────────────────────
# `cargo test` reports a unit binary as
#   Running unittests src/lib.rs (target/debug/deps/<package>-<hash>)
# with the package name's hyphens turned into underscores.
for manifest in crates/*/Cargo.toml; do
    crate_dir=$(dirname "$manifest")
    package=$(sed -n 's/^name = "\(.*\)"/\1/p' "$manifest" | head -n 1)
    binary=${package//-/_}
    if ! grep -Eq "Running unittests src/(lib|main)\.rs \(.*/deps/${binary}-[0-9a-f]+\)" "$cargo_log"; then
        echo "check-test-run: unit tests of $crate_dir ($package) did not run" >&2
        failures=$((failures + 1))
    fi
done

# ── Rust: every tests/*.rs is its own target ─────────────────────────
# Reported as `Running tests/<file> (...)`. The line does not name the
# crate, and two crates may have a test file of the same name (both the
# CLI and core have `tests/set.rs`), so the check compares counts: the
# number of crates holding `tests/<file>` against the number of times
# that file was reported as running.
declare -A expected_runs
declare -A owners
for test_file in crates/*/tests/*.rs; do
    [[ -e "$test_file" ]] || continue
    name=$(basename "$test_file")
    expected_runs[$name]=$(( ${expected_runs[$name]:-0} + 1 ))
    owners[$name]="${owners[$name]:-}${owners[$name]:+, }$(dirname "$(dirname "$test_file")")"
done
for name in "${!expected_runs[@]}"; do
    observed=$(grep -Ec "Running tests/${name//./\\.} \(" "$cargo_log" || true)
    if (( observed < expected_runs[$name] )); then
        echo "check-test-run: tests/$name exists in ${owners[$name]} (${expected_runs[$name]} target(s)) but ran $observed time(s)" >&2
        failures=$((failures + 1))
    fi
done

# ── Web app: the Vitest run happened ─────────────────────────────────
# `vitest run` ends with a summary such as
#      Test Files  13 passed (13)
#           Tests  207 passed (207)
# wrapped in ANSI colour codes even when piped, so those are stripped
# before matching.
plain_ui_log=$(sed -E $'s/\x1b\\[[0-9;]*m//g' "$ui_log")
files_passed=$(sed -nE 's/^ *Test Files +([0-9]+) passed.*/\1/p' <<<"$plain_ui_log" | tail -n 1)
tests_passed=$(sed -nE 's/^ *Tests +([0-9]+) passed.*/\1/p' <<<"$plain_ui_log" | tail -n 1)
if [[ -z "$files_passed" || -z "$tests_passed" ]]; then
    echo "check-test-run: no Vitest summary found in $ui_log — did \`npm run test\` run?" >&2
    failures=$((failures + 1))
elif (( files_passed == 0 || tests_passed == 0 )); then
    echo "check-test-run: Vitest reported $files_passed test file(s) and $tests_passed test(s) passed" >&2
    failures=$((failures + 1))
fi

if (( failures > 0 )); then
    echo "check-test-run: $failures problem(s); see above" >&2
    exit 1
fi

unit_count=$(grep -Ec "Running unittests src/(lib|main)\.rs" "$cargo_log")
integration_count=$(grep -Ec "Running tests/[^ ]+\.rs \(" "$cargo_log")
echo "check-test-run: every test target ran — $unit_count unit binaries, $integration_count integration targets, $tests_passed Vitest tests in $files_passed files"
