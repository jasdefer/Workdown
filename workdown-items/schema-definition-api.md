---
id: schema-definition-api
status: to_do
parent: schema-editor-web
title: Serve the full schema definition and the type tables to the web app
---

## In plain words

The web app knows the schema only as far as item editors need it:
field names, types, allowed values. A schema *editor* needs more — the
whole definition of every field as written in the file, and the rules
of the type system itself: which properties a type accepts, which
generators may default it, which aggregate functions apply, which type
changes are safe. Today those rules live in Rust, some in a table and
some in hand-written parser checks. This item puts them all in the
table and serves the table, so the editor never hardcodes a fact about
a type.

Decision 8 of [[schema-editor-web-design]].

## What is needed

- **`GET /api/schema/definition`.** A new endpoint beside
  `GET /api/schema`, which stays unchanged so item editors do not pay
  for the extra payload. It returns, typed through `ts_rs`:
  - every field's full persisted definition in declaration order,
    including `compute`/`when`/`pull`/`aggregate` blocks as text for
    read-only display, and whether the field's entry in `schema.yaml`
    carries comments;
  - every rule, as the file has it;
  - the property table (`allowed_field_properties` in
    `crates/core/src/model/schema.rs`) keyed by type;
  - the aggregate-function table keyed by type;
  - the generator-per-type table;
  - the widening table of allowed type changes (decision 12);
  - a content hash of `schema.yaml`, which the write endpoints later
    demand back ([[schema-field-write-backend]]).
- **Generator pairing into the table.** Which generator may default
  which type is decided today by hand-written checks in
  `crates/core/src/parser/schema.rs` (around the `Generator::Today =>
  field_type == Date` block). Move it into the schema model's tables
  and make the parser ask the table, exactly as
  [[compute-type-support-mismatch]] did for `compute` and `pull`. The
  parser's messages gain the same "valid on:" suffix.
- **Widening table.** New in the schema model: integer → float; every
  single-value type → string; multichoice and links → list. Pure data,
  with a test that every listed pair is one the store's coercion
  accepts unchanged.
- The same three failure tiers as every endpoint (ADR-013): `422` with
  the load diagnostic when the schema does not load.

## Not in scope

- Any write. Rendering it: [[schema-page-read-view]].
