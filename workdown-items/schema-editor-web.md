---
id: schema-editor-web
status: to_do
title: See and edit the schema in the web app
---

## In plain words

The schema is the file that decides what a work item *is* in this
project — which fields exist, what values they accept, what is
required. Today it can only be changed by opening `schema.yaml` in an
editor and knowing the YAML. The web app can show it, but read-only,
and nothing in the app lets you add a field.

That makes the app a viewer for the most important decision a project
makes. Someone setting up a project has to leave the tool to do the one
thing that shapes everything else. **Example:** you notice halfway
through that you want a `priority` field with three values — right now
that means finding the file, matching the YAML shape of a `choice`
field, and reloading; it should be a form.

From GitHub issue #50.

## Where this stands today

`GET /api/schema` exists and is the only schema route — the server
serves it, the web app renders it, nothing writes it.

The precedent for writing is already set, though, and it is worth being
clear about: **the web app already edits `views.yaml`** — create,
update, delete, all through `/api/views`, with no CLI equivalent
(there is no `workdown view` command). So a schema editor is not a new
kind of thing for the web layer to do. It is the same thing, applied to
a file with much sharper edges.

## Why the edges are sharper

`views.yaml` is a list of view definitions. A bad one breaks that view
and nothing else. `schema.yaml` is different in kind:

- **Every item is validated against it.** Adding a required field with
  no default makes every existing item invalid at once. Narrowing a
  `choice`'s values invalidates every item holding a dropped value.
  Renaming a field orphans every item that wrote it, plus every view
  slot and every rule that names it.
- **It is the thing the rest is checked against.** A save that breaks
  the schema does not break one page — it makes the project fail to
  load, which under ADR-013 is the `422` tier: no data at all, for
  every endpoint, including the editor you just used. The editor has to
  survive breaking its own project.
- **It has structure the type system enforces.** Type-specific
  properties (`values` on a `choice`, `allow_cycles` on a link,
  `resource:`), `compute`/`when`/`pull`/`aggregate` blocks with a typed
  expression language behind them, and rules. A useful editor knows
  which properties belong to which type — a fact the shipped JSON
  schema and the Rust checker each hold a copy of, and which
  [[compute-type-support-mismatch]] shows they do not fully agree on.
  Whatever backs the form should read one table, not a third copy.

## What has to be settled

- **How much of the schema is editable.** Fields only, or rules and
  resource references too. A first cut that adds and edits plain fields
  is genuinely useful and much smaller than one covering computed and
  aggregated fields.
- **What happens to existing items on a breaking edit.** ADR-001 says
  a violation is a warning, not a reject — so the honest default is
  "save it, show the damage". That wants the app to show, before
  saving, how many items the change would invalidate.
- **Whether the CLI gets a counterpart.** Views set the precedent that
  it need not, and that precedent is worth confirming or overturning
  deliberately rather than by accident a second time.
- **Whether comments in `schema.yaml` survive a round-trip.** The
  shipped default schema is heavily commented and so is this repo's; a
  serde round-trip drops all of it. Losing a user's comments on first
  save is the kind of thing that gets a tool uninstalled.
- **Free-text YAML or a structured form.** A textarea with validation
  is cheap and keeps comments; a form is what makes it worth having for
  people who do not know the YAML. They are different features.

## Notes

- Ordinary web-layer rules apply: the envelope and its failure tiers,
  and the file watcher's ping so other open tabs pick the change up
  (ADR-013).
- The `422` case deserves a deliberate answer: if a save makes the
  project unloadable, the editor needs to stay usable enough to undo it.
