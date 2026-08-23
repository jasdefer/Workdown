---
id: derived-field-single-predicate
status: to_do
title: One shared answer to "is this field filled in automatically?"
parent: maintenance-review-2026-08
---

## In plain words

A field can be filled in automatically in several ways: computed from
other fields, rolled up from children, derived by condition, or pulled
from a linked item. Three different places in the code each keep their
*own* list of which mechanisms count as "automatic" — and one list is
outdated: it does not know about the newest mechanism, `pull`. So a
field that is both required and pull-filled gets a false "missing
value" error, raised before the pull has had a chance to run. The fix
is one shared list instead of three copies, so they can never disagree
again.

## The problem in detail

The "is this field derivable?" predicate is hand-enumerated at three
sites, and they have already drifted:

- `crates/core/src/store/coerce.rs:69-73` defers the required-field
  check when `aggregate`, `compute`, or `when` is configured — **but
  not `pull`**.
- `crates/core/src/store/derive.rs:786-790` (`required_check`) treats
  `pull` as derivable.
- `crates/core/src/store/derive.rs:326-330`
  (`derive_fields_in_order`) enumerates the same configs a third way.

Consequence of the drift: a `required` + `pull` field gets a
`MissingRequired` diagnostic from coercion *before the pull pass
runs* — a false positive when the pull fills the value, and a
duplicate diagnostic when it does not. No test catches this, because
the derive tests build items in memory and bypass `coerce_fields`.

`FieldDefinition::is_derived()` already exists but only covers
`compute` and `when`. Every past addition of a derivation mechanism
(there have been three: compute, when, pull) had to find all three
sites; the next one will too, unless the predicate is unified.

## Objective

- One predicate on `FieldDefinition` (for example
  `fills_absent_values()`) that names every mechanism which can fill
  an absent value, used at all three sites.
- A store-level test for the `required` + `pull` path — the exact seam
  the in-memory fixtures currently bypass.

## Out of scope

- Changing when the derive passes run or what they compute.
