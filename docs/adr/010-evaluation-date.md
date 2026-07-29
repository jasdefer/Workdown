# ADR-010: The evaluation date as an explicit input

**Status:** Accepted
**Date:** 2026-07-29

## Context

`$today` existed only as a default generator: resolved while `workdown
add` runs, stamped as a literal into the file. Nothing could reference
the current date during evaluation, so no computed field could say
anything about the present ("days until the end date"). The repo being
the single source of truth meant evaluation had *no* input besides the
repository — which made every derived value reproducible, and every
statement about "now" inexpressible.

## Decision

- Evaluation may read the current date, spelled `$today` in compute
  expressions — the same token as the add-time generator, resolved at a
  different moment. It types as `date` and participates in the existing
  algebra; no other grammar change.
- The date is an **explicit input**, resolved exactly once per load at
  the top of project loading and threaded down — never read ambiently
  inside evaluation. Every consumer of one load (computed fields today,
  rules when they gain date references) sees the same value.
- Every evaluating command (`validate`, `query`, `render`, `serve`)
  accepts `--as-of <DATE>` to pin it. Unpinned, it is the current local
  date. A pinned run of a given commit is byte-reproducible on any day.
- The override is a flag only — deliberately not a config key, because a
  committed pinned date would silently freeze every collaborator's
  views. An unpinned `workdown serve` resolves per request (cold-load),
  so a long-running server stays current across midnight; a pinned one
  holds its date for the process lifetime and says so at startup.
- Local time zone, matching the add-time generator. Collaborators in
  different zones can derive different values near midnight; contexts
  that need agreement (CI, committed renders) should pin.
- `workdown render` prints a notice when any compute expression
  references `$today`, so a diff on an untouched repository carries its
  explanation.

## Consequences

- Rendered output is a function of the repository *and the calendar*
  wherever `$today` is used. That is the feature — the board genuinely
  changes overnight — with `--as-of` as the reproducibility escape.
- First precedent of a value entering evaluation from outside the
  repository. The bar for the next one (`$now`, environment values)
  is the same: explicit, overridable, resolved once.
- One token, two resolution moments: at `add` time in a `default:`, at
  evaluation time in a `compute:`. Reads naturally, but documentation
  must keep the distinction sharp.
