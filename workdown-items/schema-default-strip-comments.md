---
id: schema-default-strip-comments
status: done
parent: schema-editor-web
title: Move the tutorial comments out of the shipped default schema
---

## In plain words

The default `schema.yaml` that `workdown init` copies into a project
is half comments: explanations of each section and commented-out
examples for aggregated, computed and pulled fields. The web editor
writes the file back without comments ([[schema-field-write-backend]],
requirement 2), so the first browser save deletes all of that. The
knowledge should live where a save cannot delete it, and the shipped
file should contain what the program reads.

## What is needed

- Move the explanations and the commented-out examples from
  `crates/core/defaults/schema.yaml` into the README or the schema
  docs page the comments already point to, keeping the examples
  copy-pasteable.
- Leave a short header comment pointing at that page; nothing else.
- Field descriptions carry any per-field explanation from now on.
- Check the resources and views defaults for the same pattern and
  note whether they need the same treatment; do not do it here.

## Not in scope

- Any change to what the schema contains.
- The web editor itself.

## Decisions taken

Done on 2026-09-22.

1. **The schema guide is the destination, not the README.** Every
   explanation and example the comments carried was already in
   `docs/schema.md` — the aggregate, compute, when and pull sections
   and the rule examples for levels 2 to 4 — except the one thing the
   comments sketched across three blocks: the scheduling setup that
   combines them. That became a "Scheduling recipe" section, one
   copy-pasteable block using the default schema's own `parent` and
   `depends_on`, verified to load.
2. **The header links the guide by URL**, because a consumer project
   has no `docs/` directory; `See docs/schema.md` only worked inside
   this repository. Four lines, nothing else.
3. **`status_color` is the one field that explains itself.** It was
   the only field with a comment of its own; the comment became its
   `description`. The relation fields had a section label, not an
   explanation, and get nothing.
4. **The same pattern elsewhere, noted and left.** `resources.yaml`
   has it too — sample `teams` and `sprints` sections and the whole
   `constants` explanation are comments — and needs the same treatment
   the day a resources editor writes the file; until then nothing
   deletes them. `views.yaml` has only a four-line header, already
   fine. `config.yaml` explains its display roles and `serve` section
   in comments, but no writer touches it (it is read once at startup),
   so they are safe where they are.
