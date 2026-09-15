---
id: schema-page-read-view
status: done
parent: schema-editor-web
depends_on: [schema-definition-api]
title: A schema page that shows the fields and rules
---

## In plain words

The web app has no page for the schema. Someone who wants to know
which fields exist and what they accept opens `schema.yaml` in an
editor. This item adds a `/schema` route that shows the schema as the
project has it, read-only. It is the surface every editing item after
it builds on, and useful by itself.

The page as defined in [[schema-editor-web-design]], without the
editing affordances.

## What is needed

- **Route and navigation.** `/schema` in the app shell, reachable
  from the navigation next to the views.
- **Fields section.** A table in declaration order: name, type,
  filled by, required, default, first line of the description. `id` is
  the first row. Rows are not yet clickable. Under the type, a muted
  one-line summary of the type-specific settings and the resource
  (decision 4); the "Filled by" column lists *computed*, *conditional*,
  *pulled* and *aggregated* (decision 8).
- **Rules section.** One entry per rule: name, description, and the
  `match` and `require` blocks rendered as the file has them.
- **Broken state.** When the schema does not load, the page shows the
  load diagnostic and the file path in place of both sections. Every
  other read is in the `422` tier at that moment; this page is where
  the reason is legible.
- **Live refresh.** The page refetches on the file watcher's ping like
  every other view, so an edit in a text editor shows up.
- Reads only `GET /api/schema/definition`. No type knowledge in the
  frontend beyond what the payload carries.

## Decisions taken

Settled on 2026-09-11 before implementation. The design is in
[[schema-editor-web-design]]; these are the choices this page adds.

1. **Header link in the reserved slot.** A plain text link "Schema"
   right after the view pills, in the slot the layout already keeps
   for non-view destinations, highlighted when current. Not a pill,
   so it reads as "not a view". Outside the view navigation so it
   stays visible when the project has no views — which is exactly the
   case when the schema is broken. The header will want a redesign at
   some point; not now.
2. **Broken state stays on the page.** The load function does not
   throw on `422` like the view pages do. The page renders its own
   heading, the schema file path (`source_path` on the load
   diagnostic) and the parser's message in place of both sections.
   No in-browser repair: a file that does not parse has no structure
   to put in a form; fix it in a text editor and the watcher ping
   refreshes the page.
3. **Page load function, not a store.** The layout re-runs every load
   function on the file-change ping, so refresh is free; the cached
   schema store the item editors use is never re-fetched by the ping.
   No other page needs this payload, and the later slide-over editor
   is a child of this page and takes it as a prop.
4. **Type summary line.** A pure function takes a field's `shape` and
   returns one line of text — `to_do · in_progress · done` for a
   choice, `0 to 100` for a numeric, `at most 1d 16h` for a duration,
   `pattern ^[a-z-]+$` for text, `inverse children · no cycles` for a
   relation, nothing for a plain scalar — rendered muted under the
   type. Without it the page shows less than the YAML file; with it the
   page answers the question people open the file for. One case per
   shape kind, unit-tested. The frontend still learns nothing about
   types on its own; it prints what the payload carries. Durations go
   through the formatter every table and chart uses (2026-09-14), so a
   bound written as `40h` reads `1d 16h` here as it does everywhere
   else in the app; the payload carries seconds, not the written form.
5. **Default column.** A generator renders as its monospace token
   (`$today`). A literal renders through the table view's `Cell`
   component with the field's type, so a choice default is a chip and
   a boolean a check mark, the same as in tables. An `invalid` default
   renders as its raw text with a warning mark and the reason on
   hover; after [[schema-default-coercion-check]] lands this case can
   no longer reach a loaded schema and the mark is a fallback.
6. **Description column.** The text as written, cut with an ellipsis
   at the column width, full text on hover — the item panel's habit,
   where the description is a hover on the field name. Every real
   description is one line today; "first line" is a guard. YAML
   comments above a field are not data and never reach the page, so
   this repo's schema shows less prose here than in the file.
7. **Rules section.** Name, severity badge, description, body as a
   monospace block as the file has it. No collapsing; "No rules
   defined" when the list is empty.
8. **"Filled by" column, not badges** (2026-09-15, after seeing the
   page). Badges beside the name made every row need reading to find
   the derived fields. A column between Type and Required lists the
   fill mechanisms — *computed*, *conditional*, *pulled*,
   *aggregated*, two when compute and aggregate combine — and stays
   blank for a field written by hand, so the few derived fields stand
   out on a scan. *Resource-backed* moved out of that set into the
   settings line under the type as `from people`: a resource says
   which values are allowed, the same kind of fact as a choice's values
   or a numeric range, while the column says where a value comes from.
   Also from the same look: the settings line wraps instead of being
   cut, the rules list spans the table's width, and the severity label
   explains itself on hover.

Tests per the web-app rule: unit tests for the pure helpers (fill
mechanisms, settings line, type summary, description cut, row order,
load failure) and the new `pageLabel` case. No component test. No
server change.

## Not in scope

- Clicking a row, adding, removing, reordering. Those are
  [[schema-field-editor-shell]] and [[schema-field-reorder]].
