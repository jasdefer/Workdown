---
id: schema-definition-api
status: done
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
    read-only display;
  - every rule: name, description and severity as data, the `match`,
    `require` and `count` blocks as YAML text;
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
  accepts unchanged. Pairs the test rejects are dropped from the table
  (decision 8 below).
- `422` with the schema load diagnostic when `schema.yaml` does not
  parse (ADR-013). No other tier applies: the endpoint reads only the
  schema file (decision 3 below).

## Decisions taken

Settled on 2026-09-10 before implementation.

1. **Two reads of the same bytes.** Plain properties come from the
   typed schema model. Derived blocks are YAML text re-serialized from
   a generic second parse of the same bytes: content exact, formatting
   normalized, no new dependency. The span-aware parser arrives with
   [[schema-field-write-backend]] and may replace the text source then.
2. **No comment handling.** The "carries comments" flag is dropped and
   the editor is comment-blind: it never writes a comment and never
   promises to keep one. Which existing comments survive a write is
   decided by the write strategy alone, in the write backend.
3. **Schema file only.** The endpoint reads `schema.yaml` once, hashes
   those bytes, parses the schema from them and re-parses them for the
   text blocks. It does not load items, resources or views: it needs
   none of them, there is no gap between hash and content, and the
   schema page works while the items directory is broken. Recorded as
   an exemption in `docs/architecture.md` beside `GET /api/project`.
4. **Opaque content hash** of the raw bytes. Only the write endpoints
   interpret it, by re-hashing what they read at write time.
5. **Discriminated union for the type-specific part.** A common header
   (name, type, required, description, default, resource) and one of
   five branches matching the editor shapes: plain scalar, numeric,
   text, value list, relation. Chosen over a flat struct of optionals
   because an impossible combination (a choice with `min`) cannot be
   expressed, and switching type in the editor is constructing a new
   branch. Numeric bounds as floats, duration bounds in seconds, as
   `SchemaData` already does.
6. **Default as a tagged value.** A literal carried as a `FieldValue`
   coerced through the field's own definition, so the item panel's
   editor can render it, or a generator carried by its token.
7. **Rules as typed header plus body text.** The first cut only
   displays rules. The structured shape a rule form needs is defined
   with [[schema-rules-editor]], which is the last child of the
   milestone; adding it beside the text is additive.
8. **The widening table is trimmed by its test.** String coercion
   rejects non-string scalars (`coerce_string_rejects_number` pins
   it), so integer, float and boolean → string are expected to fail
   and are dropped, with the reason documented beside the table.
   Loosening coercion or rewriting items on type change are both out
   of scope.

## Not in scope

- Any write. Rendering it: [[schema-page-read-view]].

## Done

Implemented on 2026-09-10 on branch `schema-editor-web-design`
(uncommitted until the milestone PR): `crates/core/src/schema_definition_data.rs`
(`load`, `build`, `content_hash`, the wire types), the generator and
widening tables in `crates/core/src/model/schema.rs`, the parser asking
the generator table, the route in `crates/server/src/api/schema.rs`,
TypeScript exports in `gen_types.rs`, tests at all three layers, and the
architecture-page exemption. Every gate in `ci.yml` passed locally.

Found on the way, not fixed here: the schema parser checks a literal
`default:` only by YAML kind, so `default: 1.5` on an integer field or
`default: tomorrow` on a date field loads and then fails at
`workdown add`. The payload reports such a default as `invalid` with
the coercion's reason; tightening the parser is a candidate item.
