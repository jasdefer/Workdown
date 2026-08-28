---
id: testing-strategy-design
title: Work out the testing approach and break the milestone into items
status: to_do
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
