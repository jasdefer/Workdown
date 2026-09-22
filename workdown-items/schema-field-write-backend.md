---
id: schema-field-write-backend
status: in_progress
parent: schema-editor-web
depends_on: [schema-definition-api]
title: Write one field definition into schema.yaml and say what uses a field
---

## In plain words

The server must be able to add, replace, remove and reorder a single
field in `schema.yaml` without changing the content of any other
field. It must refuse a write that would leave the project unable to
load. And it must be able to say, for any field, what depends on it:
which config roles, views, rules and recipes name it, and how many
items hold a value. The editor uses that answer to grey out *Remove*
with an explanation instead of asking "continue?" after the fact.

Decisions 3, 4, 6, 9 and 11 of [[schema-editor-web-design]], as
revised on 2026-09-22 (see "Requirements" below and the revision notes
in the design item).

## Requirements

Settled with the user on 2026-09-22, replacing the earlier splice,
preview and hash design.

1. Add, change, remove and reorder one field at a time from the web
   app.
2. `schema.yaml` is the single source of truth for *content*, not for
   formatting. A write reads the file fresh, changes only that field,
   and writes the whole file back. Comments are not preserved.
   Untouched fields keep their content and their order. The browser
   never sends the whole schema back.
3. A write that would leave the schema unloadable is refused with the
   parse error. Nothing is written.
4. Otherwise the write happens and any resulting project warnings are
   returned and shown in the banner, exactly as for items and views
   (save-with-warning, ADR-001). No preview dialog.
5. The server can say, per field, what uses it: config roles, views,
   rules, other fields' recipes, and how many items hold a value. The
   editor greys out *Remove* while a role, a view, a rule or a recipe
   still names the field, and shows why. Items holding a value do not
   block; they get the existing unknown-field warning after the write.
6. `id` can never be removed. A field with a recipe (`compute`, `when`,
   `pull`, `aggregate`) keeps its type and its recipe. Any other type
   change must be a pair in the widening table.
7. No conflict detection. Last write wins. The panel's draft survives
   background refetches; the file watcher's ping refreshes the page
   behind it, as it does for every YAML change today.

## What is needed

- **`operations/schema_write.rs`** in core, beside `view_write.rs`.
  Four operations: add a field (appended at the end of `fields:`),
  replace a field's plain properties, remove a field, reorder fields
  from a full ordered name list.
- **Generic-tree edit.** Parse the file into a `serde_yaml` tree, edit
  the one entry under `fields:`, serialize the tree back. For a
  replace, start from the field's existing mapping and overwrite only
  the plain properties the browser sent, so recipe keys survive
  untouched. For an add, write the properties in a fixed order: type,
  the type-specific properties, required, default, description,
  resource.
- **Candidate check before disk.** Run the serialized candidate
  through `parse_schema`. A load failure is a hard refusal carrying
  the parse error. Then load the store, resources and views against
  the candidate schema, write atomically, and diff diagnostics against
  the pre-write load for `mutation_caused_warning`, as `view_write`
  does.
- **Field usage.** `field_usage(config, project_root, field_name)` in
  core: remove the field from the schema in memory, try to parse; if
  the candidate does not parse, the parse error is the blocker. If it
  parses, load the project once with the candidate schema and once
  with the current one and return the diagnostics that are new. Each
  carries its scope (ADR-007), so the client can name the view or rule
  and count the item ones. The config roles (board, tree, graph,
  effort, display) are added from `config.yaml` directly if the load
  does not already report them.
- **Loader variant.** `load_project` gains a sibling that takes an
  already parsed `Schema` and the schema path instead of reading the
  file, and becomes a wrapper over it. `operations::diagnostics` gains
  a function returning the *list* of diagnostics introduced by a
  change; `introduced_by_mutation` is derived from it.
- **Hard refusals, in core.** Removing `id`. Removing a field a
  config role names. Changing the type of a field with a recipe. A
  type change not in the widening table. An invalid name. Adding a
  name that exists. Replacing or removing a name that does not exist.
  A reorder list that is not a permutation of the current names.
- **Payload.** The write reuses the definition API's field shape
  (header plus type-specific union), made deserializable, without
  `derived`. A literal default arrives in the form the item editor's
  set endpoint uses and goes through the field's own coercion.
- **Endpoints.** `POST /api/schema/fields` (201), `PUT` and `DELETE`
  on `/api/schema/fields/{name}` (200), `PUT /api/schema/fields/order`
  with the full name list (200), `GET /api/schema/fields/{name}/usage`
  (200 with the list). `422` for an unloadable candidate, `404` for an
  unknown name, `409` for adding an existing name, `500` on I/O. Same
  envelope and origin guard as every mutation (ADR-013). The
  definition endpoint stays schema-file-only so the page works while
  the items directory is broken; usage is fetched when a panel opens.
- **Tests**, per the three layers in `docs/architecture.md`. Core: one
  test per operation on the shipped default schema, checking the file
  and that untouched fields are content-identical; one refused
  unloadable candidate; one per hard refusal; usage showing a view, a
  rule and an item count, and the parse-error blocker. Server: one
  success per endpoint and each status once.

## Not in scope

- Rename. [[schema-field-rename]].
- Rewriting items on remove. Remove is remove.
- Any UI. [[schema-field-editor-shell]].
- CLI commands for schema fields. The operation lives in core so a
  command later is a thin wrapper; a command today would need a flag
  per property per type, and the CLI user has an editor with
  autocomplete for this file.
- Stripping the tutorial comments from the shipped default schema,
  which a browser save now deletes: [[schema-default-strip-comments]].

## Decisions taken

Settled on 2026-09-22 with the user, after the requirements above
replaced the splice design.

1. **Generic-tree edit, not a typed round trip.** The typed model has
   no writer for field properties or recipes, and one would have to
   turn compiled expressions back into text and could reformat fields
   nobody touched. Editing the `serde_yaml` tree cannot damage what it
   does not touch and keeps recipes for free. What is given up: a
   normalized file (compact `[a, b]` lists come back one per line on
   the first save), and a reusable model writer, which nothing needs
   yet. Views already work this way.
2. **The write payload is the read payload.** The definition API's
   header-plus-union shape, made deserializable, minus `derived`. What
   the form GETs is what it PUTs, and an impossible combination (a
   choice with `min`) stays inexpressible. A literal default cannot
   come back as the untagged `FieldValue` (a date and a string look
   the same on the wire), so it arrives as the item editor sends
   values and is coerced server-side.
3. **REST endpoints like `/api/views`.** The one-message alternative
   was justified only by a shared preview/apply body; with the preview
   gone it buys nothing.
4. **Usage by simulated removal, not by enumeration.** Enumerating
   where a field appears means walking every view slot kind, filters,
   display roles, metric rows and gantt inputs, and rots silently when
   a view kind is added. Loading the project with the field removed
   and diffing diagnostics asks the checks that would complain after a
   real removal, so it cannot drift. Cost: two project loads per panel
   open, what opening a view twice costs today. Served per field on
   its own address so the definition endpoint stays schema-file-only.
5. **Hard refusals live in core.** The server maps each to a status;
   a CLI wrapper later inherits them.
6. **Tests per layer**, as listed under "What is needed".

## Insights

- A schema write may only ever be "read fresh, change one field, write
  back". If the browser sent the whole schema, a stale tab would
  overwrite fields it never touched. Last-write-wins is only safe per
  field.
- Some cross-references are parse errors, not warnings: `aggregate.over`
  and `inverse` are checked inside `parse_schema`. Removing a link
  field another field aggregates over makes the candidate unloadable,
  which is why usage must report the parse error as a blocker rather
  than only diffing diagnostics.
- The item editor never loses edits on a watcher ping because it
  saves each field on change. The schema panel holds a draft (a
  definition is only valid as a whole), so the panel must keep its
  draft across the page's refetches. That is the one place the
  editor-shell item has to be careful.
