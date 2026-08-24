# ADR-012: Checks that judge final values run after the fill-in phase

**Status:** Accepted
**Date:** 2026-08-24

## Context

Loading a project runs as a fixed pipeline: collect files, parse and coerce, build the link graph, run the fill-in mechanisms (compute, condition, pull, roll-up), then validate. The resource check already sat after the fill-in phase, deliberately, so a derived value is held to the same standard as a hand-written one.

One check violated that shape. "Is this required field filled in?" ran in two halves: an early half during coercion for fields no mechanism could fill, and a late half after the fill-in phase for fields something might have filled. Each half decided which fields were its business by probing the mechanism configs from its own hand-maintained list. The lists had to stay exact complements, enforced by nothing — and drifted once (`pull` was added to only one), producing false and doubled errors for required pull-filled fields.

The early half existed for one real reason: a value that fails type conversion is dropped from the item, so after the fill-in phase "written but invalid" and "never written" are both an absent key. Checking early was how a required field holding an invalid value avoided a second, false, "missing" complaint on top of the invalid-value error.

## Decision

**A check that judges a field's final value runs after the fill-in phase; anything earlier may only judge what was literally written.** The required check is one check, its own pipeline phase between the fill-ins and the resource check. Coercion judges only what is written.

What crosses the phase boundary is a record, not a judgment: coercion notes per item which fields were written but failed conversion. The required check stays silent for those fields (the invalid-value diagnostic already stands), and the fill-in phase refuses to fill them (a broken hand-written value must be fixed, not silently overridden — its subtree's aggregate contributions still pass through).

Supporting decisions:

- "Which fill mechanisms exist" is one closed enumeration on the field definition (`FillMechanism`: aggregate, compute, pull, when), produced in exactly one place. Code that behaves differently per mechanism matches on it exhaustively, so a fifth mechanism is a compile error at every site not yet taught about it.
- Required findings report item-first (ascending item id, schema declaration order within an item): users fix files, not schema fields. The check keeps the cause-naming messages — a computed field's absent inputs, a pull's incomplete targets, a conditional field's unmatched branches — and stays silent where the pull pass already reported the same inputs.
- The load pipeline is written as named phase functions with the ordering contract stated once in the store module docs. No stage framework: the phases are few, fixed, and heterogeneous; the safety comes from the enumeration, not from scaffolding.

## Rationale

Consolidating the check does not centralize the mirror-list invariant — it deletes it: there is no second list left to disagree with. The alternative (keep the split, derive both predicates from one shared function) fixed the drift risk but kept the boundary-straddling special case, two reporting orders, and a per-mechanism blind spot for the next feature.

The accepted cost: answering "is it missing?" now always requires the whole project loaded, since another item's fill-in may supply the value. A future single-file lint could not reuse this check cheaply. Nothing needs per-file checking today — every command loads the full project.

## Consequences

- New validation belongs after the fill-in phase unless it explicitly judges what was written (coercion's job); the store module docs state the contract.
- A new fill mechanism extends `FillMechanism`, and the compiler walks the author through every consumer, including the required check's cause-naming.
- Message order changed once for projects with a forgotten required field (item-first, previously split between two orders); documented in the changelog.
- "Written but invalid" is now consistently terminal for a field: one diagnostic, no fill-in, no rollup overwrite — previously derivable fields could be silently filled over a broken hand-written value.
