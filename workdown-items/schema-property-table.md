---
id: schema-property-table
status: done
title: Table-drive the "is this property allowed on this field type?" check
parent: maintenance-review-2026-08
---

## In plain words

When the schema parser checks whether a property makes sense on a
field type (`min` on a number: yes; `min` on a checkbox: no), the
rules are spelled out as ~190 lines of near-identical blocks, one per
field type. Adding a thirteenth type or a new property means editing
all twelve blocks and hoping none is forgotten. A lookup table
("type → allowed properties") plus a small loop does the same job in
~40 lines and makes omissions impossible rather than merely unlikely.

## The problem in detail

`crates/core/src/parser/schema.rs:647-838`
(`validate_type_specific_properties`): each of the twelve field types
repeats near-identical `reject_prop` blocks; the only difference
between arms is which one or two properties are *allowed*. The
inverted representation — a table mapping each type to its allowed
property set, iterated by one loop — expresses the same rules in a
form where a new type or property is a table row, not twelve edits.

This is the only bloated part of `parser/schema.rs`; the rest of the
file's length is legitimate (roughly half is colocated tests).

## Objective

Replace the per-type arms with a table plus loop, keeping the exact
same diagnostics — the existing parser tests assert on the messages
and must stay green unchanged.

## Out of scope

- Changing which properties are valid on which type.
