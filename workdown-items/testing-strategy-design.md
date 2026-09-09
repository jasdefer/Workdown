---
id: testing-strategy-design
title: Work out the testing approach and break the milestone into items
status: done
parent: testing-strategy
---

## In plain words

Turn the problem described in [[testing-strategy]] into decisions, then
split whatever follows from them into separate items to build.

This is the thinking half of the milestone. Nothing here is settled in
advance: the questions below are the ones worth arguing about, with a
starting leaning noted where there is one, and those leanings are
inputs to the discussion rather than conclusions to confirm.
**Example:** one question is whether the ~26,000 lines of unit tests
inside `crates/core` are earning their keep; the honest answer today is
that nobody knows, so part of the work is deciding how we would find
out before deciding anything about them.

No product code is written here. What comes out is a set of decisions
with reasons, and follow-up items with clear scope.

## Done when

- Each question below has a chosen answer with the reasoning recorded,
  or is explicitly parked with a note on what would bring it back.
- Anything that ends up describing "how we test here" is written down
  somewhere a future change will actually meet it, rather than living
  only in this item.
- Follow-up items exist for the work that falls out, each scoped on its
  own.

## Questions to work through

### What kinds of test do we want, and what is each one for?

Right now there are effectively three groups — in-file Rust tests,
`tests/` directories that build a real project on disk, and pure-module
tests in the web app — but no statement of what distinguishes them
beyond where the file sits. Do we want a named set of layers with a
job each? Or is a single distinction enough (say, needs a project on
disk or does not)?

**Starting leaning:** fewer layers than we could justify. A model with
five tiers is a model nobody consults. But this is genuinely open.

### Where does a new test go?

Whatever the answer above, the value only lasts if it is decidable in
the moment. What is the rule, in one or two sentences, that someone
adding behaviour tomorrow can apply without re-reading this item? And
where does that rule live so it is met at the right time?

### Should anything drive the built binary, and if so what?

Nothing does today. The candidates are not the same thing:

- Running each command once, to prove it is wired up at all.
- Running realistic sequences — initialize, add, mutate, validate,
  render — to prove the commands compose, which no current test covers
  since each one exercises a single operation.
- Running only the parts that are invisible in daily use, exit codes
  being the obvious example.

These have very different costs and catch different failures. Worth
being explicit about which failure we are buying protection against
before picking.

**Note:** `workdown serve` blocks forever and has no shutdown handler,
so it constrains any answer that involves running commands to
completion. One slice of it is testable regardless: the project config
is loaded before the server binds a port, so `serve` without a project
exits without blocking. Excluding the command entirely, or writing a
shutdown handler first, are the other two answers.

Three questions come with this one, inherited from
[[stateful-test-gaps]] when the CLI half moved here. They only matter if
the answer above is "yes, something drives it":

- **How does a test get a project?** Running `workdown init` in a
  temporary directory is self-bootstrapping and exercises the shipped
  defaults, at the cost of coupling every test to
  `defaults/schema.yaml`. Writing purpose-built config and schema
  strings — what `crates/server/tests` already does — is explicit and
  stable but verbose. A checked-in fixture copied per test is the third
  shape. Note that `crates/core/tests/init.rs` already covers the
  scaffold and that its config loads, so the first option may buy less
  than it looks.
- **One shared fixture helper across the crates, or accept the
  duplication?** Three crates would be hand-rolling project fixtures.
  Only the "write these strings to a temporary directory" part actually
  overlaps; each crate wants a different thing out of the result.
- **Substring assertions or snapshots?** Snapshots would freeze the
  wording `message-style-consistency` just standardised, at the cost of
  churn on every deliberate change. There is a third shape worth
  weighing: several commands have a `--format json` mode, and that
  output is a contract in a way human prose is not.

**Also inherited:** building a CLI test binary needs `ui/dist/` present,
because the CLI depends on `workdown-server`, whose `UiAssets` embed
only switches to the committed fixture under `cargo test` *for the
server crate*. A partial gate run misleads here.

### Does the exit-code contract need pinning, and by what?

The contract itself is settled and implemented in
[[cli-exit-code-contract]] — `0` succeeded, `1` the work failed or
warned, `2` the invocation was malformed, stated in
`docs/architecture.md`. What is open is whether anything should hold it
in place.

It is the clearest example in the project of behaviour that is
invisible while using the tool: a wrong exit code prints nothing and
looks exactly like success, and the generated pre-commit hook depends
on one. That makes it a useful test case for the previous question — if
the answer there is that nothing drives the built binary, this stays
unpinned, and that should be a decision rather than an oversight.

### Is the existing unit layer carrying its weight, and how would we tell?

This is the question with the largest number attached to it and the
least evidence behind it. Before any opinion about deleting or moving
tests, decide how the question gets answered — coverage measurement,
reading the largest blocks against the integration tests that overlap
them, or something else. It is entirely possible the answer is "yes,
mostly", and that is a fine outcome.

### Forward-only, or retro-fit?

If a rule lands that the existing tests do not follow, applying it
backwards means moving or rewriting a large amount of test code for no
behaviour change, with a real risk of losing coverage in transit.
Applying it only to new tests is cheap but leaves the bulk of the suite
outside it, possibly permanently.

**Starting leaning:** forward-only by default, with targeted cleanup
only where the previous question turns up genuine duplication.

### How do stateful modules in the web app get tested?

Two shapes, and they are not exclusive: pull the decision out of the
stateful module into a pure function and test that (which is already
the habit everywhere else in `ui/`), or add the machinery — Svelte
compiler plugin, and a browser-like environment if components ever
follow — and drive the module as it is. The first is cheaper and
narrower; the second covers the plumbing the first leaves out, and is
the only route if rendered components are ever in scope.

[[stateful-test-gaps]] took the first route for the timer's two silent
paths and handed the rest of the question here. Concretely, what is
still undecided is whether `ui/src/lib/stores/timer.svelte.ts` becomes
`createTimerStore({ api })` so its plumbing can be driven — the ten or
so lines of await-and-assign that a store calling `undoMutation` and
then ignoring its answer would slip past. It would cost
`@sveltejs/vite-plugin-svelte` in the Vitest config and a teardown story
for the store's heartbeat and effects, since each test would build its
own instance.

One claim to discount while weighing it: the factory was argued to have
a benefit independent of testing, because the store's module-level
`$effect.root` fires on import. It does, but `ui/src/routes/+layout.svelte`
imports `timerStore`, so it loads on every page anyway — there are no
code paths in the running app that avoid it. Decide this on the testing
merits alone.

### What does CI gate, and how would we notice a silent green?

The whole workspace runs today, but only because
[[ci-workspace-coverage]] caught that it did not. Is there something
cheap that would make the same class of failure loud — a reported test
count, a coverage floor, something else — or is that machinery we would
regret?

### What is the work breakdown?

The last question: what items come out of all of the above, in what
order, and which of them are worth doing at all once the earlier
answers are in.

## Out of scope

- Writing the tests themselves. Any code here is a throwaway spike to
  answer a question, not the implementation.
- Deleting or moving existing tests. If that turns out to be warranted,
  it becomes its own item with its own justification.

## Decisions taken

### 1. Three layers, and where behaviour assertions live (2026-09-08)

**Decided:** three layers, each answering one question, and a behaviour is
asserted once, in the crate that owns it, at that crate's public entry point.
Layers above assert only on what they add.

| Layer | Question it answers | Where it lives |
|---|---|---|
| Unit | Does this function handle every input shape? | `#[cfg(test)]` in the file; `*.test.ts` beside a pure UI module |
| Integration | Does this crate's public entry point do the right thing to a real project on disk? | `crates/<crate>/tests/` |
| Binary | Is the shipped command wired up, with the right exit code? | `crates/cli/tests/`, running the compiled executable |

**Per crate:**

- **core** holds the behaviour assertions: one test per user-visible
  behaviour of an operation, plus each distinct error, against a temp
  project, asserting on returned values and files on disk.
- **server** is thin. It asserts only on the HTTP contract: routing,
  status codes, JSON shape, origin guard, request validation. A server
  test that asserts on file contents belongs in core. Exception:
  behaviour the server crate owns outright (today the timer state
  machine), which it tests fully because nothing below it does.
- **cli** is thin. A new `crates/cli/tests/` runs the built binary and
  asserts that each command reaches its operation, flags arrive, exit
  codes follow the `0`/`1`/`2` contract, and `serve` exits cleanly
  without a project. It does not re-test what operations do.
- **git**: owns repository behaviour, but is tested today only through
  the server's git endpoint. Check who actually consumes the crate
  before deciding whether those tests move down. Open.
- **web app**: pure-module unit tests in Node only. Logic that would sit
  in a component is pulled into a module first (existing habit).

**Unit scope rule:** unit tests are for functions with a real input
space (parsers, evaluators, date arithmetic, sorting and grouping), not
for glue that loads three things and calls a fourth; the integration
test already covers that path. This is also the lens for judging the
existing in-file blocks: a large block under a parser is expected, a
large block under an operation module is suspect.

**Where a new test goes:** find the crate that owns the behaviour and
write the test there. If the change also touched a handler or a
command, add one contract test above it. If the function has more input
shapes than the integration test sends through, add unit tests for the
shapes.

**Why this and not testing only at the fronts:** core has two
independent fronts (the CLI and the server are siblings; the server does
not use the CLI, the CLI links the server in). Asserting behaviour at a
front means either asserting it twice or leaving the other front's path
a guess; behaviours with no endpoint (`render`, `validate`, hook
installation) would have to move to the slow, text-asserting binary
layer. Core's code running inside a front test is not testing core: the
front test asserts only on its own contract, so dropping core tests
would leave the behaviour unasserted anywhere. Considered and rejected:
behaviour assertions at the server with no core `tests/`. Coherent, but
three times the ceremony per case across hundreds of cases, and a red
test would not say whether the handler or the operation broke.

**Parked:** browser tests (thin client over tested endpoints; not worth
the machinery, revisit if a UI bug reaches a user that a pure-module test
could not have caught). The Svelte store factory question stays open as
its own later decision.

### 2. How much to test: case rules, not a coverage number (2026-09-08)

**Decided:** no coverage percentage as a target or a CI gate. A number
cannot tell a parser with fifty tested shapes from glue with one, it
rewards testing glue to move the number, and the failures that have
actually bitten this project (an inverted exit code, a CI job that
skipped most of the suite) would leave coverage unchanged. Coverage is
run once as a finder: look at what has zero coverage and decide case by
case under the rules below, then stop measuring until the next time we
wonder.

The target is: every rule below is met, and nothing else is required.

**Core integration, per operation:**

- The happy path, once, checking the result and the file on disk.
- Each distinct error the user can hit, once. Distinct means a different
  message or cause, not a different input producing the same error.
- Each option or mode that changes the outcome, once. A flag that only
  changes a log line is not a case.
- Anything touching more than one item: parent chains, rollups, cycles.
  Only the integration test can see these interactions.
- Not a case: internal branches with the same visible outcome, and
  anything a unit test below already pins.

**Server contract, per endpoint:**

- One success response: status and JSON shape, not file contents.
- Each distinct status code the endpoint can return, once (404, 409,
  422, ...). Each maps from one core error class; the test proves the
  mapping.
- The origin guard, once per mutating endpoint, or once for all if the
  guard is shared.
- Not a case: the same status for different bad values.

**CLI binary, per command:**

- One invocation that reaches the operation and exits `0`.
- Each flag arrives at the operation, once per flag, proven by a flag
  that visibly changes the result.
- Exit `1` on an operation error and `2` on a malformed invocation, once
  per command, since each command has its own parse.
- `serve` without a project exits without blocking.
- Not a case: what the operation does. Proven in core.

**Unit, per function with an input space:**

- Every input shape the function distinguishes: for a parser, each field
  type times each option that alters parsing; for the evaluator, each
  operator times each accepted type pairing plus the absent-value case;
  for date arithmetic, month and year boundaries.
- The rejections: each shape the function refuses, with the right
  diagnostic.
- Not a case: functions with one shape. If no second input would take a
  different path, the integration test already covers it and the unit
  test restates the code.

**Web app unit:** as Rust unit tests, for pure modules. Decision logic is
pulled into a module before it gets a test.

**Expected shape, to be checked by the measurement step:** core
integration grows with behaviour; server and CLI grow only with new
endpoints, status codes, commands and flags; unit tests grow only where
an input space grows. Prediction for today: core `tests/` and the server
suite are roughly right-sized, the CLI suite is missing and is a few
dozen tests, and any excess sits in the in-file unit blocks.

### 3. The existing suite: audit against the rules, delete what fails them (2026-09-08)

**Measured:** every `#[cfg(test)]` block ranked by size and read against
the decision-two lens. Findings:

- The large input-space blocks (schema parser, `coerce`, query evaluator,
  views parser, `compute_check`, `where_check`, expression typechecker,
  duration, gantt placement, derive, rollup, cycles) are large because
  their input spaces are, and their test names are distinct shapes. Right
  by rule.
- The prediction that excess sits in the unit layer was wrong. What sits
  there instead is **misplacement**: the in-file tests under
  `operations/set` (16 product lines, 1,202 test lines), `rename`,
  `view_write`, `body`, `install_hooks` and `frontmatter_io` write a temp
  project and assert on files. They are integration tests by decision
  one, living in the file. `crates/core/tests/` covers only add, init,
  templates and validate. Nothing is duplicated: each operation is tested
  once, half in-file, half in `tests/`.
- The git crate has two consumers. The server uses status, commit, pull,
  push; the CLI's `changes` command uses the preview builder and scope,
  hook installation uses the path helpers. Preview and scope are shared
  and are tested in the git crate (in-file). Commit, pull and push are
  server-only and are tested through the endpoint, the top of a
  one-consumer stack. **Decided:** leave the git tests where they are.
- Coverage (`cargo llvm-cov --workspace`, one-off, 2026-09-08): 92.6% of
  lines overall, but the number is not the finding. Every file with zero
  coverage is in the CLI: `main.rs` (1.5%), `cli/mod.rs`, `cli/output.rs`,
  and every `commands/*.rs` except `serve.rs` (33%) and `schema_args.rs`
  (97%, unit-tested). That is the binary gap of decision one, measured:
  about 1,300 uncovered lines, all of them wiring. Outside the CLI the
  only zero is `core/src/model/template.rs` (57 lines of error type and
  display), and the only files under 80% are `rules/assertion.rs` (65%),
  `model/condition.rs` (62%), `model/assertion.rs` (70%) and the server
  watcher (75%). Nothing else needs a test the rules would ask for.
  Coverage is not run again until the next time we wonder.

**Decided:** not forward-only. The rules apply to the existing suite
too, and the milestone must end with fewer tests, or better ones, than
it started with. Concretely:

- **Delete** every test that fails a decision-two rule: unit tests of
  glue with one input shape; unit tests restating an assertion an
  integration test in `tests/` already makes; server tests asserting on
  file contents rather than the HTTP contract; several tests proving the
  same status code or the same error for different inputs. Each deletion
  names the test that still covers the behaviour, or states that the
  behaviour has no user-visible outcome.
- **Relocate** the in-file operations integration tests to
  `crates/core/tests/`, so the location rule holds everywhere and the
  file-level counts stop lying about what is a unit test.
- **Add** only what the rules require and nothing has: the CLI binary
  layer.
- **Measure before and after:** test count and test lines per layer,
  recorded in the milestone, so "fewer or better" is checked rather than
  claimed.

The audit is a build item, not part of this design item. It works file
by file through the ranked list, largest block first, and each deletion
or move is justified in the commit against the rule it applies.

### 4. How CI notices a silent green (2026-09-09)

**Decided:** a short script in the CI job, run after `cargo test`, that
lists every test target the repository contains (each crate's unit
binary and each `crates/*/tests/*.rs` file) and compares that list
against the `Running ...` lines `cargo test` printed. A target that
exists but did not run fails the build by name. The same step checks
that the Vitest run happened. No coverage tool in CI, no test-count
floor to maintain: the expected list is derived from the tree, so it
stays correct as files come and go. Rejected: a coverage floor (decision
two) and a fixed test count (manual upkeep, says nothing about which
tests ran).

### 5. Work breakdown (2026-09-09)

**Decided:** four items under [[testing-strategy]], in this order.

1. [[cli-binary-tests]]: new `crates/cli/tests/`, running the built
   executable, with the decision-two cases.
2. [[test-audit]]: file by file down the ranked list, delete what fails
   a decision-two rule, relocate the in-file operations integration
   tests to `crates/core/tests/`, count before and after.
3. [[ci-test-run-completeness]]: the script from decision four.
4. [[testing-guide]]: decisions one and two written into
   `docs/architecture.md`, with a one-line pointer in `CLAUDE.md`.

Parked, not items: browser tests; the Svelte store factory.

"Do we have too many tests somewhere?" Not visibly. The large blocks are
input-space functions and are large for a reason. The only places the
inventory flagged are named in [[test-audit]], and the audit decides
them.

## Baseline for the later comparison (measured 2026-09-09)

The "after" column is an **estimate made before any test was touched.
It is not a target and must not steer the audit or the CLI tests**: a
block is deleted because it fails a rule, never to hit a number, and a
CLI test is written because a rule asks for it, never to fill a line
budget. The column exists only so the milestone can be compared to its
starting point when it closes.

| Layer | Today (measured) | After (estimate, not a target) |
|---|---|---|
| Rust unit, in-file | 32,300 lines | 27,000 to 28,000 |
| Core integration, `crates/core/tests/` | 5,300 | 8,800 (3,500 relocated) |
| Server integration, `crates/server/tests/` | 4,700 | 4,300 to 4,700 |
| CLI binary, `crates/cli/tests/` | 0 | 400 to 600 |
| Web app unit | 2,100 | 2,100 |
| **Total** | **44,400** | **42,500 to 44,000** |

| | Today (measured) | After (estimate) |
|---|---|---|
| Rust test functions | 1,960 | about 1,850 to 1,950 |
| Web app test functions | 207 | 207 |
| Line coverage, workspace | 92.6% | around 95% |
| Line coverage, CLI crate | 78% | around 95% |
| Line coverage, core / git / server | 96% / 90% / 90% | unchanged |

How the "today" numbers were taken, so the closing count uses the same
method: in-file lines are everything from a file's first `#[cfg(test)]`
to its end, summed per crate; `tests/` lines are `wc -l` over the
directory; UI lines are `wc -l` over `ui/src/**/*.test.ts`; test
functions are `#[test]` and `#[tokio::test]` attributes in `crates/`,
and `it(`/`test(` calls in the UI; coverage is
`cargo llvm-cov --workspace --summary-only` in the dev container.

The structural change is the point, not the totals: every test gets a
reason and a place, the 1,300 uncovered lines of CLI wiring get covered,
and CI can no longer be green while most of the suite did not run.

## Outcome (2026-09-09)

All eight questions answered or parked, recorded above as decisions one
to five. Four follow-up items created under [[testing-strategy]]. The
rules themselves land in `docs/architecture.md` through
[[testing-guide]], which is where a future change meets them.
