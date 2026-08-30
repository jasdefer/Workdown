---
id: schema-editor-web-design
status: to_do
parent: schema-editor-web
title: Decide how much of the schema the web app edits, and what a breaking save does
---

## In plain words

Editing the schema from the browser is not hard to build; it is hard to
build *safely*. A saved schema is what every item in the project is
checked against, so a careless save can invalidate the whole project at
once — including the page you saved from. [[schema-editor-web]] lists
the questions that raises. This item answers them and breaks out the
work.

## What this has to produce

- **Scope of the first cut.** Plain fields only, or rules, resources
  and the derived-value blocks (`compute` / `when` / `pull` /
  `aggregate`) too. A field-only editor is a real feature and a
  fraction of the size.
- **The editing surface.** Validated YAML text or a structured form.
  They are different features with different audiences, and the answer
  interacts with whether comments survive a round-trip — the shipped
  default schema is mostly comments, and losing a user's would be
  unforgivable on first save.
- **What a damaging save does.** ADR-001 says a violation warns rather
  than rejects, so the default is "save it, show the damage" — which
  means deciding whether the app previews the blast radius (how many
  items a change would invalidate) before writing, and how the editor
  stays usable when a save makes the project unloadable (the `422`
  tier in ADR-013).
- **Whether the CLI gets a counterpart.** `views.yaml` is already
  editable from the web app with no `workdown view` command behind it.
  Confirm that precedent deliberately or overturn it — but decide,
  rather than setting it a second time by accident.
- **Where the type/property table lives.** A form that knows which
  properties a `choice` field accepts must read the same table the
  validator does, not a third hand-kept copy —
  [[compute-type-support-mismatch]] is what the second copy already
  cost.
- **Follow-up implementation items.**
